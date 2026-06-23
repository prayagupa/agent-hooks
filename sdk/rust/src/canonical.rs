// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
//! Canonical JSON serialization and context identity (§10).

use crate::types::HookContext;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt::Write;

const L0: &[&str] = &[
    "spec",
    "hook_point",
    "timestamp",
    "sequence",
    "agent",
    "session",
    "target",
];
const L0_AGENT: &[&str] = &["id", "framework"];
const L0_SESSION: &[&str] = &["id"];

fn l1_for(hp: &str) -> &'static [&'static str] {
    match hp {
        "agent_startup" => &["agent_init"],
        "input" => &["input"],
        "pre_model_call" => &["model", "messages"],
        "post_model_call" => &["model", "response"],
        "pre_tool_call" => &["tool_call"],
        "post_tool_call" => &["tool_call", "tool_result"],
        "output" => &["output"],
        "agent_shutdown" => &["summary"],
        _ => &[],
    }
}

fn encode(v: &Value, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            // serde_json::Number already produces shortest-round-trip;
            // strip trailing .0 to match ECMA-262 / cross-SDK identities.
            let s = n.to_string();
            out.push_str(s.strip_suffix(".0").unwrap_or(&s));
        }
        Value::String(s) => {
            // RFC 8259 minimal escapes via serde_json's serializer.
            out.push_str(&serde_json::to_string(s).expect("string serialize"));
        }
        Value::Array(a) => {
            out.push('[');
            for (i, e) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                encode(e, out);
            }
            out.push(']');
        }
        Value::Object(m) => {
            out.push('{');
            let mut keys: Vec<_> = m.keys().collect();
            keys.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(k).expect("key serialize"));
                out.push(':');
                encode(&m[*k], out);
            }
            out.push('}');
        }
    }
}

/// Serialize `v` per §10.1: lexicographic keys, no whitespace, ECMA-262
/// numbers, RFC 8259 minimal string escapes.
pub fn canonical_json(v: &Value) -> String {
    let mut out = String::new();
    encode(v, &mut out);
    out
}

fn strip_to_l01(ctx: &HookContext) -> Value {
    let hp = ctx
        .get("hook_point")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let l1 = l1_for(hp);
    let mut out = serde_json::Map::new();
    for (k, v) in ctx {
        if !L0.contains(&k.as_str()) && !l1.contains(&k.as_str()) {
            continue;
        }
        let v = match k.as_str() {
            "agent" => filter_obj(v, L0_AGENT),
            "session" => filter_obj(v, L0_SESSION),
            _ => v.clone(),
        };
        out.insert(k.clone(), v);
    }
    Value::Object(out)
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

/// `"sha256:" + hex(SHA-256(canonical_json(ctx_L01)))` (§10.2).
pub fn context_identity(ctx: &HookContext) -> String {
    let json = canonical_json(&strip_to_l01(ctx));
    let digest = Sha256::digest(json.as_bytes());
    let mut out = String::with_capacity(7 + 64);
    out.push_str("sha256:");
    for b in digest {
        write!(out, "{b:02x}").expect("write hex");
    }
    out
}
