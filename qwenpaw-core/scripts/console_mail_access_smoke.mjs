// Drive the original mail drawer. HTTP calls only observe the resulting state.
export async function runMailAccessScenario(client, { evaluateValue, waitForValue }) {
  const drawer = "document.querySelector('.qwenpaw-drawer-content')";
  const table = index => `${drawer}?.querySelectorAll('.qwenpaw-table-tbody')[${index}]`;
  const row = (index, key) => `${table(index)}?.querySelector(${JSON.stringify(`[data-row-key="${key}"]`)})`;
  const click = async (scope, label) => {
    const expression = `[...(${scope}?.querySelectorAll('button') ?? [])].find(button =>
      button.innerText.trim() === ${JSON.stringify(label)} && !button.disabled)`;
    await waitForValue(client, `Boolean(${expression})`, `Missing enabled ${label} button`);
    await evaluateValue(client, `${expression}.click()`);
  };
  const open = async () => {
    await click("document", "Mail Access Control");
    await waitForValue(client, `${drawer}?.innerText.includes('Pending Senders')`, "Mail drawer missing");
  };
  const observe = async predicate => {
    await waitForValue(client, `(async () => {
      const response = await fetch('/api/mail-access-control');
      if (!response.ok) return false;
      const data = await response.json();
      return ${predicate};
    })()`, `Mail state did not match: ${predicate}`);
  };
  await open();
  console.error("Mail drawer: opened");
  await waitForValue(client, `${table(0)}?.querySelectorAll('[data-row-key]').length === 3`,
    "Expected only the three mail-enabled pending rows");
  const initial = await evaluateValue(client, `(async () => {
    const response = await fetch('/api/mail-access-control/agents');
    if (!response.ok) throw new Error('Cannot inspect mail Agents');
    return response.json();
  })()`);
  if (JSON.stringify(initial) !== JSON.stringify({ agents: ["writer", "reader"] })) {
    throw new Error(`Unexpected mail Agent choices: ${JSON.stringify(initial)}`);
  }
  for (const [key, label] of [
    ["writer:same@example.com", "Approve"],
    ["reader:same@example.com", "Block"],
    ["writer:dismiss@example.com", "Dismiss"],
  ]) {
    console.error(`Mail drawer: ${label} ${key}`);
    await click(row(0, key), label);
    await waitForValue(client, `!${row(0, key)}`, `${label} row did not disappear`);
  }
  await observe(`data.writer.pending.length === 0 && data.reader.pending.length === 0
    && data.writer.whitelist['same@example.com'] && data.reader.blacklist['same@example.com']
    && data.writer.approved_replay.length === 1 && !data.off`);

  await click(drawer, "Add Sender");
  console.error("Mail drawer: add sender modal");
  const modal = "document.querySelector('.qwenpaw-modal-content')";
  const type = async (placeholder, value) => {
    const input = `${modal}?.querySelector(${JSON.stringify(`input[placeholder="${placeholder}"]`)})`;
    await waitForValue(client, `Boolean(${input})`, `Input missing: ${placeholder}`);
    await evaluateValue(client, `${input}.focus()`);
    await client.send("Input.insertText", { text: value });
    await waitForValue(client, `${input}?.value === ${JSON.stringify(value)}`, `Input not entered: ${placeholder}`);
  };
  await type("user@domain.com / *@domain.com", "*@example.org");
  await type("Sender Name", "Team");
  await type("Remark", "Shared contact");
  await click(modal, "OK");
  console.error("Mail drawer: broadcast submitted");
  await observe(`data.writer.whitelist['*@example.org']?.remark === 'Shared contact'
    && data.reader.whitelist['*@example.org']?.display_name === 'Team'`);
  await waitForValue(client, `${table(1)}?.querySelectorAll('[data-row-key]').length === 3`,
    "Broadcast whitelist did not render for both Agents");

  const selector = `${drawer}?.querySelectorAll('.qwenpaw-select')[1]`;
  console.error("Mail drawer: filter writer");
  await evaluateValue(client, `${selector}.querySelector('input').focus()`);
  await client.send("Input.dispatchKeyEvent", { type: "keyDown", key: "ArrowDown", code: "ArrowDown", windowsVirtualKeyCode: 40 });
  await client.send("Input.dispatchKeyEvent", { type: "keyUp", key: "ArrowDown", code: "ArrowDown", windowsVirtualKeyCode: 40 });
  const writerOption = `[...document.querySelectorAll('.qwenpaw-select-dropdown:not(.qwenpaw-select-dropdown-hidden) .qwenpaw-select-item-option')]
    .find(option => option.innerText.trim() === 'writer')`;
  await waitForValue(client, `Boolean(${writerOption})`, "Writer filter option missing");
  await evaluateValue(client, `${writerOption}.click()`);
  await waitForValue(client, `${table(1)}?.querySelectorAll('[data-row-key]').length === 2
    && !${row(1, "reader:*@example.org")}`, "Agent list filter did not isolate writer");
  await evaluateValue(client, `${row(1, "writer:*@example.org")}.querySelector('button.qwenpaw-btn-dangerous').click()`);
  await click("document.querySelector('.qwenpaw-popconfirm')", "OK");
  console.error("Mail drawer: remove writer submitted");
  await observe(`!data.writer.whitelist['*@example.org'] && Boolean(data.reader.whitelist['*@example.org'])`);
  await client.send("Page.reload");
  console.error("Mail drawer: reload");
  await open();
  await waitForValue(client, `${table(1)}?.querySelectorAll('[data-row-key]').length === 2
    && Boolean(${row(1, "reader:*@example.org")}) && Boolean(${row(1, "writer:same@example.com")})`,
    "Whitelist did not survive reload");
  const blacklistTab = `[...(${drawer}?.querySelectorAll('[role="tab"]') ?? [])]
    .find(tab => tab.innerText.trim() === 'Blacklist')`;
  await evaluateValue(client, `${blacklistTab}.click()`);
  await waitForValue(client, `${table(1)}?.querySelectorAll('[data-row-key]').length === 1
    && Boolean(${row(1, "reader:same@example.com")})`, "Blacklist did not survive reload");
  return { approved: true, blocked: true, dismissed: true, broadcast: true,
    filtered: true, removedOnlyWriter: true, reload: true };
}
