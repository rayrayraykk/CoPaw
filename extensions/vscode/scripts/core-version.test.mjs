import assert from "node:assert/strict";
import test from "node:test";

import { verifyCoreVersionResult } from "./core-version.mjs";

test("accepts the locked Core version", () => {
  assert.doesNotThrow(() =>
    verifyCoreVersionResult("0.1.0", {
      status: 0,
      stdout: "qwenpaw-core 0.1.0\n",
    }),
  );
});

test("rejects a Core binary with a different version", () => {
  assert.throws(
    () =>
      verifyCoreVersionResult("0.1.0", {
        status: 0,
        stdout: "qwenpaw-core 0.2.0\n",
      }),
    /version mismatch/,
  );
});

test("rejects a Core binary that cannot report its version", () => {
  assert.throws(
    () =>
      verifyCoreVersionResult("0.1.0", {
        status: 1,
        stdout: "",
      }),
    /--version failed/,
  );
});
