// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
// Canonical JSON serialization and context identity (§10).

using System.Globalization;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;

namespace AgentHooks;

public static class Canonical
{
    private static readonly HashSet<string> L0 =
        ["spec", "hook_point", "timestamp", "sequence", "agent", "session", "target"];
    private static readonly HashSet<string> L0Agent = ["id", "framework"];
    private static readonly HashSet<string> L0Session = ["id"];

    private static readonly Dictionary<string, string[]> L1 = new()
    {
        ["agent_startup"] = ["agent_init"],
        ["input"] = ["input"],
        ["pre_model_call"] = ["model", "messages"],
        ["post_model_call"] = ["model", "response"],
        ["pre_tool_call"] = ["tool_call"],
        ["post_tool_call"] = ["tool_call", "tool_result"],
        ["output"] = ["output"],
        ["agent_shutdown"] = ["summary"],
    };

    /// <summary>Serialize per §10.1: lexicographic keys, no whitespace,
    /// ECMA-262 numbers, RFC 8259 minimal string escapes.</summary>
    public static string Json(JsonNode? node)
    {
        var sb = new StringBuilder();
        Encode(node, sb);
        return sb.ToString();
    }

    /// <summary><c>"sha256:" + hex(SHA-256(Json(ctx_L01)))</c> (§10.2).</summary>
    public static string ContextIdentity(HookContext ctx)
    {
        var stripped = StripToL01(ctx);
        var bytes = Encoding.UTF8.GetBytes(Json(stripped));
        return "sha256:" + Convert.ToHexString(SHA256.HashData(bytes)).ToLowerInvariant();
    }

    private static void Encode(JsonNode? n, StringBuilder sb)
    {
        switch (n)
        {
            case null:
                sb.Append("null");
                break;
            case JsonValue v when v.TryGetValue(out bool b):
                sb.Append(b ? "true" : "false");
                break;
            case JsonValue v when v.TryGetValue(out double d):
                EncodeNumber(d, sb);
                break;
            case JsonValue v when v.TryGetValue(out long l):
                sb.Append(l.ToString(CultureInfo.InvariantCulture));
                break;
            case JsonValue v when v.TryGetValue(out string? s):
                sb.Append(JsonSerializer.Serialize(s));
                break;
            case JsonArray a:
                sb.Append('[');
                for (var i = 0; i < a.Count; i++)
                {
                    if (i > 0) sb.Append(',');
                    Encode(a[i], sb);
                }
                sb.Append(']');
                break;
            case JsonObject o:
                sb.Append('{');
                var keys = o.Select(kv => kv.Key)
                    .OrderBy(k => k, StringComparer.Ordinal)
                    .ToList();
                for (var i = 0; i < keys.Count; i++)
                {
                    if (i > 0) sb.Append(',');
                    sb.Append(JsonSerializer.Serialize(keys[i]));
                    sb.Append(':');
                    Encode(o[keys[i]], sb);
                }
                sb.Append('}');
                break;
            default:
                throw new NotSupportedException($"canonical JSON cannot encode {n?.GetType().Name}");
        }
    }

    private static void EncodeNumber(double d, StringBuilder sb)
    {
        if (!double.IsFinite(d)) throw new ArgumentException("canonical JSON does not admit NaN/Infinity");
        if (d == 0) { sb.Append('0'); return; }
        // "R" round-trip then strip integral ".0" to match ECMA-262 ToString.
        var s = d.ToString("R", CultureInfo.InvariantCulture);
        sb.Append(d == Math.Truncate(d) && !s.Contains('E') ? ((long)d).ToString(CultureInfo.InvariantCulture) : s);
    }

    private static JsonObject StripToL01(HookContext ctx)
    {
        var hp = (string)ctx.Json["hook_point"]!;
        var l1 = L1.TryGetValue(hp, out var x) ? new HashSet<string>(x) : [];
        var outObj = new JsonObject();
        foreach (var (k, v) in ctx.Json)
        {
            if (!L0.Contains(k) && !l1.Contains(k)) continue;
            outObj[k] = k switch
            {
                "agent" => Filter((JsonObject)v!, L0Agent),
                "session" => Filter((JsonObject)v!, L0Session),
                _ => v?.DeepClone(),
            };
        }
        return outObj;
    }

    private static JsonObject Filter(JsonObject src, HashSet<string> keep)
    {
        var o = new JsonObject();
        foreach (var (k, v) in src)
            if (keep.Contains(k)) o[k] = v?.DeepClone();
        return o;
    }
}
