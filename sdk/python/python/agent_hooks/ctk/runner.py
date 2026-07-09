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
from agent_hooks._marshal import dumps
from agent_hooks._types import EnforcementMode
from agent_hooks.composition import CompositionConfig
from agent_hooks.ctk.harness import Harness, RunRecord, Scenario
from agent_hooks.ctk.scripted import (
    RecordingInterceptor,
    ScriptedInterceptor,
    ScriptedResolver,
)

#: TODO(stage-4): vectors still authored in the pre-P-003 five-verdict
#: wire vocabulary (``warn``, ``escalate``, ``approval_resolver_missing``).
#: Stage 4 rewrites them to the three-verdict shapes (§5.1); until then
#: they are stale and skipped here — and ONLY here. Mirrors the Rust
#: exclusion list in ``sdk/rust/core/tests/ctk_reference.rs``.
TODO_STAGE_4: frozenset[str] = frozenset(
    {
        "AH-CTK-030",  # escalate-approve → deny+approval / resolution
        "AH-CTK-031",  # escalate-reject → deny+approval / reject
        "AH-CTK-032",  # escalate-no-resolver → liftable deny stands (§9)
        "AH-CTK-050",  # warn-passthrough → allow+warnings
        "AH-CTK-072",  # resolver-identity-mismatch → echo rule via seam
        "AH-CTK-073",  # resolver-raises → approval_resolver_failed via seam
    }
)


@dataclass(slots=True)
class VectorResult:
    id: str
    title: str
    status: str  # "pass" | "fail" | "skip"
    detail: str = ""
    failures: list[str] = field(default_factory=list)


def load_vectors(directory: str | pathlib.Path) -> list[dict[str, Any]]:
    d = pathlib.Path(directory)
    return [json.loads(f.read_text()) for f in sorted(d.glob("AH-CTK-*.json"))]


def _run_record_to_wire(rr: RunRecord) -> str:
    return dumps(
        {
            "outcome": rr.outcome.value,
            "final_output": rr.final_output,
            "tool_invocations": rr.tool_invocations,
            "error": rr.error,
            "identities": [
                {"input_identity": i, "enforced_identity": e} for i, e in rr.identities
            ],
        }
    )


async def run_vector(harness: Harness, vector: dict[str, Any]) -> VectorResult:
    vid, title = vector["id"], vector["title"]
    if vid in TODO_STAGE_4:
        return VectorResult(
            vid, title, "skip", detail="TODO(stage-4): stale pre-P-003 vector, rewritten in stage 4"
        )
    vector_json = dumps(vector)

    caps_json = dumps(sorted(c.value for c in harness.capabilities))
    skip = json.loads(_core.ctk_should_skip(vector_json, caps_json))
    if skip is not None:
        return VectorResult(vid, title, "skip", detail=skip)

    scenario = Scenario.from_wire(vector["scenario"])
    # Multi-interceptor vectors (§7.4 fold-through) use interceptor_scripts;
    # single-interceptor vectors use interceptor_script. Only the FIRST
    # interceptor records: expect.interceptions describes each emission as
    # the first-registered interceptor saw it.
    scripts = vector.get("interceptor_scripts")
    if scripts is None:
        scripts = [vector["interceptor_script"]]
    first = RecordingInterceptor(ScriptedInterceptor(scripts[0])) if scripts else None
    interceptors: list[Any] = [first] if first else []
    interceptors += [ScriptedInterceptor(s) for s in scripts[1:]]
    approval = vector.get("approval_script")
    resolver = ScriptedResolver(approval) if approval else None
    mode = EnforcementMode(vector.get("mode", "enforce"))
    # §13.2: composition vectors carry the profile/knobs they apply to;
    # absent means the pre-P-003 default (§7.2).
    composition = CompositionConfig.from_wire(vector.get("composition"))

    harness.setup(scenario, interceptors, resolver, mode, composition)
    try:
        rr = await harness.run()
    except Exception as e:  # noqa: BLE001
        return VectorResult(vid, title, "fail", failures=[f"harness.run raised: {e!r}"])
    finally:
        harness.teardown()

    result = json.loads(
        _core.ctk_assert(
            vector_json,
            dumps(first.recorded if first else []),
            _run_record_to_wire(rr),
        )
    )
    return VectorResult(
        id=result["id"],
        title=result["title"],
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
