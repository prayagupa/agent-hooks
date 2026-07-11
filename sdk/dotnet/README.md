# agent-hooks (.NET SDK)

.NET implementation of
[AGENT-HOOKS-0.1](https://github.com/responsibleai/agent-hooks/blob/main/spec/AGENT-HOOKS-0.1.md)
over the canonical Rust core (`libagent_hooks_ffi` via
`LibraryImport`): interception points, `AgentContextBuilder`,
`Verdict` types, host-side `InterceptionEmitter` with the four
composition profiles, the identity-provider seam, and the CTK runner.

> **Trust model.** agent-hooks is a *cooperative contract*, not a security
> boundary: the host framework is fully trusted, interceptors run in-process
> with full data access, and no complete-mediation claim is made. Read
> [SECURITY.md](https://github.com/responsibleai/agent-hooks/blob/main/SECURITY.md)
> and [spec §1.4](https://github.com/responsibleai/agent-hooks/blob/main/spec/AGENT-HOOKS-0.1.md#14-trust-model-and-non-goals)
> before relying on it.

```bash
# Not yet published to NuGet — build from source (needs a Rust toolchain):
git clone https://github.com/responsibleai/agent-hooks && cd agent-hooks
cargo build --release --manifest-path sdk/rust/Cargo.toml -p agent-hooks-ffi
dotnet build sdk/dotnet
# at runtime the native library must be resolvable, e.g.:
# LD_LIBRARY_PATH=sdk/rust/target/release dotnet run
```

## Usage

```csharp
using AgentHooks;

var emitter = new InterceptionEmitter(EnforcementMode.Enforce, resolver: null)
    .Register(new MyPolicy());
var builder = new AgentContextBuilder("my-agent", "my-fw", "s-1");

var ctx = builder.PreToolCall("tc-1", "http_get", new JsonObject { ["url"] = url });
var record = await emitter.EmitUncheckedAsync(ctx);
if (!record.Proceeds) return ToolError(record.Verdict.Reason);
// proceed with ctx["tool_call"]["args"] (post-transform)
```

`Verdict.Warn(..)` / `Verdict.Escalate(..)` are the §5 constructor
shortcuts. Run the conformance tests with
`LD_LIBRARY_PATH=../rust/target/release dotnet test`.
