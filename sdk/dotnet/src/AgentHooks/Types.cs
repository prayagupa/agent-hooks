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

/// <summary>The closed set of agent lifecycle interception points (§3).</summary>
public enum InterceptionPoint
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

public static class InterceptionPointExtensions
{
    public static string ToWireName(this InterceptionPoint hp) => hp switch
    {
        InterceptionPoint.AgentStartup => "agent_startup",
        InterceptionPoint.Input => "input",
        InterceptionPoint.PreModelCall => "pre_model_call",
        InterceptionPoint.PostModelCall => "post_model_call",
        InterceptionPoint.PreToolCall => "pre_tool_call",
        InterceptionPoint.PostToolCall => "post_tool_call",
        InterceptionPoint.Output => "output",
        InterceptionPoint.AgentShutdown => "agent_shutdown",
        _ => throw new ArgumentOutOfRangeException(nameof(hp)),
    };

    public static InterceptionPoint FromWireName(string s) => s switch
    {
        "agent_startup" => InterceptionPoint.AgentStartup,
        "input" => InterceptionPoint.Input,
        "pre_model_call" => InterceptionPoint.PreModelCall,
        "post_model_call" => InterceptionPoint.PostModelCall,
        "pre_tool_call" => InterceptionPoint.PreToolCall,
        "post_tool_call" => InterceptionPoint.PostToolCall,
        "output" => InterceptionPoint.Output,
        "agent_shutdown" => InterceptionPoint.AgentShutdown,
        _ => throw new ArgumentOutOfRangeException(nameof(s), s, "Unknown interception point"),
    };

    /// <summary>Whether a <c>transform</c> verdict is permitted at this point (§3, §4.3).</summary>
    public static bool TransformPermitted(this InterceptionPoint hp) =>
        hp is not (InterceptionPoint.AgentStartup or InterceptionPoint.AgentShutdown);
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

    public static Decision FromWireName(string s) => s switch
    {
        "allow" => Decision.Allow,
        "deny" => Decision.Deny,
        "warn" => Decision.Warn,
        "escalate" => Decision.Escalate,
        "transform" => Decision.Transform,
        _ => throw new ArgumentOutOfRangeException(nameof(s), s, "Unknown decision"),
    };

    /// <summary>Whether the action proceeds under this decision (§2 permit class).</summary>
    public static bool Permits(this Decision d) =>
        d is Decision.Allow or Decision.Warn or Decision.Transform;
}

/// <summary>Whether the host acts on verdicts (§8).</summary>
public enum EnforcementMode { Enforce, EvaluateOnly }

/// <summary>Reserved <c>host_error:*</c> reasons a host synthesizes (§11).</summary>
public static class HostError
{
    public const string ContextInvalid = "host_error:context_invalid";
    public const string InterceptorFailed = "host_error:interceptor_failed";
    public const string InterceptorTimeout = "host_error:interceptor_timeout";
    public const string VerdictInvalid = "host_error:verdict_invalid";
    public const string TransformInvalid = "host_error:transform_invalid";
    public const string TransformTargetForbidden = "host_error:transform_target_forbidden";
    public const string ApprovalResolverMissing = "host_error:approval_resolver_missing";
    public const string ApprovalResolverFailed = "host_error:approval_resolver_failed";
    public const string ApprovalUnresolved = "host_error:approval_unresolved";
    public const string ApprovalActionMismatch = "host_error:approval_action_mismatch";
    public const string AdapterUnsupported = "host_error:adapter_unsupported";
    public const string StreamingUnsupported = "host_error:streaming_unsupported";
    public const string NoInterceptor = "host_error:no_interceptor";
}

/// <summary>A single <c>$target</c>-rooted replacement (§5.2).</summary>
public sealed record Transform(string Path, JsonNode? Value);

/// <summary>Opaque pointer to an offline-verifiable artefact (§5.3).</summary>
public sealed record Evidence(
    string? Artefact = null,
    IReadOnlyDictionary<string, string>? VerificationPointers = null);

