# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
"""Host-side emitter: dispatch context → interceptors → verdict → record (§6–§9).

Per-language orchestrator over the Rust core:

- Interceptor dispatch (§7) and approval-seam resolution (§9) stay here
  because they call back into user Python code.
- Verdict validation (§5), transform fold-through (§7.1), identity
  computation (§10), and target write-back (§4.3) delegate to
  ``agent_hooks._core`` so behaviour is byte-identical across SDKs.

§7.1 sequential fold-through: interceptors run in registration order;
each receives a deep copy of the context as it stands *after* prior
transforms were applied, so an earlier interceptor's redaction is
visible to later ones. The first block verdict short-circuits.

Fail-closed defaults: an ``enforce``-mode emission with zero registered
interceptors yields ``deny host_error:no_interceptor`` (§7), and
:meth:`InterceptionEmitter.emit` **raises** :class:`InterceptionBlocked`
on any block — the ignorable-result variant is the explicitly named
:meth:`emit_unchecked`.
"""
from __future__ import annotations

import copy
import inspect
import json

from agent_hooks import _core
from agent_hooks._types import (
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
from agent_hooks.context import AgentContext
from agent_hooks.exceptions import InterceptionBlocked
from agent_hooks.interceptor import Interceptor


class InterceptionEmitter:
    """Host-side helper that implements §6–§9 once so adapters don't have to."""

    __slots__ = ("_interceptors", "_mode", "_records", "_resolver")

    def __init__(
        self,
        *,
        mode: EnforcementMode = EnforcementMode.ENFORCE,
        resolver: ApprovalResolver | None = None,
    ) -> None:
        self._interceptors: list[Interceptor] = []
        self._resolver = resolver
        self._mode = mode
        self._records: list[InterceptionRecord] = []

    @property
    def mode(self) -> EnforcementMode:
        return self._mode

    @property
    def results(self) -> list[InterceptionRecord]:
        """All interception records emitted so far in this session, in order."""
        return list(self._records)

    def register(self, interceptor: Interceptor) -> InterceptionEmitter:
        self._interceptors.append(interceptor)
        return self

    # -------------------------------------------------------------------------

    async def emit(self, ctx: AgentContext) -> InterceptionRecord:
        """Run the interception and **raise** :class:`InterceptionBlocked`
        if the guarded action must not proceed (§6). This is the primary
        entry point; the safe path is the default."""
        record = await self.emit_unchecked(ctx)
        if not record.proceeds:
            raise InterceptionBlocked(record)
        return record

    async def emit_unchecked(self, ctx: AgentContext) -> InterceptionRecord:
        """Run the interception and return the record without raising.

        The caller MUST inspect :attr:`InterceptionRecord.proceeds` and
        halt the guarded action itself; prefer :meth:`emit`.
        """
        # §10.2: input identity binds to the context BEFORE dispatch, so
        # neither interceptor mutation nor fold-through can retroactively
        # alter what the record claims was evaluated.
        input_id = _core.context_identity(json.dumps(ctx))

        verdict = await self._dispatch(ctx)

        if verdict.decision is Decision.ESCALATE and self._mode is EnforcementMode.ENFORCE:
            ip = InterceptionPoint(ctx["interception_point"])
            verdict = self._resolve_escalate(ip, ctx, verdict, input_id)
            # An approve MAY carry a transform (§9); it is subject to the
            # same fold rules as an interceptor transform.
            if verdict.decision is Decision.TRANSFORM:
                verdict = self._fold_transform(ctx, verdict)

        record_json = _core.finalize(
            json.dumps(ctx), json.dumps(verdict.to_wire()), self._mode.value, input_id
        )
        record = InterceptionRecord.from_core(json.loads(record_json))
        self._records.append(record)
        return record

    # -------------------------------------------------------------------------

    async def _dispatch(self, ctx: AgentContext) -> Verdict:
        """§7 dispatch with §7.1 sequential fold-through."""
        if not self._interceptors:
            # §7: zero interceptors fails closed. Register an explicit
            # allow-all interceptor for a deliberate passthrough.
            return Verdict.host_error(HostError.NO_INTERCEPTOR)

        combined = Verdict(decision=Decision.ALLOW)
        for c in self._interceptors:
            try:
                # §7.1/N05: each interceptor gets its own deep copy — an
                # in-place mutation of the copy cannot alter enforcement.
                raw = c.intercept(copy.deepcopy(ctx))
                if inspect.isawaitable(raw):
                    raw = await raw
                w = raw.to_wire() if isinstance(raw, Verdict) else raw
                _core.validate_verdict(json.dumps(w))  # §5
                v = Verdict.from_wire(w)
            except _core.AgentHooksCoreError as e:
                return Verdict.host_error(
                    HostError(getattr(e, "code", HostError.VERDICT_INVALID.value)), str(e)
                )
            except Exception as e:  # noqa: BLE001 — fail closed per §6.3
                return Verdict.host_error(HostError.INTERCEPTOR_FAILED, type(e).__name__)

            if v.decision.blocks:
                return v  # first block short-circuits (§7.1)
            if v.decision is Decision.TRANSFORM:
                v = self._fold_transform(ctx, v)
                if v.decision.blocks:  # transform failed closed
                    return v
                combined = v
            elif v.decision is Decision.WARN and combined.decision is Decision.ALLOW:
                combined = v
        return combined

    def _fold_transform(self, ctx: AgentContext, v: Verdict) -> Verdict:
        """Apply (enforce) or validate (evaluate_only) one transform (§7.1, §8)."""
        assert v.transform is not None
        try:
            if self._mode is EnforcementMode.ENFORCE:
                new_ctx = json.loads(
                    _core.apply_transform_ctx(
                        json.dumps(ctx), v.transform.path, json.dumps(v.transform.value)
                    )
                )
                ctx.clear()
                ctx.update(new_ctx)
            else:
                _core.validate_transform_ctx(
                    json.dumps(ctx), v.transform.path, json.dumps(v.transform.value)
                )
        except _core.AgentHooksCoreError as e:
            return Verdict.host_error(
                HostError(getattr(e, "code", HostError.TRANSFORM_INVALID.value)), str(e)
            )
        return v

    def _resolve_escalate(
        self, ip: InterceptionPoint, ctx: AgentContext, verdict: Verdict, identity: str
    ) -> Verdict:
        if self._resolver is None:
            return Verdict.host_error(HostError.APPROVAL_RESOLVER_MISSING)
        try:
            res = self._resolver.resolve(
                ApprovalRequest(
                    context_identity=identity,
                    interception_point=ip,
                    verdict=verdict,
                    context=ctx,
                )
            )
        except Exception as e:  # noqa: BLE001
            return Verdict.host_error(HostError.APPROVAL_RESOLVER_FAILED, type(e).__name__)
        if res.context_identity != identity:
            return Verdict.host_error(HostError.APPROVAL_ACTION_MISMATCH)
        if res.outcome is ApprovalOutcome.UNRESOLVED or res.verdict is None:
            return Verdict.host_error(HostError.APPROVAL_UNRESOLVED)
        try:
            # §9/N04: the resolver's verdict crosses the same §5 gate as
            # an interceptor's.
            _core.validate_verdict(json.dumps(res.verdict.to_wire()))
        except _core.AgentHooksCoreError as e:
            return Verdict.host_error(HostError.VERDICT_INVALID, str(e))
        return res.verdict
