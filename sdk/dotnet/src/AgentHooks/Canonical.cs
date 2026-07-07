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

    /// <summary>§7.1 fold-through: apply one transform to the context's
    /// target (and its L1 alias). Returns the updated context. Rust core.</summary>
    public static JsonObject ApplyTransformCtx(AgentContext ctx, string path, JsonNode? value) =>
        (JsonObject)JsonNode.Parse(Native.ApplyTransformCtx(
            ctx.Json.ToJsonString(Compact),
            path,
            value?.ToJsonString(Compact) ?? "null"))!;

    /// <summary>§8 <c>evaluate_only</c>: validate a transform against the
    /// context's target without applying it. Rust core.</summary>
    public static void ValidateTransformCtx(AgentContext ctx, string path, JsonNode? value) =>
        Native.ValidateTransformCtx(
            ctx.Json.ToJsonString(Compact),
            path,
            value?.ToJsonString(Compact) ?? "null");

    /// <summary>§6/§10: build the <c>InterceptionRecord</c> for one completed
    /// interception. Rust core.</summary>
    public static JsonObject Finalize(
        AgentContext ctx, JsonNode verdict, EnforcementMode mode, string inputIdentity,
        int? decidedBy = null) =>
        (JsonObject)JsonNode.Parse(Native.Finalize(
            ctx.Json.ToJsonString(Compact),
            verdict.ToJsonString(Compact),
            mode == EnforcementMode.Enforce ? "enforce" : "evaluate_only",
            inputIdentity,
            decidedBy ?? -1))!;
}
