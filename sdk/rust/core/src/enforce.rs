// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
//! Stateless enforcement primitives (§6, §7.1, §8, §10).
//!
//! With §7.1 sequential fold-through, transform application happens
//! **during** interceptor dispatch (each interceptor sees the prior
//! transforms' effect), so the per-language emitter loop calls
//! [`apply_transform_to_ctx`] between interceptors (in `enforce` mode)
//! and [`finalize`] once at the end to compute identities and build the
//! [`InterceptionRecord`]. Both are pure; everything that calls back
//! into user code stays in the wrapper.

use crate::canonical::context_identity;
use crate::path;
use crate::types::{
    AgentContext, EnforcementMode, HostError, InterceptionPoint, InterceptionRecord, Transform,
    Verdict,
};
use serde_json::Value;
use std::str::FromStr;

impl FromStr for InterceptionPoint {
    type Err = HostError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "agent_startup" => Ok(Self::AgentStartup),
            "input" => Ok(Self::Input),
            "pre_model_call" => Ok(Self::PreModelCall),
            "post_model_call" => Ok(Self::PostModelCall),
            "pre_tool_call" => Ok(Self::PreToolCall),
            "post_tool_call" => Ok(Self::PostToolCall),
            "output" => Ok(Self::Output),
            "agent_shutdown" => Ok(Self::AgentShutdown),
            _ => Err(HostError::ContextInvalid),
        }
    }
}

fn interception_point_of(ctx: &AgentContext) -> Result<InterceptionPoint, HostError> {
    ctx.get("interception_point")
        .and_then(Value::as_str)
        .ok_or(HostError::ContextInvalid)?
        .parse()
}

/// Apply one `transform` to the context's `target` and mirror it into
/// the L1 field it aliases (§4.3, §5.2). Fails closed on a forbidden
/// point or unresolvable path; the caller synthesizes the `host_error`
/// deny. In `evaluate_only` mode callers use [`validate_transform`]
/// instead (§8: validated, not applied).
pub fn apply_transform_to_ctx(
    ctx: &mut AgentContext,
    transform: &Transform,
) -> Result<(), HostError> {
    let ip = interception_point_of(ctx)?;
    if !ip.transform_permitted() {
        return Err(HostError::TransformTargetForbidden);
    }
    let target = ctx.get("target").cloned().unwrap_or(Value::Null);
    let applied = path::apply(target, &transform.path, transform.value.clone())?;
    ctx.insert("target".into(), applied.clone());
    write_back_target(ip, ctx, &applied);
    Ok(())
}

/// §8 `evaluate_only`: validate a transform against the current target
/// without applying it.
pub fn validate_transform(ctx: &AgentContext, transform: &Transform) -> Result<(), HostError> {
    let ip = interception_point_of(ctx)?;
    if !ip.transform_permitted() {
        return Err(HostError::TransformTargetForbidden);
    }
    let target = ctx.get("target").cloned().unwrap_or(Value::Null);
    path::apply(target, &transform.path, transform.value.clone()).map(|_| ())
}

/// Build the [`InterceptionRecord`] for one completed interception
/// (§6, §10). `input_identity` MUST have been computed from the context
/// **before** interceptor dispatch (§10.2); `enforced_identity` is
/// computed here from the post-fold context, so the two differ exactly
/// when a transform was applied.
pub fn finalize(
    ctx: &AgentContext,
    verdict: Verdict,
    mode: EnforcementMode,
    input_identity: &str,
) -> InterceptionRecord {
    let ip = interception_point_of(ctx).unwrap_or(InterceptionPoint::AgentStartup);
    InterceptionRecord {
        interception_point: ip,
        mode,
        verdict,
        input_identity: input_identity.to_owned(),
        enforced_identity: context_identity(ctx),
    }
}

/// Mirror the transformed target back into the L1 field it aliases (§4.3).
fn write_back_target(ip: InterceptionPoint, ctx: &mut AgentContext, transformed: &Value) {
    match ip {
        InterceptionPoint::Input => {
            ctx.insert("input".into(), transformed.clone());
        }
        InterceptionPoint::PreModelCall => {
            ctx.insert("messages".into(), transformed.clone());
        }
        InterceptionPoint::PostModelCall => {
            ctx.insert("response".into(), transformed.clone());
        }
        InterceptionPoint::PreToolCall => {
            if let Some(Value::Object(tc)) = ctx.get_mut("tool_call") {
                tc.insert("args".into(), transformed.clone());
            }
        }
        InterceptionPoint::PostToolCall => {
            if let Some(Value::Object(tr)) = ctx.get_mut("tool_result") {
                tr.insert("value".into(), transformed.clone());
            }
        }
        InterceptionPoint::Output => {
            ctx.insert("output".into(), transformed.clone());
        }
        InterceptionPoint::AgentStartup | InterceptionPoint::AgentShutdown => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Decision;
    use serde_json::json;

    fn ctx(ip: &str, target: Value) -> AgentContext {
        json!({
            "spec": "agent-hooks/0.1",
            "interception_point": ip,
            "timestamp": "2026-01-01T00:00:00Z",
            "sequence": 0,
            "agent": {"id": "a", "framework": "test"},
            "session": {"id": "s"},
            "target": target,
            "tool_call": {"id": "tc-1", "name": "t", "args": target}
        })
        .as_object()
        .unwrap()
        .clone()
    }

    #[test]
    fn allow_identities_equal() {
        let c = ctx("pre_tool_call", json!({"url": "x"}));
        let input_id = context_identity(&c);
        let r = finalize(&c, Verdict::allow(), EnforcementMode::Enforce, &input_id);
        assert_eq!(r.input_identity, r.enforced_identity);
        assert!(r.proceeds());
    }

    #[test]
    fn transform_applies_and_writes_back() {
        let mut c = ctx("pre_tool_call", json!({"url": "evil"}));
        let input_id = context_identity(&c);
        let t = Transform {
            path: "$target.url".into(),
            value: json!("safe"),
        };
        apply_transform_to_ctx(&mut c, &t).unwrap();
        assert_eq!(c["target"]["url"], json!("safe"));
        assert_eq!(c["tool_call"]["args"]["url"], json!("safe"));
        let v = Verdict {
            decision: Decision::Transform,
            transform: Some(t),
            ..Verdict::allow()
        };
        let r = finalize(&c, v, EnforcementMode::Enforce, &input_id);
        assert_ne!(r.input_identity, r.enforced_identity);
    }

    #[test]
    fn transform_forbidden_at_startup() {
        let mut c = ctx("agent_startup", json!({}));
        let t = Transform {
            path: "$target.x".into(),
            value: json!(1),
        };
        assert_eq!(
            apply_transform_to_ctx(&mut c, &t),
            Err(HostError::TransformTargetForbidden)
        );
    }

    #[test]
    fn evaluate_only_validates_without_applying() {
        let c = ctx("pre_tool_call", json!({"url": "evil"}));
        let t = Transform {
            path: "$target.url".into(),
            value: json!("safe"),
        };
        validate_transform(&c, &t).unwrap();
        assert_eq!(c["target"]["url"], json!("evil"));
        assert_eq!(
            validate_transform(&c, &Transform { path: "$target.missing.x".into(), value: json!(0) }),
            Err(HostError::TransformInvalid)
        );
    }
}
