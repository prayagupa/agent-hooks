// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Package agenthooks implements AGENT-HOOKS-0.1: a framework-neutral agent
// lifecycle hook contract (interception points, context, verdict, host obligations).
package agenthooks

import (
	"context"
	"encoding/json"
)

// SpecVersion is the spec version this module implements (§4.1 `spec` field).
const SpecVersion = "agent-hooks/0.1"

// InterceptionPoint is one of the eight agent lifecycle interception points (§3).
type InterceptionPoint string

const (
	AgentStartup  InterceptionPoint = "agent_startup"
	Input         InterceptionPoint = "input"
	PreModelCall  InterceptionPoint = "pre_model_call"
	PostModelCall InterceptionPoint = "post_model_call"
	PreToolCall   InterceptionPoint = "pre_tool_call"
	PostToolCall  InterceptionPoint = "post_tool_call"
	Output        InterceptionPoint = "output"
	AgentShutdown InterceptionPoint = "agent_shutdown"
)

// TransformPermitted reports whether a transform verdict is permitted at hp
// (§3, §4.3).
func (hp InterceptionPoint) TransformPermitted() bool {
	return hp != AgentStartup && hp != AgentShutdown
}

// Decision is a verdict decision value (§5.1).
type Decision string

const (
	Allow     Decision = "allow"
	Deny      Decision = "deny"
	Warn      Decision = "warn"
	Escalate  Decision = "escalate"
	Transform Decision = "transform"
)

// Permits reports whether the action proceeds under d (§2 permit class).
func (d Decision) Permits() bool {
	return d == Allow || d == Warn || d == Transform
}

// EnforcementMode controls whether the host acts on verdicts (§8).
type EnforcementMode string

const (
	Enforce      EnforcementMode = "enforce"
	EvaluateOnly EnforcementMode = "evaluate_only"
)

// HostError is a reserved host_error:* reason a host synthesizes (§11).
type HostError string

const (
	ErrContextInvalid           HostError = "host_error:context_invalid"
	ErrInterceptorFailed           HostError = "host_error:interceptor_failed"
	ErrInterceptorTimeout          HostError = "host_error:interceptor_timeout"
	ErrVerdictInvalid           HostError = "host_error:verdict_invalid"
	ErrTransformInvalid         HostError = "host_error:transform_invalid"
	ErrTransformTargetForbidden HostError = "host_error:transform_target_forbidden"
	ErrApprovalResolverMissing  HostError = "host_error:approval_resolver_missing"
	ErrApprovalResolverFailed   HostError = "host_error:approval_resolver_failed"
	ErrApprovalUnresolved       HostError = "host_error:approval_unresolved"
	ErrApprovalActionMismatch   HostError = "host_error:approval_action_mismatch"
	ErrAdapterUnsupported       HostError = "host_error:adapter_unsupported"
	ErrStreamingUnsupported     HostError = "host_error:streaming_unsupported"
	// ErrNoInterceptor: §7 — an enforce-mode emission with zero registered
	// interceptors fails closed rather than silently allowing everything.
	ErrNoInterceptor HostError = "host_error:no_interceptor"
)

// TransformBody is a single $target-rooted replacement (§5.2).
type TransformBody struct {
	// Path is rooted at $target (or the deprecated $policy_target alias).
	Path  string `json:"path"`
	Value any    `json:"value"`
}

// Evidence is an opaque pointer to an offline-verifiable artefact (§5.3).
type Evidence struct {
	Artefact             string            `json:"artefact,omitempty"`
	VerificationPointers map[string]string `json:"verification_pointers,omitempty"`
}

// Verdict is the interceptor return value (§5).
type Verdict struct {
	Decision     Decision       `json:"decision"`
	Reason       string         `json:"reason,omitempty"`
	Message      string         `json:"message,omitempty"`
	Transform    *TransformBody `json:"transform,omitempty"`
	Evidence     *Evidence      `json:"evidence,omitempty"`
	ResultLabels []string       `json:"result_labels,omitempty"`
}

