# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
"""``$target`` JSONPath subset: parse, resolve, and apply transforms (§5.2)."""
from __future__ import annotations

import re
from typing import Any

from agent_hooks._types import HookError

_ROOT_RE = re.compile(r"^\$(target|policy_target)")
_SEGMENT_RE = re.compile(
    r"""
    \.(?P<dot>[A-Za-z0-9_-]+)          # .member
  | \[(?P<idx>\d+)\]                   # [index]
  | \["(?P<bkt>[A-Za-z0-9_-]+)"\]      # ["member"]
    """,
    re.VERBOSE,
)


class PathError(ValueError):
    """A transform path failed to parse or resolve."""

    def __init__(self, hook_error: HookError, detail: str) -> None:
        self.hook_error = hook_error
        super().__init__(f"{hook_error.value}: {detail}")


def parse(path: str) -> list[str | int]:
    """Parse a §5.2 path into segments. Raises :class:`PathError`."""
    m = _ROOT_RE.match(path)
    if not m:
        raise PathError(
            HookError.TRANSFORM_TARGET_FORBIDDEN,
            f"path must be rooted at $target (got {path!r})",
        )
    pos = m.end()
    segs: list[str | int] = []
    while pos < len(path):
        sm = _SEGMENT_RE.match(path, pos)
        if not sm:
            raise PathError(HookError.TRANSFORM_INVALID, f"unparseable segment at {path[pos:]!r}")
        if sm.group("dot") is not None:
            segs.append(sm.group("dot"))
        elif sm.group("idx") is not None:
            segs.append(int(sm.group("idx")))
        else:
            segs.append(sm.group("bkt"))
        pos = sm.end()
    return segs


def resolve(target: Any, path: str) -> Any:
    """Return the value at ``path`` within ``target``."""
    cur = target
    for seg in parse(path):
        try:
            cur = cur[seg]
        except (KeyError, IndexError, TypeError) as e:
            raise PathError(
                HookError.TRANSFORM_INVALID, f"segment {seg!r} did not resolve: {e}"
            ) from e
    return cur


def apply(target: Any, path: str, value: Any) -> Any:
    """Return ``target`` with the value at ``path`` replaced by ``value``.

    Mutates ``target`` in place when it is a mutable container and returns
    it; when ``path`` is the bare root, returns ``value`` (the whole target
    is replaced).
    """
    segs = parse(path)
    if not segs:
        return value
    cur = target
    for seg in segs[:-1]:
        try:
            cur = cur[seg]
        except (KeyError, IndexError, TypeError) as e:
            raise PathError(
                HookError.TRANSFORM_INVALID, f"segment {seg!r} did not resolve: {e}"
            ) from e
    last = segs[-1]
    try:
        cur[last] = value
    except (IndexError, TypeError) as e:
        raise PathError(
            HookError.TRANSFORM_INVALID, f"cannot set {last!r}: {e}"
        ) from e
    return target
