# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
"""Unit tests for §5 verdict validation and §3 interception-point properties."""
from __future__ import annotations

import pytest

from agent_hooks import Decision, HostError, InterceptionPoint, Transform, Verdict


class TestInterceptionPoint:
    def test_eight_values(self) -> None:
        assert len(InterceptionPoint) == 8

    @pytest.mark.parametrize("hp", list(InterceptionPoint))
    def test_transform_permitted(self, hp: InterceptionPoint) -> None:
        forbidden = {InterceptionPoint.AGENT_STARTUP, InterceptionPoint.AGENT_SHUTDOWN}
        assert hp.transform_permitted == (hp not in forbidden)


class TestVerdict:
    def test_allow_constant(self) -> None:
        from agent_hooks import ALLOW

        assert ALLOW.decision is Decision.ALLOW
        assert ALLOW.decision.permits

    def test_transform_requires_body(self) -> None:
        with pytest.raises(ValueError, match="REQUIRED"):
            Verdict(decision=Decision.TRANSFORM)

    def test_transform_forbidden_on_allow(self) -> None:
        with pytest.raises(ValueError, match="FORBIDDEN"):
            Verdict(decision=Decision.ALLOW, transform=Transform("$target.x", 1))

    def test_interceptor_cannot_emit_host_error_reason(self) -> None:
        with pytest.raises(ValueError, match="host_error"):
            Verdict(decision=Decision.DENY, reason="host_error:nope")

    def test_host_error_factory_bypasses_check(self) -> None:
        v = Verdict.host_error(HostError.INTERCEPTOR_FAILED)
        assert v.decision is Decision.DENY
        assert v.reason == "host_error:interceptor_failed"

    def test_from_wire_roundtrip(self) -> None:
        wire = {
            "decision": "transform",
            "reason": "redact",
            "transform": {"path": "$target.url", "value": "x"},
            "result_labels": ["pii"],
        }
        v = Verdict.from_wire(wire)
        assert v.decision is Decision.TRANSFORM
        assert v.transform.path == "$target.url"
        assert v.to_wire()["transform"]["value"] == "x"

    def test_from_wire_rejects_bad_decision(self) -> None:
        with pytest.raises(ValueError):
            Verdict.from_wire({"decision": "maybe"})


class TestTransform:
    def test_rejects_foreign_root(self) -> None:
        with pytest.raises(ValueError):
            Transform(path="$snapshot.x", value=1)

    def test_accepts_policy_target_alias(self) -> None:
        Transform(path="$policy_target.x", value=1)
