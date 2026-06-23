// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
// Conformance Test Kit harness contract (§13.2).
//
// The .NET CTK runner is not yet implemented; this assembly defines IHarness
// so framework adapters can be written now. Track the runner at
// https://github.com/responsibleai/agent-hooks/issues/4; the Python
// implementation at sdk/python/src/agent_hooks/ctk/runner.py is the reference.

using System.Text.Json.Nodes;

namespace AgentHooks.Conformance;

/// <summary>Host-declared capability subset (§3.2).</summary>
public enum Capability { ModelCalls, ToolCalls, ParallelToolCalls, Streaming, MultiTurn }

public enum RunOutcome { Completed, Blocked, Suspended, Error }

/// <summary>Hermetic scripted run loaded from a CTK vector (wire-shaped).</summary>
public sealed record Scenario(
    JsonObject Input,
    IReadOnlyList<JsonObject> Tools,
    IReadOnlyList<JsonObject> ModelScript);

/// <summary>What <see cref="IHarness.RunAsync"/> returns to the CTK runner.</summary>
public sealed record RunRecord(
    RunOutcome Outcome,
    JsonNode? FinalOutput,
    IReadOnlyList<JsonObject> ToolInvocations,
    string? Error = null);

/// <summary>The single interface a framework adapter implements for the CTK.</summary>
public interface IHarness
{
    string Name { get; }
    IReadOnlySet<Capability> Capabilities { get; }

    void Setup(
        Scenario scenario,
        IHookConsumer consumer,
        IApprovalResolver? resolver,
        EnforcementMode mode);

    Task<RunRecord> RunAsync(CancellationToken ct = default);

    void Teardown();
}
