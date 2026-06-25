# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
"""Canonical JSON serialization and context identity (§10)."""
from __future__ import annotations

import hashlib
import json
import math
from typing import Any

from agent_hooks._types import InterceptionPoint

#: L0 keys (§4.1) — always retained for identity.
_L0: frozenset[str] = frozenset(
    {"spec", "interception_point", "timestamp", "sequence", "agent", "session", "target"}
)
#: L0 sub-keys retained on ``agent`` and ``session``.
_L0_AGENT: frozenset[str] = frozenset({"id", "framework"})
_L0_SESSION: frozenset[str] = frozenset({"id"})

#: L1 keys per interception point (§4.2).
_L1: dict[str, frozenset[str]] = {
    InterceptionPoint.AGENT_STARTUP.value: frozenset({"agent_init"}),
    InterceptionPoint.INPUT.value: frozenset({"input"}),
    InterceptionPoint.PRE_MODEL_CALL.value: frozenset({"model", "messages"}),
    InterceptionPoint.POST_MODEL_CALL.value: frozenset({"model", "response"}),
    InterceptionPoint.PRE_TOOL_CALL.value: frozenset({"tool_call"}),
    InterceptionPoint.POST_TOOL_CALL.value: frozenset({"tool_call", "tool_result"}),
    InterceptionPoint.OUTPUT.value: frozenset({"output"}),
    InterceptionPoint.AGENT_SHUTDOWN.value: frozenset({"summary"}),
}


def _ecma_number(x: float) -> str:
    """ECMA-262 shortest-round-trip number serialization (§10.1.3).

    Python's ``repr`` already produces the shortest round-trip for floats
    since 3.1; we strip the trailing ``.0`` for integral values to match the
    JS ``Number.prototype.toString`` output exactly so identities agree
    across SDKs.
    """
    if math.isnan(x) or math.isinf(x):
        raise ValueError("canonical JSON does not admit NaN/Infinity")
    if x == 0:
        return "0"  # collapse -0.0
    s = repr(x)
    if s.endswith(".0"):
        return s[:-2]
    return s


def _encode(obj: Any, out: list[str]) -> None:
    if obj is None:
        out.append("null")
    elif obj is True:
        out.append("true")
    elif obj is False:
        out.append("false")
    elif isinstance(obj, str):
        out.append(json.dumps(obj, ensure_ascii=False))
    elif isinstance(obj, bool):  # pragma: no cover — handled above, but guard ordering
        out.append("true" if obj else "false")
    elif isinstance(obj, int):
        out.append(str(obj))
    elif isinstance(obj, float):
        out.append(_ecma_number(obj))
    elif isinstance(obj, (list, tuple)):
        out.append("[")
        for i, v in enumerate(obj):
            if i:
                out.append(",")
            _encode(v, out)
        out.append("]")
    elif isinstance(obj, dict):
        out.append("{")
        for i, k in enumerate(sorted(obj.keys(), key=lambda s: s.encode("utf-8"))):
            if i:
                out.append(",")
            out.append(json.dumps(k, ensure_ascii=False))
            out.append(":")
            _encode(obj[k], out)
        out.append("}")
    else:
        raise TypeError(f"canonical JSON cannot encode {type(obj).__name__}")


def canonical_json(obj: Any) -> str:
    """Serialize ``obj`` per §10.1: lexicographic keys, no whitespace,
    ECMA-262 numbers, RFC 8259 minimal string escapes."""
    buf: list[str] = []
    _encode(obj, buf)
    return "".join(buf)


def _strip_to_l01(ctx: dict[str, Any]) -> dict[str, Any]:
    """Return a deep copy of ``ctx`` containing only L0 + L1 fields (§10.2)."""
    hp = ctx["interception_point"]
    keep = _L0 | _L1.get(hp, frozenset())
    out: dict[str, Any] = {}
    for k in ctx:
        if k not in keep:
            continue
        if k == "agent":
            out[k] = {sk: ctx[k][sk] for sk in ctx[k] if sk in _L0_AGENT}
        elif k == "session":
            out[k] = {sk: ctx[k][sk] for sk in ctx[k] if sk in _L0_SESSION}
        else:
            out[k] = ctx[k]
    return out


def context_identity(ctx: dict[str, Any]) -> str:
    """``"sha256:" + hex(SHA-256(canonical_json(ctx_L01)))`` (§10.2)."""
    digest = hashlib.sha256(canonical_json(_strip_to_l01(ctx)).encode("utf-8")).hexdigest()
    return f"sha256:{digest}"
