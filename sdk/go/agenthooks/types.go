// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Package agenthooks implements AGENT-HOOKS-0.1: a framework-neutral agent
// lifecycle hook contract (hook points, context, verdict, host obligations).
package agenthooks

import (
	"context"
	"encoding/json"
)

// SpecVersion is the spec version this module implements (§4.1 `spec` field).
const SpecVersion = "agent-hooks/0.1"

// HookPoint is one of the eight agent lifecycle hook points (§3).
type HookPoint string

const (
	AgentStartup  HookPoint = "agent_startup"
	Input         HookPoint = "input"
	PreModelCall  HookPoint = "pre_model_call"
	PostModelCall HookPoint = "post_model_call"
	PreToolCall   HookPoint = "pre_tool_call"
	PostToolCall  HookPoint = "post_tool_call"
	Output        HookPoint = "output"
	AgentShutdown HookPoint = "agent_shutdown"
)

// TransformPermitted reports whether a transform verdict is permitted at hp
// (§3, §4.3).
func (hp HookPoint) TransformPermitted() bool {
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

// HookError is a reserved hook_error:* reason a host synthesizes (§11).
type HookError string

const (
	ErrContextInvalid           HookError = "hook_error:context_invalid"
	ErrConsumerFailed           HookError = "hook_error:consumer_failed"
	ErrConsumerTimeout          HookError = "hook_error:consumer_timeout"
	ErrVerdictInvalid           HookError = "hook_error:verdict_invalid"
	ErrTransformInvalid         HookError = "hook_error:transform_invalid"
	ErrTransformTargetForbidden HookError = "hook_error:transform_target_forbidden"
	ErrApprovalResolverMissing  HookError = "hook_error:approval_resolver_missing"
	ErrApprovalResolverFailed   HookError = "hook_error:approval_resolver_failed"
	ErrApprovalUnresolved       HookError = "hook_error:approval_unresolved"
	ErrApprovalActionMismatch   HookError = "hook_error:approval_action_mismatch"
	ErrAdapterUnsupported       HookError = "hook_error:adapter_unsupported"
	ErrStreamingUnsupported     HookError = "hook_error:streaming_unsupported"
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

// Verdict is the consumer return value (§5).
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

// HookErrorVerdict returns a host-synthesized deny verdict for a §11 failure.
func HookErrorVerdict(e HookError, msg string) Verdict {
	return Verdict{Decision: Deny, Reason: string(e), Message: msg}
}

// Validate checks v per §5; returns a HookError on violation.
func (v Verdict) Validate() error {
	if len(v.Reason) >= 11 && v.Reason[:11] == "hook_error:" {
		return errVerdictInvalid("verdict.reason MUST NOT start with 'hook_error:'")
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

// HookContext is the wire-shaped hook context (§4): a JSON object so it
// round-trips to the schema without translation.
type HookContext map[string]any

// HookPoint extracts hook_point from ctx.
func (ctx HookContext) HookPoint() HookPoint {
	hp, _ := ctx["hook_point"].(string)
	return HookPoint(hp)
}

// HookResult is the host-side record of one hook evaluation (§6, §10).
type HookResult struct {
	HookPoint         HookPoint       `json:"hook_point"`
	Mode              EnforcementMode `json:"mode"`
	Verdict           Verdict         `json:"verdict"`
	InputIdentity     string          `json:"input_identity"`
	EnforcedIdentity  string          `json:"enforced_identity"`
	TransformedTarget any             `json:"transformed_target,omitempty"`
}

// Proceeds reports whether the guarded action executes (§6, §8).
func (r HookResult) Proceeds() bool {
	return r.Mode == EvaluateOnly || r.Verdict.Decision.Permits()
}

// HookConsumer is the consumer protocol (§7).
type HookConsumer interface {
	OnHook(ctx context.Context, hctx HookContext) (Verdict, error)
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
	HookPoint       HookPoint
	Verdict         Verdict
	Context         HookContext
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

// HookBlocked is returned by a host when a verdict blocks the guarded action (§6).
type HookBlocked struct {
	Result HookResult
}

func (e HookBlocked) Error() string {
	b, _ := json.Marshal(e.Result.Verdict)
	return string(e.Result.HookPoint) + " blocked: " + string(b)
}
