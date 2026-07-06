// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

package conformance

// CTK self-test: run all Level<=2 vectors against the in-tree
// ReferenceHarness. Assertion engine and scripted interceptor live in
// the Rust core; this proves the Go emitter/runner wiring end-to-end.

import (
	"context"
	"path/filepath"
	"strings"
	"testing"
)

func TestReferenceHarnessConformance(t *testing.T) {
	dir := filepath.Join("..", "..", "..", "conformance", "vectors")
	vectors, err := LoadVectors(dir, 2)
	if err != nil {
		t.Fatalf("LoadVectors: %v", err)
	}
	if len(vectors) == 0 {
		t.Fatalf("no vectors found under %s", dir)
	}
	ctx := context.Background()
	for _, v := range vectors {
		id, _ := v["id"].(string)
		t.Run(id, func(t *testing.T) {
			r, err := RunVector(ctx, NewReferenceHarness(), v)
			if err != nil {
				t.Fatalf("RunVector: %v", err)
			}
			switch r.Status {
			case "pass":
				// ok
			case "skip":
				t.Skipf("%s", r.Detail)
			default:
				t.Fatalf("status=%s\n  - %s", r.Status, strings.Join(r.Failures, "\n  - "))
			}
		})
	}
}
