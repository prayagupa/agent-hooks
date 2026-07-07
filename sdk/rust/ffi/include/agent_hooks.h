/* Copyright (c) Microsoft Corporation. Licensed under the MIT License. */
/* C header for libagent_hooks_ffi. Kept in sync with sdk/rust/ffi/src/lib.rs. */

#ifndef AGENT_HOOKS_FFI_H
#define AGENT_HOOKS_FFI_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct {
    /* 1 on success, 0 on error. */
    uint8_t ok;
    /* On success: JSON result. On error: detail message. UTF-8, NUL-terminated. */
    char *value;
    /* On error: host_error:* code. NULL on success. UTF-8, NUL-terminated. */
    char *error_code;
} AhResult;

/* Free an AhResult* returned by any ah_* function. */
void ah_free_result(AhResult *r);

/* Static string; do NOT free. */
const char *ah_spec_version(void);

AhResult *ah_canonical_json(const char *value_json);
AhResult *ah_context_identity(const char *ctx_json);
AhResult *ah_validate_verdict(const char *verdict_json);
AhResult *ah_apply_transform(const char *target_json, const char *path,
                             const char *value_json);
AhResult *ah_apply_transform_ctx(const char *ctx_json, const char *path,
                                 const char *value_json);
AhResult *ah_validate_transform_ctx(const char *ctx_json, const char *path,
                                    const char *value_json);
/* decided_by: registration index of the deciding interceptor; -1 = none. */
AhResult *ah_finalize(const char *ctx_json, const char *verdict_json,
                      const char *mode, const char *input_identity,
                      int64_t decided_by);

/* CTK engine (spec section 13.2) */
AhResult *ah_ctk_scripted_intercept(const char *rules_json,
                                    const char *ctx_json);
AhResult *ah_ctk_scripted_resolve(const char *rules_json,
                                  const char *ctx_json,
                                  const char *identity);
AhResult *ah_ctk_should_skip(const char *vector_json,
                             const char *harness_caps_json);
AhResult *ah_ctk_assert(const char *vector_json, const char *recorded_json,
                        const char *run_record_json);

#ifdef __cplusplus
}
#endif

#endif /* AGENT_HOOKS_FFI_H */
