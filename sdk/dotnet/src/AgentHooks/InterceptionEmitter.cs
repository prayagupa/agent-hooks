// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
// Host-side emitter: dispatch context -> interceptors -> verdict -> enforce (§6-§9).
//
// Interceptor dispatch (§7) and approval-seam resolution (§9) stay in
// C# because they call back into user code. Verdict validation (§5),
// combination (§7.1), transform application (§5.2), identity
// computation (§10), and target write-back (§4.3) delegate to the Rust
// core via Native.Enforce so behaviour is byte-identical across SDKs.
// Port of sdk/python/python/agent_hooks/emitter.py.

using System.Text.Json;
using System.Text.Json.Nodes;

namespace AgentHooks;

/// <summary>Host-side helper that implements §6–§9 once so adapters don't have to.</summary>
public sealed class InterceptionEmitter
{
    private static readonly JsonSerializerOptions Compact = new() { WriteIndented = false };

    private readonly List<IInterceptor> _interceptors = [];
    private readonly List<InterceptionRecord> _records = [];
    private readonly IApprovalResolver? _resolver;
    private readonly EnforcementMode _mode;

    public InterceptionEmitter(
        EnforcementMode mode = EnforcementMode.Enforce,
        IApprovalResolver? resolver = null)
    {
        _mode = mode;
        _resolver = resolver;
    }

    public EnforcementMode Mode => _mode;

    /// <summary>All interception records emitted so far in this session, in order.</summary>
    public IReadOnlyList<InterceptionRecord> Records => _records;

    public InterceptionEmitter Register(IInterceptor interceptor)
    {
        _interceptors.Add(interceptor);
        return this;
    }

    /// <summary>Emit one interception. Mutates <paramref name="ctx"/>.Json in
    /// place on transform (target + aliased L1 field rewritten).</summary>
    public async ValueTask<InterceptionRecord> EmitAsync(
        AgentContext ctx, CancellationToken ct = default)
    {
        var ip = ctx.InterceptionPoint;

        // §7 dispatch (native — calls user code) + §5/§7.1 (core).
        var verdict = await DispatchAsync(ctx, ct);

        // §9 approval seam (native — calls user code).
        if (verdict.Decision == Decision.Escalate && _mode == EnforcementMode.Enforce)
        {
            var inputId = Native.ContextIdentity(ctx.Json.ToJsonString(Compact));
            verdict = await ResolveEscalateAsync(ip, ctx, verdict, inputId, ct);
        }

        // §6/§8/§10 enforcement (core). Returns {record, ctx}.
        var outJson = Native.Enforce(
            ctx.Json.ToJsonString(Compact),
            verdict.ToWire().ToJsonString(Compact),
            _mode == EnforcementMode.Enforce ? "enforce" : "evaluate_only");
        var outObj = (JsonObject)JsonNode.Parse(outJson)!;

        // Write the (possibly transformed) ctx back into the caller's object
        // so the adapter reads the post-transform target/L1 field.
        var newCtx = (JsonObject)outObj["ctx"]!;
        ctx.Json.Clear();
        foreach (var (k, v) in newCtx.ToList()) ctx.Json[k] = v?.DeepClone();

        var record = RecordFromCore((JsonObject)outObj["record"]!);
        _records.Add(record);
        return record;
    }

    /// <summary><see cref="EmitAsync"/>, then throw
    /// <see cref="InterceptionBlockedException"/> if the action must halt.</summary>
    public async ValueTask<InterceptionRecord> EmitOrThrowAsync(
        AgentContext ctx, CancellationToken ct = default)
    {
        var record = await EmitAsync(ctx, ct);
        if (!record.Proceeds) throw new InterceptionBlockedException(record);
        return record;
    }

    // -------------------------------------------------------------------------

    private async ValueTask<Verdict> DispatchAsync(AgentContext ctx, CancellationToken ct)
    {
        var wire = new JsonArray();
        foreach (var i in _interceptors)
        {
            JsonObject w;
            try
            {
                var v = await i.InterceptAsync(ctx, ct);
                w = v.ToWire();
                // §5 validation via core; throws on violation.
                Native.ValidateVerdict(w.ToJsonString(Compact));
            }
            catch (AgentHooksCoreException e)
            {
                return Verdict.FromHostError(e.Code, e.Message);
            }
            catch (Exception e) // fail closed per §6.3
            {
                return Verdict.FromHostError(HostError.InterceptorFailed, e.ToString());
            }
            wire.Add(w);
            // §7.1.2 short-circuit on block.
            var d = (string)w["decision"]!;
            if (d is "deny" or "escalate") break;
        }
        var combined = (JsonObject)JsonNode.Parse(
            Native.CombineVerdicts(wire.ToJsonString(Compact)))!;
        return Verdict.FromWire(combined);
    }

    private async ValueTask<Verdict> ResolveEscalateAsync(
        InterceptionPoint ip, AgentContext ctx, Verdict verdict, string identity,
        CancellationToken ct)
    {
        if (_resolver is null)
            return Verdict.FromHostError(HostError.ApprovalResolverMissing);
        ApprovalResolution res;
        try
        {
            res = await _resolver.ResolveAsync(
                new ApprovalRequest(identity, ip, verdict, ctx), ct);
        }
        catch (Exception e)
        {
            return Verdict.FromHostError(HostError.ApprovalResolverFailed, e.ToString());
        }
        if (res.ContextIdentity != identity)
            return Verdict.FromHostError(HostError.ApprovalActionMismatch);
        if (res.Outcome == ApprovalOutcome.Unresolved || res.Verdict is null)
            return Verdict.FromHostError(HostError.ApprovalUnresolved);
        return res.Verdict;
    }

    private static InterceptionRecord RecordFromCore(JsonObject r)
    {
        var vw = (JsonObject)r["verdict"]!;
        return new InterceptionRecord(
            InterceptionPointExtensions.FromWireName((string)r["interception_point"]!),
            (string)r["mode"]! == "enforce" ? EnforcementMode.Enforce : EnforcementMode.EvaluateOnly,
            Verdict.FromWire(vw),
            (string)r["input_identity"]!,
            (string)r["enforced_identity"]!,
            r["transformed_target"]?.DeepClone());
    }
}
