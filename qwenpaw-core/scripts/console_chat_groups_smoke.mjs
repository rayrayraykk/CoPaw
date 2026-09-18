// Group mutations are performed only through the original Console controls.
export async function runChatGroupsScenario(client, { evaluateValue, waitForValue, clickButton }) {
  const evaluate = expression => evaluateValue(client, expression);
  const wait = (expression, message) => waitForValue(client, expression, message);
  // Isolated browser-profile preferences open the existing history surface.
  await evaluate(`localStorage.setItem('qwenpaw_sidebar_mode', 'full');
    localStorage.setItem('qwenpaw_history_panel_open', 'true')`);
  await client.send("Page.reload");
  await wait(`Boolean(document.querySelector('button[class*="createGroupButton"]'))`, "Original New group control missing");
  await evaluate(`document.querySelector('button[class*="createGroupButton"]').click()`);
  const fill = async (selector, text) => {
    await wait(`Boolean(document.querySelector(${JSON.stringify(selector)}))`, `Missing input ${selector}`);
    await evaluate(`(() => { const input = document.querySelector(${JSON.stringify(selector)}); input.focus(); input.select(); })()`);
    await client.send("Input.insertText", { text });
    await wait(`document.querySelector(${JSON.stringify(selector)})?.value === ${JSON.stringify(text)}`, "Group input did not update");
    await client.send("Input.dispatchKeyEvent", { type: "keyDown", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
    await client.send("Input.dispatchKeyEvent", { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
  };
  await fill('input[placeholder="Group name"]', "Browser group");
  const groups = `fetch('/api/chats/groups').then(response => { if (!response.ok) throw new Error('Group read failed'); return response.json(); })`;
  await wait(`${groups}.then(groups => groups.some(group => group.name === 'Browser group'))`, "Group creation did not persist");
  const id = await evaluate(`${groups}.then(groups => groups.find(group => group.name === 'Browser group').id)`);
  const header = name => `[...document.querySelectorAll('[role="button"]')].find(el =>
    el.querySelector('button[aria-label="Manage group"]') && [...el.querySelectorAll('span')].some(span => span.textContent === ${JSON.stringify(name)}))`;
  const menu = async (name, action) => {
    await wait(`Boolean(${header(name)})`, `Missing group header ${name}`);
    await evaluate(`(${header(name)}).querySelector('button[aria-label="Manage group"]').click()`);
    const item = `[...document.querySelectorAll('[role="menuitem"]')].find(el => el.offsetHeight && el.innerText.trim() === ${JSON.stringify(action)})`;
    await wait(`Boolean(${item})`, `Missing group action ${action}`);
    await evaluate(`(${item}).click()`);
  };
  await menu("Browser group", "Rename");
  await fill('input[class*="renameInput"]', "Renamed group");
  await wait(`${groups}.then(groups => groups.some(group => group.id === ${JSON.stringify(id)} && group.name === 'Renamed group'))`, "Group rename did not persist");
  await menu("Renamed group", "Pin group");
  await wait(`${groups}.then(groups => groups.some(group => group.id === ${JSON.stringify(id)} && group.pinned))`, "Group pin did not persist");
  await client.send("Page.reload");
  await wait(`Boolean((${header("Renamed group")})?.querySelector('[title="Pinned group"]'))`, "Pinned group did not survive reload");
  await menu("Renamed group", "Delete group");
  await wait(`Boolean(document.querySelector('[class*="-modal-confirm"]'))`, "Original group deletion confirmation missing");
  await clickButton(client, "OK");
  await wait(`${groups}.then(groups => !groups.some(group => group.id === ${JSON.stringify(id)}))`, "Group deletion did not persist");
  await wait(`!Boolean(${header("Renamed group")})`, "Deleted group remained in the original page");
  return { created: true, renamed: true, pinned: true, reload: true, deleted: true };
}
