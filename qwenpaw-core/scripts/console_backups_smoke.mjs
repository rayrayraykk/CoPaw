import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import path from "node:path";

async function fillReadyBackupName(client, helpers, name) {
  const { waitForValue, setInputByPlaceholder } = helpers;
  const input = 'document.querySelector(\'input[placeholder="Enter backup name"]\')';
  // CreateBackupModal initializes name and resets its runner in afterOpenChange,
  // not when the input first mounts. Wait for that visible initialization before
  // editing, otherwise the opening animation can reset a submitted request.
  await waitForValue(client,
    `/^Backup \\d{4}-\\d{2}-\\d{2} \\d{2}:\\d{2}$/.test(${input}?.value ?? '')`,
    "Original Backup form did not finish initializing");
  await setInputByPlaceholder(client, "Enter backup name", name);
  await waitForValue(client,
    `${input}?.value === ${JSON.stringify(name)} && [...document.querySelectorAll('[role="dialog"] button')].some(button => button.innerText.trim() === 'Confirm' && !button.disabled)`,
    "Original Backup name or enabled Confirm button did not settle");
}

// The Rust fixture holds the checkpoint lock until the first job is cancelled.
// This makes a genuinely active job survive reload without oversized archives,
// frontend mocks, or timing assumptions about ZIP compression speed.
export async function runBackupsJobsScenario(client, helpers) {
  const { evaluateValue, waitForValue, clickButton } =
    helpers;
  const evaluate = (expression) => evaluateValue(client, expression);
  const wait = (expression, message) =>
    waitForValue(client, expression, message);
  const requests = [];
  const failures = [];
  client.on("Network.requestWillBeSent", ({ request }) => {
    requests.push({
      path: new URL(request.url).pathname,
      method: request.method,
    });
  });
  client.on("Network.responseReceived", ({ response }) => {
    if (
      new URL(response.url).pathname.startsWith("/api/") &&
      response.status >= 400
    )
      failures.push({
        path: new URL(response.url).pathname,
        status: response.status,
      });
  });
  const create = async (name) => {
    await wait(
      'document.body.innerText.includes("Create Backup")',
      "Original Backups page did not render",
    );
    await clickButton(client, "Create Backup");
    await wait(
      "Boolean(document.querySelector('input[placeholder=\"Enter backup name\"]'))",
      "Original creation form did not render",
    );
    await fillReadyBackupName(client, helpers, name);
    await clickButton(client, "Confirm");
  };
  await create("Browser Cancelled Backup");
  await wait(
    'fetch("/api/backups/jobs/active").then(r => r.json())',
    "Created job did not become active",
  );
  const active = await evaluate(
    'fetch("/api/backups/jobs/active").then(r => r.json())',
  );
  const eventsPath = `/api/backups/jobs/${active.job_id}/events`;
  await wait(
    'Boolean(document.querySelector("[role=dialog] [role=progressbar]"))',
    "Original progress view did not render",
  );
  const timeOrigin = await evaluate("performance.timeOrigin");
  await client.send("Page.reload", { ignoreCache: true });
  await wait(
    `performance.timeOrigin !== ${JSON.stringify(
      timeOrigin,
    )} && Boolean(document.querySelector("[role=dialog] [role=progressbar]"))`,
    "Reload did not resume the active progress modal",
  );
  const resumed = await evaluate(
    'fetch("/api/backups/jobs/active").then(r => r.json())',
  );
  assert.equal(resumed.job_id, active.job_id);
  assert.equal(resumed.backup_id, active.backup_id);
  assert.ok(["pending", "running"].includes(resumed.status));
  assert.ok(
    requests.filter((item) => item.path === eventsPath).length >= 2,
    "Reload must reconnect the real SSE observer to the same active job",
  );
  assert.equal(
    requests.filter(
      (item) => item.path === "/api/backups/jobs" && item.method === "POST",
    ).length,
    1,
    "Reload must not create a second job",
  );
  await clickButton(client, "Cancel");
  await wait(
    '!document.querySelector("[role=dialog]")',
    "Original Cancel did not close the progress modal",
  );
  await wait(
    `fetch(${JSON.stringify(
      `/api/backups/jobs/${active.job_id}`,
    )}).then(r => r.json()).then(job => job.status === "cancelled" && job)`,
    "Explicit UI cancellation did not reach a terminal job state",
  );
  const cancelled = await evaluate(
    `fetch(${JSON.stringify(
      `/api/backups/jobs/${active.job_id}`,
    )}).then(r => r.json())`,
  );
  assert.equal(cancelled.result, null);
  assert.deepEqual(
    await evaluate('fetch("/api/backups").then(r => r.json())'),
    [],
  );
  assert.equal(
    await evaluate('fetch("/api/backups/jobs/active").then(r => r.json())'),
    null,
  );
  await create("Browser After Cancellation");
  await wait(
    'Boolean(document.querySelector("tr[data-row-key]")) && document.body.innerText.includes("Browser After Cancellation")',
    "A subsequent creation did not complete after cancellation",
  );
  const archives = await evaluate('fetch("/api/backups").then(r => r.json())');
  assert.equal(archives.length, 1);
  assert.equal(archives[0].name, "Browser After Cancellation");
  assert.notEqual(archives[0].id, active.backup_id);
  assert.deepEqual(failures, []);
  return {
    activeReload: true,
    sseReconnected: true,
    cancelled: true,
    noPartialArchive: true,
    subsequentCreation: true,
  };
}

