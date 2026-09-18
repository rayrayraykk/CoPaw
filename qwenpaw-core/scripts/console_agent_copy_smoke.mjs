export async function runAgentJobsCopyScenario(client, {
  evaluateValue, waitForValue, clickAgentRowAction,
  clickButton, setInputByPlaceholder,
}) {
  const ids = [];
  for (const copyJobs of [false, true]) {
    await waitForValue(client,
      `Boolean(document.querySelector('tr[data-row-key="default"]'))`,
      "Default Agent row missing");
    await clickAgentRowAction(client, "default", 2);
    await waitForValue(client,
      `document.querySelector('[role="dialog"] input[placeholder="e.g.: My Agent"]')?.value.endsWith(' Copy')`,
      "Copy modal did not initialize");
    const name = copyJobs ? "Browser with jobs" : "Browser without jobs";
    await setInputByPlaceholder(client, "e.g.: My Agent", name);
    const options = await evaluateValue(client, `(() => {
      const inputs = [...document.querySelectorAll('[role="dialog"] input[type="checkbox"]')];
      return inputs.map(input => ({checked: input.checked, disabled: input.disabled}));
    })()`);
    if (JSON.stringify(options) !== JSON.stringify([
      {checked: true, disabled: true}, {checked: true, disabled: false},
      {checked: false, disabled: false}, {checked: false, disabled: false},
    ])) throw new Error("Original copy defaults changed");
    if (copyJobs) {
      await evaluateValue(client,
        `document.querySelectorAll('[role="dialog"] input[type="checkbox"]')[3].click()`);
      await waitForValue(client,
        `document.querySelectorAll('[role="dialog"] input[type="checkbox"]')[3]?.checked === true`,
        "Copy jobs checkbox did not toggle");
    }
    await clickButton(client, "Confirm");
    await waitForValue(client,
      `fetch('/api/agents').then(response => response.json()).then(value => value.agents.some(agent => agent.name === ${JSON.stringify(name)}))`,
      "Copy did not persist");
    const id = await evaluateValue(client,
      `fetch('/api/agents').then(response => response.json()).then(value => value.agents.find(agent => agent.name === ${JSON.stringify(name)}).id)`);
    ids.push(id);
    await client.send("Page.reload");
    await waitForValue(client,
      `Boolean(document.querySelector('tr[data-row-key="${id}"]'))`,
      "Copied Agent disappeared after reload");
  }
  return {withoutJobs: ids[0], withJobs: ids[1], defaults: true, reload: true};
}
