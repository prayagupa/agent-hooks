// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
// Reference in-memory Level-2 host. Self-test target for the CTK.
// Port of sdk/python/python/agent_hooks/ctk/reference.py.

using System.Text.Json.Nodes;

namespace AgentHooks.Conformance;

public sealed class ReferenceHarness : IHarness
{
    public string Name => "reference-agent";
    public IReadOnlySet<Capability> Capabilities { get; } =
        new HashSet<Capability> { Capability.ModelCalls, Capability.ToolCalls };

    private Scenario? _scenario;
    private InterceptionEmitter? _emitter;
    private AgentContextBuilder? _builder;
    private readonly List<JsonObject> _toolLog = [];

    public void Setup(
        Scenario scenario, IInterceptor interceptor,
        IApprovalResolver? resolver, EnforcementMode mode)
    {
        _scenario = scenario;
        _toolLog.Clear();
        _emitter = new InterceptionEmitter(mode, resolver).Register(interceptor);
        _builder = new AgentContextBuilder(
            agentId: "ref-agent",
            framework: "reference-agent",
            sessionId: Guid.NewGuid().ToString());
    }

    public async Task<RunRecord> RunAsync(CancellationToken ct = default)
    {
        var s = _scenario!; var em = _emitter!; var b = _builder!;
        var outcome = RunOutcome.Completed;
        JsonNode? final = null;
        try
        {
            await em.EmitOrThrowAsync(
                b.AgentStartup(s.Tools.Keys.OrderBy(k => k)), ct);
            await em.EmitOrThrowAsync(
                b.Input(s.Input["content"]?.DeepClone(), (string)s.Input["role"]!), ct);

            var messages = new JsonArray(new JsonObject
            {
                ["role"] = (string)s.Input["role"]!,
                ["content"] = s.Input["content"]?.DeepClone(),
            });

            foreach (var resp in s.ModelScript)
            {
                var pre = b.PreModelCall("mock", (JsonArray)messages.DeepClone());
                await em.EmitOrThrowAsync(pre, ct);
                messages = (JsonArray)pre.Json["messages"]!.DeepClone(); // may be transformed

                await em.EmitOrThrowAsync(
                    b.PostModelCall("mock", resp.Content?.DeepClone(),
                        (JsonArray)resp.ToolCalls.DeepClone(), resp.FinishReason), ct);

                if (resp.ToolCalls.Count > 0)
                {
                    foreach (var tc in resp.ToolCalls.Cast<JsonObject>())
                    {
                        try
                        {
                            await DoToolCallAsync(tc, messages, ct);
                        }
                        catch (InterceptionBlockedException e)
                        {
                            messages.Add(new JsonObject
                            {
                                ["role"] = "tool",
                                ["content"] = $"blocked: {e.Result.Verdict.Reason}",
                            });
                        }
                    }
                    messages.Add(new JsonObject
                    {
                        ["role"] = "assistant",
                        ["content"] = resp.Content?.DeepClone() ?? "",
                    });
                }
                else
                {
                    final = resp.Content?.DeepClone();
                    break;
                }
            }

            if (final is not null)
            {
                var outCtx = b.Output(final);
                await em.EmitOrThrowAsync(outCtx, ct);
                final = outCtx.Json["output"]?["content"]?.DeepClone();
            }
        }
        catch (InterceptionBlockedException)
        {
            outcome = RunOutcome.Blocked;
            final = null;
        }

        await em.EmitAsync(
            b.AgentShutdown(outcome == RunOutcome.Completed ? "completed" : "error"), ct);

        return new RunRecord(
            outcome,
            final,
            _toolLog.Select(t => (JsonObject)t.DeepClone()).ToList(),
            Identities: em.Records
                .Select(r => (r.InputIdentity, r.EnforcedIdentity))
                .ToList());
    }

    public void Teardown()
    {
        _scenario = null; _emitter = null; _builder = null;
    }

    private async Task DoToolCallAsync(JsonObject tc, JsonArray messages, CancellationToken ct)
    {
        var s = _scenario!; var em = _emitter!; var b = _builder!;
        var callId = (string)tc["id"]!;
        var name = (string)tc["name"]!;
        var args = (JsonObject)tc["args"]!.DeepClone();

        var pre = b.PreToolCall(callId, name, args);
        await em.EmitOrThrowAsync(pre, ct);
        var postArgs = (JsonObject)pre.Json["tool_call"]!["args"]!.DeepClone(); // post-transform

        var (value, isError) = s.Tools[name].Invoke(postArgs);
        _toolLog.Add(new JsonObject
        {
            ["name"] = name, ["args"] = postArgs.DeepClone(),
        });

        await em.EmitOrThrowAsync(
            b.PostToolCall(callId, name, (JsonObject)postArgs.DeepClone(),
                value, isError), ct);

        messages.Add(new JsonObject
        {
            ["role"] = "tool", ["content"] = value?.DeepClone(),
        });
    }
}
