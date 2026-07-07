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

## What agent-hooks is not

agent-hooks is a cooperative contract, **not a security boundary**. The
host is fully trusted; interceptors run in-process with full data
access; the eight interception points do not guarantee complete
mediation; and a conformance level is not a security certification.
See [`SECURITY.md`](SECURITY.md) and
[spec §1.4](spec/AGENT-HOOKS-0.1.md#14-trust-model-and-non-goals) for
the normative statement.

[acs]: https://github.com/microsoft/agent-governance-toolkit

## What's here

| Path | What |
| --- | --- |
| [`spec/AGENT-HOOKS-0.1.md`](spec/AGENT-HOOKS-0.1.md) | Normative RFC-2119 spec |
| [`spec/schema/`](spec/schema/) | Machine-readable JSON Schemas (interception-point, agent-context, verdict, …) |
| [`conformance/vectors/`](conformance/vectors/) | Language-agnostic CTK test vectors |
| [`conformance/HARNESS.md`](conformance/HARNESS.md) | How to write a harness for your framework |
| [`sdk/python/`](sdk/python/) | Reference SDK: types + emitter + **complete CTK runner** |
| [`sdk/{rust,typescript,dotnet,go}/`](sdk/) | Bindings over the Rust core: types, emitter, CTK runner, ReferenceHarness |

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
pytest --agent-hooks-harness=your_pkg:YourHarness
```

## Conformance

A host is **conformant** when it passes 100% of the CTK vectors
applicable to its declared capability subset — a single bar covering
correct emission and correct enforcement (`deny`, `transform`
fold-through, `escalate`, `evaluate_only`, fail-closed). There are no
tiers, and a conformance claim is not a security certification.

See [`conformance/CLAIMS.md`](conformance/CLAIMS.md).

## Versioning

The **spec** is versioned `MAJOR.MINOR` independently of the **SDKs**
(semver). Each SDK declares the spec version it implements via
`SPEC_VERSION`. See [`VERSIONING.md`](VERSIONING.md).

## License

MIT — see [`LICENSE`](LICENSE).
