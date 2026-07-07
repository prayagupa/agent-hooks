# Conformance claims

> **A conformance claim is not a security certification.** It attests
> that the host adapter honours the verdict contract under CTK
> conditions — a hermetic run with a mocked model and tools. It does
> not test adversarial bypass, does not assure that the production
> code path matches the harness, and says nothing about the
> interceptors registered in production. See [`SECURITY.md`](../SECURITY.md)
> and [spec §1.4](../spec/AGENT-HOOKS-0.1.md#14-trust-model-and-non-goals).

A host is **conformant** when it passes 100% of the CTK vectors
applicable to its declared capability subset (spec §13.1). There are no
tiers.

| Framework | Adapter version | Spec | Capabilities | SDK | Verified at | Notes |
| --- | --- | --- | --- | --- | --- | --- |
| reference-agent | 0.1.0 | agent-hooks/0.1 | model_calls, tool_calls | python, typescript, dotnet, go | (CI) | In-tree reference; CTK self-test |

To file a claim, open a PR adding a row with a link to a passing CTK
run, and confirm in the PR description that the harness drives the
framework's production dispatch path with only model/tool I/O mocked.
