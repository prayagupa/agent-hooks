// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
// CTK runner: load vectors, drive an IHarness, assert `expect`.
//
// The assertion engine, capability skip check, and scripted
// interceptor/resolver evaluation live in the Rust core (Native.Ctk*).
// This class keeps only vector globbing, the harness setup/run/teardown
// orchestration (native callbacks), and RunRecord marshalling.
// See conformance/RUNNER.md.

using System.Text.Json;
using System.Text.Json.Nodes;

namespace AgentHooks.Conformance;

public sealed record VectorResult(
    string Id, string Title, int Level, string Status,
    string Detail, IReadOnlyList<string> Failures);

public static class Runner
{
    private static readonly JsonSerializerOptions Compact = new() { WriteIndented = false };

    public static IEnumerable<JsonObject> LoadVectors(string directory, int maxLevel = 3)
    {
        foreach (var f in Directory.EnumerateFiles(directory, "AH-CTK-*.json").OrderBy(p => p))
        {
            var v = (JsonObject)JsonNode.Parse(File.ReadAllText(f))!;
            if ((int)v["level"]! <= maxLevel) yield return v;
        }
    }

    public static async Task<VectorResult> RunVectorAsync(
        IHarness harness, JsonObject vector, CancellationToken ct = default)
    {
        var id = (string)vector["id"]!;
        var title = (string)vector["title"]!;
        var level = (int)vector["level"]!;
        var vectorJson = vector.ToJsonString(Compact);

        // Capability skip via core.
        var caps = new JsonArray(
            harness.Capabilities.Select(c => (JsonNode)c.ToWireName()).ToArray());
        var skip = JsonNode.Parse(
            Native.CtkShouldSkip(vectorJson, caps.ToJsonString(Compact)));
        if (skip is JsonValue sv && sv.TryGetValue(out string? reason))
            return new VectorResult(id, title, level, "skip", reason!, []);

        var scenario = Scenario.FromWire((JsonObject)vector["scenario"]!);
        var rulesJson = (vector["interceptor_script"] ?? new JsonArray()).ToJsonString(Compact);
        var recorded = new List<JsonObject>();
        var interceptor = new RecordingScriptedInterceptor(rulesJson, recorded);

        var approval = vector["approval_script"] as JsonArray;
        IApprovalResolver? resolver = approval is { Count: > 0 }
            ? new ScriptedResolver(approval.ToJsonString(Compact))
            : null;

        var mode = (string?)vector["mode"] == "evaluate_only"
            ? EnforcementMode.EvaluateOnly : EnforcementMode.Enforce;

        harness.Setup(scenario, interceptor, resolver, mode);
        RunRecord rr;
        try
        {
            rr = await harness.RunAsync(ct);
        }
        catch (Exception e)
        {
            return new VectorResult(id, title, level, "fail", "",
                [$"harness.RunAsync raised: {e}"]);
        }
        finally
        {
            harness.Teardown();
        }

        var recordedJson = new JsonArray(
            recorded.Select(c => (JsonNode)c).ToArray()).ToJsonString(Compact);
        var rrJson = RunRecordToWire(rr);
        var result = (JsonObject)JsonNode.Parse(
            Native.CtkAssert(vectorJson, recordedJson, rrJson))!;
        return new VectorResult(
            (string)result["id"]!,
            (string)result["title"]!,
            (int)result["level"]!,
            (string)result["status"]!,
            (string?)result["detail"] ?? "",
            (result["failures"] as JsonArray)?.Select(n => (string)n!).ToList() ?? []);
    }

    private static string RunRecordToWire(RunRecord rr)
    {
        var identities = new JsonArray();
        foreach (var (i, e) in rr.Identities ?? [])
            identities.Add(new JsonObject
            {
                ["input_identity"] = i, ["enforced_identity"] = e,
            });
        var o = new JsonObject
        {
            ["outcome"] = rr.Outcome.ToWireName(),
            ["final_output"] = rr.FinalOutput?.DeepClone(),
            ["tool_invocations"] = new JsonArray(
                rr.ToolInvocations.Select(t => (JsonNode)t.DeepClone()).ToArray()),
            ["error"] = rr.Error,
            ["identities"] = identities,
        };
        return o.ToJsonString(Compact);
    }

    /// <summary>Records every ctx (deep copy) then replays the vector's
    /// interceptor_script via the Rust core.</summary>
    private sealed class RecordingScriptedInterceptor(
        string rulesJson, List<JsonObject> recorded) : IInterceptor
    {
        public ValueTask<Verdict> InterceptAsync(AgentContext ctx, CancellationToken ct = default)
        {
            recorded.Add((JsonObject)ctx.Json.DeepClone());
            var w = (JsonObject)JsonNode.Parse(
                Native.CtkScriptedIntercept(rulesJson, ctx.Json.ToJsonString(Compact)))!;
            return ValueTask.FromResult(Verdict.FromWire(w));
        }
    }

    /// <summary>Replays the vector's approval_script via the Rust core.</summary>
    private sealed class ScriptedResolver(string rulesJson) : IApprovalResolver
    {
        public ValueTask<ApprovalResolution> ResolveAsync(
            ApprovalRequest req, CancellationToken ct = default)
        {
            var r = (JsonObject)JsonNode.Parse(
                Native.CtkScriptedResolve(
                    rulesJson, req.Context.Json.ToJsonString(Compact), req.ContextIdentity))!;
            var outcome = (string)r["outcome"]! switch
            {
                "approve" => ApprovalOutcome.Approve,
                "reject" => ApprovalOutcome.Reject,
                _ => ApprovalOutcome.Unresolved,
            };
            var v = r["verdict"] is JsonObject vw ? Verdict.FromWire(vw) : null;
            return ValueTask.FromResult(
                new ApprovalResolution(outcome, (string)r["context_identity"]!, v));
        }
    }
}
