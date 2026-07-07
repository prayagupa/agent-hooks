# Agent Hooks Specification — Version 0.1

> **Status:** Draft · **Version:** `0.1.0-alpha` · **Date:** 2026-06-19
> **Editors:** Responsible AI / Agent Governance Toolkit
>
> This document defines a framework-neutral contract for **lifecycle hooks** in
> AI agent systems: a fixed set of interception points, the context payload a host
> framework supplies at each point, the verdict an interceptor returns, and
> the obligations a host MUST honour for each verdict. It is extracted from and
> supersedes §4, §13, §14, §17, and §18 of the Agent Control Specification
> v0.3.1-beta, which becomes one conformant interceptor of this contract.

---

## Table of contents

1. [Introduction](#1-introduction)
2. [Terminology](#2-terminology)
3. [Interception points](#3-interception-points)
4. [Agent context](#4-agent-context)
5. [Verdict](#5-verdict)
6. [Host obligations](#6-host-obligations)
7. [Interceptor contract](#7-interceptor-contract)
8. [Enforcement mode](#8-enforcement-mode)
9. [Approval seam](#9-approval-seam)
10. [Canonical serialization and action identity](#10-canonical-serialization-and-action-identity)
11. [Reserved reasons](#11-reserved-reasons)
12. [Streaming and parallel tool calls](#12-streaming-and-parallel-tool-calls)
13. [Conformance](#13-conformance)
14. [Security considerations](#14-security-considerations)
15. [References](#15-references)

---

## 1. Introduction

### 1.1 Scope

[Pure Specification]

This specification defines the bidirectional control contract between a
**host** (an agent framework or runtime that executes an agent loop) and an
**interceptor** (a component that controls the agent at well-defined
lifecycle points by returning a verdict the host MUST act on). It defines:

- the closed set of **interception points** at which a host MUST invoke registered
  interceptors,
- the **`AgentContext`** JSON payload a host MUST construct at each interception point,
- the **`Verdict`** JSON payload an interceptor MUST return,
- the **obligations** a host MUST honour for each verdict decision,
- a capability-scoped **conformance** model and a language-agnostic
  **Conformance Test Kit** (CTK).

This specification does NOT define how an interceptor computes a verdict.
Policy languages, manifests, dispatchers, annotators, and information-flow
lattices are out of scope and are defined by interceptor specifications such
as the Agent Control Specification (ACS).

This specification is a control plane, not a telemetry plane. It does NOT
define a passive observation, tracing, or metrics interface. Every
registered interceptor MUST return a `Verdict`; an interceptor that has no
control decision to make returns `allow`. A host MAY surface `AgentContext`
to its own telemetry, but that is outside this contract.

### 1.2 Key words

The key words MUST, MUST NOT, REQUIRED, SHALL, SHALL NOT, SHOULD, SHOULD NOT,
RECOMMENDED, MAY, and OPTIONAL in this document are to be interpreted as
described in [RFC 2119] and [RFC 8174] when, and only when, they appear in all
capitals.

### 1.3 Design invariants

[Pure Specification]

- **Determinism.** Given the same `AgentContext` and the same interceptor state, a
  interceptor SHOULD return the same `Verdict`. A host MUST NOT depend on
  non-determinism for correctness.
- **Fail closed.** A host that cannot construct a valid `AgentContext`, cannot
  reach a registered interceptor, or receives an invalid `Verdict` MUST treat the
  outcome as `deny` with a `host_error:*` reason per §11.
- **No silent bypass.** A host MUST NOT execute the action guarded by an
  interception point without first invoking every registered interceptor
  for that point and honouring the resulting verdict per §6.

### 1.4 Trust model and non-goals

[Pure Specification]

This specification constrains a **cooperating host**. It is not a
security boundary against a hostile or buggy host, and conformance is
not a security certification.

- The host is inside the trust boundary. Every obligation in §6 and
  §1.3 is a MUST on the host; a host that does not honour them voids
  the contract. This specification defines no mechanism to detect a
  host that skips interception points, ignores verdicts, or
  misreports enforcement mode.
- Registered interceptors are inside the host process and are fully
  trusted by the host. They receive the raw `target` (which may
  contain user PII, secrets in tool arguments, and model output) and
  any registered interceptor MAY return `deny` or `transform` at any
  interception point. Registering an interceptor is equivalent to
  granting it write access to every action the agent takes.
- The approval resolver (§9) is inside the host process and is fully
  trusted by the host.
- The eight interception points in §3 are the paths a conformant host
  wires. This specification does not claim complete mediation: a
  framework MAY expose code paths (direct tool execution, plugin
  hooks, background tasks) that do not reach any interception point,
  and the CTK cannot detect them.
- This specification does not define sandboxing, process isolation,
  authentication of interceptors, or authorization of who may register
  an interceptor. Those are host concerns.
- Conformance (§13) attests that the host adapter honours the verdict
  contract under CTK conditions with a mocked model and tools. It is
  not adversarial testing and MUST NOT be presented as a security
  certification.

---

## 2. Terminology

| Term | Definition |
| --- | --- |
| **Host** | An agent framework, SDK, or runtime that executes an agent loop and dispatches to interceptors at each interception point. |
| **Interceptor** | A component registered with the host that receives an `AgentContext` and returns a `Verdict`. |
| **Interception point** | One of the eight named lifecycle positions in §3. |
| **Agent context** | The JSON payload the host constructs at an interception point per §4. |
| **Target** | The JSON value within the agent context that the guarded action will consume or has produced. The value a `transform` verdict rewrites. |
| **Verdict** | The JSON payload an interceptor returns per §5. |
| **Action** | The host operation immediately following a `pre_*` interception point or immediately preceding a `post_*` interception point (a model call, a tool invocation, emitting output). |
| **Permit verdict** | A verdict whose `decision` is `allow`, `warn`, or `transform`. |
| **Block verdict** | A verdict whose `decision` is `deny` or `escalate`. |
| **CTK** | The Conformance Test Kit defined in §13 and shipped under `conformance/`. |

---

## 3. Interception points

[Pure Specification]

A host MUST emit the following eight interception points. The set is closed; a host
MUST NOT emit an `AgentContext` whose `interception_point` is not one of these values.

| `interception_point` | Position in the agent loop | Target (§4.3) | `transform` permitted |
| --- | --- | --- | --- |
| `agent_startup` | Once, before the first `input` of a session. | `agent_init` | no |
| `input` | On ingress of each external request into the session. | `input` | yes |
| `pre_model_call` | Immediately before each model request is dispatched. | `messages` | yes |
| `post_model_call` | Immediately after each model response is received. | `response` | yes |
| `pre_tool_call` | Immediately before each tool invocation. | `tool_call.args` | yes |
| `post_tool_call` | Immediately after each tool invocation completes (success or error). | `tool_result.value` | yes |
| `output` | Immediately before the final response is returned to the caller. | `output` | yes |
| `agent_shutdown` | Once, after the last `output` of a session or on abnormal termination. | `summary` | no |

### 3.1 Ordering

[Pure Specification]

Within a single session a host MUST emit interception points such that:

1. `agent_startup` precedes every other interception point.
2. `agent_shutdown` follows every other interception point.
3. Each `input` precedes the `pre_model_call`, `pre_tool_call`, and `output`
   hooks that result from it.
4. Each `pre_model_call` is followed by exactly one `post_model_call` for the
   same `request_id` unless the model call is blocked per §6.
5. Each `pre_tool_call` is followed by exactly one `post_tool_call` for the
   same `tool_call.id` unless the tool call is blocked per §6.
6. `sequence` (§4.1) is strictly increasing across all interception points in a
   session.

A host that supports multi-turn sessions MAY emit multiple
`input` → … → `output` cycles between a single `agent_startup` and
`agent_shutdown`.

### 3.2 Capability subsetting

[Default Implementation]

A host that does not perform model calls (e.g., a pure tool router) MAY omit
`pre_model_call` and `post_model_call`. A host that does not invoke tools MAY
omit `pre_tool_call` and `post_tool_call`. Such a host MUST declare its omitted
capabilities to the CTK per §13.2 and MUST still emit `agent_startup`, `input`,
`output`, and `agent_shutdown`.

---

## 4. Agent context

[Pure Specification]

A `AgentContext` is a JSON object. Its schema is tiered:

- **L0** fields are REQUIRED at every interception point.
- **L1** fields are REQUIRED at the specific interception point(s) listed.
- **L2** fields are well-known and OPTIONAL.
- **L3** fields are namespaced extensions under `extensions.<namespace>`.

The machine-readable schema is `spec/schema/agent-context.schema.json` with
per-point closed schemas at `spec/schema/agent-context/<interception_point>.schema.json`.

### 4.1 L0 — required core

```jsonc
{
  "spec": "agent-hooks/0.1",
  "interception_point": "<one of §3>",
  "timestamp": "2026-06-19T14:03:11.123Z",
  "sequence": 0,
  "agent":   { "id": "string", "framework": "string" },
  "session": { "id": "string" },
  "target": <any JSON>
}
```

| Field | Type | Constraint |
| --- | --- | --- |
| `spec` | string | MUST match `^agent-hooks/\d+\.\d+$`. Identifies the spec version the host targets. |
| `interception_point` | string | MUST be one of the eight values in §3. |
| `timestamp` | string | RFC 3339 UTC instant at which the host constructed the context. |
| `sequence` | integer ≥ 0 | MUST be strictly increasing within `session.id`. Starts at 0 on `agent_startup`. |
| `agent.id` | string | Stable identifier for the agent definition. MUST be stable across sessions of the same agent. |
| `agent.framework` | string | Lowercase identifier of the host framework, matching `^[a-z0-9_-]+$` (e.g., `langchain`, `openai-agents`, `semantic-kernel`). |
| `session.id` | string | Stable identifier for the run/conversation. MUST be stable across all hooks in one session. |
| `target` | any | The value-under-evaluation per §4.3. The root that `$target` in a `transform.path` resolves against. |

### 4.2 L1 — per-interception-point required fields

A host MUST populate the following fields at the indicated interception point in
addition to L0. `target` MUST be set to the indicated value.

#### `agent_startup`

```jsonc
{
  "agent_init": {
    "tools_registered": ["string", ...]
  },
  "target": <agent_init>
}
```

#### `input`

```jsonc
{
  "input": {
    "content": "string | object",
    "role": "user" | "system" | "external"
  },
  "target": <input>
}
```

#### `pre_model_call`

```jsonc
{
  "model": { "id": "string" },
  "messages": [ { "role": "string", "content": "string | object" }, ... ],
  "target": <messages>
}
```

#### `post_model_call`

```jsonc
{
  "model": { "id": "string" },
  "response": {
    "content": "string | object | null",
    "tool_calls": [ { "id": "string", "name": "string", "args": {} }, ... ],
    "finish_reason": "string"
  },
  "target": <response>
}
```

`response.tool_calls` MUST be present and MUST be an empty array when the model
returned no tool calls.

#### `pre_tool_call`

```jsonc
{
  "tool_call": { "id": "string", "name": "string", "args": {} },
  "target": <tool_call.args>
}
```

#### `post_tool_call`

```jsonc
{
  "tool_call": { "id": "string", "name": "string", "args": {} },
  "tool_result": { "value": <any>, "is_error": false },
  "target": <tool_result.value>
}
```

`tool_call.args` at `post_tool_call` MUST reflect the arguments actually passed
to the tool, i.e., the post-`transform` value when a `transform` verdict was
applied at `pre_tool_call`.

#### `output`

```jsonc
{
  "output": { "content": "string | object" },
  "target": <output>
}
```

#### `agent_shutdown`

```jsonc
{
  "summary": { "reason": "completed" | "error" | "cancelled" },
  "target": <summary>
}
```

### 4.3 Target

[Pure Specification]

`target` is the slice of the context that the guarded action will consume
(`pre_*`, `input`) or has produced (`post_*`, `output`). It is the **only**
value a `transform` verdict may rewrite. A host MUST set `target` to a deep
reference (or deep copy followed by write-back) of the value indicated in the
§3 table such that applying a `transform` to `target` is observable in the
subsequent action.

A `transform` verdict at `agent_startup` or `agent_shutdown` MUST be rejected
by the host with `host_error:transform_target_forbidden` per §11.

### 4.4 L2 — well-known optional fields

A host SHOULD populate the following fields when the underlying framework
exposes the data. Absence is conformant.

| Field | Interception point(s) | Type |
| --- | --- | --- |
| `agent.name`, `agent.version` | all | string |
| `session.started_at` | all | RFC 3339 |
| `session.turn` | all | integer ≥ 0 |
| `model.vendor`, `model.params` | `*_model_call` | string, object |
| `request_id` | `*_model_call` | string |
| `tools` | `pre_model_call` | array of `{name, description?, schema?}` |
| `usage.prompt_tokens`, `usage.completion_tokens` | `post_model_call` | integer |
| `tool_result.duration_ms` | `post_tool_call` | number |
| `tool_call.content_hash` | `pre_tool_call` | string `sha256:<hex>` |
| `messages` | any | full message chain |
| `trace.trace_id`, `trace.span_id` | all | W3C Trace Context hex strings |
| `tenant.id`, `tenant.name` | all | string |
| `actor.id`, `actor.kind` | all | string, `human`\|`service`\|`agent` |
| `budgets.tool_call_count`, `.token_count`, `.elapsed_seconds`, `.cost_usd` | all | number |

### 4.5 L3 — extensions

```jsonc
{
  "extensions": {
    "<namespace>": <any JSON>
  }
}
```

`<namespace>` MUST match `^[a-z][a-z0-9_]*$`. The namespaces `acs`, `ctk`, and
`agent_hooks` are reserved by this specification. A host MUST pass `extensions`
through to interceptors verbatim and MUST NOT interpret namespaces it does not
own.

---

## 5. Verdict

[Pure Specification]

An interceptor MUST return a JSON object conforming to
`spec/schema/verdict.schema.json`.

```jsonc
{
  "decision": "allow" | "deny" | "warn" | "escalate" | "transform",
  "reason": "string",
  "message": "string",
  "transform": { "path": "$target...", "value": <any> },
  "evidence": { "artefact": "string", "verification_pointers": { "<k>": "uri" } },
  "result_labels": ["string", ...]
}
```

| Member | Required | Constraint |
| --- | --- | --- |
| `decision` | yes | One of the five values above. |
| `reason` | no | MUST NOT start with `host_error:`. Free-form machine identifier. |
| `message` | no | Free-form human-readable text. |
| `transform` | iff `decision == "transform"` | See §5.2. MUST be absent for all other decisions. |
| `evidence` | no | See §5.3. Serialized size MUST NOT exceed 4096 bytes. |
| `result_labels` | no | Array of strings. See §5.4. |

A host that receives a verdict that is not a JSON object, whose `decision` is
absent or invalid, whose `reason` starts with `host_error:`, whose `transform`
presence violates the constraint above, whose `evidence` is not an object, or
whose `result_labels` is not an array of strings MUST treat it as
`{"decision": "deny", "reason": "host_error:verdict_invalid"}`.

### 5.1 Decision semantics

| Decision | Class | Semantics |
| --- | --- | --- |
| `allow` | permit | The action proceeds with `target` unchanged. |
| `warn` | permit | The action proceeds with `target` unchanged. The host SHOULD record `reason`/`message` as a warning. |
| `transform` | permit | The action proceeds with `target` rewritten per §5.2. |
| `deny` | block | The action MUST NOT proceed. |
| `escalate` | block | The action MUST NOT proceed until the approval seam (§9) resolves to a permit verdict. |

### 5.2 Transform

```jsonc
{ "path": "$target.<jsonpath-segments>", "value": <any> }
```

- `path` MUST be rooted at `$target`. For backward compatibility with ACS
  v0.3.x, a host MUST also accept the deprecated alias `$policy_target` and
  treat it as `$target`. The alias is removed in agent-hooks v0.3.
- `path` segments follow the subset of JSONPath defined in
  `spec/schema/path-grammar.abnf`: dot-member (`.foo`), bracket-index (`[0]`),
  and bracket-member (`["foo"]`) only.
- `value` is any JSON value, including `null`.
- A host MUST resolve `path` against the context's `target` value and replace
  the addressed location with `value`. The replacement MUST NOT modify any
  other part of the `AgentContext`.
- A `path` rooted elsewhere than `$target`/`$policy_target` MUST yield
  `host_error:transform_target_forbidden`.
- A `path` that does not resolve, or whose segment types are incompatible with
  `target`'s structure, MUST yield `host_error:transform_invalid`.

### 5.3 Evidence

`evidence` is an opaque pointer to an offline-verifiable artefact supporting
the verdict. A host MUST NOT dereference `verification_pointers`. A host MUST
propagate `evidence` to its audit sink unchanged when present.

### 5.4 Result labels

`result_labels` is the interceptor's return channel for label-flow tracking. A
host MUST persist `result_labels` alongside the data the interception point's `target`
produced (a tool result, a model output, a final output) and SHOULD resurface
them as `extensions.<interceptor-namespace>.source_labels` on later hooks whose
`target` derives from that data. A host MUST NOT persist `result_labels` for an
action that did not proceed (a `deny`, or an `escalate` not approved).

---

## 6. Host obligations

[Pure Specification]

For each interception point, after obtaining a verdict in `enforce` mode (§8):

| Verdict | Host MUST |
| --- | --- |
| `allow` | Proceed with the action using `target` unchanged. |
| `warn` | Proceed with the action using `target` unchanged. Record the warning. |
| `transform` | Apply the transform to `target` per §5.2, then proceed with the action using the transformed value. |
| `deny` | NOT proceed with the action. At `pre_*` hooks the guarded call MUST NOT be dispatched. At `input` the turn MUST NOT begin. At `output` the response MUST NOT be returned to the caller. |
| `escalate` | NOT proceed with the action until the approval seam (§9) returns a permit verdict. If approval returns `deny`, treat as `deny`. |

### 6.1 Post-action block semantics

At `post_model_call` and `post_tool_call` the action has already executed. A
`deny` or unresolved `escalate` at these points means the host MUST NOT
incorporate the result into subsequent agent state: the model response or tool
result MUST be discarded as if it had errored, and the host MUST NOT
re-execute the action.

### 6.2 Block propagation

When a `pre_*` interception point yields a block verdict, the host MUST NOT emit the
corresponding `post_*` interception point for that action. The host MUST continue the agent
loop as if the action had failed (e.g., surface a tool error to the model)
unless the host's own semantics terminate the turn.

### 6.3 Failure handling

A host that fails to construct a valid `AgentContext` for an interception point MUST
treat the verdict as `{"decision": "deny", "reason": "host_error:context_invalid"}`.
A host whose registered interceptor raises, times out, or returns a non-conformant
value MUST treat the verdict as `deny` with `host_error:interceptor_failed` or
`host_error:interceptor_timeout` respectively.

---

## 7. Interceptor contract

[Pure Specification]

An interceptor is a callable `intercept(context: AgentContext) -> Verdict`. A host:

- MUST invoke registered interceptors sequentially, in registration
  order, with respect to the guarded action (the action MUST NOT begin
  until every invoked interceptor has returned and the fold in §7.1 is
  complete).
- MUST pass each interceptor its own copy of the context; an
  interceptor's in-place mutation of the object it received MUST NOT
  affect enforcement, identity computation, or later interceptors.
- MUST, in `enforce` mode with zero registered interceptors, treat every
  emission as `deny` with reason `host_error:no_interceptor` (§11). A
  deliberate passthrough is expressed by registering an explicit
  allow-all interceptor.
- SHOULD bound interceptor execution with a configurable timeout
  (RECOMMENDED default: 5000 ms) and apply §6.3 on breach.

### 7.1 Sequential fold-through

[Pure Specification]

Interceptors compose by folding transforms through the dispatch order:

1. Invoke interceptors in registration order.
2. The first block verdict (`deny` or `escalate`) short-circuits:
   remaining interceptors are NOT invoked and it becomes the combined
   verdict.
3. When an interceptor returns `transform` in `enforce` mode, the host
   MUST apply it to `target` (per §5.2, including the §4.3 write-back)
   **before** invoking the next interceptor, so each interceptor
   observes the context as already transformed by its predecessors. In
   `evaluate_only` mode the transform is validated but not applied
   (§8), so later interceptors observe the untransformed context.
4. A transform that fails to apply (§5.2) becomes a `deny` with the
   corresponding `host_error:*` reason and short-circuits.
5. If no block occurred, the combined verdict recorded is the last
   `transform` returned; otherwise `warn` if any interceptor returned
   `warn`; otherwise `allow`. `input_identity` is computed before step
   1 and `enforced_identity` after the fold (§10.2), so the record
   captures the cumulative effect regardless of which single verdict is
   recorded.

## 8. Enforcement mode

[Pure Specification]

A host MUST support two modes, selected per session or per interception point:

| Mode | Behaviour |
| --- | --- |
| `enforce` | The host honours verdicts per §6. |
| `evaluate_only` | The host invokes interceptors and records verdicts but proceeds with every action as if the verdict were `allow`. The host MUST validate `transform` per §5.2 but MUST NOT apply it. The host MUST NOT present an `evaluate_only` outcome as enforcement to any downstream system. |

---

## 9. Approval seam

[Pure Specification]

When a verdict is `escalate`, the host MUST consult a registered
**approval resolver** before the action proceeds.

```jsonc
// ApprovalRequest
{
  "context_identity": "sha256:<hex>",     // §10
  "interception_point": "<§3>",
  "verdict": <the escalate Verdict>,
  "context": <the AgentContext>            // MAY be redacted per host policy
}

// ApprovalResolution
{
  "outcome": "approve" | "reject" | "unresolved",
  "context_identity": "sha256:<hex>",     // MUST equal the request's
  "verdict": <a permit or deny Verdict>   // present iff outcome != "unresolved"
}
```

- `approve` MUST carry a permit verdict (`allow`, `warn`, or `transform`). The
  host applies §6 to that verdict.
- `reject` MUST carry `{"decision": "deny", ...}`. The host applies §6.
- `unresolved` means the resolver could not decide. The host MUST treat it as
  `deny` with `host_error:approval_unresolved`.
- A resolution whose `context_identity` differs from the request's MUST be
  rejected with `host_error:approval_action_mismatch`.
- A host with no registered resolver MUST treat `escalate` as `deny` with
  `host_error:approval_resolver_missing`.

---

## 10. Canonical serialization and action identity

[Pure Specification]

### 10.1 Canonical JSON

A host MUST serialize per RFC 8785 (JSON Canonicalization Scheme):
object members sorted by UTF-16 code units, numbers per ECMA-262
`Number::toString`, minimal RFC 8259 string escapes, no insignificant
whitespace. The canonical Rust core delegates to an RFC 8785
implementation; every binding inherits it, and the golden vectors in
`conformance/golden/identity.json` pin the exact bytes.

### 10.2 Context identity

`context_identity(ctx)` is `"sha256:" + lowercase_hex(SHA-256(canonical_json(ctx_L01)))`
where `ctx_L01` is the **closed** L0+L1 projection of `ctx` for its
interception point: exactly the fields marked required in the per-point
schemas under `spec/schema/agent-context/`, including the nested
subfield whitelists (`agent.{id,framework}`, `session.{id}`,
`model.{id}`, `tool_call.{id,name,args}`, `tool_result.{value,is_error}`,
`response.{content,tool_calls,finish_reason}`, `input.{content,role}`,
`agent_init.{tools_registered}`, `output.{content}`,
`summary.{reason}`). All other members — top-level L2/L3 and nested
optional subfields such as `tool_result.duration_ms` or
`tool_call.content_hash` — are excluded, so adding optional data never
perturbs the identity.

A host MUST compute two identities per interception:

| Identity | Definition |
| --- | --- |
| `input_identity` | `context_identity(ctx)` **before** interceptor dispatch (§7.1 step 1). |
| `enforced_identity` | `context_identity(ctx)` after the §7.1 fold completes. Equal to `input_identity` when no transform was applied, and always equal in `evaluate_only` mode. |

Approval binding (§9) uses `input_identity` as presented to the
resolver; the record carries both.

## 11. Reserved reasons

[Pure Specification]

A host MUST use the following `reason` values, and only these, when it
synthesizes a `deny` verdict per §6.3, §5.2, or §9. An interceptor MUST NOT emit a
`reason` beginning with `host_error:`.

| Reason | Cause |
| --- | --- |
| `host_error:context_invalid` | The host could not construct a schema-valid `AgentContext`. |
| `host_error:interceptor_failed` | An interceptor raised or returned a non-JSON value. |
| `host_error:interceptor_timeout` | An interceptor exceeded the host's timeout. |
| `host_error:verdict_invalid` | An interceptor returned a value that fails §5 validation. |
| `host_error:transform_invalid` | `transform.path` did not resolve or `value` could not be set. |
| `host_error:transform_target_forbidden` | `transform.path` is not rooted at `$target`, or the interception point forbids transform. |
| `host_error:approval_resolver_missing` | `escalate` with no registered resolver. |
| `host_error:approval_resolver_failed` | The resolver raised or timed out. |
| `host_error:approval_unresolved` | The resolver returned `unresolved`. |
| `host_error:approval_action_mismatch` | The resolver's `context_identity` did not match. |
| `host_error:adapter_unsupported` | The host adapter cannot emit this interception point. |
| `host_error:no_interceptor` | An `enforce`-mode emission with zero registered interceptors (§7). |
| `host_error:streaming_unsupported` | The host cannot satisfy §12 for a streaming response. |

The machine-readable inventory is `spec/reserved-reasons.json`.

---

## 12. Streaming and parallel tool calls

[Pure Specification]

### 12.1 Streaming model responses

A host that streams model output MUST assemble the complete response before
emitting `post_model_call`. A host that cannot assemble MUST emit
`post_model_call` with `response.finish_reason: "stream_incomplete"` and a
`deny` self-verdict with `host_error:streaming_unsupported`, and MUST NOT
incorporate the partial response.

### 12.2 Parallel tool calls

When a model response carries N tool calls that the host invokes concurrently,
the host MUST emit N independent `pre_tool_call`/`post_tool_call` pairs, each
with a distinct `tool_call.id`. The host MAY interleave them. `sequence` MUST
remain strictly increasing across the interleaving.

---

## 13. Conformance

### 13.1 Conformance

| Requirement |
| --- |
| A host is **conformant** when it passes 100% of the CTK vectors applicable to its declared capability subset (§3.2). |

There are no conformance tiers. The single bar includes correct
emission (order, schema-valid L0+L1 contexts per §3–§4) and correct
enforcement (`deny`, `transform` fold-through, `escalate` with the
approval seam, `evaluate_only`, and the fail-closed rules of §6.3 and
§7). A host that only wants observation is out of scope for this
specification (§1.1); partial adapters under development can run vector
subsets locally but MUST NOT claim conformance.

Populating L2 fields where the framework has the data is a SHOULD and
is not conformance-gated.

### 13.2 CTK

The Conformance Test Kit under `conformance/` is the normative test of §13.1.
A host claims conformance by:

1. Implementing the `Harness` interface in at least one SDK language
   (`conformance/HARNESS.md`).
2. Declaring its `capabilities` subset (§3.2).
3. Passing 100% of non-skipped vectors.

Vectors are language-agnostic JSON under `conformance/vectors/` validated by
`conformance/vectors.schema.json`. Per-language CTK runners are provided under
`sdk/<lang>/`.

### 13.3 Claims

A conformance claim is the tuple
`(<framework>, <adapter-version>, agent-hooks/<spec-version>, <capabilities>, <sdk-lang>@<sdk-version>)`
recorded in `conformance/CLAIMS.md`.

---

## 14. Security considerations

- An interceptor receives the full `target`, which may contain user PII, secrets in
  tool arguments, or model output. Hosts SHOULD redact known-sensitive fields
  before constructing `AgentContext` when the interceptor's trust level does not
  warrant raw access, and MUST document any redaction in
  `extensions.<host>.redacted: ["<jsonpath>", ...]`.
- `evidence.verification_pointers` are URIs a host MUST NOT dereference
  automatically; doing so is an SSRF vector.
- A `transform` verdict rewrites data the agent will act on. Hosts SHOULD
  authenticate interceptors and SHOULD log every applied transform with both
  `input_identity` and `enforced_identity`.
- `evaluate_only` mode MUST NOT be presented to downstream systems as
  enforcement; doing so is a compliance hazard.

---

## 15. References

- [RFC 2119] Key words for use in RFCs to Indicate Requirement Levels.
- [RFC 8174] Ambiguity of Uppercase vs Lowercase in RFC 2119 Key Words.
- [RFC 8259] The JavaScript Object Notation (JSON) Data Interchange Format.
- [RFC 3339] Date and Time on the Internet: Timestamps.
- [RFC 8785] JSON Canonicalization Scheme (JCS).
- Agent Control Specification v0.3.1-beta. `policy-engine/spec/SPECIFICATION.md`.
- AGT-SNAPSHOT-1.0. `policy-engine/spec/agt/AGT-SNAPSHOT-1.0.md`.
