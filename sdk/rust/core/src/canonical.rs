// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
//! Canonical JSON serialization and context identity (§10).
//!
//! §10.1 is RFC 8785 (JSON Canonicalization Scheme), delegated to the
//! `serde_jcs` crate: object members sorted by UTF-16 code units, numbers
//! per ECMA-262 `Number::toString`, minimal string escapes.
//!
//! §10.2 defines the identity preimage as the **closed** L0+L1 field set
//! for the context's interception point — including nested subfield
//! whitelists — so that adding any L2/L3 data (top-level or nested, e.g.
//! `tool_result.duration_ms` or `model.params`) never perturbs
//! `context_identity`.

use crate::types::AgentContext;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt::Write;

/// Serialize `v` per §10.1 (RFC 8785 / JCS).
pub fn canonical_json(v: &Value) -> String {
    serde_jcs::to_string(v).expect("JCS serialization of in-memory Value cannot fail")
}

/// Field whitelist: `(key, allowed_subfields)`. `None` = keep value whole.
type Keep = (&'static str, Option<&'static [&'static str]>);

const L0: &[Keep] = &[
    ("spec", None),
    ("interception_point", None),
    ("timestamp", None),
    ("sequence", None),
    ("agent", Some(&["id", "framework"])),
    ("session", Some(&["id"])),
    ("target", None),
];

/// Closed L1 preimage per interception point (§10.2). Mirrors the
/// per-point closed schemas in `spec/schema/agent-context/`.
fn l1_for(ip: &str) -> &'static [Keep] {
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

fn strip_to_l01(ctx: &AgentContext) -> Value {
    let ip = ctx
        .get("interception_point")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let mut out = serde_json::Map::new();
    for (key, sub) in L0.iter().chain(l1_for(ip)) {
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

/// `"sha256:" + hex(SHA-256(canonical_json(ctx_L01)))` (§10.2).
pub fn context_identity(ctx: &AgentContext) -> String {
    let json = canonical_json(&strip_to_l01(ctx));
    let digest = Sha256::digest(json.as_bytes());
    let mut out = String::with_capacity(7 + 64);
    out.push_str("sha256:");
    for b in digest {
        write!(out, "{b:02x}").expect("write hex");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
    fn nested_l2_stripped() {
        let ctx: AgentContext = json!({
            "spec": "agent-hooks/0.1",
            "interception_point": "post_tool_call",
            "timestamp": "t", "sequence": 5,
            "agent": {"id": "a", "framework": "x", "name": "L2"},
            "session": {"id": "s", "turn": 3},
            "target": "v",
            "tool_call": {"id": "tc", "name": "t", "args": {}, "content_hash": "sha256:00"},
            "tool_result": {"value": "v", "is_error": false, "duration_ms": 12.5}
        })
        .as_object()
        .unwrap()
        .clone();
        let mut bare = ctx.clone();
        // Remove every nested L2 field; identity must be unchanged.
        bare.get_mut("tool_result").unwrap().as_object_mut().unwrap().remove("duration_ms");
        bare.get_mut("tool_call").unwrap().as_object_mut().unwrap().remove("content_hash");
        bare.get_mut("agent").unwrap().as_object_mut().unwrap().remove("name");
        bare.get_mut("session").unwrap().as_object_mut().unwrap().remove("turn");
        assert_eq!(context_identity(&ctx), context_identity(&bare));
    }
}
