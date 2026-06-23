// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
/**
 * Conformance Test Kit harness contract (§13.2).
 *
 * The TypeScript CTK runner is not yet implemented; this module defines the
 * `Harness` interface so framework adapters can be written now. Track the
 * runner at https://github.com/responsibleai/agent-hooks/issues/3; the
 * Python implementation at `sdk/python/src/agent_hooks/ctk/runner.py` is the
 * reference.
 */

import type {
  ApprovalResolver,
  EnforcementMode,
  HookConsumer,
  JsonValue,
} from "../index.js";

/** Host-declared capability subset (§3.2). */
export type Capability =
  | "model_calls"
  | "tool_calls"
  | "parallel_tool_calls"
  | "streaming"
  | "multi_turn";

export type RunOutcome = "completed" | "blocked" | "suspended" | "error";

/** Hermetic scripted run loaded from a CTK vector (wire-shaped). */
export interface Scenario {
  input: { content: JsonValue; role: "user" | "system" | "external" };
  tools?: Array<{
    name: string;
    schema?: Record<string, JsonValue>;
    behavior: Array<{ when_args?: Record<string, JsonValue>; return: JsonValue; is_error?: boolean }>;
  }>;
  model_script?: Array<{
    respond: {
      content: JsonValue;
      tool_calls: Array<{ id: string; name: string; args: Record<string, JsonValue> }>;
      finish_reason: string;
    };
  }>;
}

/** What `Harness.run` returns to the CTK runner. */
export interface RunRecord {
  outcome: RunOutcome;
  final_output: JsonValue | null;
  tool_invocations: Array<{ name: string; args: Record<string, JsonValue> }>;
  error?: string;
}

/** The single interface a framework adapter implements for the CTK. */
export interface Harness {
  readonly name: string;
  readonly capabilities: ReadonlySet<Capability>;

  setup(
    scenario: Scenario,
    consumer: HookConsumer,
    resolver: ApprovalResolver | null,
    mode: EnforcementMode,
  ): void;

  run(): Promise<RunRecord>;

  teardown(): void;
}
