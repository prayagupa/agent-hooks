// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
// Canonical JSON serialization and context identity (§10).
//
// Delegates to the Rust core via libagent_hooks_ffi so every SDK
// produces byte-identical output. The pure-C# implementation was
// removed once the core became canonical (see
// sdk/rust/core/src/canonical.rs).

using System.Text.Json;
using System.Text.Json.Nodes;

namespace AgentHooks;

public static class Canonical
{
    private static readonly JsonSerializerOptions Compact = new() { WriteIndented = false };

    /// <summary>Serialize per §10.1. Implemented by the Rust core.</summary>
    public static string Json(JsonNode? node) =>
        Native.CanonicalJson(node?.ToJsonString(Compact) ?? "null");

    /// <summary><c>"sha256:" + hex(SHA-256(Json(ctx_L01)))</c> (§10.2). Rust core.</summary>
    public static string ContextIdentity(AgentContext ctx) =>
        Native.ContextIdentity(ctx.Json.ToJsonString(Compact));

    /// <summary>§5: validate an interceptor's wire return. Rust core.</summary>
    public static void ValidateVerdict(JsonNode verdict) =>
        Native.ValidateVerdict(verdict.ToJsonString(Compact));

    /// <summary>§5.2: apply a <c>$target</c>-rooted transform. Rust core.</summary>
    public static JsonNode? ApplyTransform(JsonNode? target, string path, JsonNode? value) =>
        JsonNode.Parse(Native.ApplyTransform(
            target?.ToJsonString(Compact) ?? "null",
            path,
            value?.ToJsonString(Compact) ?? "null"));

    /// <summary>§7.1: combine an ordered array of verdicts. Rust core.</summary>
    public static JsonNode CombineVerdicts(JsonArray verdicts) =>
        JsonNode.Parse(Native.CombineVerdicts(verdicts.ToJsonString(Compact)))!;

    /// <summary>§6/§8/§10: enforcement step. Returns <c>{record, ctx}</c>. Rust core.</summary>
    public static JsonNode Enforce(AgentContext ctx, JsonNode verdict, EnforcementMode mode) =>
        JsonNode.Parse(Native.Enforce(
            ctx.Json.ToJsonString(Compact),
            verdict.ToJsonString(Compact),
            mode == EnforcementMode.Enforce ? "enforce" : "evaluate_only"))!;
}
