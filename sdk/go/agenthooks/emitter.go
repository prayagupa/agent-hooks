// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

package agenthooks

// Host-side emitter: dispatch context → interceptors → verdict → record
// (§6–§9).
//
// Per-language orchestrator over the Rust core. Interceptor dispatch (§7)
// and approval-seam resolution (§9) stay here because they call back into
// user Go code. Verdict validation (§5), transform fold-through (§7.1),
// identity computation (§10), and target write-back (§4.3) delegate to
// the core so behaviour is byte-identical across SDKs.
//
// §7.1 sequential fold-through: interceptors run in registration order;
// each receives a deep copy of the context as it stands *after* prior
// transforms were applied, so an earlier interceptor's redaction is
// visible to later ones. The first block verdict short-circuits.
//
// Fail-closed defaults: an enforce-mode emission with zero registered
// interceptors yields deny host_error:no_interceptor (§7), and Emit
// returns InterceptionBlocked on any block — the ignorable-result
// variant is the explicitly named EmitUnchecked.

import (
	"strings"
	"sync"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"time"
)

// DefaultTimeout is the §7 RECOMMENDED interceptor/resolver timeout.
const DefaultTimeout = 5000 * time.Millisecond

// InterceptionEmitter implements §6–§9 once so adapters do not have to.
// One instance per session.
type InterceptionEmitter struct {
	interceptors []Interceptor
	resolver     ApprovalResolver
	mode         EnforcementMode

	// Timeout bounds each interceptor OnHook and resolver Resolve call
	// (§7, RECOMMENDED default 5000 ms); breach fails closed with
	// host_error:interceptor_timeout / approval_resolver_failed. The
	// callee receives a cancelled context on breach, but if it ignores
	// cancellation its goroutine keeps running detached until it
	// returns. Set to 0 (or negative) to disable enforcement. Set
	// before the first Emit; not synchronized.
	Timeout time.Duration

	mu sync.Mutex
	// records holds every InterceptionRecord emitted so far, in
	// sequence order. Guarded by mu: emissions for different tool
	// calls may run concurrently (§12.2).
	records []InterceptionRecord
}

// callWithTimeout runs fn under the §7 timeout d (d <= 0 disables). On
// breach the eventual result is discarded and timedOut is true.
func callWithTimeout[T any](
	ctx context.Context, d time.Duration, fn func(context.Context) (T, error),
) (out T, err error, timedOut bool) {
	if d <= 0 {
		out, err = fn(ctx)
		return out, err, false
	}
	tctx, cancel := context.WithTimeout(ctx, d)
	defer cancel()
	type result struct {
		v   T
		err error
	}
	ch := make(chan result, 1)
	go func() {
		v, e := fn(tctx)
		ch <- result{v, e}
	}()
	select {
	case r := <-ch:
		return r.v, r.err, false
	case <-tctx.Done():
		if ctx.Err() != nil {
			// Parent cancellation, not our timeout.
			return out, ctx.Err(), false
		}
		return out, nil, true
	}
}

// Records returns a snapshot of every InterceptionRecord emitted so far.
func (e *InterceptionEmitter) Records() []InterceptionRecord {
	e.mu.Lock()
	defer e.mu.Unlock()
	out := make([]InterceptionRecord, len(e.records))
	copy(out, e.records)
	return out
}

// NewInterceptionEmitter constructs an emitter in the given mode with an
// optional approval resolver.
func NewInterceptionEmitter(mode EnforcementMode, resolver ApprovalResolver) *InterceptionEmitter {
	return &InterceptionEmitter{mode: mode, resolver: resolver, Timeout: DefaultTimeout}
}

// Mode returns the enforcement mode.
func (e *InterceptionEmitter) Mode() EnforcementMode { return e.mode }

// Register appends an interceptor and returns the emitter for chaining.
func (e *InterceptionEmitter) Register(i Interceptor) *InterceptionEmitter {
	e.interceptors = append(e.interceptors, i)
	return e
}

// Emit runs the interception and returns InterceptionBlocked as the
// error if the guarded action must not proceed (§6). This is the primary
// entry point; the safe path is the default.
func (e *InterceptionEmitter) Emit(ctx context.Context, actx AgentContext) (InterceptionRecord, error) {
	rec, err := e.EmitUnchecked(ctx, actx)
	if err != nil {
		return rec, err
	}
	if !rec.Proceeds() {
		return rec, InterceptionBlocked{Result: rec}
	}
	return rec, nil
}

