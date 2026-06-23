# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
"""Core enums and value types for AGENT-HOOKS-0.1 (§3, §5, §8, §11)."""
from __future__ import annotations

from dataclasses import dataclass, field
from enum import Enum
from typing import Any, Final

#: Spec version this SDK implements (§4.1 ``spec`` field).
SPEC_VERSION: Final[str] = "agent-hooks/0.1"


class HookPoint(str, Enum):
    """The closed set of agent lifecycle hook points (§3)."""

    AGENT_STARTUP = "agent_startup"
    INPUT = "input"
    PRE_MODEL_CALL = "pre_model_call"
    POST_MODEL_CALL = "post_model_call"
    PRE_TOOL_CALL = "pre_tool_call"
    POST_TOOL_CALL = "post_tool_call"
    OUTPUT = "output"
    AGENT_SHUTDOWN = "agent_shutdown"

    @property
    def transform_permitted(self) -> bool:
        """Whether a ``transform`` verdict is permitted at this point (§3, §4.3)."""
        return self not in {HookPoint.AGENT_STARTUP, HookPoint.AGENT_SHUTDOWN}

    @property
    def is_pre(self) -> bool:
        return self in {HookPoint.PRE_MODEL_CALL, HookPoint.PRE_TOOL_CALL}

    @property
    def is_post(self) -> bool:
        return self in {HookPoint.POST_MODEL_CALL, HookPoint.POST_TOOL_CALL}


class Decision(str, Enum):
    """Verdict decision values (§5.1)."""

    ALLOW = "allow"
    DENY = "deny"
    WARN = "warn"
    ESCALATE = "escalate"
    TRANSFORM = "transform"

    @property
    def permits(self) -> bool:
        """Whether the action proceeds under this decision (§2)."""
        return self in {Decision.ALLOW, Decision.WARN, Decision.TRANSFORM}

    @property
    def blocks(self) -> bool:
        return not self.permits


class EnforcementMode(str, Enum):
    """Whether the host acts on verdicts (§8)."""

    ENFORCE = "enforce"
    EVALUATE_ONLY = "evaluate_only"


class HookError(str, Enum):
    """Reserved ``hook_error:*`` reasons a host synthesizes (§11)."""

    CONTEXT_INVALID = "hook_error:context_invalid"
    CONSUMER_FAILED = "hook_error:consumer_failed"
    CONSUMER_TIMEOUT = "hook_error:consumer_timeout"
    VERDICT_INVALID = "hook_error:verdict_invalid"
    TRANSFORM_INVALID = "hook_error:transform_invalid"
    TRANSFORM_TARGET_FORBIDDEN = "hook_error:transform_target_forbidden"
    APPROVAL_RESOLVER_MISSING = "hook_error:approval_resolver_missing"
    APPROVAL_RESOLVER_FAILED = "hook_error:approval_resolver_failed"
    APPROVAL_UNRESOLVED = "hook_error:approval_unresolved"
    APPROVAL_ACTION_MISMATCH = "hook_error:approval_action_mismatch"
    ADAPTER_UNSUPPORTED = "hook_error:adapter_unsupported"
    STREAMING_UNSUPPORTED = "hook_error:streaming_unsupported"


@dataclass(frozen=True, slots=True)
class Transform:
    """A single ``$target``-rooted replacement (§5.2)."""

    path: str
    value: Any

    def __post_init__(self) -> None:
        if not (self.path.startswith("$target") or self.path.startswith("$policy_target")):
            raise ValueError(
                f"transform.path must be rooted at $target (got {self.path!r})"
            )

    def to_wire(self) -> dict[str, Any]:
        return {"path": self.path, "value": self.value}

    @classmethod
    def from_wire(cls, obj: dict[str, Any]) -> Transform:
        return cls(path=obj["path"], value=obj["value"])


@dataclass(frozen=True, slots=True)
class Evidence:
    """Opaque pointer to an offline-verifiable artefact (§5.3)."""

    artefact: str | None = None
    verification_pointers: dict[str, str] = field(default_factory=dict)

    def to_wire(self) -> dict[str, Any]:
        out: dict[str, Any] = {}
        if self.artefact is not None:
            out["artefact"] = self.artefact
        if self.verification_pointers:
            out["verification_pointers"] = dict(self.verification_pointers)
        return out

    @classmethod
    def from_wire(cls, obj: dict[str, Any]) -> Evidence:
        return cls(
            artefact=obj.get("artefact"),
            verification_pointers=dict(obj.get("verification_pointers") or {}),
        )


