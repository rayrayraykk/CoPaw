import assert from "node:assert/strict";
import test from "node:test";

import { waitForMcpAuthorization } from "../src/mcpOAuth";

test("polls until MCP OAuth becomes authorized", async () => {
  let checks = 0;
  let sleeps = 0;
  const result = await waitForMcpAuthorization({
    readAuthorized: async () => {
      checks += 1;
      return checks === 3;
    },
    isCancelled: () => false,
    sleep: async () => {
      sleeps += 1;
    },
  });

  assert.equal(result, "authorized");
  assert.equal(checks, 3);
  assert.equal(sleeps, 2);
});

test("stops MCP OAuth polling when cancelled", async () => {
  let checks = 0;
  const result = await waitForMcpAuthorization({
    readAuthorized: async () => {
      checks += 1;
      return false;
    },
    isCancelled: () => true,
    sleep: async () => undefined,
  });

  assert.equal(result, "cancelled");
  assert.equal(checks, 0);
});

test("bounds MCP OAuth polling", async () => {
  let checks = 0;
  const result = await waitForMcpAuthorization({
    readAuthorized: async () => {
      checks += 1;
      return false;
    },
    isCancelled: () => false,
    sleep: async () => undefined,
    maxAttempts: 3,
  });

  assert.equal(result, "timedOut");
  assert.equal(checks, 3);
});
