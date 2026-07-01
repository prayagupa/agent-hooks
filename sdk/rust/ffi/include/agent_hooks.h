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
AhResult *ah_combine_verdicts(const char *verdicts_json);
AhResult *ah_enforce(const char *ctx_json, const char *verdict_json,
                     const char *mode);

#ifdef __cplusplus
}
#endif

#endif /* AGENT_HOOKS_FFI_H */
