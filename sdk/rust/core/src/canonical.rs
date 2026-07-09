// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
//! The `jcs-sha256` identity provider (§10.1–§10.2).
//!
//! §10.2's canonical form is RFC 8785 (JSON Canonicalization Scheme),
//! delegated to the `serde_jcs` crate: object members sorted by UTF-16
//! code units, numbers per ECMA-262 `Number::toString`, minimal string
//! escapes.
//!
//! The identity preimage is the **closed** required+conditional field
//! set for the context's interception point — including nested subfield
//! whitelists — so that adding any optional/namespaced data (top-level
//! or nested, e.g. `tool_result.duration_ms` or `model.params`) never
//! perturbs `context_identity`.
//!
//! Input domain (§10.2): fail closed, never normalize. RFC 8785 defines
//! canonical bytes only for I-JSON; an integral value outside ±(2⁵³−1)
//! in the projection would silently round (non-injective identity), so
//! the provider rejects it with a remediation-detail message.
//! Non-finite numbers and lone surrogates fail at the parse funnel
//! before this module runs.

use crate::types::{AgentContext, HostError};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt::Write;

/// Largest integer magnitude an IEEE-754 double represents exactly
/// (2⁵³−1); the I-JSON interoperability bound (§4.4, §10.2).
pub const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

/// Serialize `v` per §10.2 (RFC 8785 / JCS).
pub fn canonical_json(v: &Value) -> String {
    serde_jcs::to_string(v).expect("JCS serialization of in-memory Value cannot fail")
}

