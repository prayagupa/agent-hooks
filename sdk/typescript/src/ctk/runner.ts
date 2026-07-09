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
  Composition,
  CompositionConfig,
  EnforcementMode,
  Interceptor,
  JsonValue,
  Verdict,
} from "../index";
import { native } from "../native";
import type { Harness, RunRecord, Scenario } from "./index";

/** TODO(stage-4): vectors still authored in the pre-P-003 five-verdict
 * wire vocabulary (`warn`, `escalate`, `approval_resolver_missing`).
 * Stage 4 rewrites them to the three-verdict shapes (§5.1); until then
 * their scripted verdicts fail the §5 gate by design (fail closed) and
 * no longer exercise the seam/warning semantics they were written for,
 * so they are skipped here. */
const TODO_STAGE_4 = [
  "AH-CTK-030", // escalate-approve → deny+approval / resolution
  "AH-CTK-031", // escalate-reject → deny+approval / reject
  "AH-CTK-032", // escalate-no-resolver → liftable deny stands
  "AH-CTK-050", // warn-passthrough → allow+warnings
  "AH-CTK-072", // resolver-identity-mismatch → approval_identity_mismatch
  "AH-CTK-073", // resolver-raises → approval_resolver_failed
];

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
    // §10.1: identity may be null (null provider). The scripted engine
    // works in strings; "" round-trips to null below.
    const requestIdentity = req.context_identity ?? "";
    const r = JSON.parse(
      native.ctkScriptedResolve(this.rulesJson, JSON.stringify(req.context), requestIdentity),
    );
    if (r !== null && typeof r === "object" && "__ctk_fault__" in r) {
      // NOW-10 fault injection: exercise §9 approval_resolver_failed.
      throw new Error("ctk scripted fault: raise");
    }
    if (r.context_identity === "" && req.context_identity === null) {
      r.context_identity = null;
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

  if (TODO_STAGE_4.some((id) => (v.id as string).startsWith(id))) {
    return {
      id: v.id as string,
      title: v.title as string,
      status: "skip",
      detail: "TODO(stage-4): stale pre-P-003 vector, rewritten in stage 4",
      failures: [],
    };
  }

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
  // §13.2: composition vectors carry the profile/knobs they apply to;
  // absent means the pre-P-003 default (`sequential/first_deny, stop`).
  const composition =
    (v.composition as unknown as CompositionConfig | undefined) ?? Composition.default();

  harness.setup(v.scenario as unknown as Scenario, interceptors, resolver, mode, composition);
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
