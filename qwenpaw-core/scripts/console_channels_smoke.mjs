// Use the original sidebar, Console card and drawer. HTTP only observes results.
export async function runChannelScopeScenario(client, {
  evaluateValue, waitForValue, selectAgentFromSidebar, setInputByPlaceholder,
}) {
  const card = `[...document.querySelectorAll('[class*="channelCard"]')]
    .find(card => card.querySelector('[class*="cardTitle"]')?.innerText.trim() === 'Console')`;
  const drawer = `document.querySelector('.qwenpaw-drawer-content')`;
  const input = `${drawer}?.querySelector('input[placeholder="@bot"]')`;
  const expected = { default: "default baseline", writer: "writer baseline", editor: "editor baseline" };
  const snapshot = () => evaluateValue(client, `(async () => {
    const entries = await Promise.all(['default', 'writer', 'editor'].map(async agent => {
      const response = await fetch('/api/config/channels/console', { headers: { 'X-Agent-Id': agent } });
      if (!response.ok) throw new Error('Channel read failed: ' + response.status);
      return [agent, await response.json()];
    }));
    return Object.fromEntries(entries);
  })()`);
  const baseline = await snapshot();
  const button = label => `[...(${drawer}?.querySelectorAll('button') ?? [])]
    .find(button => button.innerText.trim() === ${JSON.stringify(label)} && !button.disabled)`;
  const open = async agent => {
    await waitForValue(client, `Boolean(${card}) && ${card}.innerText.includes(${JSON.stringify(expected[agent])})`,
      `Console card did not load ${agent}'s configuration`);
    await evaluateValue(client, `${card}.click()`);
    await waitForValue(client, `${input}?.value === ${JSON.stringify(expected[agent])}`,
      `Console drawer did not load ${agent}'s configuration`);
  };
  const close = async label => {
    await waitForValue(client, `Boolean(${button(label)})`, `Missing ${label} button`);
    await evaluateValue(client, `${button(label)}.click()`);
    await waitForValue(client, `!${input}`, 'Console drawer did not close');
  };
  for (const agent of ['writer', 'editor']) {
    await selectAgentFromSidebar(client, agent);
    await open(agent);
    const next = `${agent} saved through original drawer`;
    await setInputByPlaceholder(client, '@bot', next);
    await waitForValue(client, `${input}?.value === ${JSON.stringify(next)}`, 'Bot Prefix input was not entered');
    await close('Save');
    expected[agent] = next;
    baseline[agent].bot_prefix = next;
    const actual = await snapshot();
    if (JSON.stringify(actual) !== JSON.stringify(baseline)) {
      throw new Error(`Saving ${agent} changed another Workspace or lost configuration fields`);
    }
  }
  await client.send('Page.reload');
  await open('editor');
  await close('Cancel');
  for (const agent of ['writer', 'default']) {
    await selectAgentFromSidebar(client, agent);
    await open(agent);
    await close('Cancel');
  }
  return { savedThroughOriginalDrawer: ['writer', 'editor'],
    switchedThroughOriginalSidebar: true, defaultUnchanged: true,
    wholeConfigurationsIsolated: true, reload: true };
}
