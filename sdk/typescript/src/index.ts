// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
/**
 * agent-hooks: framework-neutral agent lifecycle hook contract.
 * Implements AGENT-HOOKS-0.1. Lifted and adapted from
 * `policy-engine/sdk/node/src/index.ts`.
 */

import { native, AgentHooksCoreError } from "./native";
export { AgentHooksCoreError };

/** Spec version this SDK implements (§4.1 `spec` field). */
export const SPEC_VERSION = "agent-hooks/0.1";

export type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue };

/** The closed set of agent lifecycle interception points (§3). */
export const InterceptionPoint = Object.freeze({
  AgentStartup: "agent_startup",
  Input: "input",
  PreModelCall: "pre_model_call",
  PostModelCall: "post_model_call",
  PreToolCall: "pre_tool_call",
  PostToolCall: "post_tool_call",
  Output: "output",
  AgentShutdown: "agent_shutdown",
} as const);
export type InterceptionPoint = (typeof InterceptionPoint)[keyof typeof InterceptionPoint];

/** Whether a `transform` verdict is permitted at `hp` (§3, §4.3). */
export function transformPermitted(hp: InterceptionPoint): boolean {
  return hp !== InterceptionPoint.AgentStartup && hp !== InterceptionPoint.AgentShutdown;
}

/** Verdict decision values (§5.1). */
export const Decision = Object.freeze({
  Allow: "allow",
  Deny: "deny",
  Warn: "warn",
  Escalate: "escalate",
  Transform: "transform",
} as const);
export type Decision = (typeof Decision)[keyof typeof Decision];

/** Whether the action proceeds under `d` (§2 permit class). */
export function permits(d: Decision): boolean {
  return d === Decision.Allow || d === Decision.Warn || d === Decision.Transform;
}

/** Whether the host acts on verdicts (§8). */
export const EnforcementMode = Object.freeze({
  Enforce: "enforce",
  EvaluateOnly: "evaluate_only",
} as const);
export type EnforcementMode = (typeof EnforcementMode)[keyof typeof EnforcementMode];

/** Reserved `host_error:*` reasons a host synthesizes (§11). */
export const HostError = Object.freeze({
  ContextInvalid: "host_error:context_invalid",
  InterceptorFailed: "host_error:interceptor_failed",
  InterceptorTimeout: "host_error:interceptor_timeout",
  VerdictInvalid: "host_error:verdict_invalid",
  TransformInvalid: "host_error:transform_invalid",
  TransformTargetForbidden: "host_error:transform_target_forbidden",
  ApprovalResolverMissing: "host_error:approval_resolver_missing",
  ApprovalResolverFailed: "host_error:approval_resolver_failed",
  ApprovalUnresolved: "host_error:approval_unresolved",
  ApprovalActionMismatch: "host_error:approval_action_mismatch",
  AdapterUnsupported: "host_error:adapter_unsupported",
  StreamingUnsupported: "host_error:streaming_unsupported",
  NoInterceptor: "host_error:no_interceptor",
} as const);
export type HostError = (typeof HostError)[keyof typeof HostError];

/** A single `$target`-rooted replacement (§5.2). */
export interface Transform {
  /** Path rooted at `$target` (or the deprecated `$policy_target` alias). */
  path: string;
  value: JsonValue;
}

/** Opaque pointer to an offline-verifiable artefact (§5.3). */
export interface Evidence {
  artefact?: string | null;
  verification_pointers?: Record<string, string>;
}

/** Interceptor return value (§5). */
export interface Verdict {
  decision: Decision;
  reason?: string | null;
  message?: string | null;
  transform?: Transform;
  evidence?: Evidence;
  result_labels?: string[];
}

/** The trivial permit verdict. */
export const ALLOW: Readonly<Verdict> = Object.freeze({ decision: Decision.Allow });

/** Host-synthesized deny verdict for a §11 failure. */
export function hostErrorVerdict(err: HostError, message?: string): Verdict {
  return { decision: Decision.Deny, reason: err, message };
}

/** Wire-shaped agent context (§4). L0 fields typed; L1/L2 indexed. */
export interface AgentContext {
  spec: string;
  interception_point: InterceptionPoint;
  timestamp: string;
  sequence: number;
  agent: { id: string; framework: string; name?: string; version?: string };
  session: { id: string; started_at?: string; turn?: number };
  target: JsonValue;
  extensions?: Record<string, JsonValue>;
  [l1l2: string]: JsonValue | undefined;
}

/** Host-side record of one interception (§6, §10).
 *
 * Identity-only by design: the identities bind the record to the exact
 * pre/post-fold context without duplicating the (possibly sensitive)
 * payload into audit storage. Hosts that need the raw transformed value
 * log it at the callsite. */
