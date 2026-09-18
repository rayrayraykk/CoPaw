import assert from "node:assert/strict";

// The isolated Rust fixture hosts a real redirect and code-exchange simulator.
export async function runProviderOAuthScenario(client, helpers) {
  const { evaluateValue, waitForValue, clickModelsTab, clickButton } = helpers;
  const evaluate = (expression) => evaluateValue(client, expression);
  const wait = (expression, message) =>
    waitForValue(client, expression, message);
  const responses = [];
  client.on("Network.responseReceived", ({ response }) => {
    const path = new URL(response.url).pathname;
    if (path.startsWith("/api/"))
      responses.push({ path, status: response.status });
  });
  await wait(
    "Boolean(document.querySelector('button[aria-label=\"Select model\"]'))",
    "Original Chat model selector did not render",
  );
  await evaluate(
    "document.querySelector('button[aria-label=\"Select model\"]').click()",
  );
  await clickButton(client, "FREE");
  await wait(
    'document.body.innerText.includes("Connect OpenRouter to use free models")',
    "OpenRouter OAuth entry did not render",
  );
  await clickButton(client, "Connect OpenRouter to use free models");
  await wait(
    'document.body.innerText.includes("Connect to OpenRouter")',
    "Original OAuth confirmation did not open",
  );
  const result = await client.send("Runtime.evaluate", {
    expression: `(() => {
      const button = [...document.querySelectorAll('[role="dialog"] button')].find(button => button.innerText.trim() === 'Continue');
      button?.click(); return Boolean(button);
    })()`,
    userGesture: true,
    returnByValue: true,
  });
  assert.equal(result.result.value, true);
  await wait(
    'location.pathname === "/models" && document.body.innerText.includes("OpenRouter — Model Management")',
    "Original OAuth polling did not navigate to model management",
  );
  assert.ok(responses.some(({ path }) => path.endsWith("/oauth/start")));
  assert.ok(responses.some(({ path }) => path.endsWith("/oauth/status")));
  await clickButton(client, "Add Models");
  await wait(
    'document.querySelector("[role=dialog]")?.innerText.includes("fixture")',
    "OAuth model discovery did not reach the original model manager",
  );
  await clickButton(client, "Filter Models");
  await wait(
    'document.body.innerText.includes("model")',
    "Authorized catalog filter did not return its model",
  );
  await clickButton(client, "Add");
  await wait(
    'fetch("/api/models").then(r=>r.json()).then(providers=>providers.find(p=>p.id==="openrouter").extra_models.some(m=>m.id==="fixture/model"))',
    "Model was not added after OAuth",
  );
  const origin = await evaluate("performance.timeOrigin");
  await evaluate('location.assign("/models")');
  await wait(
    `performance.timeOrigin !== ${origin} && document.body.innerText.includes("Cloud Providers")`,
    "Models page did not reload",
  );
  await clickModelsTab(client, "Cloud Providers");
  const connected = await evaluate(`(() => {
    const card = [...document.querySelectorAll('[class*="groupCardGlass"]')].find(card => card.innerText.includes('OpenRouter'));
    return Boolean(card && card.innerText.includes('********') && ![...card.querySelectorAll('button')].some(button=>button.innerText.trim()==='OAuth'));
  })()`);
  assert.equal(
    connected,
    true,
    "Original provider card lost its connected state after reload",
  );
  await evaluate('location.assign("/chat")');
  await wait(
    "Boolean(document.querySelector('button[aria-label=\"Select model\"]'))",
    "Chat did not reopen after OAuth model configuration",
  );
  assert.deepEqual(
    responses.filter(({ status }) => status >= 400),
    [],
  );
  return {
    confirmation: true,
    externalRedirect: true,
    polling: true,
    modelDiscovery: true,
    modelAdded: true,
    reload: true,
  };
}
