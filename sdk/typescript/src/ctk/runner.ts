// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
/**
 * CTK runner: load vectors, drive a harness, assert `expect`.
 *
 * The assertion engine, capability skip check, and scripted
 * interceptor/resolver evaluation live in the Rust core (native.ctk*).
 * This module keeps only vector globbing, the recording wrapper, and
 * the orchestration loop that calls the native `Harness`.
 */

import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

import {
  AgentContext,
  ApprovalRequest,
  ApprovalResolution,
  EnforcementMode,
  Interceptor,
  JsonValue,
  Verdict,
} from "../index";
import { native } from "../native";
import type { Harness, RunRecord, Scenario } from "./index";

export interface VectorResult {
  id: string;
  title: string;
  level: number;
  status: "pass" | "fail" | "skip";
  detail: string;
  failures: string[];
}

export function loadVectors(dir: string, maxLevel = 3): JsonValue[] {
  return readdirSync(dir)
    .filter((f) => /^AH-CTK-.*\.json$/.test(f))
    .sort()
    .map((f) => JSON.parse(readFileSync(join(dir, f), "utf8")) as JsonValue)
    .filter((v) => (v as { level: number }).level <= maxLevel);
}

/** Wraps `ctk_scripted_intercept` and records every ctx passed. */
class RecordingInterceptor implements Interceptor {
  readonly recorded: AgentContext[] = [];
  private readonly rulesJson: string;
  constructor(rules: JsonValue) {
    this.rulesJson = JSON.stringify(rules);
  }
  intercept(ctx: AgentContext): Verdict {
    this.recorded.push(JSON.parse(JSON.stringify(ctx)));
    return JSON.parse(native.ctkScriptedIntercept(this.rulesJson, JSON.stringify(ctx)));
  }
}

class ScriptedResolver {
  private readonly rulesJson: string;
  constructor(rules: JsonValue) {
    this.rulesJson = JSON.stringify(rules);
  }
  resolve(req: ApprovalRequest): ApprovalResolution {
    return JSON.parse(
      native.ctkScriptedResolve(this.rulesJson, JSON.stringify(req.context), req.context_identity),
    );
  }
}

function runRecordToWire(rr: RunRecord): string {
  return JSON.stringify({
    outcome: rr.outcome,
    final_output: rr.final_output ?? null,
    tool_invocations: rr.tool_invocations,
    error: rr.error ?? null,
    identities: rr.identities.map(([i, e]) => ({ input_identity: i, enforced_identity: e })),
  });
}

export async function runVector(harness: Harness, vector: JsonValue): Promise<VectorResult> {
  const v = vector as Record<string, JsonValue>;
  const vectorJson = JSON.stringify(vector);

  const capsJson = JSON.stringify([...harness.capabilities].sort());
  const skip = JSON.parse(native.ctkShouldSkip(vectorJson, capsJson));
  if (skip !== null) {
    return {
      id: v.id as string,
      title: v.title as string,
      level: v.level as number,
      status: "skip",
      detail: skip,
      failures: [],
    };
  }

  const interceptor = new RecordingInterceptor(v.interceptor_script);
  const approval = v.approval_script as JsonValue[] | undefined;
  const resolver = approval ? new ScriptedResolver(approval) : null;
  const mode = ((v.mode as string) ?? "enforce") as EnforcementMode;

  harness.setup(v.scenario as unknown as Scenario, interceptor, resolver, mode);
  let rr: RunRecord;
  try {
    rr = await harness.run();
  } catch (e) {
    return {
      id: v.id as string,
      title: v.title as string,
      level: v.level as number,
      status: "fail",
      detail: "",
      failures: [`harness.run threw: ${e}`],
    };
  } finally {
    harness.teardown();
  }

  return JSON.parse(
    native.ctkAssert(vectorJson, JSON.stringify(interceptor.recorded), runRecordToWire(rr)),
  );
}

export async function runVectors(
  harnessFactory: () => Harness,
  vectors: JsonValue[],
): Promise<VectorResult[]> {
  const out: VectorResult[] = [];
  for (const v of vectors) {
    out.push(await runVector(harnessFactory(), v));
  }
  return out;
}
