#!/usr/bin/env python3
"""Generate conformance/golden/identity.json from the Rust core.

The fixtures exercise the corners that historically caused cross-SDK
divergence (RM-N01/RM-N02): number forms, negative zero, key ordering,
non-ASCII, L2/L3 stripping, and every interception point's L1 shape.
Expected outputs are computed by the Rust core via the Python FFI
binding, so any language binding that agrees with these values agrees
with Rust.
"""
from __future__ import annotations

import json
import pathlib
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] / "sdk" / "python" / "python"))
from agent_hooks import _core  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parents[1]
OUT = ROOT / "conformance" / "golden" / "identity.json"


def ctx(ip: str, seq: int, extra: dict) -> dict:
    return {
        "spec": "agent-hooks/0.1",
        "interception_point": ip,
        "timestamp": "2026-01-01T00:00:00.000Z",
        "sequence": seq,
        "agent": {"id": "agent-1", "framework": "reference"},
        "session": {"id": "sess-1"},
        **extra,
    }


FIXTURES: list[tuple[str, str, dict]] = [
    # (id, note, ctx)
    (
        "G-01-startup",
        "agent_startup baseline",
        ctx("agent_startup", 0, {
            "agent_init": {"tools_registered": ["a", "b"]},
            "target": {"tools_registered": ["a", "b"]},
        }),
    ),
    (
        "G-02-input-unicode",
        "input with non-ASCII content and emoji",
        ctx("input", 1, {
            "input": {"content": "héllo 🌍", "role": "user"},
            "target": {"content": "héllo 🌍", "role": "user"},
        }),
    ),
    (
        "G-03-pre-tool-numbers",
        "pre_tool_call with number edge cases in args",
        ctx("pre_tool_call", 4, {
            "tool_call": {
                "id": "tc-1", "name": "calc",
                "args": {"i": 1, "f": 1.0, "neg0": -0.0, "big": 1e21,
                         "small": 1e-7, "pi": 3.141592653589793},
            },
            "target": {"i": 1, "f": 1.0, "neg0": -0.0, "big": 1e21,
                       "small": 1e-7, "pi": 3.141592653589793},
        }),
    ),
    (
        "G-04-key-order",
        "post_model_call with keys supplied in reverse order",
        ctx("post_model_call", 3, {
            "model": {"id": "m"},
            "response": {"finish_reason": "stop", "tool_calls": [],
                         "content": {"z": 1, "a": 2, "m": 3}},
            "target": {"finish_reason": "stop", "tool_calls": [],
                       "content": {"z": 1, "a": 2, "m": 3}},
        }),
    ),
    (
        "G-05-l2-l3-stripped",
        "L2 (trace, budgets, agent.name) and L3 (extensions) MUST NOT affect identity",
        ctx("input", 1, {
            "input": {"content": "hi", "role": "user"},
            "target": {"content": "hi", "role": "user"},
            "agent": {"id": "agent-1", "framework": "reference",
                      "name": "IGNORED", "version": "IGNORED"},
            "session": {"id": "sess-1", "started_at": "IGNORED", "turn": 99},
            "trace": {"trace_id": "IGNORED"},
            "budgets": {"tool_call_count": 999},
            "extensions": {"acs": {"anything": True}},
        }),
    ),
    (
        "G-05b-l2-l3-baseline",
        "same required+conditional fields as G-05 without optional/namespaced ones; identity MUST equal G-05",
        ctx("input", 1, {
            "input": {"content": "hi", "role": "user"},
            "target": {"content": "hi", "role": "user"},
        }),
    ),
    (
        "G-06-nested-array",
        "pre_model_call with nested message array",
        ctx("pre_model_call", 2, {
            "model": {"id": "gpt-x"},
            "messages": [
                {"role": "system", "content": "a"},
                {"role": "user", "content": {"parts": [1, 2, {"k": "v"}]}},
            ],
            "target": [
                {"role": "system", "content": "a"},
                {"role": "user", "content": {"parts": [1, 2, {"k": "v"}]}},
            ],
        }),
    ),
    (
        "G-07-post-tool-null",
        "post_tool_call with null result value",
        ctx("post_tool_call", 5, {
            "tool_call": {"id": "tc-1", "name": "t", "args": {}},
            "tool_result": {"value": None, "is_error": False},
            "target": None,
        }),
    ),
    (
        "G-08-output-escapes",
        "output with control chars requiring escape",
        ctx("output", 6, {
            "output": {"content": "line1\nline2\t\"q\"\\end"},
            "target": {"content": "line1\nline2\t\"q\"\\end"},
        }),
    ),
    (
        "G-09-shutdown",
        "agent_shutdown baseline",
        ctx("agent_shutdown", 7, {
            "summary": {"reason": "completed"},
            "target": {"reason": "completed"},
        }),
    ),
    (
        "G-10-empty-target",
        "pre_tool_call with empty args object",
        ctx("pre_tool_call", 4, {
            "tool_call": {"id": "tc-1", "name": "noop", "args": {}},
            "target": {},
        }),
    ),
]


def main() -> None:
    out = []
    for fid, note, c in FIXTURES:
        ctx_json = json.dumps(c, ensure_ascii=False)
        canon = _core.canonical_json(ctx_json)
        ident = _core.context_identity(ctx_json)
        out.append({
            "id": fid,
            "note": note,
            "ctx": c,
            "expect": {"canonical_json": canon, "context_identity": ident},
        })
    # G-05 and G-05b MUST have identical identity (L2/L3 stripped).
    g05 = next(f for f in out if f["id"] == "G-05-l2-l3-stripped")
    g05b = next(f for f in out if f["id"] == "G-05b-l2-l3-baseline")
    assert g05["expect"]["context_identity"] == g05b["expect"]["context_identity"], (
        "L2/L3 stripping is broken in the core"
    )
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_text(
        json.dumps(
            {
                "spec": "agent-hooks/0.1",
                "generator": "scripts/gen-golden-identity.py via Rust core",
                "fixtures": out,
            },
            indent=2,
            ensure_ascii=False,
        )
        + "\n"
    )
    print(f"wrote {OUT.relative_to(ROOT)} ({len(out)} fixtures)")


if __name__ == "__main__":
    main()
