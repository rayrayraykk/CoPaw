import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { execFileSync, spawn } from "node:child_process";
import { once } from "node:events";
import {
  readFile,
  readdir,
  mkdtemp,
  writeFile,
  realpath,
} from "node:fs/promises";
import { createServer } from "node:http";
import { createRequire } from "node:module";
import { join } from "node:path";
import { probeNativeExecution } from "./native-execution.mjs";

// Run only against previously extracted QA packages, never an installed user app.
const [scratchArgument, repositoryArgument, outputArgument] =
  process.argv.slice(2);
assert.ok(
  scratchArgument && repositoryArgument && outputArgument,
  "Usage: node qa-package-smoke.mjs <extracted-fixture-root> <repo> <output-dir>",
);
const scratch = await realpath(scratchArgument);
const repository = await realpath(repositoryArgument);
const output = await realpath(outputArgument);
const require = createRequire(import.meta.url);
const { QwenPaw } = require(join(
  scratch,
  "installed-ts/node_modules/@qwenpaw/sdk",
));
const checksum = (bytes) => createHash("sha256").update(bytes).digest("hex");
const hash = async (path) => checksum(await readFile(path));
const coreResource = "Contents/Resources/binaries/qwenpaw-core";
const zipApp = join(scratch, "unpacked/desktop/QwenPaw Desktop.app");
const dmgApp = join(scratch, "dmg-mounted/QwenPaw Desktop.app");
const sourceCore = join(repository, "qwenpaw-core/target/release/qwenpaw-core");
const archiveCore = join(scratch, "unpacked/core/qwenpaw-core");
const platform = join(scratch, "unpacked/platform/extension");
const universal = join(scratch, "unpacked/universal/extension");
const platformCore = join(platform, "resources/core/darwin-arm64/qwenpaw-core");
const clientInfo = { name: "qa_package_acceptance", version: "0.2.0" };
const answer = "Packaged Rust Core QA response";
const png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";
const modelRequests = [];
const model = createServer(async (request, response) => {
  assert.equal(request.url, "/v1/chat/completions");
  assert.equal(request.headers.authorization, "Bearer qa-fixture-not-a-secret");
  let body = "";
  for await (const chunk of request) body += chunk;
  const payload = JSON.parse(body);
  assert.equal(payload.model, "qa-fixture");
  modelRequests.push(payload.messages);
  response.writeHead(200, { "content-type": "text/event-stream" });
  response.end(
    `data: ${JSON.stringify({
      choices: [{ index: 0, delta: { content: answer } }],
    })}\n\n` +
      `data: ${JSON.stringify({
        choices: [{ index: 0, delta: {}, finish_reason: "stop" }],
      })}\n\n` +
      "data: [DONE]\n\n",
  );
});
model.listen(0, "127.0.0.1");
await once(model, "listening");
const baseUrl = `http://127.0.0.1:${model.address().port}/v1`;
const results = {};

function environment(directory) {
  return {
    ...Object.fromEntries(
      Object.entries(process.env).filter(
        ([name]) =>
          !/KEY|TOKEN|SECRET|PASSWORD|QWENPAW|COPAW|MCP|PROXY/i.test(name),
      ),
    ),
    QWENPAW_HOME: directory,
    QWENPAW_DEFAULT_WORKSPACE: directory,
    QWENPAW_API_KEY: "qa-fixture-not-a-secret",
    QWENPAW_BASE_URL: baseUrl,
    QWENPAW_MODEL: "qa-fixture",
  };
}

async function files(directory, prefix = "") {
  const result = {};
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const relative = prefix + entry.name;
    const path = join(directory, entry.name);
    if (entry.isDirectory())
      Object.assign(result, await files(path, `${relative}/`));
    else if (entry.isFile()) result[relative] = await hash(path);
    else assert.fail(`Unexpected asset type: ${relative}`);
  }
  return Object.fromEntries(
    Object.entries(result).sort(([a], [b]) => a.localeCompare(b)),
  );
}

async function sdkProbe(name, corePath) {
  console.log(`Checking SDK with ${name}`);
  const directory = await mkdtemp(join(scratch, `runtime-${name}-`));
  await writeFile(join(directory, "image.png"), Buffer.from(png, "base64"));
  const options = {
    corePath,
    env: environment(directory),
    cwd: directory,
    clientInfo,
    onStderr: (chunk) => process.stderr.write(chunk),
  };
  let sdk = await QwenPaw.start(options);
  let id;
  try {
    assert.deepEqual(sdk.client.serverInfo, {
      name: "qwenpaw-core",
      version: "0.2.0",
    });
    const thread = await sdk.startThread({ workspaceRoot: directory });
    id = thread.id;
    const result = await thread.run([{ type: "text", text: `Verify ${name}` },
      { type: "image", path: "image.png" }]);
    assert.equal(result.finalResponse, answer);
    assert.equal(result.turn.status, "completed");
    assert.deepEqual(modelRequests.at(-1).at(-1).content, [{ type: "text", text: `Verify ${name}` },
      { type: "image_url", image_url: { url: `data:image/png;base64,${png}` } }]);
    assert.equal(result.items[0].input[1].type, "image");
  } finally {
    await sdk.close();
  }
  sdk = await QwenPaw.start(options);
  try {
    const reopened = await sdk.resumeThread(id);
    assert.equal(reopened.id, id);
    const history = await sdk.client.request("thread/read", { threadId: id });
    assert.equal(history.turns.length, 1);
    assert.equal(history.turns[0].status, "completed");
    await writeFile(join(directory, "image.png"), "changed after restart");
    assert.equal((await reopened.run("Recall the original image")).finalResponse, answer);
    assert.deepEqual(modelRequests.at(-1).find(message => message.role === "user").content,
      [{ type: "text", text: `Verify ${name}` },
        { type: "image_url", image_url: { url: `data:image/png;base64,${png}` } }]);
  } finally {
    await sdk.close();
  }
  results[name] = { handshake: true, modelTurn: true, imageInput: true, restartHistory: true };
}

