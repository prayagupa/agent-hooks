# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
"""Host-side emitter: dispatch context → interceptors → verdict → enforce (§6–§9).

The emitter is the per-language orchestrator over the Rust core:

- Interceptor dispatch (§7) and approval-seam resolution (§9) stay here
  because they call back into user Python code.
- Verdict validation (§5), verdict combination (§7.1), transform
  application (§5.2), identity computation (§10), and target write-back
  (§4.3) delegate to ``agent_hooks._core.enforce`` so behaviour is
  byte-identical across SDKs.
"""
from __future__ import annotations

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
    """Host-side helper that implements §6–§9 once so adapters don't have to.

    An adapter constructs one :class:`InterceptionEmitter` per session,
    registers interceptors and an optional approval resolver, then calls
    :meth:`emit` at each interception point with a built
    :class:`AgentContext`. The emitter:

    1. Dispatches to interceptors (native), validating each return via the
       Rust core (§5) and combining per §7.1.
    2. On ``escalate`` in ``enforce`` mode, consults the resolver (§9).
    3. Hands ``(ctx, verdict, mode)`` to ``_core.enforce`` which computes
       both identities, applies the transform, writes the target back into
       the L1 field, and returns an :class:`InterceptionRecord`.
    """

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
        ip = InterceptionPoint(ctx["interception_point"])

        # §7 dispatch (native — calls user code) + §5/§7.1 (core).
        verdict = await self._dispatch(ctx)

        # §9 approval seam (native — calls user code). Needs input_identity,
        # so compute it once via core; enforce() will recompute deterministically.
        if verdict.decision is Decision.ESCALATE and self._mode is EnforcementMode.ENFORCE:
            input_id = _core.context_identity(json.dumps(ctx))
            verdict = self._resolve_escalate(ip, ctx, verdict, input_id)

        # §6/§8/§10 enforcement (core). Returns {record, ctx}; ctx may have
        # target + L1 field rewritten on transform.
        out = json.loads(
            _core.enforce(json.dumps(ctx), json.dumps(verdict.to_wire()), self._mode.value)
        )
        ctx.clear()
        ctx.update(out["ctx"])
        record = InterceptionRecord.from_core(out["record"])

        self._records.append(record)
        return record

    async def emit_or_raise(self, ctx: AgentContext) -> InterceptionRecord:
        """:meth:`emit`, then raise :class:`InterceptionBlocked` if the action must halt."""
        record = await self.emit(ctx)
        if not record.proceeds:
            raise InterceptionBlocked(record)
        return record

    # -------------------------------------------------------------------------

    async def _dispatch(self, ctx: AgentContext) -> Verdict:
        """Invoke interceptors in order; validate + combine via core (§5, §7.1)."""
        wire_verdicts: list[dict] = []
        for c in self._interceptors:
            try:
                raw = c.intercept(ctx)
                if inspect.isawaitable(raw):
                    raw = await raw
                w = raw.to_wire() if isinstance(raw, Verdict) else raw
                # §5 validation via core; raises on violation.
                _core.validate_verdict(json.dumps(w))
            except _core.AgentHooksCoreError as e:
                return Verdict.host_error(
                    HostError(getattr(e, "code", HostError.VERDICT_INVALID.value)), str(e)
                )
            except Exception as e:  # noqa: BLE001 — fail closed per §6.3
                return Verdict.host_error(HostError.INTERCEPTOR_FAILED, repr(e))
            wire_verdicts.append(w)
            # §7.1.2 short-circuit: stop calling further interceptors on block.
            if w.get("decision") in ("deny", "escalate"):
                break
        combined = json.loads(_core.combine_verdicts(json.dumps(wire_verdicts)))
        return Verdict.from_wire(combined)

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
            return Verdict.host_error(HostError.APPROVAL_RESOLVER_FAILED, repr(e))
        if res.context_identity != identity:
            return Verdict.host_error(HostError.APPROVAL_ACTION_MISMATCH)
        if res.outcome is ApprovalOutcome.UNRESOLVED or res.verdict is None:
            return Verdict.host_error(HostError.APPROVAL_UNRESOLVED)
        return res.verdict
