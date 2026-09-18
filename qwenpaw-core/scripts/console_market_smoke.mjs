import assert from "node:assert/strict";

// This scenario runs against an isolated Rust fixture, never public catalogs.
export async function runMarketScenario(client, helpers) {
  const { evaluateValue, waitForValue, clickButton, setInputByPlaceholder } = helpers;
  const evaluate = (expression) => evaluateValue(client, expression);
  const wait = (expression, message) => waitForValue(client, expression, message);
  const requests = [];
  const responses = [];
  client.on("Network.requestWillBeSent", ({ request }) => {
    const path = new URL(request.url).pathname;
    if (request.method === "POST" && ["/api/market/search", "/api/skills/hub/install/start"].includes(path))
      requests.push({ path, body: JSON.parse(request.postData) });
  });
  client.on("Network.responseReceived", ({ response }) => {
    if (new URL(response.url).pathname.startsWith("/api/"))
      responses.push({ path: new URL(response.url).pathname, status: response.status });
  });
  await wait('document.body.innerText.includes("Fixture Market Skill")', "Original market did not browse the fixture catalog");
  await clickButton(client, "Engineering");
  const categoryDeadline = Date.now() + 10_000;
  while (!requests.some(({ body }) => body.category === "engineering-development")) {
    assert.ok(Date.now() < categoryDeadline, "Original category chip did not send its category");
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  await setInputByPlaceholder(client, "Search skills across platforms", "fixture");
  const deadline = Date.now() + 10_000;
  while (!requests.some(({ body }) => body.query === "fixture")) {
    assert.ok(Date.now() < deadline, "Original search input did not send a search");
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  assert.deepEqual(requests.find(({ body }) => body.query === "fixture").body, {
    query: "fixture", provider_pages: { qwenpaw: 1 }, limit: 10, lang: "en",
  });
  // Typing a query clears the category in the original frontend.
  assert.equal(await evaluate(`document.querySelector('[aria-pressed="true"]')?.innerText`), "All");
  await evaluate(`(() => {
    const cards = [...document.querySelectorAll('h3')];
    cards.at(-1)?.scrollIntoView();
    const more = [...document.querySelectorAll('button')].find(button => button.innerText.trim() === 'Load more');
    if (more && !more.disabled) more.click();
  })()`);
  const pagingDeadline = Date.now() + 10_000;
  while (!requests.some(({ body }) => body.query === "fixture" && body.provider_pages?.qwenpaw === 2)) {
    assert.ok(Date.now() < pagingDeadline, "Original market did not request the next provider page");
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  await wait('document.body.innerText.includes("More Fixture Skill")', "Second page did not append to the original results");
  await wait('document.body.innerText.includes("Fixture Market Skill") && document.body.innerText.includes("fixture")', "Search result did not render");
  const clicked = await evaluate(`(() => {
    const title = [...document.querySelectorAll('h3')].find(item => item.innerText === 'Fixture Market Skill');
    title?.click(); return Boolean(title);
  })()`);
  assert.ok(clicked, "Original market result card is missing");
  await wait('document.body.innerText.includes("Skill details")', "Original detail drawer did not open");
  await clickButton(client, "Save");
  await wait('document.body.innerText.includes("Done") && document.body.innerText.includes("browser_market")', "Original install queue did not complete");
  const installs = requests.filter(({ path }) => path.endsWith("/install/start"));
  assert.equal(installs.length, 1);
  assert.equal(installs[0].body.bundle_url, "https://platform.agentscope.io/skills/12345678-1234-1234-1234-123456789abc");
  assert.equal(installs[0].body.enable, true);
  const origin = await evaluate("performance.timeOrigin");
  await client.send("Page.navigate", { url: await evaluate('location.origin + "/skills"') });
  await wait(`performance.timeOrigin !== ${origin} && document.body.innerText.includes("browser_market")`, "Installed skill did not appear on the original Skills page");
  assert.deepEqual(responses.filter(({ status }) => status >= 400), []);
  return { browse: true, category: true, search: true, pagination: true, detail: true, installQueue: true, persistedSkill: true };
}
