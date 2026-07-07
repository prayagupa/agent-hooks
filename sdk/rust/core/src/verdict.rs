// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
//! Verdict wire normalization (§5) and multi-interceptor combination (§7.1).

use crate::types::{Decision, Evidence, HostError, Transform, Verdict};
use serde_json::Value;

/// Parse and validate an interceptor's wire return value into a `Verdict`
/// per §5. Any violation yields `HostError::VerdictInvalid` with a detail
/// message; the caller synthesizes the `deny` verdict.
pub fn from_wire(raw: &Value) -> Result<Verdict, (HostError, String)> {
    let obj = raw
        .as_object()
        .ok_or((HostError::VerdictInvalid, "verdict must be a JSON object".into()))?;

    let decision = match obj.get("decision").and_then(Value::as_str) {
        Some("allow") => Decision::Allow,
        Some("deny") => Decision::Deny,
        Some("warn") => Decision::Warn,
        Some("escalate") => Decision::Escalate,
        Some("transform") => Decision::Transform,
        other => {
            return Err((
                HostError::VerdictInvalid,
                format!("verdict.decision invalid: {other:?}"),
            ))
        }
    };

    let reason = opt_string(obj, "reason")?;
    if let Some(r) = &reason {
        if r.starts_with("host_error:") {
            return Err((
                HostError::VerdictInvalid,
                "verdict.reason MUST NOT start with 'host_error:' (§5)".into(),
            ));
        }
    }
    let message = opt_string(obj, "message")?;

    let transform = match obj.get("transform") {
        None | Some(Value::Null) => None,
        Some(Value::Object(t)) => {
            let path = t
                .get("path")
                .and_then(Value::as_str)
                .ok_or((HostError::VerdictInvalid, "transform.path missing".into()))?;
            if !(path.starts_with("$target") || path.starts_with("$policy_target")) {
                return Err((
                    HostError::TransformTargetForbidden,
                    format!("transform.path must be rooted at $target (got {path:?})"),
                ));
            }
            let value = t
                .get("value")
                .cloned()
                .ok_or((HostError::VerdictInvalid, "transform.value missing".into()))?;
            Some(Transform {
                path: path.to_string(),
                value,
            })
        }
        Some(_) => {
            return Err((
                HostError::VerdictInvalid,
                "verdict.transform must be {path, value}".into(),
            ))
        }
    };

    match (decision, &transform) {
        (Decision::Transform, None) => {
            return Err((
                HostError::VerdictInvalid,
                "transform body REQUIRED when decision=='transform' (§5)".into(),
            ))
        }
        (d, Some(_)) if d != Decision::Transform => {
            return Err((
                HostError::VerdictInvalid,
                "transform body FORBIDDEN when decision!='transform' (§5)".into(),
            ))
        }
        _ => {}
    }

    let evidence = match obj.get("evidence") {
        None | Some(Value::Null) => None,
        Some(Value::Object(_)) => Some(
            serde_json::from_value::<Evidence>(obj["evidence"].clone())
                .map_err(|e| (HostError::VerdictInvalid, format!("evidence: {e}")))?,
        ),
        Some(_) => {
            return Err((
                HostError::VerdictInvalid,
                "verdict.evidence must be an object".into(),
            ))
        }
    };

    let result_labels = match obj.get("result_labels") {
        None | Some(Value::Null) => Vec::new(),
        Some(Value::Array(a)) => a
            .iter()
            .map(|v| {
                v.as_str().map(str::to_owned).ok_or((
                    HostError::VerdictInvalid,
                    "result_labels must be an array of strings".into(),
                ))
            })
            .collect::<Result<_, _>>()?,
        Some(_) => {
            return Err((
                HostError::VerdictInvalid,
                "result_labels must be an array of strings".into(),
            ))
        }
    };

    Ok(Verdict {
        decision,
        reason,
        message,
        transform,
        evidence,
        result_labels,
    })
}

fn opt_string(
    obj: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, (HostError, String)> {
    match obj.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err((
            HostError::VerdictInvalid,
            format!("verdict.{key} must be string or null"),
        )),
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn from_wire_allow() {
        let v = from_wire(&json!({"decision": "allow"})).unwrap();
        assert_eq!(v.decision, Decision::Allow);
    }

    #[test]
    fn from_wire_transform_roundtrip() {
        let v = from_wire(&json!({
            "decision": "transform",
            "reason": "redact",
            "transform": {"path": "$target.url", "value": "x"},
            "result_labels": ["pii"]
        }))
        .unwrap();
        assert_eq!(v.decision, Decision::Transform);
        assert_eq!(v.transform.as_ref().unwrap().path, "$target.url");
        assert_eq!(v.result_labels, vec!["pii"]);
    }

    #[test]
    fn from_wire_rejects_bad_decision() {
        assert!(from_wire(&json!({"decision": "maybe"})).is_err());
    }

    #[test]
    fn from_wire_rejects_host_error_reason() {
        let e = from_wire(&json!({"decision": "deny", "reason": "host_error:x"})).unwrap_err();
        assert_eq!(e.0, HostError::VerdictInvalid);
    }

    #[test]
    fn from_wire_transform_body_required() {
        assert!(from_wire(&json!({"decision": "transform"})).is_err());
    }

    #[test]
    fn from_wire_transform_body_forbidden() {
        assert!(from_wire(&json!({
            "decision": "allow",
            "transform": {"path": "$target.x", "value": 1}
        }))
        .is_err());
    }



}
