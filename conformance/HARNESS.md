# Implementing a CTK Harness

A **harness** is the ~100-line shim a framework author writes once so the
agent-hooks CTK can drive that framework through the conformance vectors. The
harness owns three responsibilities:

1. **Wire the scenario.** Inject the vector's mock model (a deterministic
   response script) and mock tools (a `name → args → return` lookup table)
   into the framework so a run is hermetic — no real LLM, no real I/O.
2. **Register the consumer.** Attach the CTK-supplied `HookConsumer` (a
   `ScriptedConsumer` that records every `HookContext` and replays the
   vector's `consumer_script`) at every hook point the framework supports.
3. **Run and report.** Execute one agent session and return a `RunRecord`
   describing what happened.

The CTK runner handles everything else: loading vectors, building the
scripted consumer/resolver, schema-validating recorded contexts, and asserting
`expect`.

## Interface (per language)

The exact signature is defined per SDK. The Python shape is canonical:

```python
from agent_hooks import HookConsumer, ApprovalResolver, EnforcementMode
from agent_hooks.ctk import Harness, Scenario, RunRecord, Capability

class MyFrameworkHarness(Harness):
    name = "my-framework"
    capabilities = {Capability.MODEL_CALLS, Capability.TOOL_CALLS}

    def setup(
        self,
        scenario: Scenario,
        consumer: HookConsumer,
        resolver: ApprovalResolver | None,
        mode: EnforcementMode,
    ) -> None:
        # 1. Build a mock model from scenario.model_script
        # 2. Build mock tools from scenario.tools (record every invocation!)
        # 3. Construct your framework's agent with those mocks
        # 4. Register `consumer` so it receives a HookContext at every hook
        # 5. Register `resolver` as the approval seam (if your framework
        #    supports escalate)
        # 6. Set enforcement mode
        ...

    async def run(self) -> RunRecord:
        # Execute one session with scenario.input. Catch HookBlocked.
        # Return outcome, final_output, and the tool-invocation log captured
        # by your mock tools.
        ...

    def teardown(self) -> None:
        ...
```

Equivalent interfaces ship in `sdk/typescript/src/ctk/harness.ts`,
`sdk/dotnet/src/AgentHooks.Conformance/IHarness.cs`,
`sdk/go/conformance/harness.go`, and `sdk/rust/conformance/src/harness.rs`.

## Mock model

`scenario.model_script` is an ordered list of responses. The Nth
`pre_model_call` your framework dispatches MUST receive `model_script[N]` as
its response. Your mock model implementation is typically a closure over a
counter.

## Mock tools

`scenario.tools[].behavior` is a list of `{when_args?, return, is_error?}`
clauses evaluated top-down; the first whose `when_args` deep-equals the
invocation args (or has no `when_args`) wins. **Your mock MUST record every
invocation** `{name, args}` into a list the harness returns in
`RunRecord.tool_invocations` — this is how the CTK proves a `transform` was
actually honoured independently of what the host *reports* in
`post_tool_call`.

## Capabilities

Declare only what your framework actually does. A vector whose
`capabilities` are not a subset of yours is **skipped**, not failed. The
mandatory baseline is `{}` (lifecycle only: `agent_startup`, `input`,
`output`, `agent_shutdown`).

## Running

```bash
# Python
pytest --agent-hooks-harness=my_pkg.MyFrameworkHarness \
       --agent-hooks-vectors=path/to/conformance/vectors \
       --agent-hooks-level=2
```

See per-language `sdk/<lang>/README.md` for the equivalent invocation.
