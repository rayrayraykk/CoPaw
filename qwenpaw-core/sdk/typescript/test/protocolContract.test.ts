import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";

import {
  APP_PROTOCOL_REQUEST_METHODS,
  APP_PROTOCOL_SERVER_NOTIFICATION_METHODS,
  PROTOCOL_VERSION,
} from "../src/protocol";

test("matches the shared App Protocol fixtures", async () => {
  const fixturePath = path.resolve(
    __dirname,
    "../../../../docs/api-contract/fixtures/app-protocol-v3.json",
  );
  const fixture = JSON.parse(await readFile(fixturePath, "utf8")) as {
    protocolVersion: number;
    requests: Record<string, unknown>;
    serverNotifications: Record<string, unknown>;
  };
  assert.equal(fixture.protocolVersion, PROTOCOL_VERSION);
  assert.deepEqual(
    Object.keys(fixture.requests).sort(),
    [...APP_PROTOCOL_REQUEST_METHODS].sort(),
  );
  assert.deepEqual(
    Object.keys(fixture.serverNotifications).sort(),
    [...APP_PROTOCOL_SERVER_NOTIFICATION_METHODS].sort(),
  );
});
