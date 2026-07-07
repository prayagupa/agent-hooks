// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
//! CTK self-test: run all vectors against the Rust ReferenceHarness.
#![cfg(feature = "ctk")]

use agent_hooks::ctk::{load_vectors, run_vector, ReferenceHarness};

#[tokio::test]
async fn ctk_reference_all_vectors() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../conformance/vectors");
    let vectors = load_vectors(dir).expect("load vectors");
    assert!(vectors.len() >= 16, "expected >=16 vectors, got {}", vectors.len());
    for vector in &vectors {
        let mut harness = ReferenceHarness::new();
        let result = run_vector(&mut harness, vector).await;
        assert!(
            result.status == "pass" || result.status == "skip",
            "{}: {:?}",
            result.id,
            result.failures
        );
    }
}
