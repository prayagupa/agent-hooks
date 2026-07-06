// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
// P/Invoke bindings for libagent_hooks_ffi (sdk/rust/ffi).
//
// The cdylib exposes a JSON-string surface over agent_hooks::ffi_surface.
// Every function returns a heap-allocated AhResult* the caller frees with
// ah_free_result. See sdk/rust/ffi/include/agent_hooks.h.

using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

[assembly: InternalsVisibleTo("AgentHooks.Conformance")]
[assembly: InternalsVisibleTo("AgentHooks.Tests")]

namespace AgentHooks;

/// <summary>Thrown by every native-backed function on failure.
/// <see cref="Code"/> is the §11 <c>host_error:*</c> wire string.</summary>
public sealed class AgentHooksCoreException(string code, string detail)
    : InvalidOperationException($"{code}: {detail}")
{
    public string Code { get; } = code;
}

internal static partial class Native
{
    private const string Lib = "agent_hooks_ffi";

    [StructLayout(LayoutKind.Sequential)]
    private struct AhResult
    {
        public byte Ok;
        public IntPtr Value;
        public IntPtr ErrorCode;
    }

    [LibraryImport(Lib)]
    private static partial IntPtr ah_spec_version();

    [LibraryImport(Lib)]
    private static partial void ah_free_result(IntPtr r);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    private static partial IntPtr ah_canonical_json(string valueJson);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    private static partial IntPtr ah_context_identity(string ctxJson);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    private static partial IntPtr ah_validate_verdict(string verdictJson);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    private static partial IntPtr ah_apply_transform(string targetJson, string path, string valueJson);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    private static partial IntPtr ah_combine_verdicts(string verdictsJson);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    private static partial IntPtr ah_enforce(string ctxJson, string verdictJson, string mode);

    // ---- CTK engine (§13.2) ------------------------------------------------

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    private static partial IntPtr ah_ctk_scripted_intercept(string rulesJson, string ctxJson);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    private static partial IntPtr ah_ctk_scripted_resolve(string rulesJson, string ctxJson, string identity);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    private static partial IntPtr ah_ctk_should_skip(string vectorJson, string harnessCapsJson);

    [LibraryImport(Lib, StringMarshalling = StringMarshalling.Utf8)]
    private static partial IntPtr ah_ctk_assert(string vectorJson, string recordedJson, string runRecordJson);

    private static string Unwrap(IntPtr p)
    {
        if (p == IntPtr.Zero)
            throw new AgentHooksCoreException("host_error:context_invalid", "null result");
        try
        {
            var r = Marshal.PtrToStructure<AhResult>(p);
            var value = Marshal.PtrToStringUTF8(r.Value) ?? string.Empty;
            if (r.Ok == 1) return value;
            var code = Marshal.PtrToStringUTF8(r.ErrorCode) ?? "host_error:context_invalid";
            throw new AgentHooksCoreException(code, value);
        }
        finally
        {
            ah_free_result(p);
        }
    }

    internal static string SpecVersion() =>
        Marshal.PtrToStringUTF8(ah_spec_version()) ?? string.Empty;

    internal static string CanonicalJson(string valueJson) => Unwrap(ah_canonical_json(valueJson));
    internal static string ContextIdentity(string ctxJson) => Unwrap(ah_context_identity(ctxJson));
    internal static string ValidateVerdict(string verdictJson) => Unwrap(ah_validate_verdict(verdictJson));
    internal static string ApplyTransform(string targetJson, string path, string valueJson) =>
        Unwrap(ah_apply_transform(targetJson, path, valueJson));
    internal static string CombineVerdicts(string verdictsJson) => Unwrap(ah_combine_verdicts(verdictsJson));
    internal static string Enforce(string ctxJson, string verdictJson, string mode) =>
        Unwrap(ah_enforce(ctxJson, verdictJson, mode));

    internal static string CtkScriptedIntercept(string rulesJson, string ctxJson) =>
        Unwrap(ah_ctk_scripted_intercept(rulesJson, ctxJson));
    internal static string CtkScriptedResolve(string rulesJson, string ctxJson, string identity) =>
        Unwrap(ah_ctk_scripted_resolve(rulesJson, ctxJson, identity));
    internal static string CtkShouldSkip(string vectorJson, string harnessCapsJson) =>
        Unwrap(ah_ctk_should_skip(vectorJson, harnessCapsJson));
    internal static string CtkAssert(string vectorJson, string recordedJson, string runRecordJson) =>
        Unwrap(ah_ctk_assert(vectorJson, recordedJson, runRecordJson));
}
