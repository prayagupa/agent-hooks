// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

package agenthooks

// Canonical JSON serialization and context identity (§10).
//
// Delegates to the Rust core via cgo (libagent_hooks_ffi) so every SDK
// produces byte-identical output. The pure-Go implementation was removed
// once the core became canonical (see sdk/rust/core/src/canonical.rs).

import "encoding/json"

// CanonicalJSON serializes v per §10.1. Implemented by the Rust core.
func CanonicalJSON(v any) (string, error) {
	b, err := json.Marshal(v)
	if err != nil {
		return "", err
	}
	return nativeCanonicalJSON(string(b))
}

// ContextIdentity returns "sha256:" + hex(SHA-256(CanonicalJSON(ctx_L01)))
// (§10.2). Implemented by the Rust core.
func ContextIdentity(ctx AgentContext) (string, error) {
	b, err := json.Marshal(map[string]any(ctx))
	if err != nil {
		return "", err
	}
	return nativeContextIdentity(string(b))
}

// ApplyTransform applies a $target-rooted path to target and returns the
// new target (§5.2). Implemented by the Rust core.
func ApplyTransform(target any, path string, value any) (any, error) {
	tb, err := json.Marshal(target)
	if err != nil {
		return nil, err
	}
	vb, err := json.Marshal(value)
	if err != nil {
		return nil, err
	}
	out, err := nativeApplyTransform(string(tb), path, string(vb))
	if err != nil {
		return nil, err
	}
	var result any
	return result, json.Unmarshal([]byte(out), &result)
}

// ValidateVerdict validates an interceptor's wire return per §5.
// Implemented by the Rust core.
func ValidateVerdict(v Verdict) error {
	b, err := json.Marshal(v)
	if err != nil {
		return err
	}
	_, err = nativeValidateVerdict(string(b))
	return err
}

// CombineVerdicts combines an ordered slice of verdicts per §7.1.
// Implemented by the Rust core.
func CombineVerdicts(vs []Verdict) (Verdict, error) {
	b, err := json.Marshal(vs)
	if err != nil {
		return Verdict{}, err
	}
	out, err := nativeCombineVerdicts(string(b))
	if err != nil {
		return Verdict{}, err
	}
	var v Verdict
	return v, json.Unmarshal([]byte(out), &v)
}
