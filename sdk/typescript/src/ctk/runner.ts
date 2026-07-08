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
  status: "pass" | "fail" | "skip";
  detail: string;
  failures: string[];
}

export function loadVectors(dir: string): JsonValue[] {
  return readdirSync(dir)
    .filter((f) => /^AH-CTK-.*\.json$/.test(f))
    .sort()
    .map((f) => JSON.parse(readFileSync(join(dir, f), "utf8")) as JsonValue);
}

/** Replays one `interceptor_script` rule list via the Rust core. */
class ScriptedInterceptor implements Interceptor {
  protected readonly rulesJson: string;
  constructor(rules: JsonValue) {
    this.rulesJson = JSON.stringify(rules);
  }
  intercept(ctx: AgentContext): Verdict {
    const w = JSON.parse(native.ctkScriptedIntercept(this.rulesJson, JSON.stringify(ctx)));
    if (w !== null && typeof w === "object" && "__ctk_fault__" in w) {
      // NOW-10 fault injection: exercise §6.3 interceptor_failed.
      throw new Error("ctk scripted fault: raise");
    }
    return w;
  }
}

/** Wraps the scripted interceptor and records every ctx passed. */
class RecordingInterceptor extends ScriptedInterceptor {
  readonly recorded: AgentContext[] = [];
  override intercept(ctx: AgentContext): Verdict {
    this.recorded.push(JSON.parse(JSON.stringify(ctx)));
    return super.intercept(ctx);
  }
}

class ScriptedResolver {
  private readonly rulesJson: string;
  constructor(rules: JsonValue) {
    this.rulesJson = JSON.stringify(rules);
  }
  resolve(req: ApprovalRequest): ApprovalResolution {
    const r = JSON.parse(
      native.ctkScriptedResolve(this.rulesJson, JSON.stringify(req.context), req.context_identity),
    );
    if (r !== null && typeof r === "object" && "__ctk_fault__" in r) {
      // NOW-10 fault injection: exercise §9 approval_resolver_failed.
      throw new Error("ctk scripted fault: raise");
    }
    return r;
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
      status: "skip",
      detail: skip,
      failures: [],
    };
  }

  // Multi-interceptor vectors (§7.1 fold-through) use interceptor_scripts;
  // single-interceptor vectors use interceptor_script. Only the FIRST
  // interceptor records: expect.interceptions describes each emission as
  // the first-registered interceptor saw it. An empty interceptor_scripts
  // registers zero interceptors (§7 fail-closed vector).
  const scripts = (v.interceptor_scripts as JsonValue[] | undefined) ?? [v.interceptor_script];
  const first = scripts.length > 0 ? new RecordingInterceptor(scripts[0]) : null;
  const interceptors: Interceptor[] = first ? [first] : [];
  for (const s of scripts.slice(1)) interceptors.push(new ScriptedInterceptor(s));

  const approval = v.approval_script as JsonValue[] | undefined;
  const resolver = approval ? new ScriptedResolver(approval) : null;
  const mode = ((v.mode as string) ?? "enforce") as EnforcementMode;

  harness.setup(v.scenario as unknown as Scenario, interceptors, resolver, mode);
  let rr: RunRecord;
  try {
    rr = await harness.run();
  } catch (e) {
    return {
      id: v.id as string,
      title: v.title as string,
      status: "fail",
      detail: "",
      failures: [`harness.run threw: ${e}`],
    };
  } finally {
    harness.teardown();
  }

  return JSON.parse(
    native.ctkAssert(vectorJson, JSON.stringify(first?.recorded ?? []), runRecordToWire(rr)),
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
