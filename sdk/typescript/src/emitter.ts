// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
/**
 * Host-side emitter: dispatch context → interceptors → verdict → record (§6–§9).
 *
 * Per-language orchestrator over the Rust core:
 *
 * - Interceptor dispatch (§7) and approval-seam resolution (§9) stay here
 *   because they call back into user JS code.
 * - Verdict validation (§5), transform fold-through (§7.1), identity
 *   computation (§10), and target write-back (§4.3) delegate to the Rust
 *   core so behaviour is byte-identical across SDKs.
 *
 * §7.1 sequential fold-through: interceptors run in registration order;
 * each receives a deep copy of the context as it stands *after* prior
 * transforms were applied, so an earlier interceptor's redaction is
 * visible to later ones. The first block verdict short-circuits.
 *
 * Fail-closed defaults: an `enforce`-mode emission with zero registered
 * interceptors yields `deny host_error:no_interceptor` (§7), and
 * {@link InterceptionEmitter.emit} **throws** {@link InterceptionBlocked}
 * on any block — the ignorable-result variant is the explicitly named
 * {@link InterceptionEmitter.emitUnchecked}.
 *
 * Concurrency (§12.2): emissions for different tool calls may interleave
 * on the event loop; sequence assignment and record append are atomic on
 * a single JS thread. Sharing one emitter across workers is unsupported.
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
  permits,
} from "./index";
import { AgentHooksCoreError, native } from "./native";

function proceeds(r: InterceptionRecord): boolean {
  return r.mode === EnforcementMode.EvaluateOnly || permits(r.verdict.decision);
}

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

  /** Run the interception and **throw** {@link InterceptionBlocked} if the
   * guarded action must not proceed (§6). Primary entry point. */
  async emit(ctx: AgentContext): Promise<InterceptionRecord> {
    const record = await this.emitUnchecked(ctx);
    if (!proceeds(record)) throw new InterceptionBlocked(record);
    return record;
  }

  /** Run the interception and return the record without throwing. The
   * caller MUST inspect `proceeds` and halt the guarded action itself;
   * prefer {@link emit}. */
  async emitUnchecked(ctx: AgentContext): Promise<InterceptionRecord> {
    // §10.2: input identity binds to the context BEFORE dispatch, so
    // neither interceptor mutation nor fold-through can retroactively
    // alter what the record claims was evaluated.
    const inputId = native.contextIdentity(JSON.stringify(ctx));

    let [verdict, decidedBy] = await this.dispatch(ctx);

    if (verdict.decision === Decision.Escalate && this.mode === EnforcementMode.Enforce) {
      verdict = await this.resolveEscalate(ctx, verdict, inputId);
      // An approve MAY carry a transform (§9); it is subject to the
      // same fold rules as an interceptor transform.
      if (verdict.decision === Decision.Transform) {
        verdict = this.foldTransform(ctx, verdict);
      }
      // A resolver-substituted verdict keeps the escalating
      // interceptor's index; host-synthesized failures do not.
      if (verdict.reason?.startsWith("host_error:")) decidedBy = null;
    }

    const record: InterceptionRecord = JSON.parse(
      native.finalize(
        JSON.stringify(ctx),
        JSON.stringify(verdict),
        this.mode,
        inputId,
        decidedBy ?? -1,
      ),
    );
    this._records.push(record);
    return record;
  }

  // ---------------------------------------------------------------------------

  /** §7 dispatch with §7.1 sequential fold-through. Returns the combined
   * verdict and the deciding interceptor's registration index (`null`
   * for pure allow or host-synthesized). */
  private async dispatch(ctx: AgentContext): Promise<[Verdict, number | null]> {
    if (this.interceptors.length === 0) {
      // §7: zero interceptors fails closed. Register an explicit
      // allow-all interceptor for a deliberate passthrough.
      return [hostErrorVerdict(HostError.NoInterceptor), null];
    }

    let combined: Verdict = { decision: Decision.Allow };
    let decidedBy: number | null = null;
    for (let i = 0; i < this.interceptors.length; i++) {
      const c = this.interceptors[i];
      let v: Verdict;
      try {
        // §7.1/N05: each interceptor gets its own deep copy — an
        // in-place mutation of the copy cannot alter enforcement.
        v = await c.intercept(JSON.parse(JSON.stringify(ctx)));
        native.validateVerdict(JSON.stringify(v)); // §5
      } catch (e) {
        if (e instanceof AgentHooksCoreError) {
          return [hostErrorVerdict(e.code as HostError, e.message), null];
        }
        return [
          hostErrorVerdict(HostError.InterceptorFailed, (e as Error)?.constructor?.name ?? "Error"),
          null,
        ];
      }

      if (v.decision === Decision.Deny || v.decision === Decision.Escalate) {
        return [v, i]; // first block short-circuits (§7.1)
      }
      if (v.decision === Decision.Transform) {
        v = this.foldTransform(ctx, v);
        if (!permits(v.decision)) return [v, null]; // transform failed closed
        combined = v;
        decidedBy = i;
      } else if (v.decision === Decision.Warn && combined.decision === Decision.Allow) {
        combined = v;
        decidedBy = i;
      }
    }
    return [combined, decidedBy];
  }

  /** Apply (enforce) or validate (evaluate_only) one transform (§7.1, §8). */
  private foldTransform(ctx: AgentContext, v: Verdict): Verdict {
    const t = v.transform!;
    try {
      if (this.mode === EnforcementMode.Enforce) {
        const newCtx: AgentContext = JSON.parse(
          native.applyTransformCtx(JSON.stringify(ctx), t.path, JSON.stringify(t.value)),
        );
        for (const k of Object.keys(ctx)) delete (ctx as Record<string, unknown>)[k];
        Object.assign(ctx, newCtx);
      } else {
        native.validateTransformCtx(JSON.stringify(ctx), t.path, JSON.stringify(t.value));
      }
    } catch (e) {
      if (e instanceof AgentHooksCoreError) {
        return hostErrorVerdict(e.code as HostError, e.message);
      }
      return hostErrorVerdict(HostError.TransformInvalid, String(e));
    }
    return v;
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
      return hostErrorVerdict(HostError.ApprovalResolverFailed, (e as Error)?.constructor?.name ?? "Error");
    }
    if (res.context_identity !== identity) {
      return hostErrorVerdict(HostError.ApprovalActionMismatch);
    }
    if (res.outcome === ApprovalOutcome.Unresolved || !res.verdict) {
      return hostErrorVerdict(HostError.ApprovalUnresolved);
    }
    try {
      // §9/N04: the resolver's verdict crosses the same §5 gate as an
      // interceptor's.
      native.validateVerdict(JSON.stringify(res.verdict));
    } catch (e) {
      if (e instanceof AgentHooksCoreError) {
        return hostErrorVerdict(HostError.VerdictInvalid, e.message);
      }
      return hostErrorVerdict(HostError.VerdictInvalid, String(e));
    }
    return res.verdict;
  }
}
