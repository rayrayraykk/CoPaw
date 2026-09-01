import { createHash } from "node:crypto";
import { spawnSync } from "node:child_process";
import { access, chmod, copyFile, mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import { verifyCoreVersionResult } from "./core-version.mjs";
import { parsePackageKind } from "./package-kind.mjs";

const supportedTargets = new Set([
  "darwin-arm64",
  "darwin-x64",
  "linux-x64",
  "win32-x64",
]);
const extensionRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const binarySetting = process.env.QWENPAW_CORE_BIN;
const target = process.env.QWENPAW_VSCODE_TARGET;
const packageKind = parsePackageKind(
  process.env.QWENPAW_VSCODE_PACKAGE_KIND,
);

if (!binarySetting) {
  throw new Error("QWENPAW_CORE_BIN must point to a release Core binary");
}
if (!target || !supportedTargets.has(target)) {
  throw new Error(
    `QWENPAW_VSCODE_TARGET must be one of ${[...supportedTargets].join(", ")}`,
  );
}

const source = resolve(process.cwd(), binarySetting);
await access(source);
const executable = target.startsWith("win32-")
  ? "qwenpaw-core.exe"
  : "qwenpaw-core";
const coreRoot = join(extensionRoot, "resources", "core");
const destination = join(coreRoot, target, executable);
const release = JSON.parse(
  await readFile(join(extensionRoot, "core-release.json"), "utf8"),
);
if (
  typeof release.version !== "string" ||
  !release.version ||
  !Number.isInteger(release.protocolVersion)
) {
  throw new Error("core-release.json is invalid");
}
verifyCoreVersion(source, release.version);
if (target.startsWith("darwin-") && packageKind === "release") {
  verifyMacRelease(source);
} else if (target.startsWith("darwin-")) {
  process.stderr.write(
    "QA package: skipping Developer ID and Gatekeeper checks for macOS Core\n",
  );
}

await rm(coreRoot, { recursive: true, force: true });
await mkdir(dirname(destination), { recursive: true });
await copyFile(source, destination);
if (!target.startsWith("win32-")) {
  await chmod(destination, 0o755);
}
if (target.startsWith("darwin-") && packageKind === "release") {
  verifyMacRelease(destination);
}
const binary = await readFile(destination);
const manifest = {
  version: release.version,
  protocolVersion: release.protocolVersion,
  target,
  executable,
  packageKind,
  sha256: createHash("sha256").update(binary).digest("hex"),
};
await writeFile(
  join(coreRoot, "manifest.json"),
  `${JSON.stringify(manifest, null, 2)}\n`,
);
process.stdout.write(`Staged ${destination}\n`);

function verifyMacRelease(path) {
  if (process.platform !== "darwin") {
    throw new Error(
      "macOS Core packages must be staged on macOS after signing and notarization",
    );
  }
  runSecurityCheck("codesign", ["--verify", "--deep", "--strict", path]);
  runSecurityCheck("spctl", ["--assess", "--type", "execute", path]);
}

function verifyCoreVersion(path, version) {
  const result = spawnSync(path, ["--version"], { encoding: "utf8" });
  verifyCoreVersionResult(version, result);
}

function runSecurityCheck(command, args) {
  const result = spawnSync(command, args, { encoding: "utf8" });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    const detail = `${result.stdout ?? ""}${result.stderr ?? ""}`.trim();
    throw new Error(
      `${command} rejected the Core release${detail ? `: ${detail}` : ""}`,
    );
  }
}
