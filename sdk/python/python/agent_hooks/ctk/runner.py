# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
"""CTK runner: load vectors, drive a harness, assert ``expect``.

The assertion engine, capability skip check, and scripted
interceptor/resolver evaluation live in the Rust core
(``_core.ctk_*``). This module keeps only:

- vector file globbing (filesystem access stays per-language),
- the orchestration loop that calls ``harness.setup/run/teardown``
  (native callbacks into the framework under test),
- ``RunRecord`` → wire-JSON marshalling for the core.

Every other language SDK's runner has the same shape.
"""
from __future__ import annotations

import asyncio
import json
import pathlib
from dataclasses import dataclass, field
from typing import Any

from agent_hooks import _core
from agent_hooks._types import EnforcementMode
from agent_hooks.ctk.harness import Harness, RunRecord, Scenario
from agent_hooks.ctk.scripted import (
    RecordingInterceptor,
    ScriptedInterceptor,
    ScriptedResolver,
)


@dataclass(slots=True)
class VectorResult:
    id: str
    title: str
    level: int
    status: str  # "pass" | "fail" | "skip"
    detail: str = ""
    failures: list[str] = field(default_factory=list)


def load_vectors(directory: str | pathlib.Path, *, max_level: int = 3) -> list[dict[str, Any]]:
    d = pathlib.Path(directory)
    out: list[dict[str, Any]] = []
    for f in sorted(d.glob("AH-CTK-*.json")):
        v = json.loads(f.read_text())
        if v["level"] <= max_level:
            out.append(v)
    return out


def _run_record_to_wire(rr: RunRecord) -> str:
    return json.dumps(
        {
            "outcome": rr.outcome.value,
            "final_output": rr.final_output,
            "tool_invocations": rr.tool_invocations,
            "error": rr.error,
        }
    )


async def run_vector(harness: Harness, vector: dict[str, Any]) -> VectorResult:
    vid, title, level = vector["id"], vector["title"], vector["level"]
    vector_json = json.dumps(vector)

    caps_json = json.dumps(sorted(c.value for c in harness.capabilities))
    skip = json.loads(_core.ctk_should_skip(vector_json, caps_json))
    if skip is not None:
        return VectorResult(vid, title, level, "skip", detail=skip)

    scenario = Scenario.from_wire(vector["scenario"])
    interceptor = RecordingInterceptor(ScriptedInterceptor(vector["interceptor_script"]))
    approval = vector.get("approval_script")
    resolver = ScriptedResolver(approval) if approval else None
    mode = EnforcementMode(vector.get("mode", "enforce"))

    harness.setup(scenario, interceptor, resolver, mode)
    try:
        rr = await harness.run()
    except Exception as e:  # noqa: BLE001
        return VectorResult(vid, title, level, "fail", failures=[f"harness.run raised: {e!r}"])
    finally:
        harness.teardown()

    result = json.loads(
        _core.ctk_assert(
            vector_json,
            json.dumps(interceptor.recorded),
            _run_record_to_wire(rr),
        )
    )
    return VectorResult(
        id=result["id"],
        title=result["title"],
        level=result["level"],
        status=result["status"],
        detail=result.get("detail", ""),
        failures=result.get("failures", []),
    )


def run_vectors(harness_factory: Any, vectors: list[dict[str, Any]]) -> list[VectorResult]:
    """Run all vectors against a fresh harness instance per vector."""

    async def _go() -> list[VectorResult]:
        out = []
        for v in vectors:
            out.append(await run_vector(harness_factory(), v))
        return out

    return asyncio.run(_go())
