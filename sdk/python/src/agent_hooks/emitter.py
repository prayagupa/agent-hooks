# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
"""Host-side emitter: dispatch context → interceptors → verdict → enforce (§6, §7, §8, §9)."""
from __future__ import annotations

import copy
import inspect
from typing import Any

from agent_hooks._types import (
    ALLOW,
    Decision,
    EnforcementMode,
    HostError,
    InterceptionPoint,
    InterceptionRecord,
    Verdict,
)
from agent_hooks.approval import (
    ApprovalOutcome,
    ApprovalRequest,
    ApprovalResolver,
)
from agent_hooks.canonical import context_identity
from agent_hooks.context import AgentContext
from agent_hooks.exceptions import InterceptionBlocked
from agent_hooks.interceptor import Interceptor
from agent_hooks.path import PathError
from agent_hooks.path import apply as apply_path


class InterceptionEmitter:
    """Host-side helper that implements §6–§9 once so adapters don't have to.

    An adapter constructs one :class:`InterceptionEmitter` per session, registers
    interceptors and an optional approval resolver, then calls :meth:`emit` at
    each interception point with a built :class:`AgentContext`. The emitter:

    1. Computes ``input_identity`` (§10.2).
    2. Dispatches to interceptors in registration order, combining per §7.1.
    3. On ``escalate``, consults the resolver (§9).
    4. On ``transform`` in ``enforce`` mode, applies the transform to
       ``ctx["target"]`` (§5.2, §6).
    5. Computes ``enforced_identity`` and returns a :class:`InterceptionRecord`.

    The emitter does NOT raise on block verdicts; the adapter inspects
    :attr:`InterceptionRecord.proceeds` (or calls :meth:`emit_or_raise`).
    """

    __slots__ = ("_interceptors", "_mode", "_resolver", "_results")

    def __init__(
        self,
        *,
        mode: EnforcementMode = EnforcementMode.ENFORCE,
        resolver: ApprovalResolver | None = None,
    ) -> None:
        self._interceptors: list[Interceptor] = []
        self._resolver = resolver
        self._mode = mode
        self._results: list[InterceptionRecord] = []

    @property
    def mode(self) -> EnforcementMode:
        return self._mode

    @property
    def results(self) -> list[InterceptionRecord]:
        """All interception records emitted so far in this session, in order."""
        return list(self._results)

    def register(self, interceptor: Interceptor) -> InterceptionEmitter:
        self._interceptors.append(interceptor)
        return self

    # -------------------------------------------------------------------------

    async def emit(self, ctx: AgentContext) -> InterceptionRecord:
        hp = InterceptionPoint(ctx["interception_point"])
        input_id = context_identity(ctx)
        verdict = await self._dispatch(ctx)

        if verdict.decision is Decision.ESCALATE and self._mode is EnforcementMode.ENFORCE:
            verdict = self._resolve_escalate(hp, ctx, verdict, input_id)

        transformed: Any | None = None
        enforced_id = input_id
        if verdict.decision is Decision.TRANSFORM:
            if not hp.transform_permitted:
                verdict = Verdict.host_error(HostError.TRANSFORM_TARGET_FORBIDDEN)
            elif self._mode is EnforcementMode.ENFORCE:
                try:
                    transformed = apply_path(
                        copy.deepcopy(ctx["target"]),
                        verdict.transform.path,  # type: ignore[union-attr]
                        verdict.transform.value,  # type: ignore[union-attr]
                    )
                except PathError as e:
                    verdict = Verdict.host_error(e.host_error, str(e))
                else:
                    ctx["target"] = transformed
                    self._write_back_target(hp, ctx, transformed)
                    enforced_id = context_identity(ctx)
            else:  # evaluate_only — validate only (§8)
                try:
                    apply_path(
                        copy.deepcopy(ctx["target"]),
                        verdict.transform.path,  # type: ignore[union-attr]
                        verdict.transform.value,  # type: ignore[union-attr]
                    )
                except PathError as e:
                    verdict = Verdict.host_error(e.host_error, str(e))

        result = InterceptionRecord(
            interception_point=hp,
            mode=self._mode,
            verdict=verdict,
            input_identity=input_id,
            enforced_identity=enforced_id,
            transformed_target=transformed,
        )
        self._results.append(result)
        return result

    async def emit_or_raise(self, ctx: AgentContext) -> InterceptionRecord:
        """:meth:`emit`, then raise :class:`InterceptionBlocked` if the action must not proceed."""
        result = await self.emit(ctx)
        if not result.proceeds:
            raise InterceptionBlocked(result)
        return result

    # -------------------------------------------------------------------------

    async def _dispatch(self, ctx: AgentContext) -> Verdict:
        """Combine interceptor verdicts per §7.1."""
        combined = ALLOW
        for c in self._interceptors:
            try:
                raw = c.intercept(ctx)
                if inspect.isawaitable(raw):
                    raw = await raw
                v = raw if isinstance(raw, Verdict) else Verdict.from_wire(raw)
            except ValueError as e:
                return Verdict.host_error(HostError.VERDICT_INVALID, str(e))
            except Exception as e:  # noqa: BLE001 — fail closed per §6.3
                return Verdict.host_error(HostError.INTERCEPTOR_FAILED, repr(e))
            if v.decision.blocks:
                return v  # first block short-circuits (§7.1.2)
            if v.decision is Decision.TRANSFORM:
                combined = v  # last transform wins (§7.1.3)
            elif v.decision is Decision.WARN and combined.decision is Decision.ALLOW:
                combined = v
        return combined

    def _resolve_escalate(
        self, hp: InterceptionPoint, ctx: AgentContext, verdict: Verdict, identity: str
    ) -> Verdict:
        if self._resolver is None:
            return Verdict.host_error(HostError.APPROVAL_RESOLVER_MISSING)
        try:
            res = self._resolver.resolve(
                ApprovalRequest(
                    context_identity=identity, interception_point=hp, verdict=verdict, context=ctx
                )
            )
        except Exception as e:  # noqa: BLE001
            return Verdict.host_error(HostError.APPROVAL_RESOLVER_FAILED, repr(e))
        if res.context_identity != identity:
            return Verdict.host_error(HostError.APPROVAL_ACTION_MISMATCH)
        if res.outcome is ApprovalOutcome.UNRESOLVED or res.verdict is None:
            return Verdict.host_error(HostError.APPROVAL_UNRESOLVED)
        return res.verdict

    @staticmethod
    def _write_back_target(hp: InterceptionPoint, ctx: AgentContext, transformed: Any) -> None:
        """Mirror the transformed target back into the L1 field it aliases (§4.3).

        ``target`` is defined as a *reference* to the L1 payload, but Python
        adapters typically build them as separate objects; this keeps both in
        sync so the action consumes the transformed value.
        """
        match hp:
            case InterceptionPoint.INPUT:
                ctx["input"] = transformed
            case InterceptionPoint.PRE_MODEL_CALL:
                ctx["messages"] = transformed
            case InterceptionPoint.POST_MODEL_CALL:
                ctx["response"] = transformed
            case InterceptionPoint.PRE_TOOL_CALL:
                ctx["tool_call"]["args"] = transformed
            case InterceptionPoint.POST_TOOL_CALL:
                ctx["tool_result"]["value"] = transformed
            case InterceptionPoint.OUTPUT:
                ctx["output"] = transformed
            case _:
                pass  # transform_permitted=False already handled
