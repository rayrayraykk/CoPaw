import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";

import { PROTOCOL_VERSION } from "../src/generated/protocol";

interface ProtocolLock {
  readonly protocolVersion: number;
  readonly sha256: string;
}

test("generated App Protocol matches its checked-in lock", async () => {
  const extensionRoot = path.resolve(__dirname, "../..");
  const [source, lockText] = await Promise.all([
    readFile(path.join(extensionRoot, "src/generated/protocol.ts")),
    readFile(path.join(extensionRoot, "protocol-lock.json"), "utf8"),
  ]);
  const lock = JSON.parse(lockText) as ProtocolLock;

  assert.deepEqual(
    {
      protocolVersion: PROTOCOL_VERSION,
      sha256: createHash("sha256").update(source).digest("hex"),
    },
    lock,
  );
});
