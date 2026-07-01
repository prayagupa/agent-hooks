# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
"""CTK-supplied interceptor/resolver that replay a vector's scripts."""
from __future__ import annotations

import copy
from dataclasses import dataclass, field
from typing import Any

from agent_hooks._types import Verdict
from agent_hooks.approval import ApprovalOutcome, ApprovalRequest, ApprovalResolution
from agent_hooks.context import AgentContext


def _lookup(ctx: Any, dotted: str) -> Any:
    """Resolve a dotted/bracket path (``a.b[0].c``) against ``ctx``."""
    cur: Any = ctx
    token = ""
    i = 0
    while i <= len(dotted):
        ch = dotted[i] if i < len(dotted) else "."
        if ch == ".":
            if token:
                cur = cur[token]
                token = ""
        elif ch == "[":
            if token:
                cur = cur[token]
                token = ""
            j = dotted.index("]", i)
            cur = cur[int(dotted[i + 1 : j])]
            i = j
        else:
            token += ch
        i += 1
    return cur


def _matches(ctx: AgentContext, predicates: dict[str, Any] | None) -> bool:
    if not predicates:
        return True
    for path, expected in predicates.items():
        try:
            if _lookup(ctx, path) != expected:
                return False
        except (KeyError, IndexError, TypeError):
            return False
    return True


@dataclass(slots=True)
class ScriptedInterceptor:
    """Replays a vector's ``interceptor_script``: first matching rule wins, else ``allow``."""

    rules: list[dict[str, Any]]

    def intercept(self, context: AgentContext) -> dict[str, Any]:
        hp = context["interception_point"]
        for rule in self.rules:
            if rule["at"] != hp:
                continue
            if _matches(context, rule.get("match")):
                return rule["return"]
        return {"decision": "allow"}


@dataclass(slots=True)
class RecordingInterceptor:
    """Wraps another interceptor and records every context passed through."""

    inner: ScriptedInterceptor
    recorded: list[AgentContext] = field(default_factory=list)

    def intercept(self, context: AgentContext) -> dict[str, Any]:
        # Deep-copy so later mutation (transform write-back) doesn't alter the
        # record of what the interceptor *saw*.
        self.recorded.append(copy.deepcopy(context))
        return self.inner.intercept(context)


@dataclass(slots=True)
class ScriptedResolver:
    """Replays a vector's ``approval_script``."""

    rules: list[dict[str, Any]]

    def resolve(self, request: ApprovalRequest) -> ApprovalResolution:
        for rule in self.rules:
            if _matches(request.context, rule.get("match")):
                r = rule["resolve"]
                outcome = ApprovalOutcome(r["outcome"])
                verdict = Verdict.from_wire(r["verdict"]) if "verdict" in r else None
                return ApprovalResolution(
                    outcome=outcome,
                    context_identity=request.context_identity,
                    verdict=verdict,
                )
        return ApprovalResolution(
            outcome=ApprovalOutcome.UNRESOLVED,
            context_identity=request.context_identity,
        )
