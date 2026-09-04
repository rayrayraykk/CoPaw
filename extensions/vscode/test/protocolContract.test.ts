import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import path from "node:path";
import test from "node:test";

import { PROTOCOL_VERSION } from "../src/generated/protocol";

interface ProtocolLock {
  readonly protocolVersion: number;
  readonly sha256: string;
  readonly sdkSha256: string;
}

test("generated App Protocol matches its checked-in lock", async () => {
  const extensionRoot = path.resolve(__dirname, "../..");
  const sdkFiles = ["protocol.ts", "rpcClient.ts", "appServerClient.ts"];
  const coreSdkRoot = path.resolve(
    extensionRoot,
    "../../qwenpaw-core/sdk/typescript/src",
  );
  const [sources, upstreamSources, lockText] = await Promise.all([
    Promise.all(
      sdkFiles.map(async (name) => ({
        name,
        contents: await readFile(
          path.join(extensionRoot, "src/generated", name),
        ),
      })),
    ),
    Promise.all(sdkFiles.map((name) => readFile(path.join(coreSdkRoot, name)))),
    readFile(path.join(extensionRoot, "protocol-lock.json"), "utf8"),
  ]);
  const lock = JSON.parse(lockText) as ProtocolLock;
  assert.deepEqual(
    sources.map((source) => source.contents),
    upstreamSources,
  );
  const protocol = sources[0]?.contents;
  assert.ok(protocol);
  const sdkHash = createHash("sha256");
  for (const source of sources) {
    sdkHash.update(source.name);
    sdkHash.update(source.contents);
  }

  assert.deepEqual(
    {
      protocolVersion: PROTOCOL_VERSION,
      sha256: createHash("sha256").update(protocol).digest("hex"),
      sdkSha256: sdkHash.digest("hex"),
    },
    lock,
  );
});
