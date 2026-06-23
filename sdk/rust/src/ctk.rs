// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
//! Conformance Test Kit harness contract (§13.2).
//!
//! The Rust CTK runner is not yet implemented; this module defines the
//! `Harness` trait so framework adapters can be written now. Track the
//! runner at <https://github.com/responsibleai/agent-hooks/issues/2>;
//! the Python implementation at `sdk/python/src/agent_hooks/ctk/runner.py`
//! is the reference.

use crate::{EnforcementMode, HookConsumer};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;

/// Host-declared capability subset (§3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    ModelCalls,
    ToolCalls,
    ParallelToolCalls,
    Streaming,
    MultiTurn,
}

/// Outcome of one harness run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    Completed,
    Blocked,
    Suspended,
    Error,
}

/// Hermetic scripted run loaded from a CTK vector. Wire-shaped.
#[derive(Debug, Clone, Deserialize)]
pub struct Scenario {
    pub input: Value,
    #[serde(default)]
    pub tools: Vec<Value>,
    #[serde(default)]
    pub model_script: Vec<Value>,
}

/// What `Harness::run` returns to the CTK runner.
#[derive(Debug, Clone)]
pub struct RunRecord {
    pub outcome: RunOutcome,
    pub final_output: Option<Value>,
    pub tool_invocations: Vec<Value>,
    pub error: Option<String>,
}

/// Approval resolver supplied by the CTK (replays a vector's `approval_script`).
#[async_trait]
pub trait ApprovalResolver: Send + Sync {
    async fn resolve(&self, request: crate::ApprovalRequest<'_>) -> crate::ApprovalResolution;
}

/// The single trait a framework adapter implements for the CTK.
#[async_trait]
pub trait Harness: Send {
    /// Framework identifier (e.g., `"rig"`).
    fn name(&self) -> &str;

    /// Capabilities this host supports (§3.2).
    fn capabilities(&self) -> HashSet<Capability>;

    /// Wire mock model + tools from `scenario`, register `consumer` and
    /// `resolver`, and set enforcement mode.
    fn setup(
        &mut self,
        scenario: Scenario,
        consumer: Box<dyn HookConsumer>,
        resolver: Option<Box<dyn ApprovalResolver>>,
        mode: EnforcementMode,
    );

    /// Execute one session; return what happened.
    async fn run(&mut self) -> RunRecord;

    /// Tear down anything `setup` created.
    fn teardown(&mut self);
}