// AllowVerdict is the trivial permit verdict.
var AllowVerdict = Verdict{Decision: Allow}

// HostErrorVerdict returns a host-synthesized deny verdict for a §11 failure.
func HostErrorVerdict(e HostError, msg string) Verdict {
	return Verdict{Decision: Deny, Reason: string(e), Message: msg}
}

// Validate checks v per §5; returns a HostError on violation.
func (v Verdict) Validate() error {
	if len(v.Reason) >= 11 && v.Reason[:11] == "host_error:" {
		return errVerdictInvalid("verdict.reason MUST NOT start with 'host_error:'")
	}
	if v.Decision == Transform && v.Transform == nil {
		return errVerdictInvalid("transform body REQUIRED when decision=='transform'")
	}
	if v.Decision != Transform && v.Transform != nil {
		return errVerdictInvalid("transform body FORBIDDEN when decision!='transform'")
	}
	return nil
}

type verdictError struct{ msg string }

func (e verdictError) Error() string { return string(ErrVerdictInvalid) + ": " + e.msg }
func errVerdictInvalid(m string) error { return verdictError{m} }

// AgentContext is the wire-shaped agent context (§4): a JSON object so it
// round-trips to the schema without translation.
type AgentContext map[string]any

// InterceptionPoint extracts interception_point from ctx.
func (ctx AgentContext) InterceptionPoint() InterceptionPoint {
	hp, _ := ctx["interception_point"].(string)
	return InterceptionPoint(hp)
}

// InterceptionRecord is the host-side record of one interception (§6, §10).
//
// Identity-only by design: the identities bind the record to the exact
// pre/post-fold context without duplicating the (possibly sensitive)
// payload into audit storage. Hosts that need the raw transformed value
// log it at the callsite.
type InterceptionRecord struct {
	InterceptionPoint InterceptionPoint `json:"interception_point"`
	Mode              EnforcementMode   `json:"mode"`
	Verdict           Verdict           `json:"verdict"`
	InputIdentity     string            `json:"input_identity"`
	EnforcedIdentity  string            `json:"enforced_identity"`
	// SessionID is ctx.session.id — correlates records across a session.
	SessionID string `json:"session_id"`
	// Sequence is ctx.sequence — total order within the session (§12.2.3).
	Sequence int64 `json:"sequence"`
	// DecidedBy is the registration index of the deciding interceptor;
	// nil for a pure allow or a host-synthesized host_error:* verdict.
	DecidedBy *int `json:"decided_by"`
}

// Proceeds reports whether the guarded action executes (§6, §8).
func (r InterceptionRecord) Proceeds() bool {
	return r.Mode == EvaluateOnly || r.Verdict.Decision.Permits()
}

// Interceptor is the interceptor protocol (§7).
type Interceptor interface {
	OnHook(ctx context.Context, hctx AgentContext) (Verdict, error)
}

// ApprovalOutcome is the resolver's outcome (§9).
type ApprovalOutcome string

const (
	Approve    ApprovalOutcome = "approve"
	Reject     ApprovalOutcome = "reject"
	Unresolved ApprovalOutcome = "unresolved"
)

// ApprovalRequest is what the host hands the resolver on escalate (§9).
type ApprovalRequest struct {
	ContextIdentity string
	InterceptionPoint       InterceptionPoint
	Verdict         Verdict
	Context         AgentContext
}

// ApprovalResolution is what the resolver returns (§9).
type ApprovalResolution struct {
	Outcome         ApprovalOutcome
	ContextIdentity string
	Verdict         *Verdict
}

// ApprovalResolver is the host-registered resolver for escalate (§9).
type ApprovalResolver interface {
	Resolve(ctx context.Context, req ApprovalRequest) (ApprovalResolution, error)
}

// InterceptionBlocked is returned by a host when a verdict blocks the guarded action (§6).
type InterceptionBlocked struct {
	Result InterceptionRecord
}

func (e InterceptionBlocked) Error() string {
	b, _ := json.Marshal(e.Result.Verdict)
	return string(e.Result.InterceptionPoint) + " blocked: " + string(b)
}