// EmitUnchecked runs the interception and returns the record without a
// block error. The caller MUST inspect InterceptionRecord.Proceeds and
// halt the guarded action itself; prefer Emit.
//
// On transform in enforce mode, actx is mutated in place (target and the
// aliased L1 field rewritten) so the caller's action consumes the
// transformed value. A non-nil error is an infrastructure failure only
// (JSON marshalling or core invocation), never a verdict outcome.
func (e *InterceptionEmitter) EmitUnchecked(ctx context.Context, actx AgentContext) (InterceptionRecord, error) {
	// §10.2: input identity binds to the context BEFORE dispatch, so
	// neither interceptor mutation nor fold-through can retroactively
	// alter what the record claims was evaluated.
	ctxJSON, err := json.Marshal(map[string]any(actx))
	if err != nil {
		return InterceptionRecord{}, err
	}
	inputID, err := nativeContextIdentity(string(ctxJSON))
	if err != nil {
		return InterceptionRecord{}, err
	}

	verdict, decidedBy := e.dispatch(ctx, actx)

	// §6.1a: nothing to approve at agent_shutdown.
	if verdict.Decision == Escalate && e.mode == Enforce && actx.InterceptionPoint() != AgentShutdown {
		// §9/NOW-14: approval binds to the escalation-time identity
		// (post prior fold transforms) — what the resolver actually sees.
		escCtxJSON, mErr := json.Marshal(map[string]any(actx))
		if mErr != nil {
			return InterceptionRecord{}, mErr
		}
		escalationID, iErr := nativeContextIdentity(string(escCtxJSON))
		if iErr != nil {
			return InterceptionRecord{}, iErr
		}
		verdict = e.resolveEscalate(ctx, actx.InterceptionPoint(), actx, verdict, escalationID)
		// An approve MAY carry a transform (§9); it is subject to the
		// same fold rules as an interceptor transform.
		if verdict.Decision == Transform {
			verdict = e.foldTransform(actx, verdict)
		}
		// A resolver-substituted verdict keeps the escalating
		// interceptor's index; host-synthesized failures do not.
		if strings.HasPrefix(verdict.Reason, "host_error:") {
			decidedBy = nil
		}
	}

	finalCtxJSON, err := json.Marshal(map[string]any(actx))
	if err != nil {
		return InterceptionRecord{}, err
	}
	verdictJSON, err := json.Marshal(verdict)
	if err != nil {
		return InterceptionRecord{}, err
	}
	decidedByWire := int64(-1)
	if decidedBy != nil {
		decidedByWire = int64(*decidedBy)
	}
	recJSON, err := nativeFinalize(string(finalCtxJSON), string(verdictJSON), string(e.mode), inputID, decidedByWire)
	if err != nil {
		return InterceptionRecord{}, err
	}
	var rec InterceptionRecord
	if err := json.Unmarshal([]byte(recJSON), &rec); err != nil {
		return InterceptionRecord{}, err
	}
	e.mu.Lock()
	e.records = append(e.records, rec)
	e.mu.Unlock()
	return rec, nil
}

// dispatch invokes interceptors in registration order with §7.1
// sequential fold-through. Every failure becomes a host_error deny
// verdict (§6.3); dispatch never returns an error.
// dispatch returns the combined verdict and the registration index of
// the deciding interceptor (nil for pure allow or host-synthesized).
func (e *InterceptionEmitter) dispatch(ctx context.Context, actx AgentContext) (Verdict, *int) {
	if len(e.interceptors) == 0 {
		// §7: zero interceptors fails closed. Register an explicit
		// allow-all interceptor for a deliberate passthrough.
		return HostErrorVerdict(ErrNoInterceptor,
			"register an explicit allow-all interceptor for a deliberate passthrough"), nil
	}

	combined := AllowVerdict
	var labels []string
	var decidedBy *int
	for icIdx, ic := range e.interceptors {
		// §7.1/N05: each interceptor gets its own deep copy — an
		// in-place mutation of the copy cannot alter enforcement.
		cp, err := DeepCopyContext(actx)
		if err != nil {
			return HostErrorVerdict(ErrContextInvalid, err.Error()), nil
		}
		v, err, timedOut := callWithTimeout(ctx, e.Timeout,
			func(c context.Context) (Verdict, error) { return ic.OnHook(c, cp) })
		if timedOut {
			return HostErrorVerdict(ErrInterceptorTimeout, ""), nil // §7
		}
		if err != nil {
			return HostErrorVerdict(ErrInterceptorFailed, fmt.Sprintf("%T", err)), nil // §6.3
		}
		vb, err := json.Marshal(v)
		if err != nil {
			return HostErrorVerdict(ErrInterceptorFailed, fmt.Sprintf("%T", err)), nil
		}
		if _, err := nativeValidateVerdict(string(vb)); err != nil { // §5
			return coreErrVerdict(err, ErrVerdictInvalid), nil
		}

		if !v.Decision.Permits() {
			idx := icIdx
			return v, &idx // first block short-circuits (§7.1)
		}
		if v.Decision == Transform {
			v = e.foldTransform(actx, v)
			if !v.Decision.Permits() { // transform failed closed
				return v, nil
			}
			combined = v
			idx := icIdx
			decidedBy = &idx
		} else if v.Decision == Warn && combined.Decision == Allow {
			combined = v
			idx := icIdx
			decidedBy = &idx
		}
		// §7.1 step 5: union permit-verdict labels, first-seen order.
		for _, l := range v.ResultLabels {
			seen := false
			for _, have := range labels {
				if have == l {
					seen = true
					break
				}
			}
			if !seen {
				labels = append(labels, l)
			}
		}
	}
	if len(labels) > 0 {
		combined.ResultLabels = labels
	}
	return combined, decidedBy
}

