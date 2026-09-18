import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import {
  lstat,
  readFile,
  readlink,
  writeFile,
  realpath,
} from "node:fs/promises";
import { join } from "node:path";

const [repositoryArgument, outputArgument] = process.argv.slice(2);
assert.ok(
  repositoryArgument && outputArgument,
  "Usage: node write-qa-manifest.mjs <repo> <QA output directory>",
);
const repository = await realpath(repositoryArgument);
const output = await realpath(outputArgument);
const digest = (bytes) => createHash("sha256").update(bytes).digest("hex");
const git = (...args) =>
  execFileSync("git", args, { cwd: repository, encoding: "utf8" });
const paths = [
  ...new Set(
    git(
      "ls-files",
      "-c",
      "-o",
      "--exclude-standard",
      "-z",
      "--",
      "qwenpaw-core/Cargo.toml",
      "qwenpaw-core/Cargo.lock",
      "qwenpaw-core/crates",
      "qwenpaw-core/sdk",
      "qwenpaw-core/scripts/release",
      "console/src",
      "console/public",
      "console/scripts",
      "console/src-tauri",
      "console/package.json",
      "console/package-lock.json",
      "console/vite.config.ts",
      "extensions/vscode",
      "src",
      "packages/qwenpawmail-mcp",
      "scripts/pack-tauri",
      "scripts/pack/assets",
      "website/public/docs",
      "README.md",
      "LICENSE",
      "pyproject.toml",
    )
      .split("\0")
      .filter(Boolean),
  ),
].sort();
const sources = [];
for (const path of paths) {
  const absolute = join(repository, path);
  try {
    const status = await lstat(absolute);
    const bytes = status.isSymbolicLink()
      ? await readlink(absolute)
      : await readFile(absolute);
    sources.push({
      path,
      sha256: digest(bytes),
      symlink: status.isSymbolicLink(),
    });
  } catch (error) {
    if (error.code !== "ENOENT") throw error;
    sources.push({ path, deleted: true });
  }
}
const names = [
  "QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg",
  "QwenPaw-Tauri-2.2.0b5-macOS.zip",
  "qwenpaw-core-darwin-arm64-QA.tar.gz",
  "webui/qwenpaw-webui-2.2.0b5-QA.tar.gz",
  "sdk/qwenpaw-sdk-0.2.0.tgz",
  "sdk/qwenpaw_sdk-0.2.0-py3-none-any.whl",
  "vscode/qwenpaw-vscode-universal-0.2.0-QA.vsix",
  "vscode/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix",
  "legacy/qwenpaw-2.2.0b5-py3-none-any.whl",
];
const artifacts = [];
for (const path of names) {
  const bytes = await readFile(join(output, path));
  artifacts.push({ path, bytes: bytes.length, sha256: digest(bytes) });
}
const manifest = {
  kind: "qa",
  platform: process.platform,
  arch: process.arch,
  createdAt: new Date().toISOString(),
  gitCommit: git("rev-parse", "HEAD").trim(),
  dirty: git("status", "--porcelain").length > 0,
  sourceTreeSha256: digest(JSON.stringify(sources)),
  sourceFileCount: sources.length,
  qualification:
    "Build provenance only; consult acceptance results before running or distributing.",
  artifacts,
};
await writeFile(
  join(output, "source-inputs.json"),
  `${JSON.stringify(sources, null, 2)}\n`,
);
await writeFile(
  join(output, "build-manifest.json"),
  `${JSON.stringify(manifest, null, 2)}\n`,
);
await writeFile(
  join(output, "SHA256SUMS"),
  artifacts.map(({ path, sha256 }) => `${sha256}  ${path}\n`).join(""),
);
console.log(
  JSON.stringify(
    {
      files: artifacts.length,
      sourceFiles: sources.length,
      sourceTreeSha256: manifest.sourceTreeSha256,
      dirty: manifest.dirty,
    },
    null,
    2,
  ),
);