export interface InterceptionRecord {
  interception_point: InterceptionPoint;
  mode: EnforcementMode;
  verdict: Verdict;
  input_identity: string;
  enforced_identity: string;
  /** `ctx.session.id` — correlates records across a session. */
  session_id: string;
  /** `ctx.sequence` — total order within the session (§12.2.3). */
  sequence: number;
  /** Registration index of the deciding interceptor; `null` for a pure
   * allow or a host-synthesized `host_error:*` verdict. */
  decided_by: number | null;
}

/** Whether the guarded action executes (§6, §8). */
export function proceeds(r: InterceptionRecord): boolean {
  return r.mode === EnforcementMode.EvaluateOnly || permits(r.verdict.decision);
}

/** Interceptor protocol (§7). */
export interface Interceptor {
  intercept(context: AgentContext): Verdict | Promise<Verdict>;
}

/** Approval seam (§9). */
export const ApprovalOutcome = Object.freeze({
  Approve: "approve",
  Reject: "reject",
  Unresolved: "unresolved",
} as const);
export type ApprovalOutcome = (typeof ApprovalOutcome)[keyof typeof ApprovalOutcome];

export interface ApprovalRequest {
  context_identity: string;
  interception_point: InterceptionPoint;
  verdict: Verdict;
  context: AgentContext;
}

export interface ApprovalResolution {
  outcome: ApprovalOutcome;
  context_identity: string;
  verdict?: Verdict;
}

export interface ApprovalResolver {
  resolve(request: ApprovalRequest): ApprovalResolution | Promise<ApprovalResolution>;
}

// ---- Canonical JSON & context identity (§10) -------------------------------
//
// Delegates to the Rust core via napi-rs so every SDK produces
// byte-identical output. The pure-TS implementation was removed once the
// core became canonical (see sdk/rust/core/src/canonical.rs).

/** Serialize per §10.1. Implemented by the Rust core. */
export function canonicalJson(v: JsonValue): string {
  return native.canonicalJson(JSON.stringify(v));
}

/** `"sha256:" + hex(SHA-256(canonicalJson(ctx_L01)))` (§10.2). Rust core. */
export function contextIdentity(ctx: AgentContext): string {
  return native.contextIdentity(JSON.stringify(ctx));
}

/** §5: validate an interceptor's wire return value. Rust core. */
export function validateVerdict(v: Verdict): void {
  native.validateVerdict(JSON.stringify(v));
}

/** §5.2: apply a `$target`-rooted transform. Returns a new object. Rust core. */
export function applyTransform(target: JsonValue, path: string, value: JsonValue): JsonValue {
  return JSON.parse(native.applyTransform(JSON.stringify(target), path, JSON.stringify(value)));
}

/** §7.1 fold-through: apply one transform to the context's `target` (and
 * its L1 alias) so the next interceptor sees the effect. Returns the
 * updated context. Rust core. */
export function applyTransformCtx(
  ctx: AgentContext,
  path: string,
  value: JsonValue,
): AgentContext {
  return JSON.parse(native.applyTransformCtx(JSON.stringify(ctx), path, JSON.stringify(value)));
}

/** §8 `evaluate_only`: validate a transform against the context's current
 * target without applying it. Rust core. */
export function validateTransformCtx(ctx: AgentContext, path: string, value: JsonValue): void {
  native.validateTransformCtx(JSON.stringify(ctx), path, JSON.stringify(value));
}

/** §6/§10: build the `InterceptionRecord` for one completed interception.
 * `inputIdentity` MUST have been computed before interceptor dispatch;
 * transforms were already applied during the §7.1 fold. Rust core. */
export function finalize(
  ctx: AgentContext,
  verdict: Verdict,
  mode: EnforcementMode,
  inputIdentity: string,
  decidedBy: number | null = null,
): InterceptionRecord {
  return JSON.parse(
    native.finalize(
      JSON.stringify(ctx),
      JSON.stringify(verdict),
      mode,
      inputIdentity,
      decidedBy ?? -1,
    ),
  );
}

export { AgentContextBuilder } from "./builder";
export { InterceptionEmitter } from "./emitter";

/** Raised by a host when a verdict blocks the guarded action (§6). */
export class InterceptionBlocked extends Error {
  constructor(public readonly result: InterceptionRecord) {
    super(
      `${result.interception_point} blocked: ${result.verdict.decision} (${result.verdict.reason ?? "no reason"})`,
    );
    this.name = "InterceptionBlocked";
  }
}