@dataclass(frozen=True, slots=True)
class Verdict:
    """Consumer return value (§5).

    Hosts construct a Verdict from a consumer's wire output via
    :meth:`from_wire`, which validates per §5 and raises :class:`ValueError`
    on violation; the emitter maps that to ``hook_error:verdict_invalid``.
    """

    decision: Decision
    reason: str | None = None
    message: str | None = None
    transform: Transform | None = None
    evidence: Evidence | None = None
    result_labels: tuple[str, ...] = ()

    def __post_init__(self) -> None:
        if self.reason is not None and self.reason.startswith("hook_error:"):
            raise ValueError("verdict.reason MUST NOT start with 'hook_error:' (§5)")
        if self.decision is Decision.TRANSFORM and self.transform is None:
            raise ValueError("transform body REQUIRED when decision=='transform' (§5)")
        if self.decision is not Decision.TRANSFORM and self.transform is not None:
            raise ValueError("transform body FORBIDDEN when decision!='transform' (§5)")

    def to_wire(self) -> dict[str, Any]:
        out: dict[str, Any] = {"decision": self.decision.value}
        if self.reason is not None:
            out["reason"] = self.reason
        if self.message is not None:
            out["message"] = self.message
        if self.transform is not None:
            out["transform"] = self.transform.to_wire()
        if self.evidence is not None:
            out["evidence"] = self.evidence.to_wire()
        if self.result_labels:
            out["result_labels"] = list(self.result_labels)
        return out

    @classmethod
    def from_wire(cls, obj: Any) -> Verdict:
        if not isinstance(obj, dict):
            raise ValueError("verdict must be a JSON object")
        try:
            decision = Decision(obj["decision"])
        except (KeyError, ValueError) as e:
            raise ValueError(f"verdict.decision invalid: {e}") from e
        reason = obj.get("reason")
        if reason is not None and not isinstance(reason, str):
            raise ValueError("verdict.reason must be string or null")
        message = obj.get("message")
        if message is not None and not isinstance(message, str):
            raise ValueError("verdict.message must be string or null")
        transform = None
        if "transform" in obj and obj["transform"] is not None:
            t = obj["transform"]
            if not isinstance(t, dict) or "path" not in t or "value" not in t:
                raise ValueError("verdict.transform must be {path, value}")
            transform = Transform.from_wire(t)
        evidence = None
        if "evidence" in obj and obj["evidence"] is not None:
            if not isinstance(obj["evidence"], dict):
                raise ValueError("verdict.evidence must be an object")
            evidence = Evidence.from_wire(obj["evidence"])
        labels = obj.get("result_labels") or []
        if not isinstance(labels, list) or not all(isinstance(s, str) for s in labels):
            raise ValueError("verdict.result_labels must be an array of strings")
        return cls(
            decision=decision,
            reason=reason,
            message=message,
            transform=transform,
            evidence=evidence,
            result_labels=tuple(labels),
        )

    @classmethod
    def hook_error(cls, err: HookError, message: str | None = None) -> Verdict:
        """Host-synthesized deny verdict for a §11 failure.

        ``reason`` carries the reserved ``hook_error:*`` identifier. This
        bypasses the consumer-side prefix check by constructing directly.
        """
        v = object.__new__(cls)
        object.__setattr__(v, "decision", Decision.DENY)
        object.__setattr__(v, "reason", err.value)
        object.__setattr__(v, "message", message)
        object.__setattr__(v, "transform", None)
        object.__setattr__(v, "evidence", None)
        object.__setattr__(v, "result_labels", ())
        return v


#: Convenience constant for the trivial permit verdict.
ALLOW: Final[Verdict] = Verdict(decision=Decision.ALLOW)


@dataclass(frozen=True, slots=True)
class HookResult:
    """Host-side record of one hook evaluation (§6, §10)."""

    hook_point: HookPoint
    mode: EnforcementMode
    verdict: Verdict
    input_identity: str
    enforced_identity: str
    transformed_target: Any | None = None

    @property
    def proceeds(self) -> bool:
        """Whether the guarded action executes (§6, §8)."""
        if self.mode is EnforcementMode.EVALUATE_ONLY:
            return True
        return self.verdict.decision.permits
