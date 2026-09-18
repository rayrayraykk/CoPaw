// Mutate checkpoints only through the original controls; HTTP observes state.
export async function runCheckpointsScenario(client, {
  evaluateValue, waitForValue, selectAgentFromSidebar, runningRestore = false,
}) {
  const evaluate = expression => evaluateValue(client, expression);
  const wait = (expression, message) => waitForValue(client, expression, message);
  const modal = "document.querySelector('.qwenpaw-modal-wrap:not([style*=\"display: none\"]) .qwenpaw-modal-content')";
  const click = async (scope, label) => {
    const button = `[...(${scope}?.querySelectorAll('button') ?? [])].find(button =>
      button.offsetHeight && !button.disabled && button.innerText.trim() === ${JSON.stringify(label)})`;
    await wait(`Boolean(${button})`, `Missing ${label} control`);
    await evaluate(`${button}.click()`);
  };
  const read = (actor, suffix) => `fetch('/api/workspace/checkpoints${suffix}',
    {headers:{'X-Agent-Id':${JSON.stringify(actor)}}}).then(response => {
      if (!response.ok) throw new Error('Checkpoint observation failed');
      return response.json();
    })`;
  const toggle = "document.querySelector('[class*=\"autoControl\"] [role=\"switch\"]')";
  const select = async actor => {
    await selectAgentFromSidebar(client, actor);
    const suffix = actor === "writer" ? "/data/workspaces/writer" : "/workspace";
    await wait(`document.querySelector('[class*="workspacePath"]')?.textContent.replaceAll('\\\\','/').endsWith(${JSON.stringify(suffix)})
      && ${toggle} && !${toggle}.disabled`, `Base Workspace did not switch to ${actor}`);
  };
  const menu = async label => {
    await evaluate(`document.querySelector('button[aria-label="More actions"]').click()`);
    const item = `[...document.querySelectorAll('[role="menuitem"]')].find(item =>
      item.offsetHeight && item.innerText.trim() === ${JSON.stringify(label)})`;
    await wait(`Boolean(${item})`, `Missing ${label} menu item`);
    await evaluate(`${item}.click()`);
  };
  await select("writer");
  await wait(`${toggle}.getAttribute('aria-checked') === 'false'`, "Writer auto must start disabled");
  await evaluate(`${toggle}.click()`);
  await wait(`Promise.all([${read("writer", "/status")}, ${read("default", "/status")}])
    .then(([writer, initial]) => writer.auto_enabled && !initial.auto_enabled)`, "Auto setting crossed Agent scopes");
  await click("document", "Snapshot");
  const input = `${modal}?.querySelector('input[placeholder="Optional snapshot name"]')`;
  await wait(`Boolean(${input})`, "Snapshot name input missing");
  await evaluate(`${input}.focus()`);
  await client.send("Input.insertText", { text: "Writer checkpoint" });
  await wait(`${input}.value === 'Writer checkpoint'`, "Snapshot name did not update");
  await click(modal, "Snapshot");
  await wait(`${read("writer", "/graph")}.then(graph => graph.nodes.length === 2
    && graph.nodes.some(node => node.name === 'Writer checkpoint'))`, "Writer snapshot missing");
  await wait(`!${modal}`, "Snapshot modal did not close");
  await menu("Automatic cleanup settings");
  const count = `${modal}?.querySelector('#gc_keep_count')`;
  await wait(`${count}?.value === '20'`, "Initial cleanup count missing");
  await evaluate(`${count}.focus(); ${count}.select()`);
  await client.send("Input.insertText", { text: "3" });
  await click(modal, "Save");
  await wait(`${read("writer", "/gc/settings")}.then(settings => settings.gc_keep_count === 3)`, "Cleanup setting did not persist");
  await wait(`!${modal}`, "Cleanup settings modal did not close");
  await select("default");
  await wait(`${toggle}.getAttribute('aria-checked') === 'false'
    && document.body.innerText.includes('Default baseline')
    && !document.body.innerText.includes('Writer checkpoint')`, "Default graph was changed by writer");
  await menu("Automatic cleanup settings");
  await wait(`${count}?.value === '20'`, "Default cleanup count was changed by writer");
  await click(modal, "Cancel");
  await select("writer");
  await client.send("Page.reload");
  await wait(`${toggle}?.getAttribute('aria-checked') === 'true'
    && document.body.innerText.includes('Writer checkpoint')`, "Writer state did not survive reload");
  await menu("Automatic cleanup settings");
  await wait(`${count}?.value === '3'`, "Writer cleanup count did not survive reload");
  await click(modal, "Cancel");
  const baseline = `document.querySelector('button[aria-label="Writer baseline, Snapshot"]')`;
  await wait(`Boolean(${baseline})`, "Writer baseline row missing");
  await evaluate(`${baseline}.click()`);
  await click("document", "Restore");
  // Resolve the original checkbox without changing React state directly.
  const fileScope = `([...(${modal}?.querySelectorAll('label') ?? [])].find(label => label.innerText.trim() === 'Workspace files'))?.querySelector('input')`;
  await wait(`Boolean(${fileScope})`, "Restore file scope missing");
  await evaluate(`${fileScope}.click()`);
  await click(modal, "Preview restore");
  const selection = `${modal}?.querySelector('[class*="fileSelection"]')`;
  const notes = `([...(${selection}?.querySelectorAll('label') ?? [])].find(label => label.querySelector('code')?.textContent === 'notes.txt'))?.querySelector('input')`;
  const confirm = `[...(${modal}?.querySelectorAll('button') ?? [])].find(button => button.innerText.trim() === 'Restore checkpoint')`;
  await wait(`Boolean(${notes}) && ${confirm}?.disabled`, "Preview must require a selected file");
  await wait(`${selection}?.innerText.includes('unselected.txt')`, "Preview omitted an unselected changed file");
  await wait(`${read("writer", "/graph")}.then(graph => graph.summary.safety === 0)`, "Preview created a safety checkpoint");
  await evaluate(`${notes}.click()`);
  await click(modal, "Restore checkpoint");
  if (runningRestore) {
    const back = `[...(${modal}?.querySelectorAll('button') ?? [])].find(button => button.innerText.trim() === 'Back')`;
    await wait(`${confirm}?.className.includes('-btn-loading') && ${back}?.disabled
      && ${notes}?.checked`, "Restore must stay loading with its file selection while a run is active");
    // Release only the isolated model fixture, after observing the real UI.
    process.stderr.write("QWENPAW_CHECKPOINT_RESTORE_WAITING\n");
  }
  await wait(`!${modal}`, "Restore modal did not close");
  await wait(`${read("writer", "/graph")}.then(graph => graph.summary.total === 3
    && graph.summary.safety === 1 && graph.nodes.some(node => node.name === 'Writer baseline' && node.is_head))`, "Restore did not retain safety and move HEAD");
  const drawerClose = `document.querySelector('.qwenpaw-drawer-open button[aria-label="Close"]')`;
  await wait(`Boolean(${drawerClose})`, "Checkpoint details drawer did not remain available");
  await evaluate(`${drawerClose}.click()`);
  await wait(`!document.querySelector('.qwenpaw-drawer-open')`, "Checkpoint details drawer did not close");
  await menu("Clean up checkpoints");
  await click(modal, "Clean up");
  await wait(`!${modal}`, "Cleanup confirmation did not close");
  await wait(`${read("writer", "/graph")}.then(graph => graph.nodes.length === 3
    && graph.nodes.some(node => node.name === 'Writer baseline' && node.is_head))`, "Cleanup removed the session HEAD or retained history");
  await menu("Reset checkpoint data");
  await click(modal, "Reset");
  await wait(`${read("writer", "/status")}.then(state => !state.auto_enabled && !state.has_checkpoints
    && ${toggle}?.getAttribute('aria-checked') === 'false')`, "Writer reset did not update the page");
  await wait(`${read("default", "/graph")}.then(graph => graph.nodes.length === 1
    && graph.nodes[0].name === 'Default baseline')`, "Writer reset changed the default graph");
  return { autoIsolated: true, snapshot: true, gcSettings: true, switched: true,
    reload: true, restorePreview: true, selectiveRestore: true, gc: true, resetOnlyWriter: true,
    ...(runningRestore ? { waitingForRun: true } : {}) };
}
