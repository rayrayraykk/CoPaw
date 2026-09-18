import assert from "node:assert/strict";

// Only installed-directory interactions; plugin activation is a separate gate.
export async function runPawAppsScenario(client, helpers) {
  const { evaluateValue, waitForValue, clickButton, setInputByPlaceholder } = helpers;
  const evaluate = (expression) => evaluateValue(client, expression);
  const wait = (expression, message) => waitForValue(client, `Boolean(${expression})`, message);
  const requests = [];
  client.on("Network.requestWillBeSent", ({ request }) => {
    if (new URL(request.url).pathname.startsWith("/api/pawapps"))
      requests.push({ method: request.method, path: new URL(request.url).pathname });
  });
  const notes = 'document.querySelector(\'[role="button"][aria-label="Fixture Notes"]\')';
  const tasks = 'document.querySelector(\'[role="button"][aria-label="Fixture Tasks"]\')';
  await wait(`${notes} && ${tasks}`, "Original installed-app cards did not render");
  await wait(`${notes}.querySelector('img')?.naturalWidth > 0`, "PawApp icon asset did not load");
  assert.ok(await evaluate(`${notes}.innerText.includes('v1.2.3') && ${notes}.innerText.includes('Fixture app')`));
  await setInputByPlaceholder(client, "Search apps...", "Notes");
  await wait(`${notes} && !${tasks}`, "Original app search did not filter cards");
  await setInputByPlaceholder(client, "Search apps...", "nonexistent fixture");
  await wait('document.body.innerText.includes("No apps match your search")', "Missing original no-results state");
  await setInputByPlaceholder(client, "Search apps...", "");
  await wait(`${notes} && ${tasks}`, "Clearing search did not restore cards");
  const openCategory = async () => {
    const point = await evaluate(`(() => {
      const toolbar = document.querySelector('input[aria-label="Search apps..."]').closest('[class*="toolbar"]');
      const rect = toolbar.querySelector('.qwenpaw-select-selector').getBoundingClientRect();
      return { x: rect.x + rect.width / 2, y: rect.y + rect.height / 2 };
    })()`);
    await client.send("Input.dispatchMouseEvent", { type: "mousePressed", button: "left", clickCount: 1, ...point });
    await client.send("Input.dispatchMouseEvent", { type: "mouseReleased", button: "left", clickCount: 1, ...point });
  };
  await openCategory();
  await wait(`document.querySelector('.qwenpaw-select-item-option[title="tasks"]')`, "Category selector did not open");
  await evaluate(`document.querySelector('.qwenpaw-select-item-option[title="tasks"]').click()`);
  await wait(`!${notes} && ${tasks}`, "Category selection did not filter cards");
  await openCategory();
  await wait(`document.querySelector('.qwenpaw-select-item-option[title="All"]')`, "Category selector did not reopen");
  await evaluate(`document.querySelector('.qwenpaw-select-item-option[title="All"]').click()`);
  await wait(`${notes} && ${tasks}`, "All categories did not restore cards");
  await evaluate(`document.querySelector('button[aria-label="Refresh"]').click()`);
  await wait(`${notes} && ${tasks} && !document.querySelector('button[aria-label="Refresh"]').disabled`, "Refresh did not finish");
  assert.ok(requests.some(({ method, path }) => method === "GET" && path === "/api/pawapps"));

  const uninstallNotes = `${notes}.closest('.qwenpaw-card').querySelector('button.qwenpaw-btn-dangerous').click()`;
  await evaluate(uninstallNotes);
  await wait('document.querySelector(".qwenpaw-modal-confirm")?.innerText.includes("Uninstall app?")', "Original uninstall confirmation missing");
  await clickButton(client, "Cancel");
  await wait('!document.querySelector(".qwenpaw-modal-confirm")', "Cancel did not close the confirmation");
  assert.deepEqual(requests.filter(({ method }) => method === "DELETE"), []);
  assert.ok(await evaluate(`Boolean(${notes} && ${tasks})`));
  await evaluate(uninstallNotes);
  await wait('document.querySelector(".qwenpaw-modal-confirm")?.innerText.includes("Uninstall app?")', "Second uninstall confirmation missing");
  await evaluate(`[...document.querySelectorAll('.qwenpaw-modal-confirm button')].find(button => button.innerText.trim() === 'Uninstall').click()`);
  await wait(`!${notes} && ${tasks} && !document.querySelector('.qwenpaw-modal-confirm')`, "Uninstall did not update original cards");
  assert.deepEqual(requests.filter(({ method }) => method === "DELETE"), [{ method: "DELETE", path: "/api/pawapps/notes" }]);
  const before = await evaluate("performance.timeOrigin");
  await client.send("Page.reload");
  await wait(`performance.timeOrigin !== ${before} && !${notes} && ${tasks}`, "Reload did not preserve the uninstall");
  return { list: true, icon: true, search: true, category: true, refresh: true, cancel: true, uninstall: true, reload: true };
}
