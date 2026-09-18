import { test } from "node:test";
import assert from "node:assert/strict";
import { SENTINEL_DB_PACKAGE_VERSION } from "./index.js";

// Proves the TS test harness (tsx + node's built-in test runner) actually
// wires up and runs, before there is any real logic to test
// (docs/phases/phase-01-foundation.md §2, explicit non-scope).
test("package skeleton exports a version marker", () => {
  assert.equal(SENTINEL_DB_PACKAGE_VERSION, "0.1.0");
});
