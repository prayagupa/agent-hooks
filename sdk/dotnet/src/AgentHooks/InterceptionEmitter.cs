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

    /// <summary>§7 RECOMMENDED interceptor/resolver timeout.</summary>
    public static readonly TimeSpan DefaultTimeout = TimeSpan.FromMilliseconds(5000);

    private readonly List<IInterceptor> _interceptors = [];
    private readonly List<InterceptionRecord> _records = [];
    private readonly IApprovalResolver? _resolver;
    private readonly EnforcementMode _mode;
    private readonly TimeSpan _timeout;

    /// <param name="timeout">Bounds each interceptor
    /// <c>InterceptAsync</c> and resolver <c>ResolveAsync</c> call (§7,
    /// RECOMMENDED default 5000 ms); breach fails closed with
    /// <c>host_error:interceptor_timeout</c> / <c>approval_resolver_failed</c>.
    /// The cancellation token is signalled on breach, but a callee that
    /// ignores it keeps running detached. <c>null</c> = 5000 ms;
    /// <see cref="Timeout.InfiniteTimeSpan"/> disables enforcement.</param>
    public InterceptionEmitter(
        EnforcementMode mode = EnforcementMode.Enforce,
        IApprovalResolver? resolver = null,
        TimeSpan? timeout = null)
    {
        _mode = mode;
        _resolver = resolver;
        _timeout = timeout ?? DefaultTimeout;
    }

    /// <summary>Race <paramref name="fn"/> against the §7 timeout.</summary>
    private async ValueTask<T> WithTimeoutAsync<T>(
        Func<CancellationToken, ValueTask<T>> fn, CancellationToken ct)
    {
        if (_timeout == Timeout.InfiniteTimeSpan) return await fn(ct);
        using var cts = CancellationTokenSource.CreateLinkedTokenSource(ct);
        cts.CancelAfter(_timeout);
        var task = fn(cts.Token).AsTask();
        var completed = await Task.WhenAny(task, Task.Delay(_timeout, CancellationToken.None));
        if (completed != task) throw new TimeoutException();
        return await task;
    }

    public EnforcementMode Mode => _mode;

    /// <summary>All interception records emitted so far in this session, in order.</summary>
    private readonly object _recordsLock = new();

    /// <summary>Snapshot of every record emitted so far, in sequence
    /// order. Emissions for different tool calls may run concurrently
    /// (§12.2), so the backing list is lock-guarded.</summary>
    public IReadOnlyList<InterceptionRecord> Records
    {
        get { lock (_recordsLock) return _records.ToList(); }
    }

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

        var (verdict, decidedBy) = await DispatchAsync(ctx, ct);

        if (verdict.Decision == Decision.Escalate && _mode == EnforcementMode.Enforce)
        {
            verdict = await ResolveEscalateAsync(ctx.InterceptionPoint, ctx, verdict, inputId, ct);
            // An approve MAY carry a transform (§9); it is subject to the
            // same fold rules as an interceptor transform.
            if (verdict.Decision == Decision.Transform)
                verdict = FoldTransform(ctx, verdict);
            // A resolver-substituted verdict keeps the escalating
            // interceptor's index; host-synthesized failures do not.
            if (verdict.Reason?.StartsWith("host_error:", StringComparison.Ordinal) == true)
                decidedBy = null;
        }

        var recordJson = Native.Finalize(
            ctx.Json.ToJsonString(Compact),
            verdict.ToWire().ToJsonString(Compact),
            _mode == EnforcementMode.Enforce ? "enforce" : "evaluate_only",
            inputId,
            decidedBy ?? -1);
        var record = RecordFromCore((JsonObject)JsonNode.Parse(recordJson)!);
        lock (_recordsLock) _records.Add(record);
        return record;
    }

    // -------------------------------------------------------------------------

    private async ValueTask<(Verdict, int?)> DispatchAsync(AgentContext ctx, CancellationToken ct)
    {
        if (_interceptors.Count == 0)
        {
            // §7: zero interceptors fails closed. Register an explicit
            // allow-all interceptor for a deliberate passthrough.
            return (Verdict.FromHostError(HostError.NoInterceptor), null);
        }

        var combined = Verdict.Allow;
        int? decidedBy = null;
        for (var idx = 0; idx < _interceptors.Count; idx++)
        {
            var i = _interceptors[idx];
            Verdict v;
            try
            {
                // §7.1/N05: each interceptor gets its own deep copy — an
                // in-place mutation of the copy cannot alter enforcement.
                var copy = new AgentContext((JsonObject)ctx.Json.DeepClone());
                v = await WithTimeoutAsync(t => i.InterceptAsync(copy, t), ct);
                Native.ValidateVerdict(v.ToWire().ToJsonString(Compact)); // §5
            }
            catch (TimeoutException)
            {
                return (Verdict.FromHostError(HostError.InterceptorTimeout), null);
            }
            catch (AgentHooksCoreException e)
            {
                return (Verdict.FromHostError(e.Code, e.Message), null);
            }
            catch (Exception e) // fail closed per §6.3
            {
                return (Verdict.FromHostError(HostError.InterceptorFailed, e.GetType().Name), null);
            }

            if (!v.Decision.Permits())
                return (v, idx); // first block short-circuits (§7.1)
            if (v.Decision == Decision.Transform)
            {
                v = FoldTransform(ctx, v);
                if (!v.Decision.Permits()) return (v, null); // transform failed closed
                combined = v;
                decidedBy = idx;
            }
            else if (v.Decision == Decision.Warn && combined.Decision == Decision.Allow)
            {
                combined = v;
                decidedBy = idx;
            }
        }
        return (combined, decidedBy);
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
            res = await WithTimeoutAsync(
                t => _resolver.ResolveAsync(new ApprovalRequest(identity, ip, verdict, ctx), t),
                ct);
        }
        catch (TimeoutException)
        {
            return Verdict.FromHostError(HostError.ApprovalResolverFailed, "timeout");
        }
        catch (Exception e)
        {
            return Verdict.FromHostError(HostError.ApprovalResolverFailed, e.GetType().Name);
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
            (string)r["enforced_identity"]!,
            (string?)r["session_id"] ?? string.Empty,
            (long?)r["sequence"] ?? -1,
            r["decided_by"] is null ? null : (int?)r["decided_by"]!);
    }
}
