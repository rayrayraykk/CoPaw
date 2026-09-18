import assert from "node:assert/strict";

// Called only by the isolated Rust fixture with its local provider catalog.
export async function runOpenRouterScenario(client, helpers) {
  const {
    evaluateValue,
    waitForValue,
    clickModelsTab,
    clickProviderCardAction,
    clickButton,
    setInputByPlaceholder,
  } = helpers;
  const evaluate = (expression) => evaluateValue(client, expression);
  const wait = (expression, message) =>
    waitForValue(client, expression, message);
  const responses = [];
  const requests = [];
  client.on("Network.responseReceived", ({ response }) => {
    if (new URL(response.url).pathname.startsWith("/api/"))
      responses.push({
        path: new URL(response.url).pathname,
        status: response.status,
      });
  });
  client.on("Network.requestWillBeSent", ({ request }) => {
    if (
      request.method === "POST" &&
      new URL(request.url).pathname.endsWith("/models/filter")
    )
      requests.push(JSON.parse(request.postData));
  });
  await wait(
    'document.body.innerText.includes("Cloud Providers")',
    "Model tabs did not render",
  );
  await clickModelsTab(client, "Cloud Providers");
  await wait(
    'document.body.innerText.includes("OpenRouter")',
    "OpenRouter card did not render",
  );
  await clickProviderCardAction(client, "OpenRouter", "Models");
  await wait(
    'document.body.innerText.includes("OpenRouter — Model Management")',
    "Original model manager did not open",
  );
  await clickButton(client, "Add Models");
  await wait(
    'document.body.innerText.includes("alpha") && document.body.innerText.includes("beta")',
    "Remote provider series did not load",
  );
  await setInputByPlaceholder(client, "Search provider name", "beta");
  await clickButton(client, "Deselect All");
  await setInputByPlaceholder(client, "Search provider name", "");
  const toggle = async (text) => {
    const clicked = await evaluate(`(() => {
      const labels = [...document.querySelectorAll('[role="dialog"] span, [role="dialog"] div')]
        .filter(item => item.getClientRects().length && item.innerText.trim() === ${JSON.stringify(
          text,
        )})
        .sort((a, b) => a.querySelectorAll('*').length - b.querySelectorAll('*').length);
      let row = labels[0];
      while (row && !row.querySelector('[role="switch"]')) row = row.parentElement;
      const control = row?.querySelector('[role="switch"]');
      control?.click();
      return Boolean(control);
    })()`);
    assert.ok(clicked, `Original switch is missing: ${text}`);
  };
  await toggle("Image");
  await toggle("Free Models Only:");
  await clickButton(client, "Filter Models");
  await wait(
    'document.body.innerText.includes("vision-free")',
    "Filtered model did not render",
  );
  assert.deepEqual(requests, [
    { providers: ["alpha"], input_modalities: ["image"], is_free: true },
  ]);
  const filteredText = await evaluate(
    'document.querySelector("[role=dialog]").innerText',
  );
  assert.ok(!filteredText.includes("video-paid"));
  await clickButton(client, "Add");
  await wait(
    'fetch("/api/models").then(r => r.json()).then(providers => providers.find(provider => provider.id === "openrouter").extra_models.some(model => model.id === "alpha/vision-free"))',
    "Original Add did not persist the selected model",
  );
  const origin = await evaluate("performance.timeOrigin");
  await client.send("Page.reload", { ignoreCache: true });
  await wait(
    `performance.timeOrigin !== ${origin} && document.body.innerText.includes("Cloud Providers")`,
    "Models page did not reload",
  );
  await clickModelsTab(client, "Cloud Providers");
  await clickProviderCardAction(client, "OpenRouter", "Models");
  await wait(
    'document.querySelector("[role=dialog]")?.innerText.includes("vision-free")',
    "Added model did not survive reload",
  );
  const probeSelector = '[role="dialog"] button[aria-label="Test Multimodal"]';
  await wait(
    `Boolean(document.querySelector(${JSON.stringify(probeSelector)}))`,
    "Original multimodal probe action did not render",
  );
  await evaluate(
    `document.querySelector(${JSON.stringify(probeSelector)}).click()`,
  );
  const deadline = Date.now() + 10_000;
  while (
    !responses.some((response) => response.path.endsWith("/probe-multimodal"))
  ) {
    assert.ok(Date.now() < deadline, "Original probe action did not finish");
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  assert.deepEqual(
    responses.filter((response) => response.status >= 400),
    [],
  );
  return {
    series: true,
    filtered: true,
    added: true,
    reload: true,
    metadataProbe: true,
  };
}
