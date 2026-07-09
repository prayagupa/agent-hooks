// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

package conformance

// CTK self-test: run all vectors against the in-tree
// ReferenceHarness. Assertion engine and scripted interceptor live in
// the Rust core; this proves the Go emitter/runner wiring end-to-end.

import (
	"context"
	"path/filepath"
	"strings"
	"testing"
)

// TODO(stage-4): vectors still authored in the pre-P-003 five-verdict
// wire vocabulary (`warn`, `escalate`, `approval_resolver_missing`).
// Stage 4 rewrites them to the three-verdict shapes (§5.1); until then
// their scripted verdicts fail the §5 gate by design (fail closed) and
// no longer exercise the seam/warning semantics they were written for,
// so they are skipped here — and ONLY here.
var todoStage4 = []string{
	"AH-CTK-030", // escalate-approve → deny+approval / resolution
	"AH-CTK-031", // escalate-reject → deny+approval / reject
	"AH-CTK-032", // escalate-no-resolver → liftable deny stands
	"AH-CTK-050", // warn-passthrough → allow+warnings
	"AH-CTK-072", // resolver-identity-mismatch → echo rule via seam
	"AH-CTK-073", // resolver-raises → approval_resolver_failed via seam
}

func TestReferenceHarnessConformance(t *testing.T) {
	dir := filepath.Join("..", "..", "..", "conformance", "vectors")
	vectors, err := LoadVectors(dir)
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
			for _, stale := range todoStage4 {
				if strings.HasPrefix(id, stale) {
					t.Skipf("TODO(stage-4): %s uses the pre-P-003 verdict vocabulary", stale)
				}
			}
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
