import assert from "node:assert/strict";
import test from "node:test";

import { createCoreEnvironment } from "../src/environment";

test("injects a stored API key without mutating the inherited environment", () => {
  const inherited: NodeJS.ProcessEnv = {
    PATH: "/bin",
    QWENPAW_API_KEY: "environment-key",
  };

  const environment = createCoreEnvironment(inherited, {
    baseUrl: "https://example.test/v1",
    mcpConfigPath: "/workspace/agent.json",
    model: "qwen-test",
    storedApiKey: "secret-storage-key",
  });

  assert.deepEqual(environment, {
    PATH: "/bin",
    QWENPAW_API_KEY: "secret-storage-key",
    QWENPAW_BASE_URL: "https://example.test/v1",
    QWENPAW_MCP_CONFIG: "/workspace/agent.json",
    QWENPAW_MODEL: "qwen-test",
  });
  assert.deepEqual(inherited, {
    PATH: "/bin",
    QWENPAW_API_KEY: "environment-key",
  });
});

test("preserves an environment API key when SecretStorage is empty", () => {
  const environment = createCoreEnvironment(
    { QWENPAW_API_KEY: "environment-key" },
    {
      baseUrl: "https://example.test/v1",
      mcpConfigPath: "",
      model: "qwen-test",
      storedApiKey: undefined,
    },
  );

  assert.equal(environment.QWENPAW_API_KEY, "environment-key");
});

test("preserves an inherited MCP config path when the setting is empty", () => {
  const environment = createCoreEnvironment(
    { QWENPAW_MCP_CONFIG: "/inherited/mcp.json" },
    {
      baseUrl: "https://example.test/v1",
      mcpConfigPath: "  ",
      model: "qwen-test",
      storedApiKey: undefined,
    },
  );

  assert.equal(environment.QWENPAW_MCP_CONFIG, "/inherited/mcp.json");
});
