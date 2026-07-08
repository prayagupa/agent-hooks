// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
//! Host-side emitter for Rust-native hosts: dispatch context →
//! interceptors → verdict → record (§6–§9).
//!
//! Unlike the FFI-bound SDKs, this emitter calls the crate's primitives
//! directly (no JSON round-trip). Semantics are identical and pinned by
//! the same CTK vectors:
//!
//! §7.1 sequential fold-through: interceptors run in registration order;
//! each receives its own copy of the context as it stands *after* prior
//! transforms were applied. The first block verdict short-circuits.
//!
//! Fail-closed defaults: an `enforce`-mode emission with zero registered
//! interceptors yields `deny host_error:no_interceptor` (§7), and
//! [`InterceptionEmitter::emit`] returns `Err(InterceptionBlocked)` on
//! any block — the ignorable-record variant is the explicitly named
//! [`InterceptionEmitter::emit_unchecked`].
//!
//! # Timeouts (§7)
//!
//! Unlike the Python/TypeScript/.NET/Go emitters, this emitter does
//! **not** enforce the §7 RECOMMENDED 5000 ms interceptor/resolver
//! timeout: the crate is runtime-agnostic (no tokio/async-std
//! dependency), and a portable future timeout requires a timer driver.
//! Rust hosts own the timeout at the interceptor boundary — wrap each
//! implementation with the host runtime's timeout and map the breach to
//! the reserved reasons, e.g. under tokio:
//!
//! ```ignore
//! // inside your Interceptor::intercept impl
//! match tokio::time::timeout(Duration::from_millis(5000), inner.intercept(ctx)).await {
//!     Ok(v) => v,
//!     Err(_) => Verdict::host_error(HostError::InterceptorTimeout, None),
//! }
//! ```
//!
//! The wrapped verdict flows through the normal §6.3 fail-closed path.

use crate::canonical::context_identity;
use crate::enforce::{apply_transform_to_ctx, finalize, validate_transform};
use crate::types::{
    AgentContext, ApprovalOutcome, ApprovalRequest, ApprovalResolver, Decision, EnforcementMode,
    HostError, Interceptor, InterceptionPoint, InterceptionRecord, Verdict,
};
use serde_json::Value;
use std::fmt;

/// Returned by [`InterceptionEmitter::emit`] when a verdict blocks the
/// guarded action (§6).
#[derive(Debug, Clone)]
pub struct InterceptionBlocked {
    pub record: InterceptionRecord,
}

impl fmt::Display for InterceptionBlocked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} blocked: {} ({})",
            self.record.interception_point.as_str(),
            self.record.verdict.decision.as_str(),
            self.record.verdict.reason.as_deref().unwrap_or("no reason"),
        )
    }
}

impl std::error::Error for InterceptionBlocked {}

/// Whether a verdict was synthesized by the host (§11) rather than
/// returned by an interceptor or resolver.
fn is_host_synthesized(v: &Verdict) -> bool {
    v.reason
        .as_deref()
        .is_some_and(|r| r.starts_with("host_error:"))
}

/// Host-side helper that implements §6–§9 once so adapters don't have
/// to. One instance per session.
pub struct InterceptionEmitter {
    interceptors: Vec<Box<dyn Interceptor>>,
    resolver: Option<Box<dyn ApprovalResolver>>,
    mode: EnforcementMode,
    records: Vec<InterceptionRecord>,
}

impl InterceptionEmitter {
    pub fn new(mode: EnforcementMode, resolver: Option<Box<dyn ApprovalResolver>>) -> Self {
        Self {
            interceptors: Vec::new(),
            resolver,
            mode,
            records: Vec::new(),
        }
    }

    pub fn mode(&self) -> EnforcementMode {
        self.mode
    }

    /// All interception records emitted so far in this session, in order.
    pub fn records(&self) -> &[InterceptionRecord] {
        &self.records
    }

    pub fn register(&mut self, interceptor: Box<dyn Interceptor>) -> &mut Self {
        self.interceptors.push(interceptor);
        self
    }

    // -------------------------------------------------------------------------

    /// Run the interception and return `Err(InterceptionBlocked)` if the
    /// guarded action must not proceed (§6). Primary entry point; the
    /// safe path is the default.
    pub async fn emit(
        &mut self,
        ctx: &mut AgentContext,
    ) -> Result<InterceptionRecord, InterceptionBlocked> {
        let record = self.emit_unchecked(ctx).await;
        if record.proceeds() {
            Ok(record)
        } else {
            Err(InterceptionBlocked { record })
        }
    }

