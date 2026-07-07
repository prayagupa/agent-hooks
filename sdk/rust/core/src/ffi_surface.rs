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
    canonical, enforce as enforce_mod, path, types::EnforcementMode, verdict, AgentContext,
    HostError, Transform,
};
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

/// §7.1 fold-through: apply one transform to the context's `target`
/// (and its L1 alias) so the next interceptor sees the effect. Returns
/// the updated context JSON.
pub fn apply_transform_ctx(
    ctx_json: &str,
    path_str: &str,
    value_json: &str,
) -> Result<String, FfiError> {
    let mut ctx: AgentContext = serde_json::from_str(ctx_json)
        .map_err(|e| err(HostError::ContextInvalid, format!("ctx: {e}")))?;
    let value = parse_json(value_json, "value")?;
    let t = Transform { path: path_str.to_owned(), value };
    enforce_mod::apply_transform_to_ctx(&mut ctx, &t).map_err(|e| err(e, path_str))?;
    Ok(serde_json::to_string(&ctx).expect("ctx serialize"))
}

/// §8 `evaluate_only`: validate a transform against the context's
/// current target without applying it. Returns `"null"` on success.
pub fn validate_transform_ctx(
    ctx_json: &str,
    path_str: &str,
    value_json: &str,
) -> Result<String, FfiError> {
    let ctx: AgentContext = serde_json::from_str(ctx_json)
        .map_err(|e| err(HostError::ContextInvalid, format!("ctx: {e}")))?;
    let value = parse_json(value_json, "value")?;
    let t = Transform { path: path_str.to_owned(), value };
    enforce_mod::validate_transform(&ctx, &t).map_err(|e| err(e, path_str))?;
    Ok("null".to_owned())
}

/// §6/§10: build the `InterceptionRecord` for one completed
/// interception. `input_identity` MUST have been computed via
/// [`context_identity`] before interceptor dispatch; transforms were
/// already applied during the §7.1 fold via [`apply_transform_ctx`].
/// The verdict is deserialized permissively because the emitter may
/// pass a host-synthesized `host_error:*` deny (§6.3).
/// `decided_by` uses `-1` for "none" (pure allow / host-synthesized).
pub fn finalize(
    ctx_json: &str,
    verdict_json: &str,
    mode: &str,
    input_identity: &str,
    decided_by: i64,
) -> Result<String, FfiError> {
    let ctx: AgentContext = serde_json::from_str(ctx_json)
        .map_err(|e| err(HostError::ContextInvalid, format!("ctx: {e}")))?;
    let v: crate::Verdict = serde_json::from_str(verdict_json)
        .map_err(|e| err(HostError::VerdictInvalid, format!("verdict: {e}")))?;
    let mode = match mode {
        "enforce" => EnforcementMode::Enforce,
        "evaluate_only" => EnforcementMode::EvaluateOnly,
        _ => return Err(err(HostError::ContextInvalid, format!("mode: {mode}"))),
    };
    let decided = if decided_by < 0 {
        None
    } else {
        Some(decided_by as u32)
    };
    let record = enforce_mod::finalize(&ctx, v, mode, input_identity, decided);
    Ok(serde_json::to_string(&record).expect("record serialize"))
}

/// Version stamp for binding sanity checks.
pub fn spec_version() -> &'static str {
    crate::SPEC_VERSION
}

// ---- CTK engine (§13.2) ----------------------------------------------------

/// Evaluate a vector's `interceptor_script` against `ctx`. Returns the
/// verdict JSON the scripted interceptor produced.
pub fn ctk_scripted_intercept(rules_json: &str, ctx_json: &str) -> Result<String, FfiError> {
    let rules: Vec<Value> = serde_json::from_str(rules_json)
        .map_err(|e| err(HostError::ContextInvalid, format!("rules: {e}")))?;
    let ctx = parse_json(ctx_json, "ctx")?;
    let out = crate::ctk_engine::scripted_intercept(&rules, &ctx);
    Ok(serde_json::to_string(&out).expect("verdict serialize"))
}

/// Evaluate a vector's `approval_script` against the request context.
/// Returns `{outcome, context_identity, verdict?}` echoing `identity`.
pub fn ctk_scripted_resolve(
    rules_json: &str,
    ctx_json: &str,
    identity: &str,
) -> Result<String, FfiError> {
    let rules: Vec<Value> = serde_json::from_str(rules_json)
        .map_err(|e| err(HostError::ContextInvalid, format!("rules: {e}")))?;
    let ctx = parse_json(ctx_json, "ctx")?;
    let out = crate::ctk_engine::scripted_resolve(&rules, &ctx, identity);
    Ok(serde_json::to_string(&out).expect("resolution serialize"))
}

/// Determine whether a vector should be skipped for a harness. Returns
/// `null` (no skip) or a detail string.
pub fn ctk_should_skip(vector_json: &str, harness_caps_json: &str) -> Result<String, FfiError> {
    let vector = parse_json(vector_json, "vector")?;
    let caps: Vec<String> = serde_json::from_str(harness_caps_json)
        .map_err(|e| err(HostError::ContextInvalid, format!("caps: {e}")))?;
    let caps_ref: Vec<&str> = caps.iter().map(String::as_str).collect();
    let out = crate::ctk_engine::should_skip(&vector, &caps_ref);
    Ok(serde_json::to_string(&out).expect("skip serialize"))
}

/// Run the assertion pass for one vector. Returns `VectorResult` JSON.
pub fn ctk_assert(
    vector_json: &str,
    recorded_json: &str,
    run_record_json: &str,
) -> Result<String, FfiError> {
    let vector = parse_json(vector_json, "vector")?;
    let recorded: Vec<Value> = serde_json::from_str(recorded_json)
        .map_err(|e| err(HostError::ContextInvalid, format!("recorded: {e}")))?;
    let rr: crate::ctk_engine::RunRecord = serde_json::from_str(run_record_json)
        .map_err(|e| err(HostError::ContextInvalid, format!("run_record: {e}")))?;
    let out = crate::ctk_engine::assert_vector(&vector, &recorded, &rr);
    Ok(serde_json::to_string(&out).expect("result serialize"))
}