// foldTransform applies (enforce) or validates (evaluate_only) one
// transform (§7.1, §8). On apply, actx is replaced in place with the
// core's updated context so the next interceptor sees the effect.
func (e *InterceptionEmitter) foldTransform(actx AgentContext, v Verdict) Verdict {
	if v.Transform == nil {
		return HostErrorVerdict(ErrTransformInvalid, "transform body missing")
	}
	valueJSON, err := json.Marshal(v.Transform.Value)
	if err != nil {
		return HostErrorVerdict(ErrTransformInvalid, err.Error())
	}
	ctxJSON, err := json.Marshal(map[string]any(actx))
	if err != nil {
		return HostErrorVerdict(ErrContextInvalid, err.Error())
	}
	if e.mode == Enforce {
		out, err := nativeApplyTransformCtx(string(ctxJSON), v.Transform.Path, string(valueJSON))
		if err != nil {
			return coreErrVerdict(err, ErrTransformInvalid)
		}
		var newCtx map[string]any
		if err := json.Unmarshal([]byte(out), &newCtx); err != nil {
			return HostErrorVerdict(ErrTransformInvalid, err.Error())
		}
		for k := range actx {
			delete(actx, k)
		}
		for k, val := range newCtx {
			actx[k] = val
		}
	} else {
		if _, err := nativeValidateTransformCtx(string(ctxJSON), v.Transform.Path, string(valueJSON)); err != nil {
			return coreErrVerdict(err, ErrTransformInvalid)
		}
	}
	return v
}

func (e *InterceptionEmitter) resolveEscalate(
	ctx context.Context,
	ip InterceptionPoint,
	actx AgentContext,
	verdict Verdict,
	identity string,
) Verdict {
	if e.resolver == nil {
		return HostErrorVerdict(ErrApprovalResolverMissing, "")
	}
	res, err, timedOut := callWithTimeout(ctx, e.Timeout,
		func(c context.Context) (ApprovalResolution, error) {
			return e.resolver.Resolve(c, ApprovalRequest{
				ContextIdentity:   identity,
				InterceptionPoint: ip,
				Verdict:           verdict,
				Context:           actx,
			})
		})
	if timedOut {
		return HostErrorVerdict(ErrApprovalResolverFailed, "timeout") // §7
	}
	if err != nil {
		return HostErrorVerdict(ErrApprovalResolverFailed, fmt.Sprintf("%T", err))
	}
	if res.ContextIdentity != identity {
		return HostErrorVerdict(ErrApprovalActionMismatch, "")
	}
	if res.Outcome == Unresolved || res.Verdict == nil {
		return HostErrorVerdict(ErrApprovalUnresolved, "")
	}
	// §9/N04: the resolver's verdict crosses the same §5 gate as an
	// interceptor's.
	vb, err := json.Marshal(*res.Verdict)
	if err != nil {
		return HostErrorVerdict(ErrVerdictInvalid, err.Error())
	}
	if _, err := nativeValidateVerdict(string(vb)); err != nil {
		var ce *CoreError
		if errors.As(err, &ce) {
			return HostErrorVerdict(ErrVerdictInvalid, ce.Detail)
		}
		return HostErrorVerdict(ErrVerdictInvalid, err.Error())
	}
	return *res.Verdict
}

// coreErrVerdict maps a CoreError to a host_error deny verdict, falling
// back to the given code for non-core errors.
func coreErrVerdict(err error, fallback HostError) Verdict {
	var ce *CoreError
	if errors.As(err, &ce) {
		return HostErrorVerdict(HostError(ce.Code), ce.Detail)
	}
	return HostErrorVerdict(fallback, err.Error())
}
