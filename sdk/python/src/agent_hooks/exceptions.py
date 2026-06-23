# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
"""Exceptions a host raises to its caller when a hook blocks (§6)."""
from __future__ import annotations

from agent_hooks._types import HookPoint, HookResult


class HookBlocked(RuntimeError):
    """Raised by a host when a verdict blocks the guarded action (§6).

    The host's agent loop catches this to surface a tool/turn error to the
    model rather than crashing the session.
    """

    def __init__(self, result: HookResult) -> None:
        self.result = result
        super().__init__(
            f"{result.hook_point.value} blocked: "
            f"{result.verdict.decision.value} ({result.verdict.reason or 'no reason'})"
        )

    @property
    def hook_point(self) -> HookPoint:
        return self.result.hook_point


class HookSuspended(RuntimeError):
    """Raised by a host when an ``escalate`` verdict awaits out-of-band approval (§9).

    Hosts that resolve approval synchronously never raise this; hosts that
    defer to an external workflow raise it so the caller can persist the
    pending :class:`~agent_hooks.approval.ApprovalRequest` and resume later.
    """

    def __init__(self, result: HookResult) -> None:
        self.result = result
        super().__init__(
            f"{result.hook_point.value} suspended pending approval "
            f"({result.verdict.reason or 'no reason'})"
        )