async function extensionProbe(name, extension, corePath) {
  console.log(`Checking packaged client ${name}`);
  const { resolveCoreExecutable } = require(join(
    extension,
    "out/src/coreExecutable.js",
  ));
  const selected = await resolveCoreExecutable({ extensionPath: extension });
  assert.equal(selected.source, extension === platform ? "bundled" : "path");
  if (selected.source === "bundled") assert.equal(selected.path, platformCore);
  const { AppServerClient } = require(join(
    extension,
    "out/src/generated/appServerClient.js",
  ));
  const directory = await mkdtemp(join(scratch, `protocol-${name}-`));
  const child = spawn(corePath, ["app-server", "--stdio"], {
    cwd: directory,
    env: environment(directory),
    stdio: "pipe",
  });
  child.stderr.resume();
  const exited = once(child, "exit");
  let client;
  try {
    client = await AppServerClient.connect(child.stdout, child.stdin, {
      clientInfo,
    });
    assert.deepEqual(client.serverInfo, {
      name: "qwenpaw-core",
      version: "0.2.0",
    });
    const created = await client.request("thread/start", {
      workspaceRoot: directory,
    });
    const listed = await client.request("thread/list", { limit: 10 });
    assert.deepEqual(listed.data, [created.thread]);
    const archived = await client.request("thread/archive", {
      threadId: created.thread.id,
    });
    assert.equal(archived.thread.archived, true);
  } finally {
    client?.dispose();
    child.kill();
    await exited;
  }
  results[name] = {
    packagedClient: true,
    executableSelection: selected.source,
    threadCrud: true,
  };
}

async function attempt(name, probe) {
  try {
    await probe();
  } catch (error) {
    results[name] = { passed: false, error: error.message };
    process.exitCode = 1;
    console.error(`${name}: ${error.message}`);
  }
}

try {
  assert.equal(await hash(archiveCore), await hash(sourceCore));
  assert.equal(await hash(platformCore), await hash(sourceCore));
  const signedReference = join(scratch, "signed-reference/qwenpaw-core");
  for (const app of [zipApp, dmgApp]) {
    execFileSync("codesign", ["--verify", "--deep", "--strict", app]);
    assert.equal(
      await hash(join(app, coreResource, "qwenpaw-core")),
      await hash(signedReference),
    );
    const resources = await readdir(join(app, "Contents/Resources/binaries"));
    assert.deepEqual(resources, ["qwenpaw-core"]);
  }
  const consoleFiles = await files(join(repository, "console/dist"));
  for (const directory of [
    join(scratch, "unpacked/webui/dist"),
    join(zipApp, coreResource, "console"),
    join(dmgApp, coreResource, "console"),
    join(scratch, "installed-legacy/qwenpaw/console"),
  ])
    assert.deepEqual(await files(directory), consoleFiles);
  results.console = {
    identicalFiles: Object.keys(consoleFiles).length,
    treeSha256: checksum(JSON.stringify(consoleFiles)),
  };
  const binaries = [
    ["coreArchive", archiveCore],
    ["desktopZipCore", join(zipApp, coreResource, "qwenpaw-core")],
    ["desktopDmgCore", join(dmgApp, coreResource, "qwenpaw-core")],
    ["platformVsixCore", platformCore],
  ];
  const native = {};
  for (const [name, binary] of [...binaries, ["sourceCoreControl", sourceCore]]) {
    console.log(`Checking native execution: ${name}`);
    native[name] = await probeNativeExecution(binary);
  }
  await writeFile(join(output, "native-execution.json"), `${JSON.stringify(native, null, 2)}\n`);
  results.nativeExecution = native;
  for (const [name, binary] of binaries)
    await attempt(name, async () => {
      assert.equal(native[name].passed, true, `Native startup failed; inspect native-execution.json`);
      await sdkProbe(name, binary);
    });
  await attempt("universalVsix", () =>
    native.coreArchive.passed ? extensionProbe("universalVsix", universal, archiveCore)
      : Promise.reject(new Error("Archive Core native startup failed")),
  );
  await attempt("platformVsix", () =>
    native.platformVsixCore.passed ? extensionProbe("platformVsix", platform, platformCore)
      : Promise.reject(new Error("Bundled Core native startup failed")),
  );
  // This control proves SDK behavior, not that a copied binary can be distributed.
  await attempt("sourceCoreControl", () =>
    sdkProbe("sourceCoreControl", sourceCore),
  );
  await attempt("universalClientSourceControl", () =>
    extensionProbe("universalClientSourceControl", universal, sourceCore),
  );
  await attempt("platformClientSourceControl", () =>
    extensionProbe("platformClientSourceControl", platform, sourceCore),
  );
  results.localModelRequests = modelRequests.length;
  results.sourceCoreSha256 = await hash(sourceCore);
  results.signedDesktopCoreSha256 = await hash(signedReference);
  await writeFile(
    join(output, "package-smoke.json"),
    `${JSON.stringify(results, null, 2)}\n`,
  );
  console.log(JSON.stringify(results, null, 2));
} finally {
  model.closeAllConnections();
  await new Promise((done) => model.close(done));
}
