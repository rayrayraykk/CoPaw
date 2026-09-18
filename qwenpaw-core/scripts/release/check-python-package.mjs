import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { readFile, readdir, realpath, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// Verify only the Python wheel, always connecting to the source Core.
const [outputArgument, pythonArgument] = process.argv.slice(2);
assert.ok(outputArgument && pythonArgument,
  "Usage: node check-python-package.mjs <empty-output-dir> <conda-python>");
const core = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const output = await realpath(outputArgument);
assert.deepEqual(await readdir(output), [], "Output must be fresh and empty");
const python = await realpath(pythonArgument);
const binary = await realpath(join(core, "target/release",
  process.platform === "win32" ? "qwenpaw-core.exe" : "qwenpaw-core"));
const hash = async (file) => createHash("sha256").update(await readFile(file)).digest("hex");
const env = Object.fromEntries(Object.entries(process.env).filter(
  ([key]) => !/KEY|TOKEN|SECRET|PASSWORD|QWENPAW|COPAW|MCP|PROXY|^APPLE_/i.test(key),
));
const report = { kind: "python-sdk-source-control", passed: false,
  packagedCoreExecuted: false, sourceCore: binary, sourceSha256: await hash(binary),
  checks: [] };

async function run(name, args, extra = {}) {
  const result = spawnSync(python, args, { cwd: output,
    env: { ...env, ...extra }, encoding: "utf8", maxBuffer: 8 * 1024 * 1024 });
  const log = (result.stdout ?? "") + (result.stderr ?? "");
  await writeFile(join(output, `${name}.log`), log, { flag: "wx" });
  report.checks.push({ name, code: result.status, signal: result.signal });
  if (result.error) throw result.error;
  assert.equal(result.status, 0, `${name}: ${log.slice(-3000)}`);
  console.log(`${name} passed`);
}

async function sources(directory) {
  const result = {};
  for (const file of (await readdir(directory)).filter((name) => name.endsWith(".py")))
    result[file] = await hash(join(directory, file));
  return result;
}

try {
  const sdk = join(core, "sdk/python");
  await run("build", ["-m", "build", "--no-isolation", "--wheel", "--outdir", output, sdk]);
  const wheels = (await readdir(output)).filter((name) => name.endsWith(".whl"));
  assert.equal(wheels.length, 1);
  const wheel = join(output, wheels[0]);
  report.package = { filename: wheels[0], sha256: await hash(wheel) };
  const install = join(output, "installed");
  await run("install", ["-m", "pip", "install", "--no-index", "--no-deps",
    "--no-compile", "--target", install, wheel]);
  assert.deepEqual(await sources(join(install, "qwenpaw_sdk")),
    await sources(join(sdk, "src/qwenpaw_sdk")));
  const code = [
    "import pathlib, sys, unittest, qwenpaw_sdk",
    "expected = pathlib.Path(sys.argv[1]) / f'qwenpaw_sdk' / f'__init__.py'",
    "assert pathlib.Path(qwenpaw_sdk.__file__).resolve() == expected.resolve()",
    "suite = unittest.defaultTestLoader.discover(sys.argv[2])",
    "result = unittest.TextTestRunner(verbosity=2).run(suite)",
    "assert result.testsRun == 17 and not result.skipped",
    "sys.exit(not result.wasSuccessful())",
  ].join("\n");
  await run("tests", ["-c", code, install, join(sdk, "tests")], {
    PYTHONPATH: install, PYTHONNOUSERSITE: "1", QWENPAW_CORE_BIN: binary,
  });
  assert.equal(await hash(binary), report.sourceSha256);
  report.passed = true;
} catch (error) {
  report.error = String(error);
  throw error;
} finally {
  await writeFile(join(output, "verification.json"), `${JSON.stringify(report, null, 2)}\n`, { flag: "wx" });
}
