import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { createWriteStream } from "node:fs";
import { access, cp, copyFile, mkdir, mkdtemp, readFile, realpath, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";

// Run in the qwenpaw conda environment with Node 24+ on PATH.
// This never publishes artifacts or accesses production signing credentials.
assert.equal(process.platform, "darwin");
assert.equal(process.arch, "arm64");
const scripts = dirname(fileURLToPath(import.meta.url));
const core = resolve(scripts, "../..");
const repo = dirname(core);
await mkdir(join(repo, "dist"), { recursive: true });
const date = new Date().toISOString().slice(0, 10).replaceAll("-", "");
const resume = process.argv[2] === "--after-desktop";
assert.ok(process.argv.length === 2 || (resume && process.argv.length === 4),
  "Usage: build-qa-macos.mjs [--after-desktop <QA output directory>]");
const previous = resume ? JSON.parse(await readFile(join(process.argv[3], "locations.json"), "utf8")) : null;
if (previous) {
  assert.equal(previous.repo, repo);
  assert.equal(previous.output, await realpath(process.argv[3]));
}
const output = previous?.output ?? await mkdtemp(join(repo, `dist/qa-runtime-${date}-`));
const scratch = previous?.scratch ?? await mkdtemp(join(tmpdir(), "qwenpaw-qa-runtime-"));
const consoleRoot = join(repo, "console");
const extension = join(repo, "extensions/vscode");
const binary = join(core, "target/release/qwenpaw-core");
const environment = Object.fromEntries(Object.entries(process.env).filter(
  ([name]) => !/^(APPLE_|TAURI_SIGNING_|QWENPAW_|COPAW_)/.test(name)
    && !/KEY|TOKEN|SECRET|PASSWORD/i.test(name),
));
environment.APPLE_SIGNING_IDENTITY = "-";
const locations = { output, scratch, repo };
await writeFile(join(output, "locations.json"), `${JSON.stringify(locations, null, 2)}\n`);
console.log(JSON.stringify(locations));

async function run(name, command, args, cwd = repo, extraEnv = {}) {
  console.log(`Building ${name}`);
  const log = createWriteStream(join(output, `${name}.log`), { flags: "wx" });
  const child = spawn(command, args, {
    cwd, env: { ...environment, ...extraEnv }, stdio: ["ignore", "pipe", "pipe"],
  });
  child.stdout.pipe(log, { end: false });
  child.stderr.pipe(log, { end: false });
  const status = await new Promise((done, reject) => {
    child.once("error", reject);
    child.once("close", (code, signal) => done({ code, signal }));
  });
  await new Promise(done => log.end(done));
  assert.equal(status.code, 0, `${name} failed (${status.signal}); inspect ${output}/${name}.log`);
}

for (const subdir of ["sdk", "vscode", "legacy", "webui", "tauri-macos"])
  await mkdir(join(output, subdir), { recursive: true });
if (!resume) {
await run("console-build", "npm", ["run", "build:prod"], consoleRoot);
await run("core-stage", "bash", ["scripts/pack-tauri/stage_rust_core.sh"]);
await run("tauri-build", "npm", ["exec", "--", "tauri", "build", "--config",
  "src-tauri/tauri.version.conf.json", "--bundles", "app"], consoleRoot);
const app = join(output, "tauri-macos/QwenPaw Desktop.app");
await cp(join(consoleRoot, "src-tauri/target/release/bundle/macos/QwenPaw Desktop.app"),
  app, { recursive: true });
await run("desktop-sign", "bash", ["scripts/pack-tauri/sign_macos_bundle.sh", app, "-"]);
await run("desktop-zip", "ditto", ["-c", "-k", "--sequesterRsrc", "--keepParent", app,
  join(output, "QwenPaw-Tauri-2.2.0b5-macOS.zip")]);
await run("desktop-dmg", "hdiutil", ["create", "-fs", "HFS+", "-volname", "QwenPaw QA", "-srcfolder",
  join(output, "tauri-macos"), "-format", "UDZO",
  join(output, "QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg")]);
} else {
  for (const name of ["QwenPaw-Tauri-2.2.0b5-macOS-arm64-QA.dmg", "QwenPaw-Tauri-2.2.0b5-macOS.zip"])
    await access(join(output, name));
}
const archive = join(scratch, "archive-core");
await mkdir(archive);
await copyFile(binary, join(archive, "qwenpaw-core"));
await run("core-archive", "tar", ["-czf", join(output, "qwenpaw-core-darwin-arm64-QA.tar.gz"),
  "-C", archive, "qwenpaw-core"]);
await run("webui-archive", "tar", ["-czf", join(output, "webui/qwenpaw-webui-2.2.0b5-QA.tar.gz"),
  "-C", consoleRoot, "dist"]);
await run("typescript-build", "npm", ["run", "build"], join(core, "sdk/typescript"));
await run("typescript-pack", "npm", ["pack", "--pack-destination", join(output, "sdk")],
  join(core, "sdk/typescript"));
await run("python-sdk-pack", "python", ["-m", "build", "--no-isolation", "--wheel", "--outdir",
  join(output, "sdk"), join(core, "sdk/python")]);
await run("vscode-compile", "npm", ["run", "compile"], extension);
await run("vscode-clean", "node", ["scripts/clean-core.mjs"], extension);
const vsce = join(extension, "node_modules/@vscode/vsce/vsce");
await run("vscode-universal", "node", [vsce, "package", "--no-dependencies", "--out",
  join(output, "vscode/qwenpaw-vscode-universal-0.2.0-QA.vsix")], extension);
await run("vscode-stage", "node", ["scripts/stage-core.mjs"], extension, {
  QWENPAW_CORE_BIN: binary, QWENPAW_VSCODE_TARGET: "darwin-arm64", QWENPAW_VSCODE_PACKAGE_KIND: "qa",
});
await run("vscode-platform", "node", [vsce, "package", "--no-dependencies", "--target", "darwin-arm64", "--out",
  join(output, "vscode/qwenpaw-vscode-darwin-arm64-0.2.0-QA.vsix")], extension);

// Build the retained Python product independently, without wheel_build.sh's
// deletion of the shared dist directory or mutation of the original sources.
const legacy = join(scratch, "legacy-source");
await mkdir(legacy);
for (const name of ["pyproject.toml", "README.md", "LICENSE"])
  await copyFile(join(repo, name), join(legacy, name));
await cp(join(repo, "src"), join(legacy, "src"), {
  recursive: true, filter: path => !path.includes("__pycache__")
    && path !== join(repo, "src/qwenpaw/console") && path !== join(repo, "src/qwenpaw/docs"),
});
await cp(join(repo, "packages/qwenpawmail-mcp"), join(legacy, "packages/qwenpawmail-mcp"), {
  recursive: true, filter: path => !path.includes("__pycache__"),
});
await cp(join(consoleRoot, "dist"), join(legacy, "src/qwenpaw/console"), { recursive: true });
await cp(join(repo, "website/public/docs"), join(legacy, "src/qwenpaw/docs"), { recursive: true });
await run("legacy-pack", "python", ["-m", "build", "--no-isolation", "--wheel", "--outdir",
  join(output, "legacy"), legacy]);
await run("manifest", "node", [join(scripts, "write-qa-manifest.mjs"), repo, output]);
console.log(await readFile(join(output, "build-manifest.json"), "utf8"));
