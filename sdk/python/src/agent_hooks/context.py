# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
"""``HookContext`` construction (§4).

A :class:`HookContext` is a plain ``dict[str, Any]`` so it serializes to wire
JSON without translation. :class:`HookContextBuilder` is the host-side helper
that owns the L0 envelope (agent/session/sequence) and exposes one method per
hook point that fills L1 and sets ``target``.
"""
from __future__ import annotations

from datetime import datetime, timezone
from typing import Any, TypeAlias

from agent_hooks._types import SPEC_VERSION, HookPoint

#: A hook context is wire-shaped JSON: ``dict[str, Any]`` validated against
#: ``spec/schema/hook-context.schema.json``.
HookContext: TypeAlias = dict[str, Any]


def _now() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.%fZ")


class HookContextBuilder:
    """Stateful per-session builder for :class:`HookContext` values.

    Owns ``sequence`` and the L0 ``agent``/``session`` envelope so adapter
    code never has to thread them. One instance per session.
    """

    __slots__ = ("_agent", "_session", "_seq", "_l2")

    def __init__(
        self,
        *,
        agent_id: str,
        framework: str,
        session_id: str,
        agent_name: str | None = None,
        agent_version: str | None = None,
        session_started_at: str | None = None,
    ) -> None:
        self._agent: dict[str, Any] = {"id": agent_id, "framework": framework}
        if agent_name:
            self._agent["name"] = agent_name
        if agent_version:
            self._agent["version"] = agent_version
        self._session: dict[str, Any] = {"id": session_id}
        if session_started_at:
            self._session["started_at"] = session_started_at
        self._seq = 0
        self._l2: dict[str, Any] = {}

    def with_l2(self, **fields: Any) -> HookContextBuilder:
        """Attach L2 fields (``trace``, ``tenant``, ``budgets``, ``actor``…)
        to every subsequent context."""
        self._l2.update({k: v for k, v in fields.items() if v is not None})
        return self

    def _envelope(self, hp: HookPoint, target: Any) -> HookContext:
        ctx: HookContext = {
            "spec": SPEC_VERSION,
            "hook_point": hp.value,
            "timestamp": _now(),
            "sequence": self._seq,
            "agent": dict(self._agent),
            "session": dict(self._session),
            "target": target,
        }
        ctx.update(self._l2)
        self._seq += 1
        return ctx

    # ---- per-hook L1 builders ------------------------------------------------

    def agent_startup(self, *, tools_registered: list[str], **extra: Any) -> HookContext:
        agent_init = {"tools_registered": list(tools_registered), **extra}
        ctx = self._envelope(HookPoint.AGENT_STARTUP, agent_init)
        ctx["agent_init"] = agent_init
        return ctx

    def input(self, *, content: Any, role: str = "user", **extra: Any) -> HookContext:
        inp = {"content": content, "role": role, **extra}
        ctx = self._envelope(HookPoint.INPUT, inp)
        ctx["input"] = inp
        return ctx

    def pre_model_call(
        self,
        *,
        model_id: str,
        messages: list[dict[str, Any]],
        tools: list[dict[str, Any]] | None = None,
        request_id: str | None = None,
        **model_extra: Any,
    ) -> HookContext:
        ctx = self._envelope(HookPoint.PRE_MODEL_CALL, messages)
        ctx["model"] = {"id": model_id, **model_extra}
        ctx["messages"] = messages
        if tools is not None:
            ctx["tools"] = tools
        if request_id is not None:
            ctx["request_id"] = request_id
        return ctx

    def post_model_call(
        self,
        *,
        model_id: str,
        content: Any,
        tool_calls: list[dict[str, Any]],
        finish_reason: str,
        usage: dict[str, int] | None = None,
        request_id: str | None = None,
    ) -> HookContext:
        response = {
            "content": content,
            "tool_calls": list(tool_calls),
            "finish_reason": finish_reason,
        }
        ctx = self._envelope(HookPoint.POST_MODEL_CALL, response)
        ctx["model"] = {"id": model_id}
        ctx["response"] = response
        if usage is not None:
            ctx["usage"] = usage
        if request_id is not None:
            ctx["request_id"] = request_id
        return ctx

    def pre_tool_call(
        self, *, call_id: str, name: str, args: dict[str, Any], **extra: Any
    ) -> HookContext:
        tc = {"id": call_id, "name": name, "args": args, **extra}
        ctx = self._envelope(HookPoint.PRE_TOOL_CALL, args)
        ctx["tool_call"] = tc
        return ctx

    def post_tool_call(
        self,
        *,
        call_id: str,
        name: str,
        args: dict[str, Any],
        value: Any,
        is_error: bool = False,
        duration_ms: float | None = None,
    ) -> HookContext:
        tr: dict[str, Any] = {"value": value, "is_error": is_error}
        if duration_ms is not None:
            tr["duration_ms"] = duration_ms
        ctx = self._envelope(HookPoint.POST_TOOL_CALL, value)
        ctx["tool_call"] = {"id": call_id, "name": name, "args": args}
        ctx["tool_result"] = tr
        return ctx

    def output(self, *, content: Any, **extra: Any) -> HookContext:
        out = {"content": content, **extra}
        ctx = self._envelope(HookPoint.OUTPUT, out)
        ctx["output"] = out
        return ctx

    def agent_shutdown(self, *, reason: str, **summary_extra: Any) -> HookContext:
        summary = {"reason": reason, **summary_extra}
        ctx = self._envelope(HookPoint.AGENT_SHUTDOWN, summary)
        ctx["summary"] = summary
        return ctx
