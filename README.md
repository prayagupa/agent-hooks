# agent-hooks

> **Status:** Draft · **Spec:** [AGENT-HOOKS-0.1](spec/AGENT-HOOKS-0.1.md)

A framework-neutral **control** contract for AI agent systems: a fixed set
of interception points, the agent context a host framework supplies at each,
the verdict an interceptor returns, and the obligations a host MUST honour
for each verdict. Ships with a multi-language Conformance Test Kit.

Extracted from the [Agent Control Specification][acs] (which becomes one
conformant interceptor) so that any agent framework can expose the same
control surface and any control component (policy engine, content filter,
rate limiter, approval gateway, egress guard) can target it.

agent-hooks is a control plane, not a telemetry plane. Every interceptor
returns a `Verdict` the host MUST act on. Passive observation, tracing, and
metrics emission are out of scope; use the framework's native telemetry for
those.

[acs]: https://github.com/microsoft/agent-governance-toolkit

## What's here

| Path | What |
| --- | --- |
| [`spec/AGENT-HOOKS-0.1.md`](spec/AGENT-HOOKS-0.1.md) | Normative RFC-2119 spec |
| [`spec/schema/`](spec/schema/) | Machine-readable JSON Schemas (interception-point, agent-context, verdict, …) |
| [`conformance/vectors/`](conformance/vectors/) | Language-agnostic CTK test vectors |
| [`conformance/HARNESS.md`](conformance/HARNESS.md) | How to write a harness for your framework |
| [`sdk/python/`](sdk/python/) | Reference SDK: types + emitter + **complete CTK runner** |
| [`sdk/{rust,typescript,dotnet,go}/`](sdk/) | Type definitions + harness interface (CTK runners: [#2–#5](https://github.com/responsibleai/agent-hooks/issues)) |

## The contract in one diagram

```
┌──────────────────────── host framework ─────────────────────────┐
│                                                                 │
│  agent_startup ─► input ─► pre_model_call ─► post_model_call ─► │
│                               pre_tool_call ─► post_tool_call ─►│
│                                                output ─► shutdown
│        │              │              │              │           │
│        ▼              ▼              ▼              ▼           │
│   AgentContext    AgentContext    AgentContext    AgentContext      │
│        │              │              │              │           │
└────────┼──────────────┼──────────────┼──────────────┼───────────┘
         ▼              ▼              ▼              ▼
   ┌───────────────── Interceptor.intercept(ctx) ──────────────┐
   │     (ACS, content filter, rate limiter, egress guard…)    │
   └─────────────────────────┬────────────────────────────────┘
                             ▼
                   Verdict { allow | deny | warn | escalate | transform }
                             │
         ┌───────────────────┴───────────────────┐
         ▼                                       ▼
   permit → proceed                    block → halt / approval seam
   (transform rewrites $target)
```

## Quick start

**Host (framework adapter):** see [`sdk/python/README.md`](sdk/python/README.md)
and [`conformance/HARNESS.md`](conformance/HARNESS.md).

**Interceptor:** implement `Interceptor.intercept(AgentContext) -> Verdict`
in any SDK; register with the host.

**Prove conformance:**

```bash
cd sdk/python
pip install -e .[ctk]
pytest --agent-hooks-harness=your_pkg:YourHarness --agent-hooks-level=2
```

## Conformance levels

| Level | Name | Proves |
| --- | --- | --- |
| 1 | Instrumented | All applicable interception points fire in spec order with valid L0+L1 context. Verdicts may be ignored. Adapter-development stage; not a production claim. |
| 2 | Enforcing | + host honours `deny`, `transform`, `escalate`, `evaluate_only` |
| 3 | Complete | + L2 fields, `result_labels` propagation, parallel/streaming, stable identity |

See [`conformance/LEVELS.md`](conformance/LEVELS.md) and
[`conformance/CLAIMS.md`](conformance/CLAIMS.md).

## Versioning

The **spec** is versioned `MAJOR.MINOR` independently of the **SDKs**
(semver). Each SDK declares the spec version it implements via
`SPEC_VERSION`. See [`VERSIONING.md`](VERSIONING.md).

## License

MIT — see [`LICENSE`](LICENSE).
