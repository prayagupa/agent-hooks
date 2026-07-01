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

/** Host-side record of one interception (§6, §10). */
export interface InterceptionRecord {
  interception_point: InterceptionPoint;
  mode: EnforcementMode;
  verdict: Verdict;
  input_identity: string;
  enforced_identity: string;
  transformed_target?: JsonValue;
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

/** §7.1: combine an ordered array of verdicts. Rust core. */
export function combineVerdicts(verdicts: readonly Verdict[]): Verdict {
  return JSON.parse(native.combineVerdicts(JSON.stringify(verdicts)));
}

/** §6/§8/§10: enforcement step. Returns `{record, ctx}`. Rust core. */
export function enforce(
  ctx: AgentContext,
  verdict: Verdict,
  mode: EnforcementMode,
): { record: InterceptionRecord; ctx: AgentContext } {
  return JSON.parse(native.enforce(JSON.stringify(ctx), JSON.stringify(verdict), mode));
}

/** Raised by a host when a verdict blocks the guarded action (§6). */
export class InterceptionBlocked extends Error {
  constructor(public readonly result: InterceptionRecord) {
    super(
      `${result.interception_point} blocked: ${result.verdict.decision} (${result.verdict.reason ?? "no reason"})`,
    );
    this.name = "InterceptionBlocked";
  }
}
