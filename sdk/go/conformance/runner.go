// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

package conformance

// CTK runner: load vectors, drive a harness, assert expect.
//
// Assertion engine, capability skip check, and scripted
// interceptor/resolver evaluation live in the Rust core via
// agenthooks.Ctk*. This file keeps only vector globbing, the
// orchestration loop that calls Harness.Setup/Run/Teardown, and
// RunRecord → wire-JSON marshalling. See conformance/RUNNER.md.

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"

	"github.com/responsibleai/agent-hooks/sdk/go/agenthooks"
)

// VectorResult is the outcome of one vector run.
type VectorResult = agenthooks.CtkVectorResult

// LoadVectors globs conformance/vectors/AH-CTK-*.json under dir,
// returning those with level <= maxLevel in sorted order.
func LoadVectors(dir string, maxLevel int) ([]map[string]any, error) {
	paths, err := filepath.Glob(filepath.Join(dir, "AH-CTK-*.json"))
	if err != nil {
		return nil, err
	}
	sort.Strings(paths)
	var out []map[string]any
	for _, p := range paths {
		b, err := os.ReadFile(p)
		if err != nil {
			return nil, err
		}
		var v map[string]any
		if err := json.Unmarshal(b, &v); err != nil {
			return nil, fmt.Errorf("%s: %w", p, err)
		}
		if lvl, _ := v["level"].(float64); int(lvl) <= maxLevel {
			out = append(out, v)
		}
	}
	return out, nil
}

// scriptedInterceptor wraps agenthooks.CtkScriptedIntercept and
// records every context it is handed.
type scriptedInterceptor struct {
	rulesJSON string
	recorded  []agenthooks.AgentContext
}

func (s *scriptedInterceptor) OnHook(_ context.Context, actx agenthooks.AgentContext) (agenthooks.Verdict, error) {
	cp, err := agenthooks.DeepCopyContext(actx)
	if err != nil {
		return agenthooks.Verdict{}, err
	}
	s.recorded = append(s.recorded, cp)
	return agenthooks.CtkScriptedIntercept(s.rulesJSON, actx)
}

// scriptedResolver wraps agenthooks.CtkScriptedResolve.
type scriptedResolver struct {
	rulesJSON string
}

func (s *scriptedResolver) Resolve(_ context.Context, req agenthooks.ApprovalRequest) (agenthooks.ApprovalResolution, error) {
	return agenthooks.CtkScriptedResolve(s.rulesJSON, req.Context, req.ContextIdentity)
}

func mustJSON(v any) string {
	b, err := json.Marshal(v)
	if err != nil {
		panic(err)
	}
	return string(b)
}

func runRecordToWire(rr RunRecord) string {
	invs := make([]map[string]any, len(rr.ToolInvocations))
	for i, t := range rr.ToolInvocations {
		invs[i] = map[string]any{"name": t.Name, "args": t.Args}
	}
	ids := make([]map[string]string, len(rr.Identities))
	for i, p := range rr.Identities {
		ids[i] = map[string]string{
			"input_identity":    p.InputIdentity,
			"enforced_identity": p.EnforcedIdentity,
		}
	}
	return mustJSON(map[string]any{
		"outcome":          string(rr.Outcome),
		"final_output":     rr.FinalOutput,
		"tool_invocations": invs,
		"error":            rr.Err,
		"identities":       ids,
	})
}

// RunVector drives one vector against a fresh harness instance.
func RunVector(ctx context.Context, h Harness, vector map[string]any) (VectorResult, error) {
	vectorJSON := mustJSON(vector)
	id, _ := vector["id"].(string)
	title, _ := vector["title"].(string)
	lvlF, _ := vector["level"].(float64)
	level := int(lvlF)

	caps := make([]string, 0, len(h.Capabilities()))
	for c := range h.Capabilities() {
		caps = append(caps, string(c))
	}
	sort.Strings(caps)
	if reason, err := agenthooks.CtkShouldSkip(vectorJSON, caps); err != nil {
		return VectorResult{}, err
	} else if reason != "" {
		return VectorResult{ID: id, Title: title, Level: level, Status: "skip", Detail: reason}, nil
	}

	scenRaw, _ := vector["scenario"].(map[string]any)
	scenario := scenarioFromWire(scenRaw)

	ic := &scriptedInterceptor{rulesJSON: mustJSON(vector["interceptor_script"])}
	var resolver agenthooks.ApprovalResolver
	if approval, ok := vector["approval_script"].([]any); ok && len(approval) > 0 {
		resolver = &scriptedResolver{rulesJSON: mustJSON(approval)}
	}
	mode := agenthooks.Enforce
	if m, _ := vector["mode"].(string); m != "" {
		mode = agenthooks.EnforcementMode(m)
	}

	if err := h.Setup(scenario, ic, resolver, mode); err != nil {
		return VectorResult{ID: id, Title: title, Level: level, Status: "fail",
			Failures: []string{fmt.Sprintf("harness.Setup: %v", err)}}, nil
	}
	rr, runErr := h.Run(ctx)
	h.Teardown()
	if runErr != nil {
		return VectorResult{ID: id, Title: title, Level: level, Status: "fail",
			Failures: []string{fmt.Sprintf("harness.Run: %v", runErr)}}, nil
	}

	return agenthooks.CtkAssert(vectorJSON, ic.recorded, runRecordToWire(rr))
}

func scenarioFromWire(s map[string]any) Scenario {
	var sc Scenario
	if in, ok := s["input"].(map[string]any); ok {
		sc.Input = in
	}
	if tools, ok := s["tools"].([]any); ok {
		for _, t := range tools {
			if m, ok := t.(map[string]any); ok {
				sc.Tools = append(sc.Tools, m)
			}
		}
	}
	if ms, ok := s["model_script"].([]any); ok {
		for _, m := range ms {
			if mm, ok := m.(map[string]any); ok {
				sc.ModelScript = append(sc.ModelScript, mm)
			}
		}
	}
	return sc
}
