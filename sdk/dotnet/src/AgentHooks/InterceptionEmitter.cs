// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
// Host-side emitter: dispatch context -> interceptors -> verdict -> record (§6-§9).
//
// Interceptor dispatch (§7) and approval-seam resolution (§9) stay in
// C# because they call back into user code. Verdict validation (§5),
// transform fold-through (§7.1), identity computation (§10), and target
// write-back (§4.3) delegate to the Rust core so behaviour is
// byte-identical across SDKs. Port of
// sdk/python/python/agent_hooks/emitter.py.
//
// §7.1 sequential fold-through: interceptors run in registration order;
// each receives a deep copy of the context as it stands *after* prior
// transforms were applied. The first block verdict short-circuits.
//
// Fail-closed defaults: an enforce-mode emission with zero registered
// interceptors yields deny host_error:no_interceptor (§7), and EmitAsync
// THROWS InterceptionBlockedException on any block — the ignorable-result
// variant is the explicitly named EmitUncheckedAsync.

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

    /// <summary>Run the interception and THROW
    /// <see cref="InterceptionBlockedException"/> if the guarded action must
    /// not proceed (§6). This is the primary entry point; the safe path is
    /// the default.</summary>
    public async ValueTask<InterceptionRecord> EmitAsync(
        AgentContext ctx, CancellationToken ct = default)
    {
        var record = await EmitUncheckedAsync(ctx, ct);
        if (!record.Proceeds) throw new InterceptionBlockedException(record);
        return record;
    }

    /// <summary>Run the interception and return the record without throwing.
    /// The caller MUST inspect <see cref="InterceptionRecord.Proceeds"/> and
    /// halt the guarded action itself; prefer <see cref="EmitAsync"/>.</summary>
    public async ValueTask<InterceptionRecord> EmitUncheckedAsync(
        AgentContext ctx, CancellationToken ct = default)
    {
        // §10.2: input identity binds to the context BEFORE dispatch, so
        // neither interceptor mutation nor fold-through can retroactively
        // alter what the record claims was evaluated.
        var inputId = Native.ContextIdentity(ctx.Json.ToJsonString(Compact));

        var verdict = await DispatchAsync(ctx, ct);

        if (verdict.Decision == Decision.Escalate && _mode == EnforcementMode.Enforce)
        {
            verdict = await ResolveEscalateAsync(ctx.InterceptionPoint, ctx, verdict, inputId, ct);
            // An approve MAY carry a transform (§9); it is subject to the
            // same fold rules as an interceptor transform.
            if (verdict.Decision == Decision.Transform)
                verdict = FoldTransform(ctx, verdict);
        }

        var recordJson = Native.Finalize(
            ctx.Json.ToJsonString(Compact),
            verdict.ToWire().ToJsonString(Compact),
            _mode == EnforcementMode.Enforce ? "enforce" : "evaluate_only",
            inputId);
        var record = RecordFromCore((JsonObject)JsonNode.Parse(recordJson)!);
        _records.Add(record);
        return record;
    }

    // -------------------------------------------------------------------------

    private async ValueTask<Verdict> DispatchAsync(AgentContext ctx, CancellationToken ct)
    {
        if (_interceptors.Count == 0)
        {
            // §7: zero interceptors fails closed. Register an explicit
            // allow-all interceptor for a deliberate passthrough.
            return Verdict.FromHostError(HostError.NoInterceptor);
        }

        var combined = Verdict.Allow;
        foreach (var i in _interceptors)
        {
            Verdict v;
            try
            {
                // §7.1/N05: each interceptor gets its own deep copy — an
                // in-place mutation of the copy cannot alter enforcement.
                var copy = new AgentContext((JsonObject)ctx.Json.DeepClone());
                v = await i.InterceptAsync(copy, ct);
                Native.ValidateVerdict(v.ToWire().ToJsonString(Compact)); // §5
            }
            catch (AgentHooksCoreException e)
            {
                return Verdict.FromHostError(e.Code, e.Message);
            }
            catch (Exception e) // fail closed per §6.3
            {
                return Verdict.FromHostError(HostError.InterceptorFailed, e.ToString());
            }

            if (!v.Decision.Permits())
                return v; // first block short-circuits (§7.1)
            if (v.Decision == Decision.Transform)
            {
                v = FoldTransform(ctx, v);
                if (!v.Decision.Permits()) return v; // transform failed closed
                combined = v;
            }
            else if (v.Decision == Decision.Warn && combined.Decision == Decision.Allow)
            {
                combined = v;
            }
        }
        return combined;
    }

    /// <summary>Apply (enforce) or validate (evaluate_only) one transform
    /// (§7.1, §8). Mutates <paramref name="ctx"/>.Json in place on apply.</summary>
    private Verdict FoldTransform(AgentContext ctx, Verdict v)
    {
        var t = v.Transform!;
        try
        {
            if (_mode == EnforcementMode.Enforce)
            {
                var newCtx = Canonical.ApplyTransformCtx(ctx, t.Path, t.Value);
                ctx.Json.Clear();
                foreach (var (k, val) in newCtx.ToList()) ctx.Json[k] = val?.DeepClone();
            }
            else
            {
                Canonical.ValidateTransformCtx(ctx, t.Path, t.Value);
            }
        }
        catch (AgentHooksCoreException e)
        {
            return Verdict.FromHostError(e.Code, e.Message);
        }
        return v;
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
        try
        {
            // §9/N04: the resolver's verdict crosses the same §5 gate as
            // an interceptor's.
            Native.ValidateVerdict(res.Verdict.ToWire().ToJsonString(Compact));
        }
        catch (AgentHooksCoreException e)
        {
            return Verdict.FromHostError(HostError.VerdictInvalid, e.Message);
        }
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
            (string)r["enforced_identity"]!);
    }
}
