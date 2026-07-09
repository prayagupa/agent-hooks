// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
//! CTK self-test: run all vectors against the Rust ReferenceHarness.
#![cfg(feature = "ctk")]

use agent_hooks::ctk::{load_vectors, run_vector, ReferenceHarness};

#[tokio::test]
async fn ctk_reference_all_vectors() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../conformance/vectors");
    let vectors = load_vectors(dir).expect("load vectors");
    assert!(vectors.len() >= 30, "expected >=30 vectors, got {}", vectors.len());
    let mut unexpected: Vec<String> = Vec::new();
    let mut skipped = 0usize;
    for vector in &vectors {
        let mut harness = ReferenceHarness::new();
        let result = run_vector(&mut harness, vector).await;
        if result.status == "skip" {
            skipped += 1;
            continue;
        }
        if result.status != "pass" {
            unexpected.push(format!("{}: {:?}", result.id, result.failures));
        }
    }
    assert!(unexpected.is_empty(), "{unexpected:#?}");
    // Rust holds i64: no capability-skips are expected here.
    assert_eq!(skipped, 0, "unexpected skips in the Rust self-test");
}
