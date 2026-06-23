# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
"""agent-hooks: framework-neutral agent lifecycle hook contract.

Implements AGENT-HOOKS-0.1. See ``spec/AGENT-HOOKS-0.1.md`` for the normative
text. This package provides:

- :class:`HookPoint`, :class:`Decision`, :class:`EnforcementMode` — enums (§3, §5, §8)
- :class:`Verdict`, :class:`Transform`, :class:`Evidence` — consumer return (§5)
- :class:`HookContext` and per-hook builders — host payload (§4)
- :class:`HookConsumer`, :class:`ApprovalResolver` — protocols (§7, §9)
- :class:`HookEmitter` — host-side helper that builds context, dispatches to
  consumers, applies the verdict, and returns a :class:`HookResult` (§6)
- :func:`canonical_json`, :func:`context_identity` — §10
- :mod:`agent_hooks.ctk` — Conformance Test Kit (§13)
"""
from __future__ import annotations

from agent_hooks._types import (
    ALLOW,
    SPEC_VERSION,
    Decision,
    EnforcementMode,
    Evidence,
    HookError,
    HookPoint,
    HookResult,
    Transform,
    Verdict,
)
from agent_hooks.approval import (
    ApprovalOutcome,
    ApprovalRequest,
    ApprovalResolution,
    ApprovalResolver,
)
from agent_hooks.canonical import canonical_json, context_identity
from agent_hooks.consumer import HookConsumer
from agent_hooks.context import HookContext, HookContextBuilder
from agent_hooks.emitter import HookEmitter
from agent_hooks.exceptions import HookBlocked, HookSuspended

__all__ = [
    "ALLOW",
    "SPEC_VERSION",
    "ApprovalOutcome",
    "ApprovalRequest",
    "ApprovalResolution",
    "ApprovalResolver",
    "Decision",
    "EnforcementMode",
    "Evidence",
    "HookBlocked",
    "HookConsumer",
    "HookContext",
    "HookContextBuilder",
    "HookEmitter",
    "HookError",
    "HookPoint",
    "HookResult",
    "HookSuspended",
    "Transform",
    "Verdict",
    "canonical_json",
    "context_identity",
]
