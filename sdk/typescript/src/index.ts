// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
/**
 * agent-hooks: framework-neutral agent lifecycle hook contract.
 * Implements AGENT-HOOKS-0.1. Lifted and adapted from
 * `policy-engine/sdk/node/src/index.ts`.
 */

import { createHash } from "node:crypto";

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

/** Validate per §5; throws on violation so the emitter maps to `verdict_invalid`. */
export function validateVerdict(v: Verdict): void {
  if (v.reason?.startsWith("host_error:")) {
    throw new Error("verdict.reason MUST NOT start with 'host_error:' (§5)");
  }
  if (v.decision === Decision.Transform && !v.transform) {
    throw new Error("transform body REQUIRED when decision=='transform' (§5)");
  }
  if (v.decision !== Decision.Transform && v.transform) {
    throw new Error("transform body FORBIDDEN when decision!='transform' (§5)");
  }
  if (v.transform && !/^\$(target|policy_target)(\.|\[|$)/.test(v.transform.path)) {
    throw new Error("transform.path must be rooted at $target (§5.2)");
  }
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

const L0 = new Set(["spec", "interception_point", "timestamp", "sequence", "agent", "session", "target"]);
const L0_AGENT = new Set(["id", "framework"]);
const L0_SESSION = new Set(["id"]);
const L1: Record<InterceptionPoint, readonly string[]> = {
  agent_startup: ["agent_init"],
  input: ["input"],
  pre_model_call: ["model", "messages"],
  post_model_call: ["model", "response"],
  pre_tool_call: ["tool_call"],
  post_tool_call: ["tool_call", "tool_result"],
  output: ["output"],
  agent_shutdown: ["summary"],
};

function encode(v: JsonValue, out: string[]): void {
  if (v === null) out.push("null");
  else if (typeof v === "boolean") out.push(v ? "true" : "false");
  else if (typeof v === "number") {
    if (!Number.isFinite(v)) throw new Error("canonical JSON does not admit NaN/Infinity");
    out.push(Object.is(v, -0) ? "0" : String(v)); // ECMA-262 ToString(Number)
  } else if (typeof v === "string") out.push(JSON.stringify(v));
  else if (Array.isArray(v)) {
    out.push("[");
    v.forEach((e, i) => {
      if (i) out.push(",");
      encode(e, out);
    });
    out.push("]");
  } else {
    out.push("{");
    const keys = Object.keys(v).sort((a, b) =>
      Buffer.compare(Buffer.from(a, "utf8"), Buffer.from(b, "utf8")),
    );
    keys.forEach((k, i) => {
      if (i) out.push(",");
      out.push(JSON.stringify(k), ":");
      encode(v[k]!, out);
    });
    out.push("}");
  }
}

/** Serialize per §10.1. */
export function canonicalJson(v: JsonValue): string {
  const out: string[] = [];
  encode(v, out);
  return out.join("");
}

function stripToL01(ctx: AgentContext): JsonValue {
  const l1 = new Set(L1[ctx.interception_point] ?? []);
  const out: Record<string, JsonValue> = {};
  for (const [k, v] of Object.entries(ctx)) {
    if (v === undefined) continue;
    if (!L0.has(k) && !l1.has(k)) continue;
    if (k === "agent") {
      out[k] = Object.fromEntries(
        Object.entries(v as object).filter(([sk]) => L0_AGENT.has(sk)),
      ) as JsonValue;
    } else if (k === "session") {
      out[k] = Object.fromEntries(
        Object.entries(v as object).filter(([sk]) => L0_SESSION.has(sk)),
      ) as JsonValue;
    } else {
      out[k] = v as JsonValue;
    }
  }
  return out;
}

/** `"sha256:" + hex(SHA-256(canonicalJson(ctx_L01)))` (§10.2). */
export function contextIdentity(ctx: AgentContext): string {
  const json = canonicalJson(stripToL01(ctx));
  return "sha256:" + createHash("sha256").update(json, "utf8").digest("hex");
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
