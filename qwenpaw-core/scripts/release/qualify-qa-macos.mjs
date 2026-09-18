import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createWriteStream } from "node:fs";
import { copyFile, mkdir, readFile, realpath, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scripts = dirname(fileURLToPath(import.meta.url));
const core = resolve(scripts, "../..");
const repo = dirname(core);
const output = await realpath(process.argv[2]);
const locations = JSON.parse(await readFile(join(output, "locations.json"), "utf8"));
assert.equal(locations.repo, repo);
assert.equal(locations.output, output);
const scratch = await realpath(locations.scratch);
const env = Object.fromEntries(Object.entries(process.env).filter(
  ([name]) => !/KEY|TOKEN|SECRET|PASSWORD|QWENPAW|COPAW|MCP|PROXY|^APPLE_/i.test(name),
));
const results = {};
async function run(name, command, args, cwd = scratch, extra = {}, required = true) {
  console.log(`Checking ${name}`);
  const log = createWriteStream(join(output, `${name}.log`), { flags: "wx" });
  const child = spawn(command, args, { cwd, env: { ...env, ...extra }, stdio: ["ignore", "pipe", "pipe"] });
  child.stdout.pipe(log, { end: false });
  child.stderr.pipe(log, { end: false });
  const timeout = setTimeout(() => child.kill(), 180_000);
  let result;
  try {
    result = await new Promise((done, reject) => {
      child.once("error", reject);
      child.once("close", (code, signal) => done({ code, signal }));
    });
  } finally {
    clearTimeout(timeout);
    await new Promise(done => log.end(done));
  }
  results[name] = result;
  if (required) assert.equal(result.code, 0, `${name} failed; inspect its log`);
}

let mounted = false;
try {
  await run("checksums", "shasum", ["-a", "256", "-c", "SHA256SUMS"], output);
  for (const name of ["core", "webui", "desktop", "platform", "universal"])
    await mkdir(join(scratch, "unpacked", name), { recursive: true });
  await run("extract-core", "tar", ["-xzf", join(output, "qwenpaw-core-darwin-arm64-QA.tar.gz"), "-C", join(scratch, "unpacked/core")]);
  await run("extract-webui", "tar", ["-xzf", join(output, "webui/qwenpaw-webui-2.2.0b5-QA.tar.gz"), "-C", join(scratch, "unpacked/webui")]);
  for (const [name, source] of [
    ["desktop", "QwenPaw-Tauri-2.2.0b5-macOS.zip"],
    ["platform", "vscode/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix"],
    ["universal", "vscode/qwenpaw-vscode-universal-0.2.0-QA.vsix"],
  ]) await run(`extract-${name}`, "ditto", ["-x", "-k", join(output, source), join(scratch, "unpacked", name)]);
  await mkdir(join(scratch, "dmg-mounted"));
  const dmg = join(output, "QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg");
  await run("dmg-verify", "hdiutil", ["verify", dmg]);
  await run("dmg-mount", "hdiutil", ["attach", "-readonly", "-nobrowse", "-mountpoint", join(scratch, "dmg-mounted"), dmg]);
  mounted = true;
  await mkdir(join(scratch, "signed-reference"));
  await copyFile(join(core, "target/release/qwenpaw-core"), join(scratch, "signed-reference/qwenpaw-core"));
  await run("reference-sign", "codesign", ["--force", "--sign", "-", "--timestamp=none", join(scratch, "signed-reference/qwenpaw-core")]);
  await mkdir(join(scratch, "installed-ts"));
  await run("install-typescript", "npm", ["install", "--ignore-scripts", "--no-audit", "--no-fund", "--prefix",
    join(scratch, "installed-ts"), join(output, "sdk/qwenpaw-sdk-0.2.0.tgz")]);
  for (const [name, wheel] of [
    ["sdk", "sdk/qwenpaw_sdk-0.2.0-py3-none-any.whl"],
    ["legacy", "legacy/qwenpaw-2.2.0b5-py3-none-any.whl"],
  ]) await run(`install-${name}`, "python", ["-m", "pip", "install", "--no-deps", "--target", join(scratch, `installed-${name}`), join(output, wheel)]);
  const legacyEnv = { PYTHONPATH: join(scratch, "installed-legacy"), QWENPAW_WORKING_DIR: join(scratch, "legacy-user") };
  await run("legacy-import", "python", ["-c", "import qwenpaw; print(qwenpaw.__file__)"], scratch, legacyEnv);
  await run("legacy-version", "python", ["-m", "qwenpaw", "--version"], scratch, legacyEnv);
  await run("legacy-tui", "python", ["-m", "qwenpaw", "tui", "--help"], scratch, legacyEnv);
  await run("legacy-tests", "python", ["-m", "pytest", "-o", "pythonpath=", "tests/unit/cli", "-q"], repo, legacyEnv, false);
  await run("legacy-integration-tests", "python", ["-m", "pytest", "-o", "pythonpath=", "tests/integration/test_cli_surface.py", "-q"], repo, legacyEnv, false);
  for (const [name, binary] of [
    ["python-sdk-packaged-core", join(scratch, "unpacked/core/qwenpaw-core")],
    ["python-sdk-source-control", join(core, "target/release/qwenpaw-core")],
  ]) await run(name, "python", ["-m", "unittest", "discover", "-s", join(core, "sdk/python/tests"), "-v"], scratch,
    { PYTHONPATH: join(scratch, "installed-sdk"), QWENPAW_CORE_BIN: binary }, false);
  const code = "/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code";
  for (const target of ["universal", "darwin-arm64"])
    await run(`install-vscode-${target}`, code, ["--user-data-dir", join(scratch, `vscode-${target}-user`),
      "--extensions-dir", join(scratch, `vscode-${target}-extensions`), "--install-extension",
      join(output, `vscode/qwenpaw-vscode-${target}-0.2.0-QA.vsix`)]);
  await run("package-smoke", "node", [join(scripts, "qa-package-smoke.mjs"), scratch, repo, output], scratch, {}, false);

  await run("webui-browser", "node", [join(scripts, "qa-webui-smoke.mjs"), output,
    join(scratch, "unpacked/core/qwenpaw-core")], scratch, {}, false);
} finally {
  if (mounted) await run("dmg-unmount", "hdiutil", ["detach", join(scratch, "dmg-mounted")]);
  await writeFile(join(output, "qualification.json"), `${JSON.stringify(results, null, 2)}\n`);
}
console.log(JSON.stringify(results, null, 2));
if (Object.values(results).some(result => result.code !== 0)) process.exitCode = 1;
