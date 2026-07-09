// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
// CTK self-test: run all vectors against the in-tree
// ReferenceHarness. Assertion engine is the Rust core; this proves
// the .NET emitter, builder, runner, and harness wire correctly.

using System.Text.Json.Nodes;
using AgentHooks.Conformance;
using Xunit;

namespace AgentHooks.Tests;

public sealed class CtkReferenceTests
{
    /// <summary>TODO(stage-4): vectors still authored in the pre-P-003
    /// five-verdict wire vocabulary (<c>warn</c>, <c>escalate</c>,
    /// <c>approval_resolver_missing</c>). Stage 4 rewrites them to the
    /// three-verdict shapes (§5.1); until then they fail the §5 gate by
    /// design (fail closed) — or pass through the wrong mechanism (the
    /// stale <c>escalate</c> becomes a <c>verdict_invalid</c> deny that
    /// happens to satisfy an expected "blocked") — and are excluded here,
    /// and ONLY here.</summary>
    private static readonly string[] TodoStage4 =
    [
        "AH-CTK-030", // escalate-approve → deny+approval / resolution
        "AH-CTK-031", // escalate-reject → deny+approval / reject
        "AH-CTK-032", // escalate-no-resolver → liftable deny stands
        "AH-CTK-050", // warn-passthrough → allow+warnings
        "AH-CTK-072", // approval echo rule → approval_identity_mismatch
        "AH-CTK-073", // approval unresolved → approval_unresolved
    ];

    private static string VectorsDir()
    {
        var here = Path.GetDirectoryName(typeof(CtkReferenceTests).Assembly.Location)!;
        var root = Path.GetFullPath(Path.Combine(here, "..", "..", "..", "..", "..", "..", ".."));
        return Path.Combine(root, "conformance", "vectors");
    }

    public static IEnumerable<object[]> Vectors() =>
        Runner.LoadVectors(VectorsDir())
              .Select(v => new object[] { (string)v["id"]!, v });

    [Theory]
    [MemberData(nameof(Vectors))]
    public async Task ReferenceHarnessConformance(string id, JsonObject vector)
    {
        if (TodoStage4.Any(id.StartsWith))
        {
            // xUnit has no runtime Skip; assert-pass with a diagnostic instead.
            Assert.True(true, $"skipped: TODO(stage-4) pre-P-003 vector {id}");
            return;
        }
        var result = await Runner.RunVectorAsync(new ReferenceHarness(), vector);
        if (result.Status == "skip")
        {
            // xUnit has no runtime Skip; assert-pass with a diagnostic instead.
            Assert.True(true, $"skipped: {result.Detail}");
            return;
        }
        Assert.True(
            result.Status == "pass",
            $"[{result.Id}] {result.Title}\n" +
            string.Join("\n", result.Failures.Select(f => $"  - {f}")));
    }
}
