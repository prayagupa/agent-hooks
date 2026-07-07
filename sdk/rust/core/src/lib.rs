// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
//! agent-hooks: framework-neutral agent control contract.
//!
//! This crate is the **canonical implementation** of
//! [AGENT-HOOKS-0.1](../../../spec/AGENT-HOOKS-0.1.md). The Python,
//! TypeScript, .NET, and Go SDKs bind to it via FFI so that
//! `canonical_json`, `context_identity`, transform application, verdict
//! validation, and verdict combination have exactly one implementation.
//!
//! The FFI surface (`agent-hooks-ffi` crate, plus PyO3/napi bindings in
//! each language SDK) is JSON-string in / JSON-string out over the
//! functions in [`ffi_surface`], because [`AgentContext`] and [`Verdict`]
//! are already wire-shaped JSON.
//!
//! For Rust-native hosts this crate is also a full host SDK:
//! [`InterceptionEmitter`], [`AgentContextBuilder`], and (behind the
//! `ctk` feature) the CTK runner and [`ctk::ReferenceHarness`]. The
//! other languages implement the same per-language pieces over the FFI.

#![warn(clippy::all)]

mod builder;
mod canonical;
mod emitter;
mod enforce;
mod path;
mod types;
mod verdict;

#[cfg(feature = "ctk")]
pub mod ctk;

pub mod ctk_engine;
pub mod ffi_surface;

pub use builder::AgentContextBuilder;
pub use canonical::{canonical_json, context_identity};
pub use emitter::{InterceptionBlocked, InterceptionEmitter};
pub use enforce::{apply_transform_to_ctx, finalize, validate_transform};
pub use path::{apply as apply_transform_path, parse as parse_transform_path, resolve, Segment};
pub use types::{
    AgentContext, ApprovalOutcome, ApprovalRequest, ApprovalResolution, ApprovalResolver,
    Decision, EnforcementMode, Evidence, HostError, Interceptor, InterceptionPoint,
    InterceptionRecord, Transform, Verdict, SPEC_VERSION,
};
pub use verdict::from_wire as verdict_from_wire;