    /// Run the interception and return the record without a block error.
    /// The caller MUST inspect [`InterceptionRecord::proceeds`] and halt
    /// the guarded action itself; prefer [`Self::emit`].
    pub async fn emit_unchecked(&mut self, ctx: &mut AgentContext) -> InterceptionRecord {
        // §10.2: input identity binds to the context BEFORE dispatch, so
        // neither interceptor mutation nor fold-through can retroactively
        // alter what the record claims was evaluated.
        let input_id = context_identity(ctx);

        let (mut verdict, mut decided_by) = self.dispatch(ctx).await;

        // §6.1a: nothing to approve at agent_shutdown.
        let at_shutdown = ctx.get("interception_point").and_then(serde_json::Value::as_str)
            == Some("agent_shutdown");
        if verdict.decision == Decision::Escalate
            && self.mode == EnforcementMode::Enforce
            && !at_shutdown
        {
            // §9/NOW-14: approval binds to the escalation-time identity
            // (post prior fold transforms) — what the resolver actually sees.
            let escalation_id = crate::context_identity(ctx);
            verdict = self.resolve_escalate(ctx, verdict, &escalation_id).await;
            // An approve MAY carry a transform (§9); it is subject to the
            // same fold rules as an interceptor transform.
            if verdict.decision == Decision::Transform {
                verdict = self.fold_transform(ctx, verdict);
            }
            // A resolver-substituted verdict keeps the escalating
            // interceptor's index; host-synthesized failures do not.
            if is_host_synthesized(&verdict) {
                decided_by = None;
            }
        }

        let record = finalize(ctx, verdict, self.mode, &input_id, decided_by);
        self.records.push(record.clone());
        record
    }

    // -------------------------------------------------------------------------

    /// §7 dispatch with §7.1 sequential fold-through. Returns the
    /// combined verdict and the registration index of the deciding
    /// interceptor (`None` for pure allow or host-synthesized).
    async fn dispatch(&self, ctx: &mut AgentContext) -> (Verdict, Option<u32>) {
        if self.interceptors.is_empty() {
            // §7: zero interceptors fails closed. Register an explicit
            // allow-all interceptor for a deliberate passthrough.
            return (Verdict::host_error(HostError::NoInterceptor, None), None);
        }

        let mut combined = Verdict::allow();
        let mut decided_by: Option<u32> = None;
        for (i, interceptor) in self.interceptors.iter().enumerate() {
            let i = i as u32;
            // §7.1/N05: each interceptor gets its own copy — in-place
            // mutation of the copy cannot alter enforcement.
            let copy = ctx.clone();
            let v = interceptor.intercept(&copy).await;
            if v.validate().is_err() {
                // §5 gate; the interceptor trait is infallible so the
                // only failure mode is a malformed verdict shape.
                return (Verdict::host_error(HostError::VerdictInvalid, None), None);
            }

            if !v.decision.permits() {
                return (v, Some(i)); // first block short-circuits (§7.1)
            }
            if v.decision == Decision::Transform {
                let v = self.fold_transform(ctx, v);
                if !v.decision.permits() {
                    return (v, None); // transform failed closed (host-synthesized)
                }
                combined = v;
                decided_by = Some(i);
            } else if v.decision == Decision::Warn && combined.decision == Decision::Allow {
                combined = v;
                decided_by = Some(i);
            }
        }
        (combined, decided_by)
    }

    /// Apply (enforce) or validate (evaluate_only) one transform (§7.1, §8).
    fn fold_transform(&self, ctx: &mut AgentContext, v: Verdict) -> Verdict {
        let t = match &v.transform {
            Some(t) => t.clone(),
            None => return Verdict::host_error(HostError::TransformInvalid, None),
        };
        let result = match self.mode {
            EnforcementMode::Enforce => apply_transform_to_ctx(ctx, &t),
            EnforcementMode::EvaluateOnly => validate_transform(ctx, &t),
        };
        match result {
            Ok(()) => v,
            Err(e) => Verdict::host_error(e, Some(t.path)),
        }
    }

    async fn resolve_escalate(
        &self,
        ctx: &AgentContext,
        verdict: Verdict,
        identity: &str,
    ) -> Verdict {
        let Some(resolver) = &self.resolver else {
            return Verdict::host_error(HostError::ApprovalResolverMissing, None);
        };
        let ip: InterceptionPoint = ctx
            .get("interception_point")
            .and_then(Value::as_str)
            .and_then(|s| s.parse().ok())
            .unwrap_or(InterceptionPoint::AgentStartup);
        let res = resolver
            .resolve(ApprovalRequest {
                context_identity: identity.to_owned(),
                interception_point: ip,
                verdict: &verdict,
                context: ctx,
            })
            .await;
        if res.context_identity != identity {
            return Verdict::host_error(HostError::ApprovalActionMismatch, None);
        }
        let Some(rv) = res.verdict else {
            return Verdict::host_error(HostError::ApprovalUnresolved, None);
        };
        if res.outcome == ApprovalOutcome::Unresolved {
            return Verdict::host_error(HostError::ApprovalUnresolved, None);
        }
        // §9/N04: the resolver's verdict crosses the same §5 gate as an
        // interceptor's.
        if rv.validate().is_err() {
            return Verdict::host_error(HostError::VerdictInvalid, None);
        }
        rv
    }
}
