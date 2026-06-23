// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
// Core types for AGENT-HOOKS-0.1 (§3, §5, §7, §8, §9, §11).
// Lifted and adapted from policy-engine/sdk/dotnet/.../Primitives.cs.

using System.Text.Json;
using System.Text.Json.Nodes;

namespace AgentHooks;

/// <summary>Spec version this SDK implements (§4.1 <c>spec</c> field).</summary>
public static class Spec
{
    public const string Version = "agent-hooks/0.1";
}

/// <summary>The closed set of agent lifecycle hook points (§3).</summary>
public enum HookPoint
{
    AgentStartup,
    Input,
    PreModelCall,
    PostModelCall,
    PreToolCall,
    PostToolCall,
    Output,
    AgentShutdown,
}

public static class HookPointExtensions
{
    public static string ToWireName(this HookPoint hp) => hp switch
    {
        HookPoint.AgentStartup => "agent_startup",
        HookPoint.Input => "input",
        HookPoint.PreModelCall => "pre_model_call",
        HookPoint.PostModelCall => "post_model_call",
        HookPoint.PreToolCall => "pre_tool_call",
        HookPoint.PostToolCall => "post_tool_call",
        HookPoint.Output => "output",
        HookPoint.AgentShutdown => "agent_shutdown",
        _ => throw new ArgumentOutOfRangeException(nameof(hp)),
    };

    public static HookPoint FromWireName(string s) => s switch
    {
        "agent_startup" => HookPoint.AgentStartup,
        "input" => HookPoint.Input,
        "pre_model_call" => HookPoint.PreModelCall,
        "post_model_call" => HookPoint.PostModelCall,
        "pre_tool_call" => HookPoint.PreToolCall,
        "post_tool_call" => HookPoint.PostToolCall,
        "output" => HookPoint.Output,
        "agent_shutdown" => HookPoint.AgentShutdown,
        _ => throw new ArgumentOutOfRangeException(nameof(s), s, "Unknown hook point"),
    };

    /// <summary>Whether a <c>transform</c> verdict is permitted at this point (§3, §4.3).</summary>
    public static bool TransformPermitted(this HookPoint hp) =>
        hp is not (HookPoint.AgentStartup or HookPoint.AgentShutdown);
}

/// <summary>Verdict decision values (§5.1).</summary>
public enum Decision { Allow, Deny, Warn, Escalate, Transform }

public static class DecisionExtensions
{
    public static string ToWireName(this Decision d) => d switch
    {
        Decision.Allow => "allow",
        Decision.Deny => "deny",
        Decision.Warn => "warn",
        Decision.Escalate => "escalate",
        Decision.Transform => "transform",
        _ => throw new ArgumentOutOfRangeException(nameof(d)),
    };

    /// <summary>Whether the action proceeds under this decision (§2 permit class).</summary>
    public static bool Permits(this Decision d) =>
        d is Decision.Allow or Decision.Warn or Decision.Transform;
}

/// <summary>Whether the host acts on verdicts (§8).</summary>
public enum EnforcementMode { Enforce, EvaluateOnly }

/// <summary>Reserved <c>hook_error:*</c> reasons a host synthesizes (§11).</summary>
public static class HookError
{
    public const string ContextInvalid = "hook_error:context_invalid";
    public const string ConsumerFailed = "hook_error:consumer_failed";
    public const string ConsumerTimeout = "hook_error:consumer_timeout";
    public const string VerdictInvalid = "hook_error:verdict_invalid";
    public const string TransformInvalid = "hook_error:transform_invalid";
    public const string TransformTargetForbidden = "hook_error:transform_target_forbidden";
    public const string ApprovalResolverMissing = "hook_error:approval_resolver_missing";
    public const string ApprovalResolverFailed = "hook_error:approval_resolver_failed";
    public const string ApprovalUnresolved = "hook_error:approval_unresolved";
    public const string ApprovalActionMismatch = "hook_error:approval_action_mismatch";
    public const string AdapterUnsupported = "hook_error:adapter_unsupported";
    public const string StreamingUnsupported = "hook_error:streaming_unsupported";
}