/// <summary>Interceptor return value (§5).</summary>
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
    public static Verdict FromHostError(string hookError, string? message = null) =>
        new(Decision.Deny, Reason: hookError, Message: message);

    /// <summary>Validate per §5; throws <see cref="ArgumentException"/> on violation.</summary>
    public void Validate()
    {
        if (Reason?.StartsWith("host_error:", StringComparison.Ordinal) == true)
            throw new ArgumentException("verdict.reason MUST NOT start with 'host_error:' (§5)");
        if (Decision == Decision.Transform && Transform is null)
            throw new ArgumentException("transform body REQUIRED when decision=='transform' (§5)");
        if (Decision != Decision.Transform && Transform is not null)
            throw new ArgumentException("transform body FORBIDDEN when decision!='transform' (§5)");
    }

    /// <summary>Serialize to the wire shape the Rust core consumes.</summary>
    public JsonObject ToWire()
    {
        var o = new JsonObject { ["decision"] = Decision.ToWireName() };
        if (Reason is not null) o["reason"] = Reason;
        if (Message is not null) o["message"] = Message;
        if (Transform is not null)
            o["transform"] = new JsonObject
            {
                ["path"] = Transform.Path,
                ["value"] = Transform.Value?.DeepClone(),
            };
        if (Evidence is not null)
        {
            var e = new JsonObject();
            if (Evidence.Artefact is not null) e["artefact"] = Evidence.Artefact;
            if (Evidence.VerificationPointers is { Count: > 0 })
            {
                var vp = new JsonObject();
                foreach (var (k, v) in Evidence.VerificationPointers) vp[k] = v;
                e["verification_pointers"] = vp;
            }
            o["evidence"] = e;
        }
        if (ResultLabels is { Count: > 0 })
            o["result_labels"] = new JsonArray(ResultLabels.Select(l => (JsonNode)l).ToArray());
        return o;
    }

    /// <summary>Reconstruct from a wire-shaped verdict object.
    /// Permissive: accepts host_error:* reasons (used for records emitted by the core).</summary>
    public static Verdict FromWire(JsonObject o)
    {
        Transform? t = null;
        if (o["transform"] is JsonObject to)
            t = new Transform((string)to["path"]!, to["value"]?.DeepClone());
        Evidence? ev = null;
        if (o["evidence"] is JsonObject eo)
            ev = new Evidence(
                (string?)eo["artefact"],
                (eo["verification_pointers"] as JsonObject)?
                    .ToDictionary(kv => kv.Key, kv => (string)kv.Value!));
        var labels = (o["result_labels"] as JsonArray)?
            .Select(n => (string)n!).ToList();
        return new Verdict(
            DecisionExtensions.FromWireName((string)o["decision"]!),
            (string?)o["reason"],
            (string?)o["message"],
            t, ev, labels);
    }
}

/// <summary>Wire-shaped agent context (§4). Wraps a <see cref="JsonObject"/> so
/// it round-trips to the schema without translation; <see cref="JsonObject"/>
/// is sealed so this is composition, not inheritance.</summary>
public readonly struct AgentContext(JsonObject json)
{
    public JsonObject Json { get; } = json;

    public JsonNode? this[string key] => Json[key];

    public InterceptionPoint InterceptionPoint => InterceptionPointExtensions.FromWireName((string)Json["interception_point"]!);

    public static implicit operator JsonObject(AgentContext ctx) => ctx.Json;
    public static implicit operator AgentContext(JsonObject json) => new(json);
}

/// <summary>Host-side record of one interception (§6, §10).
///
/// Identity-only by design: the identities bind the record to the exact
/// pre/post-fold context without duplicating the (possibly sensitive)
/// payload into audit storage. Hosts that need the raw transformed value
/// log it at the callsite.</summary>
public sealed record InterceptionRecord(
    InterceptionPoint InterceptionPoint,
    EnforcementMode Mode,
    Verdict Verdict,
    string InputIdentity,
    string EnforcedIdentity)
{
    /// <summary>Whether the guarded action executes (§6, §8).</summary>
    public bool Proceeds => Mode == EnforcementMode.EvaluateOnly || Verdict.Decision.Permits();
}

/// <summary>Interceptor protocol (§7).</summary>
public interface IInterceptor
{
    ValueTask<Verdict> InterceptAsync(AgentContext context, CancellationToken ct = default);
}

/// <summary>Approval seam (§9).</summary>
public enum ApprovalOutcome { Approve, Reject, Unresolved }

public sealed record ApprovalRequest(
    string ContextIdentity,
    InterceptionPoint InterceptionPoint,
    Verdict Verdict,
    AgentContext Context);

public sealed record ApprovalResolution(
    ApprovalOutcome Outcome,
    string ContextIdentity,
    Verdict? Verdict = null);

public interface IApprovalResolver
{
    ValueTask<ApprovalResolution> ResolveAsync(ApprovalRequest request, CancellationToken ct = default);
}

/// <summary>Raised by a host when a verdict blocks the guarded action (§6).</summary>
public sealed class InterceptionBlockedException(InterceptionRecord result)
    : InvalidOperationException(
        $"{result.InterceptionPoint.ToWireName()} blocked: {result.Verdict.Decision.ToWireName()} ({result.Verdict.Reason ?? "no reason"})")
{
    public InterceptionRecord Result { get; } = result;
}
