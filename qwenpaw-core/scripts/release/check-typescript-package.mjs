import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { cp, mkdir, readFile, readdir, realpath, symlink, writeFile } from "node:fs/promises";
import { createRequire } from "node:module";
import { basename, dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// This SDK-only check always uses the source Core, never a packaged executable.
const [outputArgument, npmArgument] = process.argv.slice(2);
assert.ok(outputArgument && npmArgument,
  "Usage: node check-typescript-package.mjs <empty-output-dir> <npm-cli.js>");
const core = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const output = await realpath(outputArgument);
assert.deepEqual(await readdir(output), [], "Output must be a fresh empty directory");
const npm = await realpath(npmArgument);
const binary = await realpath(join(core, "target/release",
  process.platform === "win32" ? "qwenpaw-core.exe" : "qwenpaw-core"));
const hash = async (file) => createHash("sha256").update(await readFile(file)).digest("hex");
const env = Object.fromEntries(Object.entries(process.env).filter(
  ([key]) => !/KEY|TOKEN|SECRET|PASSWORD|QWENPAW|COPAW|MCP|PROXY|^APPLE_/i.test(key),
));
env.PATH = `${dirname(process.execPath)}${process.platform === "win32" ? ";" : ":"}${env.PATH}`;
const report = { kind: "typescript-sdk-source-control", passed: false,
  packagedCoreExecuted: false, sourceCore: binary, sourceSha256: await hash(binary),
  checks: [] };

async function run(name, args, cwd, extra = {}) {
  const result = spawnSync(process.execPath, args, {
    cwd, env: { ...env, ...extra }, encoding: "utf8", maxBuffer: 8 * 1024 * 1024,
  });
  const log = (result.stdout ?? "") + (result.stderr ?? "");
  await writeFile(join(output, `${name}.log`), log, { flag: "wx" });
  report.checks.push({ name, code: result.status, signal: result.signal });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${name}: ${log.slice(-3000)}`);
  console.log(`${name} passed`);
  return result.stdout;
}

async function tree(directory) {
  const result = {};
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const file = join(directory, entry.name);
    assert.ok(entry.isFile() || entry.isDirectory(), `Unexpected entry: ${file}`);
    result[entry.name] = entry.isDirectory() ? await tree(file) : await hash(file);
  }
  return result;
}

try {
  const sdk = join(core, "sdk/typescript");
  const packed = JSON.parse(await run("pack", [npm, "pack", "--ignore-scripts",
    "--json", "--pack-destination", output], sdk));
  assert.equal(packed.length, 1);
  const filename = packed[0].filename;
  assert.equal(filename, basename(filename));
  report.package = { filename, sha256: await hash(join(output, filename)) };
  const install = join(output, "installed");
  await mkdir(install);
  await run("install", [npm, "install", "--offline", "--ignore-scripts", "--no-audit",
    "--no-fund", "--package-lock=false", "--prefix", install, join(output, filename)], output);
  const entry = join(install, "node_modules/@qwenpaw/sdk");
  const source = join(entry, "dist/src");
  const require = createRequire(import.meta.url);
  assert.equal(await realpath(dirname(require.resolve(entry))), await realpath(source));
  assert.deepEqual(await tree(source), await tree(join(sdk, "dist/src")));
  const check = join(output, "check");
  const tests = join(check, "sdk/typescript/dist/test");
  await mkdir(dirname(tests), { recursive: true });
  await cp(join(sdk, "dist/test"), tests, { recursive: true });
  const linkType = process.platform === "win32" ? "junction" : "dir";
  await symlink(source, join(dirname(tests), "src"), linkType);
  await symlink(join(core, "docs"), join(check, "docs"), linkType);
  const files = (await readdir(tests)).filter((name) => name.endsWith(".test.js")).sort();
  assert.equal(files.length, 6);
  const log = await run("tests", ["--test", "--test-reporter=tap",
    ...files.map((file) => join(tests, file))], check, { QWENPAW_CORE_BIN: binary });
  for (const expected of ["# tests 10", "# pass 10", "# fail 0", "# skipped 0", "# cancelled 0"])
    assert.ok(log.includes(expected), expected);
  assert.equal(await hash(binary), report.sourceSha256);
  report.passed = true;
} catch (error) {
  report.error = String(error);
  throw error;
} finally {
  await writeFile(join(output, "verification.json"), `${JSON.stringify(report, null, 2)}\n`, { flag: "wx" });
}
