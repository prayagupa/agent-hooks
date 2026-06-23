#!/usr/bin/env python3
"""Generate spec/schema/hook-context/<hook_point>.schema.json from the master.

Each per-point schema is a closed (additionalProperties: false on the L1
payload object) variant of hook-context.schema.json restricted to one
hook_point value, used by the CTK for strict validation.
"""
from __future__ import annotations

import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parents[1]
MASTER = ROOT / "spec" / "schema" / "hook-context.schema.json"
OUT = ROOT / "spec" / "schema" / "hook-context"

# hook_point -> (extra L1 required fields beyond L0, payload $defs to close)
L1: dict[str, tuple[list[str], list[str]]] = {
    "agent_startup": (["agent_init"], ["agent_init"]),
    "input": (["input"], ["input"]),
    "pre_model_call": (["model", "messages"], ["model", "messages"]),
    "post_model_call": (["model", "response"], ["model", "response"]),
    "pre_tool_call": (["tool_call"], ["tool_call"]),
    "post_tool_call": (["tool_call", "tool_result"], ["tool_call", "tool_result"]),
    "output": (["output"], ["output"]),
    "agent_shutdown": (["summary"], ["summary"]),
}

L0_REQUIRED = ["spec", "hook_point", "timestamp", "sequence", "agent", "session", "target"]


def main() -> None:
    master = json.loads(MASTER.read_text())
    OUT.mkdir(parents=True, exist_ok=True)
    for hp, (extra_req, close_defs) in L1.items():
        # Start from master $defs but close the L1 payload objects.
        defs = json.loads(json.dumps(master["$defs"]))  # deep copy
        for d in close_defs:
            if d in defs and defs[d].get("type") == "object":
                defs[d]["additionalProperties"] = False
        schema = {
            "$id": f"https://agent-hooks.responsibleai.dev/v0.1/hook-context/{hp}.schema.json",
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "title": f"Agent Hooks — hook context ({hp})",
            "description": (
                f"Closed L0+L1 schema for hook_point={hp} "
                f"(AGENT-HOOKS-0.1 §4.2). Used by the CTK for strict validation."
            ),
            "type": "object",
            "required": L0_REQUIRED + extra_req,
            "properties": {
                **{k: master["properties"][k] for k in master["properties"]},
                "hook_point": {"const": hp},
            },
            "$defs": defs,
        }
        out = OUT / f"{hp}.schema.json"
        out.write_text(json.dumps(schema, indent=2) + "\n")
        print(f"wrote {out.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
