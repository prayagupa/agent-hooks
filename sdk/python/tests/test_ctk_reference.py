# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.
"""CTK self-test: run all Level≤2 vectors against the in-tree ReferenceHarness."""
from __future__ import annotations

import pathlib

import pytest
from agent_hooks.ctk import load_vectors, run_vector
from agent_hooks.ctk.reference import ReferenceHarness

_VECTORS = pathlib.Path(__file__).resolve().parents[3] / "conformance" / "vectors"


@pytest.mark.parametrize(
    "vector",
    load_vectors(_VECTORS),
    ids=lambda v: v["id"],
)
def test_reference_harness_conformance(vector: dict) -> None:
    import asyncio

    result = asyncio.run(run_vector(ReferenceHarness(), vector))
    if result.status == "skip":
        pytest.skip(result.detail)
    assert result.status == "pass", "\n" + "\n".join(
        f"  - {f}" for f in result.failures
    )