// Run only against the isolated Rust browser fixture. All mutations below use
// the unchanged Console controls, except fixture setup and read-only assertions.
export async function runBackupsCrudScenario(client, helpers) {
  const {
    evaluateValue,
    waitForValue,
    clickButton,
    setInputByPlaceholder,
    downloadDirectory,
  } = helpers;
  const evaluate = (expression) => evaluateValue(client, expression);
  const wait = (expression, message) =>
    waitForValue(client, expression, message);
  const click = async (text) => {
    await wait(
      `[...document.querySelectorAll("button")].some(button =>
        button.getClientRects().length && !button.disabled &&
        button.innerText.trim() === ${JSON.stringify(text)})`,
      `Original button is not ready: ${text}`,
    );
    await clickButton(client, text);
  };
  const responses = [];
  const rejected = new Set();
  const restoreErrors = [];
  client.on("Network.responseReceived", ({ response, requestId }) => {
    const pathname = new URL(response.url).pathname;
    if (pathname.startsWith("/api/"))
      responses.push({ path: pathname, status: response.status });
    if (pathname.endsWith("/restore") && response.status >= 400)
      rejected.add(requestId);
  });
  client.on("Network.loadingFinished", ({ requestId }) => {
    if (rejected.has(requestId))
      client
        .send("Network.getResponseBody", { requestId })
        .then((result) => restoreErrors.push(result.body))
        .catch((error) => restoreErrors.push(error.message));
  });
  const pickArchive = async (archive) => {
    await wait(
      `Boolean(document.querySelector('input[type="file"]'))`,
      "Original Import control did not finish rendering",
    );
    const document = await client.send("DOM.getDocument");
    const input = await client.send("DOM.querySelector", {
      nodeId: document.root.nodeId,
      selector: 'input[type="file"]',
    });
    assert.ok(input.nodeId, "Original Import file input must exist");
    await client.send("DOM.setFileInputFiles", {
      nodeId: input.nodeId,
      files: [archive],
    });
  };
  await wait(
    'document.body.innerText.includes("Create Backup")',
    "Backups page did not render",
  );
  const before = await evaluate('fetch("/api/backups").then(r => r.json())');
  assert.deepEqual(before, [], "Browser backup fixture must start empty");
  const name = "Browser Backup Roundtrip";
  await click("Create Backup");
  await wait(
    `Boolean(document.querySelector('input[placeholder="Enter backup name"]'))`,
    "Backup name form did not render",
  );
  await fillReadyBackupName(client, helpers, name);
  await click("Confirm");
  await wait(
    `Boolean(document.querySelector('tr[data-row-key]')) && document.body.innerText.includes(${JSON.stringify(
      name,
    )})`,
    "Created backup did not appear in original table",
  );
  const backups = await evaluate('fetch("/api/backups").then(r => r.json())');
  assert.equal(backups.length, 1);
  const id = backups[0].id;
  assert.equal(backups[0].name, name);
  await client.send("Page.reload", { ignoreCache: true });
  await wait(
    `document.body.innerText.includes(${JSON.stringify(name)})`,
    "Backup did not survive page reload",
  );

  await client.send("Page.setDownloadBehavior", {
    behavior: "allow",
    downloadPath: downloadDirectory,
  });
  await click("Export");
  await wait(
    'document.body.innerText.includes("Sensitive Information Warning")',
    "Export warning did not render",
  );
  await click("Confirm Export");
  let downloaded;
  const deadline = Date.now() + 10_000;
  while (Date.now() < deadline) {
    downloaded = (await readdir(downloadDirectory)).find((name) =>
      name.endsWith(".zip"),
    );
    if (downloaded) break;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  assert.ok(downloaded, "Original Export action must download a ZIP");
  const archive = path.join(downloadDirectory, downloaded);
  const bytes = await readFile(archive);
  assert.equal(bytes.subarray(0, 4).toString("hex"), "504b0304");

  await pickArchive(archive);
  await wait(
    'document.body.innerText.includes("Overwrite")',
    "Import conflict confirmation did not render",
  );
  await click("Overwrite");
  await wait(
    '!document.querySelector("[role=dialog]")',
    "Import conflict dialog did not close",
  );

  // The fixture owns this workspace file. Changing it before restoration makes
  // a successful HTTP response insufficient to pass the roundtrip assertion.
  const changed = await evaluate(`fetch("/api/workspace/files/notes.md", {
    method: "PUT", headers: {"Content-Type": "application/json"},
    body: JSON.stringify({content: "browser changed content"})
  }).then(r => ({ok: r.ok, status: r.status}))`);
  assert.equal(changed.ok, true, JSON.stringify(changed));
  await click("Restore");
  await wait(
    'document.body.innerText.includes("Create Pre-Restore Backup")',
    "Pre-restore choice did not render",
  );
  await click("Yes, create backup first");
  await wait(
    'document.body.innerText.includes("I confirm that I want to restore this backup")',
    "Restore modal did not open after automatic backup",
  );
  await wait(
    `fetch("/api/backups").then(r => r.json()).then(items => items.length === 2)`,
    "Automatic pre-restore backup was not persisted",
  );
  const confirmed = await evaluate(`(() => {
    const label = [...document.querySelectorAll("label")].find(item => item.innerText.includes("I confirm that I want to restore this backup"));
    label?.querySelector('input[type="checkbox"]')?.click();
    return Boolean(label);
  })()`);
  assert.ok(confirmed);
  await click("Confirm");
  await wait(
    '!document.querySelector("[role=dialog]")',
    "Restore did not finish through the original modal",
  );
  const restored = await evaluate(
    'fetch("/api/workspace/files/notes.md").then(r => r.json())',
  );
  assert.equal(restored.content, "original workspace content");

  await setInputByPlaceholder(client, "Search backups by name or ID...", name);
  await wait(
    'document.querySelectorAll("tr[data-row-key]").length === 1',
    "Backup name search did not isolate the restored archive",
  );
  await click("Delete");
  await wait(
    'document.body.innerText.includes("Are you sure you want to delete this backup?")',
    "Delete confirmation did not render",
  );
  await click("OK");
  await wait(
    `fetch("/api/backups").then(r => r.json()).then(items => items.length === 1 && items.every(item => item.id !== ${JSON.stringify(
      id,
    )}))`,
    "Original delete action did not remove only the selected backup",
  );
  const foreignArchive = process.env.QWENPAW_BROWSER_FOREIGN_ARCHIVE;
  assert.ok(
    foreignArchive,
    "Foreign archive must be supplied by the isolated Rust fixture",
  );
  await pickArchive(foreignArchive);
  await wait(
    'document.body.innerText.includes("Trust this backup?")',
    "Foreign archive was not gated by original trust dialog",
  );
  const untrusted = await evaluate('fetch("/api/backups").then(r => r.json())');
  assert.equal(
    untrusted.length,
    1,
    "Unconfirmed foreign archive must not be published",
  );
  await click("Confirm");
  await wait(
    'fetch("/api/backups").then(r => r.json()).then(items => items.some(item => item.name === "Browser Foreign Backup" && item.accepted_via_trust === true))',
    "Trust acceptance was not persisted",
  );
  await setInputByPlaceholder(
    client,
    "Search backups by name or ID...",
    "Browser Foreign Backup",
  );
  await wait(
    'document.querySelectorAll("tr[data-row-key]").length === 1',
    "Imported archive did not render in search",
  );
  await click("Restore");
  await wait(
    'document.body.innerText.includes("No, restore directly")',
    "Direct restore choice did not render",
  );
  await click("No, restore directly");
  await wait(
    'document.body.innerText.includes("Imported backup - local security and MCP are preserved by default")',
    "Imported restore did not render its local protection default",
  );
  const protectedDefault = await evaluate(`(() => {
    const label = [...document.querySelectorAll("label")].find(item => item.innerText.includes("Preserve local security and MCP"));
    return label?.querySelector('input[type="radio"]')?.checked;
  })()`);
  assert.equal(protectedDefault, true);
  await evaluate(`(() => {
    const label = [...document.querySelectorAll("label")].find(item => item.innerText.includes("I confirm that I want to restore this backup"));
    label.querySelector('input[type="checkbox"]').click();
  })()`);
  await click("Confirm");
  try {
    await wait(
      '!document.querySelector("[role=dialog]")',
      "Foreign restore did not complete",
    );
  } catch (error) {
    throw new Error(`${error.message}: ${JSON.stringify(restoreErrors)}`);
  }
  const foreignRestored = await evaluate(
    'fetch("/api/workspace/files/notes.md").then(r => r.json())',
  );
  assert.equal(foreignRestored.content, "foreign workspace content");
  const imports = responses
    .filter((item) => item.path === "/api/backups/import")
    .map((item) => item.status);
  assert.deepEqual(imports, [409, 200, 400, 200]);
  const unexpected = responses.filter(
    (item) => item.status >= 400 && item.path !== "/api/backups/import",
  );
  assert.deepEqual(
    unexpected,
    [],
    "No other failing API request may be hidden by a later success",
  );
  return {
    created: true,
    reload: true,
    exportedZipBytes: bytes.length,
    importConflict: true,
    preRestoreBackup: true,
    restoredFile: true,
    deletedSelected: true,
    foreignTrust: true,
    directRestore: true,
    protectedDefault: true,
    importStatuses: imports,
  };
}
