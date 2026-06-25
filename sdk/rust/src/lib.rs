// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
//! agent-hooks: framework-neutral agent lifecycle hook contract.
//!
//! Implements [AGENT-HOOKS-0.1](../../spec/AGENT-HOOKS-0.1.md). Lifted and
//! adapted from `policy-engine/core/src/{intervention_point.rs,verdict.rs}`.

#![forbid(unsafe_code)]
#![warn(clippy::all)]

mod canonical;
mod types;

#[cfg(feature = "ctk")]
pub mod ctk;

pub use canonical::{canonical_json, context_identity};
pub use types::{
    ApprovalOutcome, ApprovalRequest, ApprovalResolution, Decision, EnforcementMode, Evidence,
    Interceptor, AgentContext, HostError, InterceptionPoint, InterceptionRecord, Transform, Verdict, SPEC_VERSION,
};
