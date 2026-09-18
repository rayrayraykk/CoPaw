// Static package inspection only: never execute or re-sign packaged programs.
import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { lstat, mkdir, mkdtemp, readFile, readdir, readlink, realpath, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

assert.equal(process.platform, "darwin");
assert.equal(process.argv.length, 3, "Usage: inspect-qa-macos.mjs <QA output directory>");
const core = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const repo = dirname(core);
const output = await realpath(process.argv[2]);
const locations = JSON.parse(await readFile(join(output, "locations.json"), "utf8"));
assert.equal(locations.repo, repo);
assert.equal(locations.output, output);
const scratch = await mkdtemp(join(await realpath(locations.scratch), "inspection-"));
const report = { kind: "static-only", inspection: scratch, packagedRuntimeTested: false,
  passed: false, checks: {} };
const digest = bytes => createHash("sha256").update(bytes).digest("hex");
const hash = async path => digest(await readFile(path));
const safe = path => {
  assert.ok(typeof path === "string" && path && !path.startsWith("/")
    && !path.includes("\\") && !path.split("/").some(part => [".", "..", ""].includes(part)));
  return path;
};
const run = (name, command, args) => {
  console.log(`Inspecting ${name}`);
  execFileSync(command, args, { stdio: "pipe", maxBuffer: 8 * 1024 * 1024 });
  report.checks[name] = true;
};
async function tree(root, include = () => true, prefix = "") {
  const entries = [];
  for (const entry of await readdir(join(root, prefix), { withFileTypes: true })) {
    const path = prefix + entry.name;
    if (entry.isDirectory()) entries.push(...await tree(root, include, `${path}/`));
    else if (entry.isFile() && include(path)) entries.push([path, await hash(join(root, path))]);
    else assert.ok(entry.isFile(), `Unexpected asset type: ${path}`);
  }
  return entries.sort(([a], [b]) => a.localeCompare(b));
}
async function same(name, source, targets, include) {
  const expected = await tree(source, include);
  assert.ok(expected.length, `Empty source: ${name}`);
  const sha256 = digest(JSON.stringify(expected));
  for (const target of targets) {
    const actual = await tree(target, include);
    assert.equal(digest(JSON.stringify(actual)), sha256, `Payload mismatch: ${target}`);
  }
  report.checks[name] = { files: expected.length, copies: targets.length, sha256 };
}
let mounted = false;
const mount = join(scratch, "dmg");
try {
  const manifest = JSON.parse(await readFile(join(output, "build-manifest.json"), "utf8"));
  const names = ["QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg", "QwenPaw-Tauri-2.2.0b5-macOS.zip",
    "qwenpaw-core-darwin-arm64-QA.tar.gz", "webui/qwenpaw-webui-2.2.0b5-QA.tar.gz",
    "sdk/qwenpaw-sdk-0.2.0.tgz", "sdk/qwenpaw_sdk-0.2.0-py3-none-any.whl",
    "vscode/qwenpaw-vscode-universal-0.2.0-QA.vsix", "vscode/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix",
    "legacy/qwenpaw-2.2.0b5-py3-none-any.whl"];
  assert.deepEqual(manifest.artifacts.map(item => item.path).sort(), names.sort());
  for (const artifact of manifest.artifacts) {
    const bytes = await readFile(join(output, safe(artifact.path)));
    assert.equal(bytes.length, artifact.bytes);
    assert.equal(digest(bytes), artifact.sha256, artifact.path);
  }
  report.checks.artifactChecksums = manifest.artifacts.length;
  assert.equal(await readFile(join(output, "SHA256SUMS"), "utf8"),
    manifest.artifacts.map(({ path, sha256 }) => `${sha256}  ${path}\n`).join(""));
  const inputs = JSON.parse(await readFile(join(output, "source-inputs.json"), "utf8"));
  assert.equal(inputs.length, manifest.sourceFileCount);
  assert.equal(digest(JSON.stringify(inputs)), manifest.sourceTreeSha256);
  for (const input of inputs) {
    const path = join(repo, safe(input.path));
    if (input.deleted) await assert.rejects(lstat(path), { code: "ENOENT" });
    else {
      assert.equal((await lstat(path)).isSymbolicLink(), input.symlink);
      assert.equal(digest(input.symlink ? await readlink(path) : await readFile(path)), input.sha256, input.path);
    }
  }
  report.checks.sourceInputs = { files: inputs.length, sha256: manifest.sourceTreeSha256 };
  for (const [name, path, tar] of [
    ["core", "qwenpaw-core-darwin-arm64-QA.tar.gz", true],
    ["webui", "webui/qwenpaw-webui-2.2.0b5-QA.tar.gz", true],
    ["typescript", "sdk/qwenpaw-sdk-0.2.0.tgz", true],
    ["desktop", "QwenPaw-Tauri-2.2.0b5-macOS.zip", false],
    ["python", "sdk/qwenpaw_sdk-0.2.0-py3-none-any.whl", false],
    ["legacy", "legacy/qwenpaw-2.2.0b5-py3-none-any.whl", false],
    ["universal", "vscode/qwenpaw-vscode-universal-0.2.0-QA.vsix", false],
    ["platform", "vscode/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix", false],
  ]) {
    const destination = join(scratch, name);
    await mkdir(destination);
    run(`extract-${name}`, tar ? "tar" : "ditto", tar
      ? ["-xzf", join(output, path), "-C", destination]
      : ["-x", "-k", join(output, path), destination]);
  }
  await mkdir(mount);
  const dmg = join(output, "QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg");
  run("dmg-verify", "hdiutil", ["verify", dmg]);
  run("dmg-mount", "hdiutil", ["attach", "-readonly", "-nobrowse", "-mountpoint", mount, dmg]);
  mounted = true;
  const apps = [join(scratch, "desktop/QwenPaw Desktop.app"), join(mount, "QwenPaw Desktop.app")];
  for (const [index, app] of apps.entries()) run(`app-signature-${index}`, "codesign", ["--verify", "--deep", "--strict", app]);
  const resources = apps.map(app => join(app, "Contents/Resources/binaries/qwenpaw-core"));
  const sourceCore = await hash(join(core, "target/release/qwenpaw-core"));
  assert.equal(await hash(join(scratch, "core/qwenpaw-core")), sourceCore);
  assert.equal(await hash(join(scratch, "platform/extension/resources/core/darwin-arm64/qwenpaw-core")), sourceCore);
  const signedCore = await hash(join(resources[0], "qwenpaw-core"));
  assert.equal(await hash(join(resources[1], "qwenpaw-core")), signedCore);
  report.checks.coreHashes = { sourceCore, signedCore, note: "Signature integrity and hashes only; no runtime probe" };
  await same("console", join(repo, "console/dist"), [join(scratch, "webui/dist"),
    ...resources.map(path => join(path, "console")), join(scratch, "legacy/qwenpaw/console")]);
  await same("typescript", join(core, "sdk/typescript/dist/src"), [join(scratch, "typescript/package/dist/src")]);
  await same("python", join(core, "sdk/python/src/qwenpaw_sdk"), [join(scratch, "python/qwenpaw_sdk")], path => path.endsWith(".py"));
  await same("vscode", join(repo, "extensions/vscode/out/src"), ["universal", "platform"].map(name => join(scratch, name, "extension/out/src")), path => path.endsWith(".js"));
  await assert.rejects(lstat(join(scratch, "universal/extension/resources/core")), { code: "ENOENT" });
  report.passed = true;
} catch (error) {
  report.error = error.message;
  throw error;
} finally {
  try {
    if (mounted) {
      report.passed = false;
      run("dmg-unmount", "hdiutil", ["detach", mount]);
      report.passed = !report.error;
    }
  } catch (error) {
    report.error = `${report.error ?? ""} DMG detach failed: ${error.message}`;
    throw error;
  } finally {
    await writeFile(join(output, "static-inspection.json"), `${JSON.stringify(report, null, 2)}\n`, { flag: "wx" });
    console.log(JSON.stringify(report, null, 2));
  }
}
