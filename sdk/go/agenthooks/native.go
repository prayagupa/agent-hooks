// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

package agenthooks

// cgo bindings for libagent_hooks_ffi (sdk/rust/ffi).
//
// The cdylib exposes a JSON-string surface over agent_hooks::ffi_surface.
// Every function returns a heap-allocated AhResult* the caller frees with
// ah_free_result. See sdk/rust/ffi/include/agent_hooks.h.
//
// Build requires the cdylib at a path cgo can find. The CI job builds
// sdk/rust with `cargo build -p agent-hooks-ffi --release` and sets
// CGO_LDFLAGS/LD_LIBRARY_PATH to sdk/rust/target/release. For local dev:
//
//	cargo build --manifest-path ../../rust/Cargo.toml -p agent-hooks-ffi --release
//	export CGO_LDFLAGS="-L$(pwd)/../../rust/target/release -lagent_hooks_ffi"
//	export LD_LIBRARY_PATH="$(pwd)/../../rust/target/release"
//	go test ./...

/*
#cgo CFLAGS: -I${SRCDIR}/../../rust/ffi/include
#cgo LDFLAGS: -lagent_hooks_ffi
#include <stdlib.h>
#include "agent_hooks.h"
*/
import "C"

import (
	"errors"
	"unsafe"
)

// CoreError wraps a §11 host_error:* code returned by the Rust core.
type CoreError struct {
	Code   string
	Detail string
}

func (e *CoreError) Error() string { return e.Code + ": " + e.Detail }

// Is allows errors.Is(err, &CoreError{Code: "host_error:..."}) matching.
func (e *CoreError) Is(target error) bool {
	var t *CoreError
	if errors.As(target, &t) {
		return t.Code == "" || t.Code == e.Code
	}
	return false
}

func unwrap(r *C.AhResult) (string, error) {
	if r == nil {
		return "", &CoreError{Code: string(ErrContextInvalid), Detail: "null result"}
	}
	defer C.ah_free_result(r)
	value := C.GoString(r.value)
	if r.ok == 1 {
		return value, nil
	}
	return "", &CoreError{Code: C.GoString(r.error_code), Detail: value}
}

func cstr(s string) (*C.char, func()) {
	c := C.CString(s)
	return c, func() { C.free(unsafe.Pointer(c)) }
}

// nativeSpecVersion returns the spec version compiled into the Rust core.
func nativeSpecVersion() string {
	return C.GoString(C.ah_spec_version())
}

func nativeCanonicalJSON(valueJSON string) (string, error) {
	c, free := cstr(valueJSON)
	defer free()
	return unwrap(C.ah_canonical_json(c))
}

func nativeContextIdentity(ctxJSON string) (string, error) {
	c, free := cstr(ctxJSON)
	defer free()
	return unwrap(C.ah_context_identity(c))
}

func nativeValidateVerdict(verdictJSON string) (string, error) {
	c, free := cstr(verdictJSON)
	defer free()
	return unwrap(C.ah_validate_verdict(c))
}

func nativeApplyTransform(targetJSON, path, valueJSON string) (string, error) {
	ct, ft := cstr(targetJSON)
	defer ft()
	cp, fp := cstr(path)
	defer fp()
	cv, fv := cstr(valueJSON)
	defer fv()
	return unwrap(C.ah_apply_transform(ct, cp, cv))
}

func nativeCombineVerdicts(verdictsJSON string) (string, error) {
	c, free := cstr(verdictsJSON)
	defer free()
	return unwrap(C.ah_combine_verdicts(c))
}

func nativeEnforce(ctxJSON, verdictJSON, mode string) (string, error) {
	cc, fc := cstr(ctxJSON)
	defer fc()
	cv, fv := cstr(verdictJSON)
	defer fv()
	cm, fm := cstr(mode)
	defer fm()
	return unwrap(C.ah_enforce(cc, cv, cm))
}

// ---- CTK engine (§13.2) ---------------------------------------------------
//
// Unexported string→string cgo shims. Exported, typed wrappers live in
// ctk.go (separate file so this cgo unit stays minimal).

func nativeCtkScriptedIntercept(rulesJSON, ctxJSON string) (string, error) {
	cr, fr := cstr(rulesJSON)
	defer fr()
	cc, fc := cstr(ctxJSON)
	defer fc()
	return unwrap(C.ah_ctk_scripted_intercept(cr, cc))
}

func nativeCtkScriptedResolve(rulesJSON, ctxJSON, identity string) (string, error) {
	cr, fr := cstr(rulesJSON)
	defer fr()
	cc, fc := cstr(ctxJSON)
	defer fc()
	ci, fi := cstr(identity)
	defer fi()
	return unwrap(C.ah_ctk_scripted_resolve(cr, cc, ci))
}

func nativeCtkShouldSkip(vectorJSON, capsJSON string) (string, error) {
	cv, fv := cstr(vectorJSON)
	defer fv()
	cc, fc := cstr(capsJSON)
	defer fc()
	return unwrap(C.ah_ctk_should_skip(cv, cc))
}

func nativeCtkAssert(vectorJSON, recordedJSON, runRecordJSON string) (string, error) {
	cv, fv := cstr(vectorJSON)
	defer fv()
	cr, fr := cstr(recordedJSON)
	defer fr()
	crr, frr := cstr(runRecordJSON)
	defer frr()
	return unwrap(C.ah_ctk_assert(cv, cr, crr))
}
