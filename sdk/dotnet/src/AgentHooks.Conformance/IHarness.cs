// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
// Conformance Test Kit harness contract (§13.2).
//
// The CTK engine (scripted interceptor/resolver, capability skip,
// assertion pass) lives in the Rust core and is exposed via
// Native.Ctk*. This assembly provides the per-language glue: the
// IHarness contract framework authors implement, a Runner that drives
// it, and a ReferenceHarness for CTK self-test.

using System.Text.Json.Nodes;

namespace AgentHooks.Conformance;

/// <summary>Host-declared capability subset (§3.2).</summary>
public enum Capability { ModelCalls, ToolCalls, ParallelToolCalls, Streaming, MultiTurn }

public static class CapabilityExtensions
{
    public static string ToWireName(this Capability c) => c switch
    {
        Capability.ModelCalls => "model_calls",
        Capability.ToolCalls => "tool_calls",
        Capability.ParallelToolCalls => "parallel_tool_calls",
        Capability.Streaming => "streaming",
        Capability.MultiTurn => "multi_turn",
        _ => throw new ArgumentOutOfRangeException(nameof(c)),
    };
}

public enum RunOutcome { Completed, Blocked, Suspended, Error }

public static class RunOutcomeExtensions
{
    public static string ToWireName(this RunOutcome o) => o switch
    {
        RunOutcome.Completed => "completed",
        RunOutcome.Blocked => "blocked",
        RunOutcome.Suspended => "suspended",
        RunOutcome.Error => "error",
        _ => throw new ArgumentOutOfRangeException(nameof(o)),
    };
}

/// <summary>One scripted mock-tool.</summary>
public sealed record ToolSpec(string Name, IReadOnlyList<JsonObject> Behavior)
{
    /// <summary>First matching behaviour clause wins.</summary>
    public (JsonNode? Value, bool IsError) Invoke(JsonObject args)
    {
        foreach (var b in Behavior)
        {
            var when = b["when_args"] as JsonObject;
            if (when is null || JsonNode.DeepEquals(when, args))
                return (b["return"]?.DeepClone(),
                        (bool?)b["is_error"] ?? false);
        }
        throw new InvalidOperationException(
            $"tool {Name!}: no behaviour clause matched {args.ToJsonString()}");
    }
}

/// <summary>One scripted model response.</summary>
public sealed record ModelResponse(
    JsonNode? Content, JsonArray ToolCalls, string FinishReason);

/// <summary>Hermetic scripted run loaded from a CTK vector.</summary>
public sealed record Scenario(
    JsonObject Input,
    IReadOnlyDictionary<string, ToolSpec> Tools,
    IReadOnlyList<ModelResponse> ModelScript)
{
    public static Scenario FromWire(JsonObject o)
    {
        var tools = new Dictionary<string, ToolSpec>();
        foreach (var t in (o["tools"] as JsonArray) ?? [])
        {
            var to = (JsonObject)t!;
            var behavior = ((JsonArray)to["behavior"]!)
                .Select(b => (JsonObject)b!.DeepClone()).ToList();
            tools[(string)to["name"]!] = new ToolSpec((string)to["name"]!, behavior);
        }
        var script = new List<ModelResponse>();
        foreach (var m in (o["model_script"] as JsonArray) ?? [])
        {
            var r = (JsonObject)m!["respond"]!;
            script.Add(new ModelResponse(
                r["content"]?.DeepClone(),
                (JsonArray)r["tool_calls"]!.DeepClone(),
                (string)r["finish_reason"]!));
        }
        return new Scenario((JsonObject)o["input"]!.DeepClone(), tools, script);
    }
}

/// <summary>What <see cref="IHarness.RunAsync"/> returns to the CTK runner.</summary>
public sealed record RunRecord(
    RunOutcome Outcome,
    JsonNode? FinalOutput,
    IReadOnlyList<JsonObject> ToolInvocations,
    string? Error = null,
    IReadOnlyList<(string InputIdentity, string EnforcedIdentity)>? Identities = null);

/// <summary>The single interface a framework adapter implements for the CTK.</summary>
public interface IHarness
{
    string Name { get; }
    IReadOnlySet<Capability> Capabilities { get; }

    void Setup(
        Scenario scenario,
        IReadOnlyList<IInterceptor> interceptors,
        IApprovalResolver? resolver,
        EnforcementMode mode);

    Task<RunRecord> RunAsync(CancellationToken ct = default);

    void Teardown();
}
