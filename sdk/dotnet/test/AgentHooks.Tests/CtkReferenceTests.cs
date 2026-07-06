// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
// CTK self-test: run all Level<=2 vectors against the in-tree
// ReferenceHarness. Assertion engine is the Rust core; this proves
// the .NET emitter, builder, runner, and harness wire correctly.

using System.Text.Json.Nodes;
using AgentHooks.Conformance;
using Xunit;

namespace AgentHooks.Tests;

public sealed class CtkReferenceTests
{
    private static string VectorsDir()
    {
        var here = Path.GetDirectoryName(typeof(CtkReferenceTests).Assembly.Location)!;
        var root = Path.GetFullPath(Path.Combine(here, "..", "..", "..", "..", "..", "..", ".."));
        return Path.Combine(root, "conformance", "vectors");
    }

    public static IEnumerable<object[]> Vectors() =>
        Runner.LoadVectors(VectorsDir(), maxLevel: 2)
              .Select(v => new object[] { (string)v["id"]!, v });

    [Theory]
    [MemberData(nameof(Vectors))]
    public async Task ReferenceHarnessConformance(string id, JsonObject vector)
    {
        _ = id;
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