/// <summary>A single <c>$target</c>-rooted replacement (§5.2).</summary>
public sealed record Transform(string Path, JsonNode? Value);

/// <summary>Opaque pointer to an offline-verifiable artefact (§5.3).</summary>
public sealed record Evidence(
    string? Artefact = null,
    IReadOnlyDictionary<string, string>? VerificationPointers = null);

/// <summary>Consumer return value (§5).</summary>
public sealed record Verdict(
    Decision Decision,
    string? Reason = null,
    string? Message = null,
    Transform? Transform = null,
    Evidence? Evidence = null,
    IReadOnlyList<string>? ResultLabels = null)
{
    /// <summary>The trivial permit verdict.</summary>
    public static readonly Verdict Allow = new(Decision.Allow);

    /// <summary>Host-synthesized deny verdict for a §11 failure.</summary>
    public static Verdict FromHookError(string hookError, string? message = null) =>
        new(Decision.Deny, Reason: hookError, Message: message);

    /// <summary>Validate per §5; throws <see cref="ArgumentException"/> on violation.</summary>
    public void Validate()
    {
        if (Reason?.StartsWith("hook_error:", StringComparison.Ordinal) == true)
            throw new ArgumentException("verdict.reason MUST NOT start with 'hook_error:' (§5)");
        if (Decision == Decision.Transform && Transform is null)
            throw new ArgumentException("transform body REQUIRED when decision=='transform' (§5)");
        if (Decision != Decision.Transform && Transform is not null)
            throw new ArgumentException("transform body FORBIDDEN when decision!='transform' (§5)");
    }
}

/// <summary>Wire-shaped hook context (§4). Wraps a <see cref="JsonObject"/> so
/// it round-trips to the schema without translation; <see cref="JsonObject"/>
/// is sealed so this is composition, not inheritance.</summary>
public readonly struct HookContext(JsonObject json)
{
    public JsonObject Json { get; } = json;

    public JsonNode? this[string key] => Json[key];

    public HookPoint HookPoint => HookPointExtensions.FromWireName((string)Json["hook_point"]!);

    public static implicit operator JsonObject(HookContext ctx) => ctx.Json;
    public static implicit operator HookContext(JsonObject json) => new(json);
}

/// <summary>Host-side record of one hook evaluation (§6, §10).</summary>
public sealed record HookResult(
    HookPoint HookPoint,
    EnforcementMode Mode,
    Verdict Verdict,
    string InputIdentity,
    string EnforcedIdentity,
    JsonNode? TransformedTarget = null)
{
    /// <summary>Whether the guarded action executes (§6, §8).</summary>
    public bool Proceeds => Mode == EnforcementMode.EvaluateOnly || Verdict.Decision.Permits();
}

/// <summary>Consumer protocol (§7).</summary>
public interface IHookConsumer
{
    ValueTask<Verdict> OnHookAsync(HookContext context, CancellationToken ct = default);
}

/// <summary>Approval seam (§9).</summary>
public enum ApprovalOutcome { Approve, Reject, Unresolved }

public sealed record ApprovalRequest(
    string ContextIdentity,
    HookPoint HookPoint,
    Verdict Verdict,
    HookContext Context);

public sealed record ApprovalResolution(
    ApprovalOutcome Outcome,
    string ContextIdentity,
    Verdict? Verdict = null);

public interface IApprovalResolver
{
    ValueTask<ApprovalResolution> ResolveAsync(ApprovalRequest request, CancellationToken ct = default);
}

/// <summary>Raised by a host when a verdict blocks the guarded action (§6).</summary>
public sealed class HookBlockedException(HookResult result)
    : InvalidOperationException(
        $"{result.HookPoint.ToWireName()} blocked: {result.Verdict.Decision.ToWireName()} ({result.Verdict.Reason ?? "no reason"})")
{
    public HookResult Result { get; } = result;
}
