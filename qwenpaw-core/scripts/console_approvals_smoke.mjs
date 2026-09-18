export async function runApprovalsScenario(client, { evaluateValue, waitForValue }) {
  const selectTab = async () => {
    await waitForValue(client, `[...document.querySelectorAll('[role="tab"]')]
      .some(tab => tab.innerText.trim().startsWith("Approvals"))`, "Inbox approval tab missing");
    await evaluateValue(client, `[...document.querySelectorAll('[role="tab"]')]
      .find(tab => tab.innerText.trim().startsWith("Approvals")).click()`);
    await waitForValue(client, `document.querySelectorAll('[class*="approvalCard"]').length === 2`,
      "Both Agents' approval cards did not render");
  };
  await selectTab();
  await waitForValue(client, `[...document.querySelectorAll('[class*="ownerAgentTag"]')]
    .some(tag => tag.innerText.trim() === "Writer")`, "Writer approval owner missing");
  await client.send("Page.reload");
  await selectTab();
  const snapshot = await evaluateValue(client, `(async () => {
    const response = await fetch('/api/console/push-messages');
    if (!response.ok) throw new Error('Approval verification failed');
    return (await response.json()).pending_approvals;
  })()`);
  if (snapshot.length !== 2 || !snapshot.some(item => item.agent_id === "writer") ||
      !snapshot.some(item => item.agent_id === "default")) {
    throw new Error("Global approvals lost an Agent after reload");
  }
  for (const [writer, label, remaining] of [[false, "Deny", 1], [true, "Approve", 0]]) {
    const clicked = await evaluateValue(client, `(() => {
      const card = [...document.querySelectorAll('[class*="approvalCard"]')].find(card =>
        (card.querySelector('[class*="ownerAgentTag"]')?.innerText.trim() === "Writer") === ${writer});
      const button = [...(card?.querySelectorAll('button') ?? [])]
        .find(button => button.innerText.trim() === ${JSON.stringify(label)} && !button.disabled);
      button?.click();
      return Boolean(button);
    })()`);
    if (!clicked) throw new Error(`Original ${label} button missing`);
    await waitForValue(client, `(async () => {
      const response = await fetch('/api/console/push-messages');
      if (!response.ok) return false;
      const pending = (await response.json()).pending_approvals;
      return pending.length === ${remaining} &&
        (${remaining} === 0 || pending[0].agent_id === "writer");
    })()`, `${label} did not resolve only the chosen approval`);
  }
  await waitForValue(client, `document.querySelectorAll('[class*="approvalCard"]').length === 0`,
    "Resolved approval cards remain visible");
  return { globalOwners: true, reload: true, deniedDefault: true, approvedWriter: true };
}
