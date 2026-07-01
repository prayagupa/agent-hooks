# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
"""CTK runner: load vectors, drive a harness, assert ``expect``."""
from __future__ import annotations

import asyncio
import json
import pathlib
from dataclasses import dataclass, field
from typing import Any

from agent_hooks._types import EnforcementMode
from agent_hooks.ctk.harness import Capability, Harness, RunRecord, Scenario
from agent_hooks.ctk.scripted import (
    RecordingInterceptor,
    ScriptedInterceptor,
    ScriptedResolver,
    _lookup,
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


def _validate_context(ctx: dict[str, Any], failures: list[str]) -> None:
    """Schema-validate one recorded context against the per-interception-point schema.

    Falls back to the ``jsonschema`` package when installed; otherwise performs
    a structural L0 check so the CTK has zero hard dependencies.
    """
    hp = ctx.get("interception_point")
    try:
        import jsonschema  # type: ignore[import-not-found]

        from agent_hooks.ctk._schemas import per_point_registry, per_point_schema

        jsonschema.validate(ctx, per_point_schema(hp), registry=per_point_registry())
    except ModuleNotFoundError:
        l0 = ("spec", "interception_point", "timestamp", "sequence", "agent",
              "session", "target")
        for k in l0:
            if k not in ctx:
                failures.append(f"{hp}: missing L0 field {k!r}")
    except Exception as e:  # noqa: BLE001
        failures.append(f"{hp}: schema validation failed: {e}")


_IP = "interception_point"


def _assert_interceptions(
    expect: dict[str, Any], recorded: list[dict[str, Any]], failures: list[str]
) -> None:
    expected = expect["interceptions"]
    strict = expect.get("sequence_strict", True)
    rec_ips = [c[_IP] for c in recorded]

    if strict:
        if rec_ips != [e[_IP] for e in expected]:
            failures.append(
                f"interception sequence mismatch:\n"
                f"  expected {[e[_IP] for e in expected]}\n"
                f"  got      {rec_ips}"
            )
            return
        pairs = list(zip(expected, recorded, strict=True))
    else:
        # Subsequence match: each expected interception point matches the
        # next recorded one of the same interception_point.
        pairs = []
        ri = 0
        for e in expected:
            while ri < len(recorded) and recorded[ri][_IP] != e[_IP]:
                ri += 1
            if ri >= len(recorded):
                failures.append(
                    f"expected interception point {e[_IP]!r} not found in sequence"
                )
                return
            pairs.append((e, recorded[ri]))
            ri += 1

    for e, r in pairs:
        if e.get("context_must_validate", True):
            _validate_context(r, failures)
        for path, want in (e.get("context") or {}).items():
            try:
                got = _lookup(r, path)
            except (KeyError, IndexError, TypeError):
                failures.append(f"{e['interception_point']}: path {path!r} did not resolve")
                continue
            if got != want:
                failures.append(f"{e['interception_point']}: {path} == {got!r}, want {want!r}")

    for absent in expect.get("interceptions_absent", []):
        if absent in rec_ips:
            failures.append(f"hook {absent!r} was emitted but MUST be absent")


def _assert_record(expect: dict[str, Any], rr: RunRecord, failures: list[str]) -> None:
    if rr.outcome.value != expect["run_outcome"]:
        failures.append(f"run_outcome == {rr.outcome.value!r}, want {expect['run_outcome']!r}")
    if "final_output" in expect and rr.final_output != expect["final_output"]:
        failures.append(f"final_output == {rr.final_output!r}, want {expect['final_output']!r}")
    if "tool_invocations" in expect and rr.tool_invocations != expect["tool_invocations"]:
        failures.append(
            f"tool_invocations == {rr.tool_invocations!r}, want {expect['tool_invocations']!r}"
        )
    for name in expect.get("tool_not_invoked", []):
        if any(inv["name"] == name for inv in rr.tool_invocations):
            failures.append(f"tool {name!r} was invoked but MUST NOT be")


async def run_vector(harness: Harness, vector: dict[str, Any]) -> VectorResult:
    vid, title, level = vector["id"], vector["title"], vector["level"]
    needed = {Capability(c) for c in vector.get("capabilities", [])}
    if not needed.issubset(harness.capabilities):
        missing = sorted(c.value for c in needed - harness.capabilities)
        return VectorResult(vid, title, level, "skip", detail=f"missing capabilities: {missing}")

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

    failures: list[str] = []
    _assert_interceptions(vector["expect"], interceptor.recorded, failures)
    _assert_record(vector["expect"], rr, failures)

    seq = [c["sequence"] for c in interceptor.recorded]
    if seq != sorted(seq) or len(set(seq)) != len(seq):
        failures.append(f"sequence not strictly increasing: {seq}")

    return VectorResult(
        vid, title, level, "fail" if failures else "pass", failures=failures
    )


def run_vectors(
    harness_factory: Any, vectors: list[dict[str, Any]]
) -> list[VectorResult]:
    """Run all vectors against a fresh harness instance per vector."""

    async def _go() -> list[VectorResult]:
        out = []
        for v in vectors:
            out.append(await run_vector(harness_factory(), v))
        return out

    return asyncio.run(_go())
