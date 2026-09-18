import assert from "node:assert/strict";

async function browseFixture({ modal, wait, evaluate, click }) {
  const directory = process.env.QWENPAW_PROJECT_FIXTURE_BROWSE;
  assert.ok(directory);
  const breadcrumb = `${modal}.querySelector('[class*="browseBreadcrumb"]')`;
  await wait(breadcrumb, "Original directory browser did not load");
  await evaluate(`${breadcrumb}.querySelector('[role="button"]').click()`);
  await wait(`${breadcrumb}.innerText.trim() === '/'`, "Directory browser did not reach root");
  await click(modal, "Hidden Folders");
  const entry = name => `[...${modal}.querySelectorAll('[class*="browseItemName"]')].find(node => node.textContent === ${JSON.stringify(name)})`;
  const navigate = async name => {
    await wait(entry(name), `Browser directory entry missing: ${name}`);
    await evaluate(`${entry(name)}.closest('[role="button"]').click()`);
  };
  for (const part of directory.replaceAll("\\", "/").split("/").filter(Boolean)) {
    await navigate(part);
    await wait(`${modal}.querySelector('[class*="breadcrumbCurrent"]')?.textContent === ${JSON.stringify(part)} && !${modal}.querySelector('[class*="browseEmpty"]')`, "Directory navigation did not settle");
  }
  await wait(entry(".hidden-fixture"), "Hidden fixture directory missing");
  await click(modal, "Hidden Folders");
  await wait(`!${entry(".hidden-fixture")} && ${entry("visible")}`, "Hidden directory toggle did not refresh listing");
  await navigate("visible");
  await wait(`${modal}.querySelector('[class*="breadcrumbCurrent"]')?.textContent === 'visible' && ${entry("..")}`, "Child directory did not open");
  await navigate("..");
  await wait(`${modal}.querySelector('[class*="breadcrumbCurrent"]')?.textContent === 'browse-parent' && ${entry("visible")}`, "Parent directory navigation failed");
  await click(modal, "Refresh");
  await wait(entry("visible"), "Directory refresh did not finish");
  await click(modal, "Open This Directory");
}

export async function runProjectOwnershipScenario(client, {
  evaluateValue, waitForValue, selectAgentFromSidebar, setInputByPlaceholder,
}) {
  const evaluate = expression => evaluateValue(client, expression);
  const wait = (expression, message) => waitForValue(client, `Boolean(${expression})`, message);
  const modal = `document.querySelector('.qwenpaw-modal-wrap:not([style*="display: none"]) .qwenpaw-modal-content')`;
  const setting = `document.querySelector('[class*="projectDirectorySetting"]')`;
  const click = async (scope, label) => {
    const button = `[...(${scope}?.querySelectorAll('button') ?? [])].find(button => button.offsetHeight && !button.disabled && button.innerText.trim() === ${JSON.stringify(label)})`;
    await wait(button, `Missing ${label} control`);
    await evaluate(`${button}.click()`);
  };
  const tab = async label => {
    const item = `[...${modal}.querySelectorAll('[role="tab"]')].find(tab => tab.innerText.trim() === ${JSON.stringify(label)})`;
    await wait(item, `Missing ${label} tab`);
    await evaluate(`${item}.click()`);
  };
  const read = actor => `fetch('/api/workspace/project-directory', {headers:{'X-Agent-Id':${JSON.stringify(actor)}}}).then(response => { if (!response.ok) throw new Error('Project observation failed'); return response.json(); })`;
  const defaultBefore = await evaluate(read("default"));
  await selectAgentFromSidebar(client, "writer");
  await wait(`${setting}?.querySelector('strong')?.textContent === 'writer'`, "Writer project setting did not render");
  const open = async () => {
    await click("document", "Change directory");
    await wait(`${modal}?.innerText.includes('Select Project Directory')`, "Original project modal missing");
    await wait(`${modal}.querySelector('[class*="recentName"]')`, "Writer recent projects did not load");
  };
  const selected = async name => {
    await wait(`!${modal} && ${setting}?.querySelector('strong')?.textContent === ${JSON.stringify(name)}`, `Project setting did not select ${name}`);
    const value = await evaluate(read("writer"));
    assert.equal(value.name, name);
    return value;
  };
  await open();
  assert.deepEqual(await evaluate(`[...${modal}.querySelectorAll('[class*="recentName"]')].map(node => node.textContent)`), ["writer-only"]);
  await tab("New Project");
  await setInputByPlaceholder(client, "my-project", "shared");
  await click(modal, "Create Project");
  const created = await selected("shared");
  assert.notEqual(created.path, defaultBefore.path);
  await open();
  const recent = `[...${modal}.querySelectorAll('.qwenpaw-list-item')].find(item => item.querySelector('[class*="recentName"]')?.textContent === 'writer-only')`;
  await wait(recent, "Writer recent item missing");
  await evaluate(`${recent}.click()`);
  await selected("writer-only");
  await open();
  await tab("Default Workspace");
  await click(modal, "Confirm");
  assert.equal((await selected("writer")).is_workspace_default, true);

  await open();
  await tab("Open Directory");
  await browseFixture({ modal, wait, evaluate, click });
  assert.equal((await selected("browse-parent")).path, process.env.QWENPAW_PROJECT_FIXTURE_BROWSE);

  await open();
  await tab("Clone Repository");
  assert.ok(process.env.QWENPAW_PROJECT_FIXTURE_SOURCE);
  await setInputByPlaceholder(client, "https://github.com/user/repo.git", process.env.QWENPAW_PROJECT_FIXTURE_SOURCE);
  await setInputByPlaceholder(client, "Leave empty to infer from URL", "browser-cloned");
  await click(modal, "Start Clone");
  await selected("browser-cloned");

  await open();
  await tab("Import Local Project");
  await wait(`${modal}.querySelector('input[webkitdirectory]')`, "Original folder input missing");
  const { root } = await client.send("DOM.getDocument");
  const { nodeId } = await client.send("DOM.querySelector", {
    nodeId: root.nodeId,
    selector: '.qwenpaw-modal-wrap:not([style*="display: none"]) input[webkitdirectory]',
  });
  assert.ok(nodeId);
  assert.ok(process.env.QWENPAW_PROJECT_FIXTURE_UPLOAD);
  await client.send("DOM.setFileInputFiles", { nodeId, files: [process.env.QWENPAW_PROJECT_FIXTURE_UPLOAD] });
  await wait(`${modal}.querySelector('[class*="selectionName"]')?.textContent === 'browser-upload'`, "Original folder selection did not read files");
  await click(modal, "Import");
  await selected("browser-upload");
  const beforeReload = await evaluate("performance.timeOrigin");
  await client.send("Page.reload");
  await wait(`performance.timeOrigin !== ${beforeReload} && ${setting}?.querySelector('strong')?.textContent === 'browser-upload'`, "Writer selection did not survive reload");
  await selectAgentFromSidebar(client, "default");
  await wait(`${setting}?.querySelector('strong')?.textContent === 'shared'`, "Default project changed with Writer");
  assert.deepEqual(await evaluate(read("default")), defaultBefore);
  return { agentSwitch: true, create: true, recent: true, reset: true, clone: true, zip: true, reload: true, defaultUnchanged: true, openDirectory: true };
}
