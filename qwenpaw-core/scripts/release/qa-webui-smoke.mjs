import assert from "node:assert/strict";
import { execFile, spawn } from "node:child_process";
import { once } from "node:events";
import { createWriteStream } from "node:fs";
import { mkdir, mkdtemp, readFile, realpath, writeFile } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const core = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const output = await realpath(process.argv[2]);
const binary = await realpath(process.argv[3] ?? join(core, "target/release/qwenpaw-core"));
const { scratch, repo } = JSON.parse(await readFile(join(output, "locations.json"), "utf8"));
assert.equal(repo, dirname(core));
const runtime = await mkdtemp(join(scratch, "webui-probe-"));
await mkdir(join(runtime, "workspace"));
const env = Object.fromEntries(Object.entries(process.env).filter(
  ([name]) => !/KEY|TOKEN|SECRET|PASSWORD|QWENPAW|COPAW|MCP|PROXY/i.test(name),
));
const log = createWriteStream(join(runtime, "server.log"));
const child = spawn(binary, ["app-server", "--listen", "127.0.0.1:0", "--desktop",
  "--console-static-dir", join(scratch, "unpacked/webui/dist")], { cwd: runtime, stdio: ["ignore", "pipe", "pipe"], env: {
    ...env, QWENPAW_HOME: runtime, QWENPAW_DEFAULT_WORKSPACE: join(runtime, "workspace"),
    QWENPAW_API_KEY: "qa-fixture-only", QWENPAW_BASE_URL: "http://127.0.0.1:1/v1",
    QWENPAW_DESKTOP_SHUTDOWN_TOKEN: "qa-webui-fixture-only",
  } });
child.stdout.pipe(log, { end: false });
child.stderr.pipe(log, { end: false });
const exited = once(child, "exit");
try {
  let port;
  for (let count = 0; count < 300 && child.exitCode === null && child.signalCode === null; count++) {
    try { port = (await readFile(join(runtime, "desktop_port"), "utf8")).trim(); break; }
    catch (error) { if (error.code !== "ENOENT") throw error; }
    await new Promise(done => setTimeout(done, 100));
  }
  assert.match(port ?? "", /^\d+$/, `WebUI server failed; inspect ${runtime}/server.log`);
  const { stdout } = await promisify(execFile)(process.execPath,
    [join(core, "scripts/console_browser_smoke.mjs"), `http://127.0.0.1:${port}`, "/models"],
    { env, timeout: 60_000, maxBuffer: 2 * 1024 * 1024 });
  const report = JSON.parse(stdout);
  assert.equal(report.ok, true);
  const recorded = `${JSON.stringify({ ...report, coreBinary: binary }, null, 2)}\n`;
  await writeFile(join(output, "webui-browser.json"), recorded);
  console.log(recorded);
} finally {
  child.kill();
  await exited;
  await new Promise(done => log.end(done));
}
