# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
"""agent-hooks: framework-neutral agent lifecycle hook contract.

Implements AGENT-HOOKS-0.1. See ``spec/AGENT-HOOKS-0.1.md`` for the normative
text. This package provides:

- :class:`InterceptionPoint`, :class:`Decision`, :class:`EnforcementMode` — enums (§3, §5, §8)
- :class:`Verdict`, :class:`Transform`, :class:`Evidence` — interceptor return (§5)
- :class:`AgentContext` and per-hook builders — host payload (§4)
- :class:`Interceptor`, :class:`ApprovalResolver` — protocols (§7, §9)
- :class:`InterceptionEmitter` — host-side helper that builds context, dispatches to
  interceptors, applies the verdict, and returns a :class:`InterceptionRecord` (§6)
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
    HostError,
    InterceptionPoint,
    InterceptionRecord,
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
from agent_hooks.context import AgentContext, AgentContextBuilder
from agent_hooks.emitter import InterceptionEmitter
from agent_hooks.exceptions import InterceptionBlocked, InterceptionSuspended
from agent_hooks.interceptor import Interceptor

__all__ = [
    "ALLOW",
    "SPEC_VERSION",
    "AgentContext",
    "AgentContextBuilder",
    "ApprovalOutcome",
    "ApprovalRequest",
    "ApprovalResolution",
    "ApprovalResolver",
    "Decision",
    "EnforcementMode",
    "Evidence",
    "HostError",
    "InterceptionBlocked",
    "InterceptionEmitter",
    "InterceptionPoint",
    "InterceptionRecord",
    "InterceptionSuspended",
    "Interceptor",
    "Transform",
    "Verdict",
    "canonical_json",
    "context_identity",
]
