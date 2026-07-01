// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
//! Stateless enforcement step (§6, §8, §10).
//!
//! Given an `AgentContext`, a combined `Verdict`, and an `EnforcementMode`,
//! this function applies the transform (in `enforce` mode), computes both
//! action identities, writes the transformed target back into the context's
//! L1 field per §4.3, and returns an `InterceptionRecord`.
//!
//! Interceptor dispatch and approval-seam resolution (§7, §9) stay in the
//! per-language wrapper because they are callbacks into user code; this
//! function is the pure part every wrapper calls after dispatch.

use crate::canonical::context_identity;
use crate::path;
use crate::types::{
    AgentContext, Decision, EnforcementMode, HostError, InterceptionPoint, InterceptionRecord,
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

/// Apply §6/§8/§10 to a combined verdict. Mutates `ctx.target` (and the
/// aliased L1 field) on transform in `enforce` mode. Returns an
/// `InterceptionRecord`; the record's verdict may differ from the input
/// (rewritten to a `host_error:*` deny on transform failure per §11).
pub fn enforce(
    ctx: &mut AgentContext,
    verdict: Verdict,
    mode: EnforcementMode,
) -> InterceptionRecord {
    let ip: InterceptionPoint = ctx
        .get("interception_point")
        .and_then(Value::as_str)
        .and_then(|s| s.parse().ok())
        .unwrap_or(InterceptionPoint::AgentStartup);

    let input_identity = context_identity(ctx);
    let mut enforced_identity = input_identity.clone();
    let mut transformed_target: Option<Value> = None;
    let mut verdict = verdict;

    if verdict.decision == Decision::Transform {
        if !ip.transform_permitted() {
            verdict = Verdict::host_error(HostError::TransformTargetForbidden, None);
        } else if let Some(t) = &verdict.transform {
            let target = ctx.get("target").cloned().unwrap_or(Value::Null);
            match path::apply(target, &t.path, t.value.clone()) {
                Ok(applied) => {
                    if mode == EnforcementMode::Enforce {
                        ctx.insert("target".into(), applied.clone());
                        write_back_target(ip, ctx, &applied);
                        enforced_identity = context_identity(ctx);
                        transformed_target = Some(applied);
                    }
                    // evaluate_only: validated but not applied (§8)
                }
                Err(e) => {
                    verdict = Verdict::host_error(e, None);
                }
            }
        }
    }

    InterceptionRecord {
        interception_point: ip,
        mode,
        verdict,
        input_identity,
        enforced_identity,
        transformed_target,
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
    use crate::types::Transform;
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
        let mut c = ctx("pre_tool_call", json!({"url": "x"}));
        let r = enforce(&mut c, Verdict::allow(), EnforcementMode::Enforce);
        assert_eq!(r.input_identity, r.enforced_identity);
        assert!(r.proceeds());
    }

    #[test]
    fn transform_applies_and_writes_back() {
        let mut c = ctx("pre_tool_call", json!({"url": "evil"}));
        let v = Verdict {
            decision: Decision::Transform,
            transform: Some(Transform {
                path: "$target.url".into(),
                value: json!("safe"),
            }),
            ..Verdict::allow()
        };
        let r = enforce(&mut c, v, EnforcementMode::Enforce);
        assert_ne!(r.input_identity, r.enforced_identity);
        assert_eq!(c["target"]["url"], json!("safe"));
        assert_eq!(c["tool_call"]["args"]["url"], json!("safe"));
        assert_eq!(r.transformed_target.unwrap()["url"], json!("safe"));
    }

    #[test]
    fn transform_forbidden_at_startup() {
        let mut c = ctx("agent_startup", json!({}));
        let v = Verdict {
            decision: Decision::Transform,
            transform: Some(Transform {
                path: "$target.x".into(),
                value: json!(1),
            }),
            ..Verdict::allow()
        };
        let r = enforce(&mut c, v, EnforcementMode::Enforce);
        assert_eq!(
            r.verdict.reason.as_deref(),
            Some("host_error:transform_target_forbidden")
        );
        assert!(!r.proceeds());
    }

    #[test]
    fn evaluate_only_validates_but_does_not_apply() {
        let mut c = ctx("pre_tool_call", json!({"url": "evil"}));
        let v = Verdict {
            decision: Decision::Transform,
            transform: Some(Transform {
                path: "$target.url".into(),
                value: json!("safe"),
            }),
            ..Verdict::allow()
        };
        let r = enforce(&mut c, v, EnforcementMode::EvaluateOnly);
        assert_eq!(r.input_identity, r.enforced_identity);
        assert_eq!(c["target"]["url"], json!("evil"));
        assert!(r.transformed_target.is_none());
        assert!(r.proceeds());
    }
}
