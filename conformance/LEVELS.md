# Conformance levels

| Level | Name | Vectors | What it proves |
| --- | --- | --- | --- |
| **1** | Instrumented | `AH-CTK-0[0-9][0-9]` | Host emits all applicable interception points in spec order with schema-valid L0+L1 context. Verdicts may be ignored. Adapter-development milestone; not a production control claim. |
| **2** | Enforcing | Level 1 + `AH-CTK-0[1-5][0-9]` | Host honours `deny`, `transform`, `escalate` (with approval seam), and `evaluate_only`. Suitable for policy/governance interceptors (ACS). |
| **3** | Complete | Level 2 + `AH-CTK-0[6-9][0-9]` | Host populates L2 fields, persists & resurfaces `result_labels`, handles parallel tool calls and streaming per §12, and produces stable `context_identity`. |

A claim of Level N requires **100% pass** on all non-skipped vectors at
levels ≤ N. Skipped means the vector's `capabilities` are not a subset of the
harness's declared capabilities.

## Vector index

| ID | Level | Clause | Title |
| --- | --- | --- | --- |
| AH-CTK-001 | 1 | §3.1, §4 | All eight interception points fire in spec order with valid context |
| AH-CTK-002 | 1 | §3.1, §3.2 | Loop without tools omits pre/post_tool_call |
| AH-CTK-003 | 1 | §3.1.6 | sequence strictly increasing |
| AH-CTK-010 | 2 | §6, §6.2 | deny at pre_tool_call halts tool; no post_tool_call |
| AH-CTK-011 | 2 | §6 | deny at input blocks the turn |
| AH-CTK-012 | 2 | §6 | deny at output prevents response return |
| AH-CTK-020 | 2 | §5.2, §6 | transform rewrites tool arg; tool receives transformed value |
| AH-CTK-021 | 2 | §5.2 | $policy_target alias accepted |
| AH-CTK-022 | 2 | §4.3, §11 | transform at agent_startup rejected |
| AH-CTK-030 | 2 | §9 | escalate → approve → proceed |
| AH-CTK-031 | 2 | §9 | escalate → reject → halt |
| AH-CTK-032 | 2 | §9, §11 | escalate with no resolver → deny |
| AH-CTK-040 | 2 | §8 | evaluate_only records verdict but proceeds |
| AH-CTK-050 | 2 | §5.1 | warn permits unchanged |

Level 3 vectors (`AH-CTK-06x..09x`) are tracked in
[#1](https://github.com/responsibleai/agent-hooks/issues/1).
