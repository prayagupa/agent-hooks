# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
"""Approval seam for ``escalate`` verdicts (§9)."""
from __future__ import annotations

from dataclasses import dataclass
from enum import Enum
from typing import Protocol, runtime_checkable

from agent_hooks._types import InterceptionPoint, Verdict
from agent_hooks.context import AgentContext


class ApprovalOutcome(str, Enum):
    APPROVE = "approve"
    REJECT = "reject"
    UNRESOLVED = "unresolved"


@dataclass(frozen=True, slots=True)
class ApprovalRequest:
    """What the host hands the resolver on ``escalate`` (§9)."""

    context_identity: str
    interception_point: InterceptionPoint
    verdict: Verdict
    context: AgentContext


@dataclass(frozen=True, slots=True)
class ApprovalResolution:
    """What the resolver returns (§9)."""

    outcome: ApprovalOutcome
    context_identity: str
    verdict: Verdict | None = None

    def __post_init__(self) -> None:
        if self.outcome is ApprovalOutcome.UNRESOLVED:
            if self.verdict is not None:
                raise ValueError("unresolved resolution MUST NOT carry a verdict")
        elif self.verdict is None:
            raise ValueError("approve/reject resolution MUST carry a verdict")
        elif self.outcome is ApprovalOutcome.APPROVE and not self.verdict.decision.permits:
            raise ValueError("approve MUST carry a permit verdict")
        elif self.outcome is ApprovalOutcome.REJECT and self.verdict.decision.permits:
            raise ValueError("reject MUST carry a deny verdict")


@runtime_checkable
class ApprovalResolver(Protocol):
    """Host-registered callable that resolves an ``escalate`` (§9)."""

    def resolve(self, request: ApprovalRequest, /) -> ApprovalResolution: ...
