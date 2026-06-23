# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
"""Host-side emitter: dispatch context → consumers → verdict → enforce (§6, §7, §8, §9)."""
from __future__ import annotations

import copy
import inspect
from typing import Any

from agent_hooks._types import (
    ALLOW,
    Decision,
    EnforcementMode,
    HookError,
    HookPoint,
    HookResult,
    Verdict,
)
from agent_hooks.approval import (
    ApprovalOutcome,
    ApprovalRequest,
    ApprovalResolver,
)
from agent_hooks.canonical import context_identity
from agent_hooks.consumer import HookConsumer
from agent_hooks.context import HookContext
from agent_hooks.exceptions import HookBlocked
from agent_hooks.path import PathError
from agent_hooks.path import apply as apply_path


class HookEmitter:
    """Host-side helper that implements §6–§9 once so adapters don't have to.

    An adapter constructs one :class:`HookEmitter` per session, registers
    consumers and an optional approval resolver, then calls :meth:`emit` at
    each hook point with a built :class:`HookContext`. The emitter:

    1. Computes ``input_identity`` (§10.2).
    2. Dispatches to consumers in registration order, combining per §7.1.
    3. On ``escalate``, consults the resolver (§9).
    4. On ``transform`` in ``enforce`` mode, applies the transform to
       ``ctx["target"]`` (§5.2, §6).
    5. Computes ``enforced_identity`` and returns a :class:`HookResult`.

    The emitter does NOT raise on block verdicts; the adapter inspects
    :attr:`HookResult.proceeds` (or calls :meth:`emit_or_raise`).
    """

    __slots__ = ("_consumers", "_mode", "_resolver", "_results")

    def __init__(
        self,
        *,
        mode: EnforcementMode = EnforcementMode.ENFORCE,
        resolver: ApprovalResolver | None = None,
    ) -> None:
        self._consumers: list[HookConsumer] = []
        self._resolver = resolver
        self._mode = mode
        self._results: list[HookResult] = []

    @property
    def mode(self) -> EnforcementMode:
        return self._mode

    @property
    def results(self) -> list[HookResult]:
        """All hook results emitted so far in this session, in order."""
        return list(self._results)

    def register(self, consumer: HookConsumer) -> HookEmitter:
        self._consumers.append(consumer)
        return self

    # -------------------------------------------------------------------------

    async def emit(self, ctx: HookContext) -> HookResult:
        hp = HookPoint(ctx["hook_point"])
        input_id = context_identity(ctx)
        verdict = await self._dispatch(ctx)

        if verdict.decision is Decision.ESCALATE and self._mode is EnforcementMode.ENFORCE:
            verdict = self._resolve_escalate(hp, ctx, verdict, input_id)

        transformed: Any | None = None
        enforced_id = input_id
        if verdict.decision is Decision.TRANSFORM:
            if not hp.transform_permitted:
                verdict = Verdict.hook_error(HookError.TRANSFORM_TARGET_FORBIDDEN)
            elif self._mode is EnforcementMode.ENFORCE:
                try:
                    transformed = apply_path(
                        copy.deepcopy(ctx["target"]),
                        verdict.transform.path,  # type: ignore[union-attr]
                        verdict.transform.value,  # type: ignore[union-attr]
                    )
                except PathError as e:
                    verdict = Verdict.hook_error(e.hook_error, str(e))
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
                    verdict = Verdict.hook_error(e.hook_error, str(e))

        result = HookResult(
            hook_point=hp,
            mode=self._mode,
            verdict=verdict,
            input_identity=input_id,
            enforced_identity=enforced_id,
            transformed_target=transformed,
        )
        self._results.append(result)
        return result

    async def emit_or_raise(self, ctx: HookContext) -> HookResult:
        """:meth:`emit`, then raise :class:`HookBlocked` if the action must not proceed."""
        result = await self.emit(ctx)
        if not result.proceeds:
            raise HookBlocked(result)
        return result

    # -------------------------------------------------------------------------

    async def _dispatch(self, ctx: HookContext) -> Verdict:
        """Combine consumer verdicts per §7.1."""
        combined = ALLOW
        for c in self._consumers:
            try:
                raw = c.on_hook(ctx)
                if inspect.isawaitable(raw):
                    raw = await raw
                v = raw if isinstance(raw, Verdict) else Verdict.from_wire(raw)
            except ValueError as e:
                return Verdict.hook_error(HookError.VERDICT_INVALID, str(e))
            except Exception as e:  # noqa: BLE001 — fail closed per §6.3
                return Verdict.hook_error(HookError.CONSUMER_FAILED, repr(e))
            if v.decision.blocks:
                return v  # first block short-circuits (§7.1.2)
            if v.decision is Decision.TRANSFORM:
                combined = v  # last transform wins (§7.1.3)
            elif v.decision is Decision.WARN and combined.decision is Decision.ALLOW:
                combined = v
        return combined

    def _resolve_escalate(
        self, hp: HookPoint, ctx: HookContext, verdict: Verdict, identity: str
    ) -> Verdict:
        if self._resolver is None:
            return Verdict.hook_error(HookError.APPROVAL_RESOLVER_MISSING)
        try:
            res = self._resolver.resolve(
                ApprovalRequest(
                    context_identity=identity, hook_point=hp, verdict=verdict, context=ctx
                )
            )
        except Exception as e:  # noqa: BLE001
            return Verdict.hook_error(HookError.APPROVAL_RESOLVER_FAILED, repr(e))
        if res.context_identity != identity:
            return Verdict.hook_error(HookError.APPROVAL_ACTION_MISMATCH)
        if res.outcome is ApprovalOutcome.UNRESOLVED or res.verdict is None:
            return Verdict.hook_error(HookError.APPROVAL_UNRESOLVED)
        return res.verdict

    @staticmethod
    def _write_back_target(hp: HookPoint, ctx: HookContext, transformed: Any) -> None:
        """Mirror the transformed target back into the L1 field it aliases (§4.3).

        ``target`` is defined as a *reference* to the L1 payload, but Python
        adapters typically build them as separate objects; this keeps both in
        sync so the action consumes the transformed value.
        """
        match hp:
            case HookPoint.INPUT:
                ctx["input"] = transformed
            case HookPoint.PRE_MODEL_CALL:
                ctx["messages"] = transformed
            case HookPoint.POST_MODEL_CALL:
                ctx["response"] = transformed
            case HookPoint.PRE_TOOL_CALL:
                ctx["tool_call"]["args"] = transformed
            case HookPoint.POST_TOOL_CALL:
                ctx["tool_result"]["value"] = transformed
            case HookPoint.OUTPUT:
                ctx["output"] = transformed
            case _:
                pass  # transform_permitted=False already handled
