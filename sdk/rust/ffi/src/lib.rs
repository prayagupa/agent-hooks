// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
//! C-ABI over `agent_hooks::ffi_surface` for Go (cgo) and .NET (P/Invoke).
//!
//! Every function takes NUL-terminated UTF-8 C strings and returns a
//! heap-allocated `AhResult*` that the caller MUST free with
//! `ah_free_result`. `ok=1` means `value` holds the JSON result; `ok=0`
//! means `error_code` holds the §11 `host_error:*` string and `value`
//! holds a detail message.

use agent_hooks::ffi_surface as core;
use std::ffi::{c_char, CStr, CString};

#[repr(C)]
pub struct AhResult {
    /// 1 on success, 0 on error.
    pub ok: u8,
    /// On success: the JSON result. On error: the detail message.
    pub value: *mut c_char,
    /// On error: the §11 `host_error:*` code. Null on success.
    pub error_code: *mut c_char,
}

fn to_c(s: String) -> *mut c_char {
    CString::new(s).unwrap_or_default().into_raw()
}

unsafe fn from_c<'a>(p: *const c_char) -> &'a str {
    if p.is_null() {
        ""
    } else {
        CStr::from_ptr(p).to_str().unwrap_or("")
    }
}

fn boxed(r: Result<String, core::FfiError>) -> *mut AhResult {
    let out = match r {
        Ok(v) => AhResult {
            ok: 1,
            value: to_c(v),
            error_code: std::ptr::null_mut(),
        },
        Err((code, detail)) => AhResult {
            ok: 0,
            value: to_c(detail),
            error_code: to_c(code),
        },
    };
    Box::into_raw(Box::new(out))
}

/// Free an `AhResult*` returned by any `ah_*` function.
///
/// # Safety
/// `r` must be a pointer previously returned by an `ah_*` function and not
/// yet freed.
#[no_mangle]
pub unsafe extern "C" fn ah_free_result(r: *mut AhResult) {
    if r.is_null() {
        return;
    }
    let b = Box::from_raw(r);
    if !b.value.is_null() {
        drop(CString::from_raw(b.value));
    }
    if !b.error_code.is_null() {
        drop(CString::from_raw(b.error_code));
    }
}

/// Return the spec version string. Caller must NOT free the returned
/// pointer (it is static).
#[no_mangle]
pub extern "C" fn ah_spec_version() -> *const c_char {
    static V: &str = concat!("agent-hooks/0.1", "\0");
    V.as_ptr() as *const c_char
}

/// §10.1
///
/// # Safety
/// `value_json` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn ah_canonical_json(value_json: *const c_char) -> *mut AhResult {
    boxed(core::canonical_json(from_c(value_json)))
}

/// §10.2
///
/// # Safety
/// `ctx_json` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn ah_context_identity(ctx_json: *const c_char) -> *mut AhResult {
    boxed(core::context_identity(from_c(ctx_json)))
}

/// §5
///
/// # Safety
/// `verdict_json` must be a valid NUL-terminated UTF-8 C string.
#[no_mangle]
pub unsafe extern "C" fn ah_validate_verdict(verdict_json: *const c_char) -> *mut AhResult {
    boxed(core::validate_verdict(from_c(verdict_json)))
}

/// §5.2
///
/// # Safety
/// All pointers must be valid NUL-terminated UTF-8 C strings.
#[no_mangle]
pub unsafe extern "C" fn ah_apply_transform(
    target_json: *const c_char,
    path: *const c_char,
    value_json: *const c_char,
) -> *mut AhResult {
    boxed(core::apply_transform(
        from_c(target_json),
        from_c(path),
        from_c(value_json),
    ))
}

/// §7.1 fold-through
///
/// # Safety
/// All pointers must be valid NUL-terminated UTF-8 C strings.
#[no_mangle]
pub unsafe extern "C" fn ah_apply_transform_ctx(
    ctx_json: *const c_char,
    path: *const c_char,
    value_json: *const c_char,
) -> *mut AhResult {
    boxed(core::apply_transform_ctx(
        from_c(ctx_json),
        from_c(path),
        from_c(value_json),
    ))
}

/// §8 evaluate_only transform validation
///
/// # Safety
/// All pointers must be valid NUL-terminated UTF-8 C strings.
#[no_mangle]
pub unsafe extern "C" fn ah_validate_transform_ctx(
    ctx_json: *const c_char,
    path: *const c_char,
    value_json: *const c_char,
) -> *mut AhResult {
    boxed(core::validate_transform_ctx(
        from_c(ctx_json),
        from_c(path),
        from_c(value_json),
    ))
}

/// §6/§10 finalize
///
/// # Safety
/// All pointers must be valid NUL-terminated UTF-8 C strings.
#[no_mangle]
pub unsafe extern "C" fn ah_finalize(
    ctx_json: *const c_char,
    verdict_json: *const c_char,
    mode: *const c_char,
    input_identity: *const c_char,
    decided_by: i64,
) -> *mut AhResult {
    boxed(core::finalize(
        from_c(ctx_json),
        from_c(verdict_json),
        from_c(mode),
        from_c(input_identity),
        decided_by,
    ))
}

// ---- CTK engine (§13.2) ----------------------------------------------------

/// # Safety
/// All pointers must be valid NUL-terminated UTF-8 C strings.
#[no_mangle]
pub unsafe extern "C" fn ah_ctk_scripted_intercept(
    rules_json: *const c_char,
    ctx_json: *const c_char,
) -> *mut AhResult {
    boxed(core::ctk_scripted_intercept(
        from_c(rules_json),
        from_c(ctx_json),
    ))
}

/// # Safety
/// All pointers must be valid NUL-terminated UTF-8 C strings.
#[no_mangle]
pub unsafe extern "C" fn ah_ctk_scripted_resolve(
    rules_json: *const c_char,
    ctx_json: *const c_char,
    identity: *const c_char,
) -> *mut AhResult {
    boxed(core::ctk_scripted_resolve(
        from_c(rules_json),
        from_c(ctx_json),
        from_c(identity),
    ))
}

/// # Safety
/// All pointers must be valid NUL-terminated UTF-8 C strings.
#[no_mangle]
pub unsafe extern "C" fn ah_ctk_should_skip(
    vector_json: *const c_char,
    harness_caps_json: *const c_char,
) -> *mut AhResult {
    boxed(core::ctk_should_skip(
        from_c(vector_json),
        from_c(harness_caps_json),
    ))
}

/// # Safety
/// All pointers must be valid NUL-terminated UTF-8 C strings.
#[no_mangle]
pub unsafe extern "C" fn ah_ctk_assert(
    vector_json: *const c_char,
    recorded_json: *const c_char,
    run_record_json: *const c_char,
) -> *mut AhResult {
    boxed(core::ctk_assert(
        from_c(vector_json),
        from_c(recorded_json),
        from_c(run_record_json),
    ))
}
