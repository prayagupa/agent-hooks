// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// Package conformance defines the CTK harness contract (§13.2).
//
// The Go CTK runner is not yet implemented; this package defines the
// Harness interface so framework adapters can be written now. Track the
// runner at https://github.com/responsibleai/agent-hooks/issues/5; the
// Python implementation at sdk/python/src/agent_hooks/ctk/runner.py is
// the reference.
package conformance

import (
	"context"

	"github.com/responsibleai/agent-hooks/sdk/go/agenthooks"
)

// Capability is a host-declared capability (§3.2).
type Capability string

const (
	ModelCalls        Capability = "model_calls"
	ToolCalls         Capability = "tool_calls"
	ParallelToolCalls Capability = "parallel_tool_calls"
	Streaming         Capability = "streaming"
	MultiTurn         Capability = "multi_turn"
)

// RunOutcome describes how a harness run ended.
type RunOutcome string

const (
	Completed RunOutcome = "completed"
	Blocked   RunOutcome = "blocked"
	Suspended RunOutcome = "suspended"
	Errored   RunOutcome = "error"
)

// Scenario is a hermetic scripted run loaded from a CTK vector (wire-shaped).
type Scenario struct {
	Input       map[string]any   `json:"input"`
	Tools       []map[string]any `json:"tools"`
	ModelScript []map[string]any `json:"model_script"`
}

// ToolInvocation is one entry in the harness's mock-tool log.
type ToolInvocation struct {
	Name string         `json:"name"`
	Args map[string]any `json:"args"`
}

// RunRecord is what Harness.Run returns to the CTK runner.
type RunRecord struct {
	Outcome         RunOutcome
	FinalOutput     any
	ToolInvocations []ToolInvocation
	Err             string
}

// Harness is the single interface a framework adapter implements for the CTK.
type Harness interface {
	Name() string
	Capabilities() map[Capability]struct{}

	Setup(
		scenario Scenario,
		interceptor agenthooks.Interceptor,
		resolver agenthooks.ApprovalResolver,
		mode agenthooks.EnforcementMode,
	) error

	Run(ctx context.Context) (RunRecord, error)

	Teardown()
}
