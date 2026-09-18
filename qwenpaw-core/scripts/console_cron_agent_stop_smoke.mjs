export async function runCronAgentStopScenario(client, {
  evaluateValue, waitForValue, clickAgentRowAction,
  clickButton, selectAgentFromSidebar,
}) {
  await waitForValue(client,
    `Boolean(document.querySelector('tr[data-row-key="writer"]')) && Boolean(document.querySelector('tr[data-row-key="editor"]'))`,
    "Both fixture Agents must render");
  await selectAgentFromSidebar(client, "writer");
  await clickAgentRowAction(client, "writer", 3);
  await clickButton(client, "Confirm");
  await waitForValue(client,
    `fetch('/api/agents').then(response => response.json()).then(value => value.agents.some(agent => agent.id === 'writer' && agent.enabled === false))`,
    "Original disable button did not disable Writer");
  await waitForValue(client,
    `localStorage.getItem('qwenpaw-last-used-agent') === 'default'`,
    "Disabling the selected Agent did not return to default");
  await waitForValue(client,
    `fetch('/api/console/push-messages').then(response => response.json()).then(value => value.pending_approvals.length === 2 && value.pending_approvals.every(approval => approval.agent_id !== 'writer'))`,
    "Writer approvals did not drain independently");
  await waitForValue(client,
    `Boolean(document.querySelector('tr[data-row-key="writer"] svg.lucide-eye'))`,
    "Writer row did not render its enable action");
  await clickAgentRowAction(client, "writer", 3);
  await clickButton(client, "Confirm");
  await waitForValue(client,
    `fetch('/api/agents').then(response => response.json()).then(value => value.agents.some(agent => agent.id === 'writer' && agent.enabled === true))`,
    "Original enable button did not re-enable Writer");
  await clickAgentRowAction(client, "editor", 4);
  await clickButton(client, "Confirm");
  await waitForValue(client,
    `fetch('/api/agents').then(response => response.json()).then(value => !value.agents.some(agent => agent.id === 'editor'))`,
    "Original delete button did not remove Editor");
  await client.send("Page.reload");
  await waitForValue(client,
    `Boolean(document.querySelector('tr[data-row-key="writer"]')) && !document.querySelector('tr[data-row-key="editor"]')`,
    "Lifecycle changes did not survive reload");
  await waitForValue(client,
    `fetch('/api/agents').then(response => response.json()).then(value => value.agents.some(agent => agent.id === 'writer' && agent.enabled === true))`,
    "Re-enabled Writer did not survive reload");
  const approvals = await evaluateValue(client,
    `fetch('/api/console/push-messages').then(response => response.json()).then(value => value.pending_approvals.map(approval => approval.agent_id))`);
  if (JSON.stringify(approvals) !== JSON.stringify(["default"])) {
    throw new Error("Stopping other Agents changed the default approval");
  }
  return {disabled: true, reenabled: true, deleted: true, selectedFallback: true, defaultPending: true, reload: true};
}
