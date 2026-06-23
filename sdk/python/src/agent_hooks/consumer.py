# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
"""Consumer protocol (§7)."""
from __future__ import annotations

from typing import Any, Protocol, runtime_checkable

from agent_hooks._types import Verdict
from agent_hooks.context import HookContext


@runtime_checkable
class HookConsumer(Protocol):
    """A callable that receives a :class:`HookContext` and returns a :class:`Verdict`.

    Consumers MAY be sync or async; the emitter awaits coroutines. A consumer
    MAY return a :class:`Verdict`, a wire-shaped ``dict``, or raise — the
    emitter normalizes per §5/§6.3.
    """

    def on_hook(self, context: HookContext, /) -> Verdict | dict[str, Any]: ...
