# agent-hooks (Python SDK)

Python implementation of [AGENT-HOOKS-0.1](../../spec/AGENT-HOOKS-0.1.md):
hook-point enums, `HookContext` builder, `Verdict` types, host-side
`HookEmitter`, and the Conformance Test Kit.

```bash
pip install agent-hooks          # types + emitter
pip install agent-hooks[ctk]     # + jsonschema + pytest plugin
```

## Host (framework adapter) usage

```python
from agent_hooks import HookContextBuilder, HookEmitter, HookBlocked

builder = HookContextBuilder(agent_id="...", framework="my-fw", session_id="...")
emitter = HookEmitter().register(my_consumer)

await emitter.emit_or_raise(builder.agent_startup(tools_registered=[...]))
ctx = builder.pre_tool_call(call_id="tc-1", name="http_get", args={"url": u})
try:
    await emitter.emit_or_raise(ctx)
except HookBlocked as e:
    return tool_error(e.result.verdict.reason)
result = invoke_tool(ctx["tool_call"]["args"])  # post-transform args
```

## Consumer usage

```python
from agent_hooks import HookConsumer, HookContext, Verdict, Decision

class MyPolicy:
    def on_hook(self, ctx: HookContext) -> Verdict:
        if ctx["hook_point"] == "pre_tool_call" and ctx["tool_call"]["name"] == "rm":
            return Verdict(Decision.DENY, reason="dangerous")
        return Verdict.ALLOW
```

## Running the CTK against your framework

Implement `agent_hooks.ctk.Harness` (see `conformance/HARNESS.md`), then:

```bash
pytest --agent-hooks-harness=my_pkg:MyHarness \
       --agent-hooks-vectors=path/to/conformance/vectors \
       --agent-hooks-level=2
```