/// Field whitelist: `(key, allowed_subfields)`. `None` = keep value whole.
type Keep = (&'static str, Option<&'static [&'static str]>);

/// Required-core preimage fields (§4.1, §10.2).
const REQUIRED: &[Keep] = &[
    ("spec", None),
    ("interception_point", None),
    ("timestamp", None),
    ("sequence", None),
    ("agent", Some(&["id", "framework"])),
    ("session", Some(&["id"])),
    ("target", None),
];

/// Closed conditional (per-point) preimage (§4.2, §10.2). Mirrors the
/// per-point closed schemas in `spec/schema/agent-context/`.
fn conditional_for(ip: &str) -> &'static [Keep] {
    match ip {
        "agent_startup" => &[("agent_init", Some(&["tools_registered"]))],
        "input" => &[("input", Some(&["content", "role"]))],
        "pre_model_call" => &[("model", Some(&["id"])), ("messages", None)],
        "post_model_call" => &[
            ("model", Some(&["id"])),
            ("response", Some(&["content", "tool_calls", "finish_reason"])),
        ],
        "pre_tool_call" => &[("tool_call", Some(&["id", "name", "args"]))],
        "post_tool_call" => &[
            ("tool_call", Some(&["id", "name", "args"])),
            ("tool_result", Some(&["value", "is_error"])),
        ],
        "output" => &[("output", Some(&["content"]))],
        "agent_shutdown" => &[("summary", Some(&["reason"]))],
        _ => &[],
    }
}

fn filter_obj(v: &Value, keep: &[&str]) -> Value {
    match v {
        Value::Object(m) => Value::Object(
            m.iter()
                .filter(|(k, _)| keep.contains(&k.as_str()))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        ),
        other => other.clone(),
    }
}

/// The closed required+conditional projection of `ctx` (§10.2).
fn project_preimage(ctx: &AgentContext) -> Value {
    let ip = ctx
        .get("interception_point")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut out = serde_json::Map::new();
    for (key, sub) in REQUIRED.iter().chain(conditional_for(ip)) {
        if let Some(v) = ctx.get(*key) {
            let v = match sub {
                Some(keep) => filter_obj(v, keep),
                None => v.clone(),
            };
            out.insert((*key).to_owned(), v);
        }
    }
    Value::Object(out)
}

/// §10.2 input domain: reject integral values outside ±(2⁵³−1) anywhere
/// in the projection. serde_json parses integer literals as i64/u64, so
/// exactly those arms detect the lossy class; float-typed values were
/// already IEEE-754 doubles at the source and are canonicalizable.
fn check_i_json(v: &Value, path: &str) -> Result<(), (HostError, String)> {
    match v {
        Value::Number(n) => {
            let out_of_range = match (n.as_u64(), n.as_i64()) {
                (Some(u), _) => u > MAX_SAFE_INTEGER,
                (None, Some(i)) => i.unsigned_abs() > MAX_SAFE_INTEGER,
                _ => false, // f64: parse already made it a double
            };
            if out_of_range {
                return Err((
                    HostError::ContextInvalid,
                    format!(
                        "{path}: integer {n} exceeds 2^53; string-encode 64-bit identifiers, see spec §4.4"
                    ),
                ));
            }
            Ok(())
        }
        Value::Array(a) => {
            for (i, item) in a.iter().enumerate() {
                check_i_json(item, &format!("{path}[{i}]"))?;
            }
            Ok(())
        }
        Value::Object(m) => {
            for (k, item) in m {
                check_i_json(item, &format!("{path}.{k}"))?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// The `jcs-sha256` provider (§10.2):
/// `"sha256:" + hex(SHA-256(canonical_json(projection)))`, failing
/// closed (`host_error:context_invalid`) on a non-I-JSON projection.
pub fn context_identity(ctx: &AgentContext) -> Result<String, (HostError, String)> {
    let preimage = project_preimage(ctx);
    check_i_json(&preimage, "$")?;
    let json = canonical_json(&preimage);
    let digest = Sha256::digest(json.as_bytes());
    let mut out = String::with_capacity(7 + 64);
    out.push_str("sha256:");
    for b in digest {
        write!(out, "{b:02x}").expect("write hex");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx(target: Value) -> AgentContext {
        json!({
            "spec": "agent-hooks/0.1",
            "interception_point": "pre_tool_call",
            "timestamp": "t", "sequence": 0,
            "agent": {"id": "a", "framework": "x"},
            "session": {"id": "s"},
            "target": target,
            "tool_call": {"id": "tc", "name": "t", "args": target.clone()}
        })
        .as_object()
        .unwrap()
        .clone()
    }

    #[test]
    fn jcs_numbers() {
        // RFC 8785 §3.2.2.3 examples (ECMA-262 ToString).
        assert_eq!(canonical_json(&json!(1.0)), "1");
        assert_eq!(canonical_json(&json!(-0.0)), "0");
        assert_eq!(canonical_json(&json!(1e21)), "1e+21");
        assert_eq!(canonical_json(&json!(1e-7)), "1e-7");
        assert_eq!(canonical_json(&json!(0.000001)), "0.000001");
    }

    #[test]
    fn jcs_key_order_utf16() {
        // U+E000 (3-byte UTF-8) vs U+10000 (4-byte UTF-8, surrogates in
        // UTF-16): UTF-16 order puts the supplementary char FIRST.
        let v = json!({"\u{e000}": 1, "\u{10000}": 2});
        assert_eq!(canonical_json(&v), "{\"\u{10000}\":2,\"\u{e000}\":1}");
    }

    #[test]
    fn nested_optional_fields_stripped() {
        let ctx: AgentContext = json!({
            "spec": "agent-hooks/0.1",
            "interception_point": "post_tool_call",
            "timestamp": "t", "sequence": 5,
            "agent": {"id": "a", "framework": "x", "name": "optional"},
            "session": {"id": "s", "turn": 3},
            "target": "v",
            "tool_call": {"id": "tc", "name": "t", "args": {}, "content_hash": "sha256:00"},
            "tool_result": {"value": "v", "is_error": false, "duration_ms": 12.5}
        })
        .as_object()
        .unwrap()
        .clone();
        let mut bare = ctx.clone();
        // Remove every nested optional field; identity must be unchanged.
        bare.get_mut("tool_result").unwrap().as_object_mut().unwrap().remove("duration_ms");
        bare.get_mut("tool_call").unwrap().as_object_mut().unwrap().remove("content_hash");
        bare.get_mut("agent").unwrap().as_object_mut().unwrap().remove("name");
        bare.get_mut("session").unwrap().as_object_mut().unwrap().remove("turn");
        assert_eq!(
            context_identity(&ctx).unwrap(),
            context_identity(&bare).unwrap()
        );
    }

    #[test]
    fn rejects_integer_beyond_2_53() {
        // 2^53+1: JCS would round this to 2^53 (non-injective).
        let c = ctx(json!({"id": 9_007_199_254_740_993_i64}));
        let (e, detail) = context_identity(&c).unwrap_err();
        assert_eq!(e, HostError::ContextInvalid);
        assert!(detail.contains("string-encode 64-bit identifiers"), "{detail}");

        let c = ctx(json!({"id": -9_007_199_254_740_993_i64}));
        assert!(context_identity(&c).is_err());

        let c = ctx(json!({"id": 18_446_744_073_709_551_615_u64}));
        assert!(context_identity(&c).is_err());
    }

    #[test]
    fn accepts_boundary_and_string_encoded() {
        let c = ctx(json!({"id": 9_007_199_254_740_991_i64}));
        assert!(context_identity(&c).is_ok());
        let c = ctx(json!({"id": "9007199254740993"}));
        assert!(context_identity(&c).is_ok());
        let c = ctx(json!({"id": -9_007_199_254_740_991_i64}));
        assert!(context_identity(&c).is_ok());
    }

    #[test]
    fn optional_fields_not_checked() {
        // The domain check applies to the closed projection only: a
        // big integer in an optional field never reaches JCS, so it
        // must not reject.
        let mut c = ctx(json!({"ok": 1}));
        c.insert("extensions".into(), json!({"host": {"big": 9_007_199_254_740_993_i64}}));
        assert!(context_identity(&c).is_ok());
    }

    #[test]
    fn non_json_floats_unrepresentable_at_parse() {
        // §4.4 pinning: NaN/Infinity are not JSON — the parse funnel
        // rejects them before any provider runs.
        assert!(serde_json::from_str::<Value>("{\"x\": NaN}").is_err());
        assert!(serde_json::from_str::<Value>("{\"x\": Infinity}").is_err());
        // Lone surrogate escape: rejected by serde_json's parser.
        assert!(serde_json::from_str::<Value>("{\"x\": \"\\ud800\"}").is_err());
    }
}
