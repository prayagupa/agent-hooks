// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

package agenthooks

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"math"
	"sort"
	"strconv"
	"strings"
)

var (
	l0        = set("spec", "interception_point", "timestamp", "sequence", "agent", "session", "target")
	l0Agent   = set("id", "framework")
	l0Session = set("id")
	l1        = map[InterceptionPoint][]string{
		AgentStartup:  {"agent_init"},
		Input:         {"input"},
		PreModelCall:  {"model", "messages"},
		PostModelCall: {"model", "response"},
		PreToolCall:   {"tool_call"},
		PostToolCall:  {"tool_call", "tool_result"},
		Output:        {"output"},
		AgentShutdown: {"summary"},
	}
)

func set(keys ...string) map[string]struct{} {
	m := make(map[string]struct{}, len(keys))
	for _, k := range keys {
		m[k] = struct{}{}
	}
	return m
}

// CanonicalJSON serializes v per §10.1: lexicographic keys, no whitespace,
// ECMA-262 numbers, RFC 8259 minimal string escapes.
func CanonicalJSON(v any) (string, error) {
	var sb strings.Builder
	if err := encode(v, &sb); err != nil {
		return "", err
	}
	return sb.String(), nil
}

func encode(v any, sb *strings.Builder) error {
	switch x := v.(type) {
	case nil:
		sb.WriteString("null")
	case bool:
		if x {
			sb.WriteString("true")
		} else {
			sb.WriteString("false")
		}
	case string:
		b, _ := json.Marshal(x)
		sb.Write(b)
	case float64:
		return encodeNumber(x, sb)
	case int:
		sb.WriteString(strconv.Itoa(x))
	case int64:
		sb.WriteString(strconv.FormatInt(x, 10))
	case json.Number:
		sb.WriteString(string(x))
	case []any:
		sb.WriteByte('[')
		for i, e := range x {
			if i > 0 {
				sb.WriteByte(',')
			}
			if err := encode(e, sb); err != nil {
				return err
			}
		}
		sb.WriteByte(']')
	case map[string]any:
		sb.WriteByte('{')
		keys := make([]string, 0, len(x))
		for k := range x {
			keys = append(keys, k)
		}
		sort.Strings(keys) // byte-wise == lexicographic for UTF-8
		for i, k := range keys {
			if i > 0 {
				sb.WriteByte(',')
			}
			kb, _ := json.Marshal(k)
			sb.Write(kb)
			sb.WriteByte(':')
			if err := encode(x[k], sb); err != nil {
				return err
			}
		}
		sb.WriteByte('}')
	case AgentContext:
		return encode(map[string]any(x), sb)
	default:
		// Round-trip through encoding/json to a generic any then re-encode.
		// Keeps the function total for struct types without reflection here.
		b, err := json.Marshal(x)
		if err != nil {
			return err
		}
		var g any
		if err := json.Unmarshal(b, &g); err != nil {
			return err
		}
		return encode(g, sb)
	}
	return nil
}

func encodeNumber(x float64, sb *strings.Builder) error {
	if math.IsNaN(x) || math.IsInf(x, 0) {
		return strconv.ErrRange
	}
	if x == 0 {
		sb.WriteByte('0')
		return nil
	}
	// strconv with -1 precision is shortest-round-trip; strip integral ".0"
	// is unnecessary because Go never emits it, but trim "+0" exponents.
	s := strconv.FormatFloat(x, 'g', -1, 64)
	sb.WriteString(s)
	return nil
}

// ContextIdentity returns "sha256:" + hex(SHA-256(CanonicalJSON(ctx_L01)))
// (§10.2).
func ContextIdentity(ctx AgentContext) (string, error) {
	stripped := stripToL01(ctx)
	js, err := CanonicalJSON(stripped)
	if err != nil {
		return "", err
	}
	sum := sha256.Sum256([]byte(js))
	return "sha256:" + hex.EncodeToString(sum[:]), nil
}

func stripToL01(ctx AgentContext) map[string]any {
	hp := ctx.InterceptionPoint()
	keep := make(map[string]struct{}, len(l0)+4)
	for k := range l0 {
		keep[k] = struct{}{}
	}
	for _, k := range l1[hp] {
		keep[k] = struct{}{}
	}
	out := make(map[string]any, len(keep))
	for k, v := range ctx {
		if _, ok := keep[k]; !ok {
			continue
		}
		switch k {
		case "agent":
			out[k] = filterObj(v, l0Agent)
		case "session":
			out[k] = filterObj(v, l0Session)
		default:
			out[k] = v
		}
	}
	return out
}

func filterObj(v any, keep map[string]struct{}) any {
	m, ok := v.(map[string]any)
	if !ok {
		return v
	}
	out := make(map[string]any, len(keep))
	for k, vv := range m {
		if _, ok := keep[k]; ok {
			out[k] = vv
		}
	}
	return out
}
