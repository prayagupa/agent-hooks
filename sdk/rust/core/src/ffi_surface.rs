// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
//! JSON-string FFI surface shared by all language bindings.
//!
//! Every function here takes and returns UTF-8 JSON strings so the same
//! surface works over PyO3, napi-rs, and a plain C ABI. Bindings marshal
//! their native string type to `&str` / `String` and delegate here; no
//! per-binding logic.
//!
//! Errors are returned as `Err((host_error_code, detail))` where
//! `host_error_code` is the §11 wire string (e.g.
//! `"host_error:verdict_invalid"`); bindings raise a native exception
//! carrying both.

use crate::{
    canonical, enforce as enforce_step, path, types::EnforcementMode, verdict, AgentContext,
    HostError, Verdict,
};
#[allow(unused_imports)]
use serde_json::Value;

/// `(host_error_code, detail_message)`
pub type FfiError = (String, String);

fn err(e: HostError, detail: impl Into<String>) -> FfiError {
    (e.to_string(), detail.into())
}

fn parse_json(s: &str, what: &str) -> Result<Value, FfiError> {
    serde_json::from_str(s).map_err(|e| err(HostError::ContextInvalid, format!("{what}: {e}")))
}

/// §10.1: canonical JSON of an arbitrary value.
pub fn canonical_json(value_json: &str) -> Result<String, FfiError> {
    let v = parse_json(value_json, "value")?;
    Ok(canonical::canonical_json(&v))
}

/// §10.2: `"sha256:" + hex(SHA-256(canonical_json(ctx_L01)))`.
pub fn context_identity(ctx_json: &str) -> Result<String, FfiError> {
    let ctx: AgentContext = serde_json::from_str(ctx_json)
        .map_err(|e| err(HostError::ContextInvalid, format!("ctx: {e}")))?;
    Ok(canonical::context_identity(&ctx))
}

/// §5: validate an interceptor's wire return value. Returns the normalized
/// verdict as JSON on success.
pub fn validate_verdict(verdict_json: &str) -> Result<String, FfiError> {
    let raw = parse_json(verdict_json, "verdict")?;
    let v = verdict::from_wire(&raw).map_err(|(e, d)| err(e, d))?;
    Ok(serde_json::to_string(&v).expect("verdict serialize"))
}

/// §5.2: apply a transform path to a target. Returns the new target JSON.
pub fn apply_transform(
    target_json: &str,
    path_str: &str,
    value_json: &str,
) -> Result<String, FfiError> {
    let target = parse_json(target_json, "target")?;
    let value = parse_json(value_json, "value")?;
    let result = path::apply(target, path_str, value).map_err(|e| err(e, path_str))?;
    Ok(serde_json::to_string(&result).expect("target serialize"))
}

/// §7.1: combine an ordered array of verdicts. Input is a JSON array of
/// verdict objects (already validated); output is the combined verdict JSON.
pub fn combine_verdicts(verdicts_json: &str) -> Result<String, FfiError> {
    let arr: Vec<Value> = serde_json::from_str(verdicts_json)
        .map_err(|e| err(HostError::VerdictInvalid, format!("verdicts: {e}")))?;
    let vs: Vec<Verdict> = arr
        .iter()
        .map(verdict::from_wire)
        .collect::<Result<_, _>>()
        .map_err(|(e, d)| err(e, d))?;
    Ok(serde_json::to_string(&verdict::combine(&vs)).expect("verdict serialize"))
}

/// §6/§8/§10: apply the enforcement step to a combined verdict. Returns
/// `{"record": InterceptionRecord, "ctx": AgentContext}` as JSON — the
/// context is returned because `enforce` may have written the transformed
/// target back into it and FFI callers passed a copy.
///
/// The verdict is deserialized permissively (not via `from_wire`) because
/// the caller may pass a host-synthesized `host_error:*` deny that the
/// wrapper produced during dispatch (§6.3); validation already happened
/// per interceptor.
pub fn enforce(ctx_json: &str, verdict_json: &str, mode: &str) -> Result<String, FfiError> {
    let mut ctx: AgentContext = serde_json::from_str(ctx_json)
        .map_err(|e| err(HostError::ContextInvalid, format!("ctx: {e}")))?;
    let v: Verdict = serde_json::from_str(verdict_json)
        .map_err(|e| err(HostError::VerdictInvalid, format!("verdict: {e}")))?;
    let mode = match mode {
        "enforce" => EnforcementMode::Enforce,
        "evaluate_only" => EnforcementMode::EvaluateOnly,
        _ => return Err(err(HostError::ContextInvalid, format!("mode: {mode}"))),
    };
    let record = enforce_step::enforce(&mut ctx, v, mode);
    let out = serde_json::json!({ "record": record, "ctx": ctx });
    Ok(serde_json::to_string(&out).expect("enforce serialize"))
}

/// Version stamp for binding sanity checks.
pub fn spec_version() -> &'static str {
    crate::SPEC_VERSION
}
