# agent-hooks

> **Status:** Draft · **Spec:** [AGENT-HOOKS-0.1](spec/AGENT-HOOKS-0.1.md)

A framework-neutral contract for **lifecycle hooks** in AI agent systems: a
fixed set of hook points, the context payload a host framework supplies at
each, the verdict a hook consumer returns, and the obligations a host MUST
honour for each verdict — plus a multi-language Conformance Test Kit.

Extracted from the [Agent Control Specification][acs] (which becomes one
conformant consumer) so that **any** agent framework can expose the same
interception surface and **any** consumer — policy engine, observability,
audit, cost tracking — can target it.

[acs]: https://github.com/microsoft/agent-governance-toolkit

## What's here

| Path | What |
| --- | --- |
| [`spec/AGENT-HOOKS-0.1.md`](spec/AGENT-HOOKS-0.1.md) | Normative RFC-2119 spec |
| [`spec/schema/`](spec/schema/) | Machine-readable JSON Schemas (hook-point, hook-context, verdict, …) |
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
│   HookContext    HookContext    HookContext    HookContext      │
│        │              │              │              │           │
└────────┼──────────────┼──────────────┼──────────────┼───────────┘
         ▼              ▼              ▼              ▼
   ┌───────────────── HookConsumer.on_hook(ctx) ──────────────┐
   │              (ACS, OTel, audit, custom …)                │
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

**Consumer:** implement `HookConsumer.on_hook(HookContext) -> Verdict` in any
SDK; register with the host.

**Prove conformance:**

```bash
cd sdk/python
pip install -e .[ctk]
pytest --agent-hooks-harness=your_pkg:YourHarness --agent-hooks-level=2
```

## Conformance levels

| Level | Name | Proves |
| --- | --- | --- |
| 1 | Observing | All applicable hooks fire in spec order with valid L0+L1 context |
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
