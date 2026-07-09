// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
//! CTK self-test: run all vectors against the Rust ReferenceHarness.
#![cfg(feature = "ctk")]

use agent_hooks::ctk::{load_vectors, run_vector, ReferenceHarness};

/// TODO(stage-4): vectors still authored in the pre-P-003 five-verdict
/// wire vocabulary (`warn`, `escalate`, `approval_resolver_missing`).
/// Stage 4 rewrites them to the three-verdict shapes (§5.1); until
/// then they fail the §5 gate by design (fail closed) and are excluded
/// here — and ONLY here.
/// NB: AH-CTK-031/032/072/073 still *run* green because their scripted
/// `escalate` now fails the §5 gate into a `verdict_invalid` deny and
/// they expected "blocked" — right outcome, wrong mechanism. They no
/// longer exercise the approval seam and MUST also be rewritten in
/// stage 4.
const TODO_STAGE_4: &[&str] = &[
    "AH-CTK-030", // escalate-approve → deny+approval / resolution
    "AH-CTK-050", // warn-passthrough → allow+warnings
];

#[tokio::test]
async fn ctk_reference_all_vectors() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../conformance/vectors");
    let vectors = load_vectors(dir).expect("load vectors");
    assert!(vectors.len() >= 16, "expected >=16 vectors, got {}", vectors.len());
    let mut unexpected: Vec<String> = Vec::new();
    for vector in &vectors {
        let mut harness = ReferenceHarness::new();
        let result = run_vector(&mut harness, vector).await;
        let ok = result.status == "pass" || result.status == "skip";
        let excluded = TODO_STAGE_4.iter().any(|id| result.id.starts_with(id));
        if !ok && !excluded {
            unexpected.push(format!("{}: {:?}", result.id, result.failures));
        }
        if ok && excluded {
            unexpected.push(format!(
                "{}: passes again — remove it from TODO_STAGE_4",
                result.id
            ));
        }
    }
    assert!(unexpected.is_empty(), "{unexpected:#?}");
}
