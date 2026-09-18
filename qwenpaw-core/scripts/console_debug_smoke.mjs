import assert from "node:assert/strict";
import { setTimeout as delay } from "node:timers/promises";

export async function runDebugLogsScenario(client, helpers) {
  const { evaluateValue, waitForValue, clickButton, setInputByPlaceholder } = helpers;
  const evaluate = (expression) => evaluateValue(client, expression);
  const wait = (expression, message) => waitForValue(client, `Boolean(${expression})`, message);
  const control = new URL(process.env.QWENPAW_DEBUG_FIXTURE_URL);
  assert.equal(control.protocol, "http:");
  assert.equal(control.hostname, "127.0.0.1");
  const emit = async (stage) => {
    const response = await fetch(new URL(`/emit/${stage}`, control), { method: "POST", signal: AbortSignal.timeout(5000) });
    assert.equal(response.status, 204);
  };
  const card = `document.querySelector('input[placeholder="Search backend logs..."]')?.closest('.qwenpaw-card')`;
  const viewer = `${card}?.querySelector('[class*="logViewer"]')`;
  const lines = `[...${viewer}.children].map(line => line.textContent)`;
  await wait(`${viewer}?.innerText.includes('warning fixture')`, "Real backend log did not reach the original Debug page");
  assert.ok(await evaluate(`${card}.querySelector('code').textContent.endsWith('/qwenpaw.log') || ${card}.querySelector('code').textContent.endsWith('\\\\qwenpaw.log')`));
  assert.deepEqual((await evaluate(lines)).map(line => line.split(": ").at(-1)), ["error fixture", "warning fixture", "info fixture", "debug fixture"]);

  const selectLevel = async (label) => {
    const point = await evaluate(`(() => {
      const rect = ${card}.querySelector('.qwenpaw-select-selector').getBoundingClientRect();
      return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
    })()`);
    await client.send("Input.dispatchMouseEvent", { type: "mousePressed", button: "left", clickCount: 1, ...point });
    await client.send("Input.dispatchMouseEvent", { type: "mouseReleased", button: "left", clickCount: 1, ...point });
    const option = `[...document.querySelectorAll('.qwenpaw-select-item-option')].find(option => option.textContent.trim() === ${JSON.stringify(label)})`;
    await wait(option, `Log level ${label} did not open`);
    await evaluate(`${option}.click()`);
  };
  for (const level of ["WARNING", "ERROR", "INFO", "DEBUG"]) {
    await selectLevel(level);
    await wait(`${lines}.length === 1 && ${lines}[0].includes(' ${level} ')`, `Original ${level} filter did not match real tracing output`);
  }
  await selectLevel("All");
  await wait(`${lines}.length === 4`, "All levels did not restore logs");
  await setInputByPlaceholder(client, "Search backend logs...", "WaRnInG");
  await wait(`${lines}.length === 1 && ${lines}[0].includes('warning fixture')`, "Case-insensitive search failed");
  assert.ok(await evaluate(`${viewer}.querySelectorAll('mark').length > 0`));

  // Capture the original click handler's exact payload, never the OS clipboard.
  await evaluate(`Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: async text => { window.__debugCopied = text; } } }); true`);
  await clickButton(client, "Copy backend logs");
  await wait(`typeof window.__debugCopied === 'string'`, "Original copy handler did not invoke clipboard.writeText");
  assert.equal(await evaluate("window.__debugCopied"), (await evaluate(lines)).join("\n"));
  await setInputByPlaceholder(client, "Search backend logs...", "");
  await wait(`${lines}.length === 4`, "Clearing search did not restore logs");
  await evaluate(`${card}.querySelectorAll('[role="switch"]')[0].click()`);
  await wait(`${lines}[0].includes('debug fixture')`, "Oldest-first ordering failed");
  assert.deepEqual((await evaluate(lines)).map(line => line.split(": ").at(-1)), ["debug fixture", "info fixture", "warning fixture", "error fixture"]);

  await emit("automatic");
  await wait(`${viewer}.innerText.includes('automatic fixture')`, "Original three-second polling did not load appended logs");
  await evaluate(`${card}.querySelectorAll('[role="switch"]')[1].click()`);
  await wait(`${card}.querySelectorAll('[role="switch"]')[1].getAttribute('aria-checked') === 'false'`, "Auto refresh did not switch off");
  // Allow any in-flight poll to finish before measuring the disabled state.
  await delay(500);
  await emit("manual");
  await delay(3500);
  assert.equal(await evaluate(`${viewer}.innerText.includes('manual fixture')`), false);
  await clickButton(client, "Refresh backend logs");
  await wait(`${viewer}.innerText.includes('manual fixture')`, "Manual refresh did not load appended logs");
  return { file: true, levels: true, search: true, sort: true, autoRefresh: true, pause: true, manualRefresh: true, copyPayload: true };
}
