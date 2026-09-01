import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import { resolveCoreExecutable } from "../src/coreExecutable";
import { PROTOCOL_VERSION } from "../src/generated/protocol";

test("uses an explicitly configured Core path first", async () => {
  assert.deepEqual(
    await resolveCoreExecutable({
      configuredPath: " /opt/qwenpaw/qwenpaw-core ",
      extensionPath: "/missing",
      platform: "linux",
      arch: "x64",
    }),
    { path: "/opt/qwenpaw/qwenpaw-core", source: "configured" },
  );
});

test("falls back to PATH when no matching bundle exists", async () => {
  assert.deepEqual(
    await resolveCoreExecutable({
      configuredPath: "",
      extensionPath: "/missing",
      platform: "win32",
      arch: "x64",
    }),
    { path: "qwenpaw-core.exe", source: "path" },
  );
});

test("selects and verifies the bundled Core for the current target", async () => {
  const fixture = await createBundleFixture();
  try {
    assert.deepEqual(
      await resolveCoreExecutable({
        configuredPath: undefined,
        extensionPath: fixture.root,
        platform: "linux",
        arch: "x64",
      }),
      { path: fixture.binaryPath, source: "bundled" },
    );
  } finally {
    await rm(fixture.root, { recursive: true, force: true });
  }
});

test("rejects a bundled Core whose checksum changed", async () => {
  const fixture = await createBundleFixture();
  try {
    await writeFile(fixture.binaryPath, "tampered");
    await assert.rejects(
      resolveCoreExecutable({
        configuredPath: undefined,
        extensionPath: fixture.root,
        platform: "linux",
        arch: "x64",
      }),
      /checksum mismatch/,
    );
  } finally {
    await rm(fixture.root, { recursive: true, force: true });
  }
});

async function createBundleFixture(): Promise<{
  root: string;
  binaryPath: string;
}> {
  const root = await mkdtemp(join(tmpdir(), "qwenpaw-extension-test-"));
  const target = "linux-x64";
  const executable = "qwenpaw-core";
  const binary = Buffer.from("core fixture");
  const binaryPath = join(root, "resources", "core", target, executable);
  await mkdir(join(root, "resources", "core", target), { recursive: true });
  await writeFile(
    join(root, "core-release.json"),
    `${JSON.stringify({
      version: "0.1.0",
      protocolVersion: PROTOCOL_VERSION,
    })}\n`,
  );
  await writeFile(binaryPath, binary);
  await writeFile(
    join(root, "resources", "core", "manifest.json"),
    `${JSON.stringify({
      version: "0.1.0",
      protocolVersion: PROTOCOL_VERSION,
      target,
      executable,
      sha256: createHash("sha256").update(binary).digest("hex"),
    })}\n`,
  );
  return { root, binaryPath };
}
