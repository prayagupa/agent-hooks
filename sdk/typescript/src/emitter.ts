// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
/**
 * Host-side emitter: dispatch context → interceptors → verdict → enforce (§6–§9).
 *
 * Interceptor dispatch (§7) and approval-seam resolution (§9) stay here
 * because they call back into user JS code. Verdict validation (§5),
 * combination (§7.1), transform application (§5.2), identity computation
 * (§10), and target write-back (§4.3) delegate to the Rust core so
 * behaviour is byte-identical across SDKs.
 */

import {
  AgentContext,
  ApprovalOutcome,
  ApprovalResolver,
  Decision,
  EnforcementMode,
  HostError,
  Interceptor,
  InterceptionBlocked,
  InterceptionRecord,
  Verdict,
  hostErrorVerdict,
} from "./index";
import { AgentHooksCoreError, native } from "./native";

export class InterceptionEmitter {
  private readonly interceptors: Interceptor[] = [];
  private readonly _records: InterceptionRecord[] = [];

  constructor(
    private readonly mode: EnforcementMode = EnforcementMode.Enforce,
    private readonly resolver: ApprovalResolver | null = null,
  ) {}

  get records(): readonly InterceptionRecord[] {
    return this._records;
  }

  register(interceptor: Interceptor): this {
    this.interceptors.push(interceptor);
    return this;
  }

  async emit(ctx: AgentContext): Promise<InterceptionRecord> {
    // §7 dispatch (native callbacks) + §5/§7.1 (core).
    let verdict = await this.dispatch(ctx);

    // §9 approval seam (native callback).
    if (verdict.decision === Decision.Escalate && this.mode === EnforcementMode.Enforce) {
      const inputId = native.contextIdentity(JSON.stringify(ctx));
      verdict = await this.resolveEscalate(ctx, verdict, inputId);
    }

    // §6/§8/§10 enforcement (core). Returns {record, ctx}; ctx may have
    // target + L1 field rewritten on transform.
    const out = JSON.parse(
      native.enforce(JSON.stringify(ctx), JSON.stringify(verdict), this.mode),
    ) as { record: InterceptionRecord; ctx: AgentContext };
    for (const k of Object.keys(ctx)) delete (ctx as Record<string, unknown>)[k];
    Object.assign(ctx, out.ctx);

    this._records.push(out.record);
    return out.record;
  }

  async emitOrThrow(ctx: AgentContext): Promise<InterceptionRecord> {
    const r = await this.emit(ctx);
    const proceed = r.mode === EnforcementMode.EvaluateOnly ||
      r.verdict.decision === Decision.Allow ||
      r.verdict.decision === Decision.Warn ||
      r.verdict.decision === Decision.Transform;
    if (!proceed) throw new InterceptionBlocked(r);
    return r;
  }

  private async dispatch(ctx: AgentContext): Promise<Verdict> {
    const wire: Verdict[] = [];
    for (const c of this.interceptors) {
      let v: Verdict;
      try {
        v = await c.intercept(ctx);
        native.validateVerdict(JSON.stringify(v));
      } catch (e) {
        if (e instanceof AgentHooksCoreError) {
          return hostErrorVerdict(e.code as HostError, e.message);
        }
        return hostErrorVerdict(HostError.InterceptorFailed, String(e));
      }
      wire.push(v);
      if (v.decision === Decision.Deny || v.decision === Decision.Escalate) break;
    }
    return JSON.parse(native.combineVerdicts(JSON.stringify(wire))) as Verdict;
  }

  private async resolveEscalate(
    ctx: AgentContext,
    verdict: Verdict,
    identity: string,
  ): Promise<Verdict> {
    if (!this.resolver) {
      return hostErrorVerdict(HostError.ApprovalResolverMissing);
    }
    let res;
    try {
      res = await this.resolver.resolve({
        context_identity: identity,
        interception_point: ctx.interception_point,
        verdict,
        context: ctx,
      });
    } catch (e) {
      return hostErrorVerdict(HostError.ApprovalResolverFailed, String(e));
    }
    if (res.context_identity !== identity) {
      return hostErrorVerdict(HostError.ApprovalActionMismatch);
    }
    if (res.outcome === ApprovalOutcome.Unresolved || !res.verdict) {
      return hostErrorVerdict(HostError.ApprovalUnresolved);
    }
    return res.verdict;
  }
}
