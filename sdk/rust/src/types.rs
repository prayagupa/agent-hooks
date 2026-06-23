// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
//! Core types for AGENT-HOOKS-0.1 (§3, §5, §7, §8, §9, §11).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Spec version this crate implements (§4.1 `spec` field).
pub const SPEC_VERSION: &str = "agent-hooks/0.1";

/// The closed set of agent lifecycle hook points (§3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookPoint {
    AgentStartup,
    Input,
    PreModelCall,
    PostModelCall,
    PreToolCall,
    PostToolCall,
    Output,
    AgentShutdown,
}

impl HookPoint {
    /// Wire name (snake_case).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AgentStartup => "agent_startup",
            Self::Input => "input",
            Self::PreModelCall => "pre_model_call",
            Self::PostModelCall => "post_model_call",
            Self::PreToolCall => "pre_tool_call",
            Self::PostToolCall => "post_tool_call",
            Self::Output => "output",
            Self::AgentShutdown => "agent_shutdown",
        }
    }

    /// Whether a `transform` verdict is permitted at this point (§3, §4.3).
    pub fn transform_permitted(self) -> bool {
        !matches!(self, Self::AgentStartup | Self::AgentShutdown)
    }
}

/// Verdict decision values (§5.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Allow,
    Deny,
    Warn,
    Escalate,
    Transform,
}

impl Decision {
    /// Whether the action proceeds under this decision (§2 permit class).
    pub fn permits(self) -> bool {
        matches!(self, Self::Allow | Self::Warn | Self::Transform)
    }
}

/// Whether the host acts on verdicts (§8).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnforcementMode {
    Enforce,
    EvaluateOnly,
}

/// Reserved `hook_error:*` reasons a host synthesizes (§11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum HookError {
    #[error("hook_error:context_invalid")]
    ContextInvalid,
    #[error("hook_error:consumer_failed")]
    ConsumerFailed,
    #[error("hook_error:consumer_timeout")]
    ConsumerTimeout,
    #[error("hook_error:verdict_invalid")]
    VerdictInvalid,
    #[error("hook_error:transform_invalid")]
    TransformInvalid,
    #[error("hook_error:transform_target_forbidden")]
    TransformTargetForbidden,
    #[error("hook_error:approval_resolver_missing")]
    ApprovalResolverMissing,
    #[error("hook_error:approval_resolver_failed")]
    ApprovalResolverFailed,
    #[error("hook_error:approval_unresolved")]
    ApprovalUnresolved,
    #[error("hook_error:approval_action_mismatch")]
    ApprovalActionMismatch,
    #[error("hook_error:adapter_unsupported")]
    AdapterUnsupported,
    #[error("hook_error:streaming_unsupported")]
    StreamingUnsupported,
}

/// A single `$target`-rooted replacement (§5.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    /// Path rooted at `$target` (or the deprecated `$policy_target` alias).
    pub path: String,
    /// New value to set at `path`.
    pub value: Value,
}

/// Opaque pointer to an offline-verifiable artefact (§5.3).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artefact: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub verification_pointers: BTreeMap<String, String>,
}

/// Consumer return value (§5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Verdict {
    pub decision: Decision,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transform: Option<Transform>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Evidence>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub result_labels: Vec<String>,
}

impl Verdict {
    /// The trivial permit verdict.
    pub const fn allow() -> Self {
        Self {
            decision: Decision::Allow,
            reason: None,
            message: None,
            transform: None,
            evidence: None,
            result_labels: Vec::new(),
        }
    }

    /// Host-synthesized deny verdict for a §11 failure.
    pub fn hook_error(err: HookError, message: Option<String>) -> Self {
        Self {
            decision: Decision::Deny,
            reason: Some(err.to_string()),
            message,
            transform: None,
            evidence: None,
            result_labels: Vec::new(),
        }
    }

    /// Validate per §5; returns `Err(HookError::VerdictInvalid)` on violation.
    pub fn validate(&self) -> Result<(), HookError> {
        if let Some(r) = &self.reason {
            if r.starts_with("hook_error:") {
                return Err(HookError::VerdictInvalid);
            }
        }
        match (self.decision, &self.transform) {
            (Decision::Transform, None) | (_, Some(_)) if self.decision != Decision::Transform => {
                Err(HookError::VerdictInvalid)
            }
            _ => Ok(()),
        }
    }
}

/// Wire-shaped hook context (§4). Use `serde_json::Map` so it round-trips to
/// the schema without translation; helpers in `canonical.rs` operate on it.
pub type HookContext = serde_json::Map<String, Value>;

/// Host-side record of one hook evaluation (§6, §10).
#[derive(Debug, Clone, Serialize)]
pub struct HookResult {
    pub hook_point: HookPoint,
    pub mode: EnforcementMode,
    pub verdict: Verdict,
    pub input_identity: String,
    pub enforced_identity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transformed_target: Option<Value>,
}

impl HookResult {
    /// Whether the guarded action executes (§6, §8).
    pub fn proceeds(&self) -> bool {
        matches!(self.mode, EnforcementMode::EvaluateOnly) || self.verdict.decision.permits()
    }
}

/// Consumer protocol (§7).
#[async_trait]
pub trait HookConsumer: Send + Sync {
    /// Receive a `HookContext` and return a `Verdict`.
    async fn on_hook(&self, context: &HookContext) -> Verdict;
}

/// Approval resolver outcome (§9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalOutcome {
    Approve,
    Reject,
    Unresolved,
}

/// What the host hands the resolver on `escalate` (§9).
#[derive(Debug, Clone)]
pub struct ApprovalRequest<'a> {
    pub context_identity: String,
    pub hook_point: HookPoint,
    pub verdict: &'a Verdict,
    pub context: &'a HookContext,
}

/// What the resolver returns (§9).
#[derive(Debug, Clone)]
pub struct ApprovalResolution {
    pub outcome: ApprovalOutcome,
    pub context_identity: String,
    pub verdict: Option<Verdict>,
}
