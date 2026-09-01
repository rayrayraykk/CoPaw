import assert from "node:assert/strict";
import test from "node:test";

import { parsePackageKind } from "./package-kind.mjs";

test("defaults platform packages to release verification", () => {
  assert.equal(parsePackageKind(undefined), "release");
});

test("accepts an explicit QA package kind", () => {
  assert.equal(parsePackageKind("qa"), "qa");
});

test("rejects unknown package kinds", () => {
  assert.throws(
    () => parsePackageKind("unsigned"),
    /must be release or qa/,
  );
});
