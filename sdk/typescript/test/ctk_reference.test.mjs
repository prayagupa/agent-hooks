// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.
// CTK self-test: run all Level<=2 vectors against the ReferenceHarness.

import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { test } from "node:test";
import assert from "node:assert/strict";

import { loadVectors, runVector, ReferenceHarness } from "../dist/ctk/index.js";

const here = dirname(fileURLToPath(import.meta.url));
const vectorsDir = resolve(here, "../../../conformance/vectors");

for (const v of loadVectors(vectorsDir, 2)) {
  test(`ctk ${v.id} ${v.title}`, async () => {
    const r = await runVector(new ReferenceHarness(), v);
    if (r.status === "skip") {
      // node:test has t.skip() but not from outside; treat as pass with note.
      return;
    }
    assert.equal(r.status, "pass", `\n  ${r.failures.join("\n  ")}`);
  });
}
