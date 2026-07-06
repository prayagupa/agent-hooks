// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

package agenthooks

// Host-side emitter: dispatch context → interceptors → verdict → enforce
// (§6–§9).
//
// Per-language orchestrator over the Rust core. Interceptor dispatch (§7)
// and approval-seam resolution (§9) stay here because they call back into
// user Go code. Verdict validation (§5), combination (§7.1), transform
// application (§5.2), identity computation (§10), and target write-back
// (§4.3) delegate to nativeEnforce so behaviour is byte-identical across
// SDKs.

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
)

// InterceptionEmitter implements §6–§9 once so adapters do not have to.
// One instance per session.
type InterceptionEmitter struct {
	interceptors []Interceptor
	resolver     ApprovalResolver
	mode         EnforcementMode
	// Records holds every InterceptionRecord emitted so far, in order.
	Records []InterceptionRecord
}

// NewInterceptionEmitter constructs an emitter in the given mode with an
// optional approval resolver.
func NewInterceptionEmitter(mode EnforcementMode, resolver ApprovalResolver) *InterceptionEmitter {
	return &InterceptionEmitter{mode: mode, resolver: resolver}
}

// Mode returns the enforcement mode.
func (e *InterceptionEmitter) Mode() EnforcementMode { return e.mode }

// Register appends an interceptor and returns the emitter for chaining.
func (e *InterceptionEmitter) Register(i Interceptor) *InterceptionEmitter {
	e.interceptors = append(e.interceptors, i)
	return e
}

// Emit builds the InterceptionRecord for one interception. On transform in
// enforce mode, actx is mutated in place (target and the aliased L1 field
// rewritten) so the caller's action consumes the transformed value.
func (e *InterceptionEmitter) Emit(ctx context.Context, actx AgentContext) (InterceptionRecord, error) {
	ip := actx.InterceptionPoint()

	// §7 dispatch (native — calls user code) + §5/§7.1 (core).
	verdict, err := e.dispatch(ctx, actx)
	if err != nil {
		return InterceptionRecord{}, err
	}

	// §9 approval seam (native — calls user code). Needs input_identity;
	// enforce() will recompute deterministically.
	if verdict.Decision == Escalate && e.mode == Enforce {
		ctxJSON, err := json.Marshal(map[string]any(actx))
		if err != nil {
			return InterceptionRecord{}, err
		}
		inputID, err := nativeContextIdentity(string(ctxJSON))
		if err != nil {
			return InterceptionRecord{}, err
		}
		verdict = e.resolveEscalate(ctx, ip, actx, verdict, inputID)
	}

	// §6/§8/§10 enforcement (core). Returns {record, ctx}; ctx may have
	// target + L1 field rewritten on transform.
	ctxJSON, err := json.Marshal(map[string]any(actx))
	if err != nil {
		return InterceptionRecord{}, err
	}
	verdictJSON, err := json.Marshal(verdict)
	if err != nil {
		return InterceptionRecord{}, err
	}
	outJSON, err := nativeEnforce(string(ctxJSON), string(verdictJSON), string(e.mode))
	if err != nil {
		return InterceptionRecord{}, err
	}
	var out struct {
		Record InterceptionRecord `json:"record"`
		Ctx    map[string]any     `json:"ctx"`
	}
	if err := json.Unmarshal([]byte(outJSON), &out); err != nil {
		return InterceptionRecord{}, err
	}
	// Write the (possibly transformed) context back into the caller's map.
	for k := range actx {
		delete(actx, k)
	}
	for k, v := range out.Ctx {
		actx[k] = v
	}

	e.Records = append(e.Records, out.Record)
	return out.Record, nil
}

// EmitOrErr calls Emit and returns InterceptionBlocked if the action must
// halt (§6).
func (e *InterceptionEmitter) EmitOrErr(ctx context.Context, actx AgentContext) (InterceptionRecord, error) {
	rec, err := e.Emit(ctx, actx)
	if err != nil {
		return rec, err
	}
	if !rec.Proceeds() {
		return rec, InterceptionBlocked{Result: rec}
	}
	return rec, nil
}

// dispatch invokes interceptors in order; validate + combine via core.
func (e *InterceptionEmitter) dispatch(ctx context.Context, actx AgentContext) (Verdict, error) {
	wire := make([]json.RawMessage, 0, len(e.interceptors))
	for _, ic := range e.interceptors {
		v, err := ic.OnHook(ctx, actx)
		if err != nil {
			// Fail closed per §6.3.
			return HostErrorVerdict(ErrInterceptorFailed, fmt.Sprintf("%v", err)), nil
		}
		vb, err := json.Marshal(v)
		if err != nil {
			return HostErrorVerdict(ErrInterceptorFailed, err.Error()), nil
		}
		// §5 validation via core.
		if _, err := nativeValidateVerdict(string(vb)); err != nil {
			var ce *CoreError
			if errors.As(err, &ce) {
				return HostErrorVerdict(HostError(ce.Code), ce.Detail), nil
			}
			return HostErrorVerdict(ErrVerdictInvalid, err.Error()), nil
		}
		wire = append(wire, vb)
		// §7.1.2 short-circuit: stop invoking further interceptors on block.
		if v.Decision == Deny || v.Decision == Escalate {
			break
		}
	}
	all, err := json.Marshal(wire)
	if err != nil {
		return Verdict{}, err
	}
	combinedJSON, err := nativeCombineVerdicts(string(all))
	if err != nil {
		return Verdict{}, err
	}
	var combined Verdict
	return combined, json.Unmarshal([]byte(combinedJSON), &combined)
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
	res, err := e.resolver.Resolve(ctx, ApprovalRequest{
		ContextIdentity:   identity,
		InterceptionPoint: ip,
		Verdict:           verdict,
		Context:           actx,
	})
	if err != nil {
		return HostErrorVerdict(ErrApprovalResolverFailed, fmt.Sprintf("%v", err))
	}
	if res.ContextIdentity != identity {
		return HostErrorVerdict(ErrApprovalActionMismatch, "")
	}
	if res.Outcome == Unresolved || res.Verdict == nil {
		return HostErrorVerdict(ErrApprovalUnresolved, "")
	}
	return *res.Verdict
}
