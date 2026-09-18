import assert from "node:assert/strict";

// Mutations use the original UI; fetch is used only for persistence assertions.
export async function runAnthropicScenario(client, helpers, gemini = false, responses = false) {
  const { evaluateValue, waitForValue, clickModelsTab, clickProviderCardAction,
    clickButton, setInputByPlaceholder, setModelIdInput } = helpers;
  const evaluate = (expression) => evaluateValue(client, expression);
  const wait = (expression, message) => waitForValue(client, expression, message);
  const base = process.env[responses ? "QWENPAW_RESPONSES_FIXTURE_URL" : gemini ? "QWENPAW_GEMINI_FIXTURE_URL" : "QWENPAW_ANTHROPIC_FIXTURE_URL"];
  const provider = responses ? "openai-response" : gemini ? "gemini" : "anthropic";
  const title = responses ? "OpenAI (Response API)" : gemini ? "Google Gemini" : "Anthropic";
  const model = responses ? "fixture-responses" : gemini ? "fixture-gemini" : "fixture-claude";
  const modelName = responses ? "Fixture Responses" : gemini ? "Fixture Gemini" : "Fixture Claude";
  const reply = responses ? "原生 Responses 回复" : gemini ? "原生 Gemini 回复" : "原生 Anthropic 回复";
  assert.equal(new URL(base).hostname, "127.0.0.1");
  const fill = async (selector, value) => {
    await wait(`Boolean(document.querySelector(${JSON.stringify(selector)}))`,
      `Input not found: ${selector}`);
    await evaluate(`(() => { const input = document.querySelector(${JSON.stringify(selector)});
      input.focus(); input.select(); })()`);
    await client.send("Input.insertText", { text: value });
  };
  await clickModelsTab(client, "Cloud Providers");
  await wait(`Boolean([...document.querySelectorAll('[class*="availableItemName"]')].find(el => el.innerText === ${JSON.stringify(title)}))`,
    `${title} entry did not render in Available Providers`);
  await evaluate(`[...document.querySelectorAll('[class*="availableItemName"]')].find(el => el.innerText === ${JSON.stringify(title)}).click()`);
  if (!gemini && !responses) await fill('input[id="base_url"]', base);
  // Gemini's original built-in provider has a frozen URL. Its isolated server
  // fixture supplies the loopback endpoint without changing that UI contract.
  await fill('input[id="api_key"]', responses ? "sk-responses-private-key" : gemini ? "gemini-fixture-private-key" : "sk-ant-fixture-private-key");
  await clickButton(client, "Save");
  await wait(`fetch('/api/models').then(r => r.json()).then(providers =>
    providers.some(p => p.id === ${JSON.stringify(provider)} && p.base_url === ${JSON.stringify(base)} && p.api_key === '********'))`,
    `Original ${title} configuration did not persist`);
  await wait(`Boolean([...document.querySelectorAll('[class*="groupCardName"]')].find(el => el.innerText === ${JSON.stringify(title)}))`,
    `Configured ${title} card did not render`);
  await clickProviderCardAction(client, title, "Models");
  await clickButton(client, "Add Model");
  await wait(`Boolean(document.querySelector('input[placeholder="e.g. GPT-4o, Gemini 2.0 Flash"]'))`,
    "Original Add Model dialog did not open");
  await setModelIdInput(client, model);
  await setInputByPlaceholder(client, "e.g. GPT-4o, Gemini 2.0 Flash", modelName);
  await clickButton(client, "Add Model");
  await wait(`fetch('/api/models').then(r => r.json()).then(providers =>
    providers.find(p => p.id === ${JSON.stringify(provider)})?.extra_models.some(m => m.id === ${JSON.stringify(model)}))`,
    "Model added through original dialog did not persist");
  if (gemini) {
    const row = `[...document.querySelectorAll('[class*="modelListItemId"]')]
      .find(el => el.textContent === ${JSON.stringify(model)})?.parentElement?.parentElement`;
    await fill('input[placeholder="Search models..."]', modelName);
    await wait(`Boolean((${row})?.querySelector('button[aria-label="Test Multimodal"]'))`,
      "Original multimodal probe action missing");
    await evaluate(`(${row}).querySelector('button[aria-label="Test Multimodal"]').click()`);
    await wait(`fetch('/api/models').then(r => r.json()).then(providers => {
      const m = providers.find(p => p.id === 'gemini')?.extra_models.find(m => m.id === ${JSON.stringify(model)});
      return m?.supports_image && m.supports_video && m.probe_source === 'probed';
    })`, "Gemini probe results did not persist");
    await wait(`(${row})?.innerText.includes('Multimodal')`, "Original multimodal tag missing");
    const oldTime = await evaluate("performance.timeOrigin");
    await client.send("Page.reload");
    await wait(`performance.timeOrigin !== ${oldTime} && document.body.innerText.includes('Cloud Providers')`,
      "Models page did not finish loading after reload");
    await clickModelsTab(client, "Cloud Providers");
    await clickProviderCardAction(client, title, "Models");
    await fill('input[placeholder="Search models..."]', modelName);
    await wait(`(${row})?.innerText.includes('Multimodal')`, "Original capability tag did not survive reload");
  }
  await client.send("Page.navigate", { url: await evaluate("location.origin + '/chat'") });
  await wait(`Boolean(document.querySelector('button[aria-label="Select model"]'))`,
    "Original Chat selector did not render");
  // DOM focus can select a textarea in a document that has not gained native
  // focus yet. Activate this isolated headless page before keyboard input.
  await client.send("Page.bringToFront");
  await wait("document.hasFocus()", "Chat page did not gain keyboard focus");
  await evaluate(`document.querySelector('button[aria-label="Select model"]').click()`);
  if (gemini || responses) await fill('input[aria-label="Search models..."]', modelName);
  try {
    await wait(`document.body.innerText.includes(${JSON.stringify(modelName)})`, `${title} model missing in Chat selector`);
  } catch (error) {
    throw new Error(`${error.message}: ${await evaluate('document.body.innerText.slice(-4000)')}`);
  }
  assert.equal(await evaluate(`(() => {
    const item = [...document.querySelectorAll('button')].find(el =>
      el.innerText.includes(${JSON.stringify(modelName)})); item?.click(); return Boolean(item);
  })()`), true);
  await wait(`fetch('/api/models/active?scope=agent&agent_id=default').then(r => r.json()).then(v =>
    v.active_llm?.provider_id === ${JSON.stringify(provider)} && v.active_llm?.model === ${JSON.stringify(model)})`,
    "Chat selection did not persist for the current Agent");
  const composer = `[...document.querySelectorAll('[contenteditable="true"], textarea')]
    .find(el => el.offsetWidth && el.offsetHeight && !el.disabled && !el.readOnly)`;
  await wait(`Boolean(${composer})`, "Chat composer missing");
  if (gemini || responses) {
    const imagePath = process.env.QWENPAW_IMAGE_FIXTURE_PATH;
    assert.ok(imagePath, "Missing local image fixture");
    await wait(`Boolean(document.querySelector('input[type="file"]'))`, "Original attachment picker missing");
    const { root } = await client.send("DOM.getDocument");
    const { nodeId } = await client.send("DOM.querySelector", { nodeId: root.nodeId, selector: 'input[type="file"]' });
    await client.send("DOM.setFileInputFiles", { nodeId, files: [imagePath] });
    await wait(`Boolean([...document.images].find(img => img.closest('[class*="status-done"]') && img.src.startsWith('data:image/png;base64,') && img.complete && img.naturalWidth > 0))`,
      "Uploaded image preview did not load");
  }
  await evaluate(`(() => {
    window.__qwenpawComposerEvents = [];
    const original = ${composer};
    window.__qwenpawComposerOriginal = original;
    for (const type of ['focusin', 'focusout', 'compositionstart', 'compositionend', 'input', 'keydown', 'keyup']) {
      document.addEventListener(type, event => {
        if (window.__qwenpawComposerEvents.length >= 30) return;
        let sender = event.target;
        while (sender && !sender.querySelector?.('[class*="sender-actions-btn"]')) sender = sender.parentElement;
        const action = sender?.querySelector('[class*="sender-actions-btn"]');
        window.__qwenpawComposerEvents.push({ type, key: event.key,
          composing: event.isComposing, time: performance.now(),
          target: event.target.tagName,
          inComposer: Boolean(event.target?.closest?.('[class*="sender"]')),
          activeTag: document.activeElement?.tagName,
          documentFocused: document.hasFocus(), visibility: document.visibilityState,
          originalConnected: original.isConnected,
          originalTarget: event.target === original,
          stateLength: sender?.querySelector('textarea')?.value.length,
          count: sender?.querySelector('[class*="sender-actions-list-length"]')?.textContent,
          sendDisabled: action?.matches(':disabled') || action?.className.includes('-disabled') });
      }, true);
    }
    original.focus();
    window.__qwenpawComposerEvents.push({ type: 'focused',
      time: performance.now(), activeTag: document.activeElement?.tagName,
      documentFocused: document.hasFocus(), visibility: document.visibilityState,
      originalConnected: original.isConnected, originalActive: document.activeElement === original });
  })()`);
  await client.send("Input.insertText", { text: "Read fixture.txt and reply." });
  await client.send("Input.dispatchKeyEvent", { type: "keyDown", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
  await client.send("Input.dispatchKeyEvent", { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
  console.error(`Composer input events: ${JSON.stringify(await evaluate('window.__qwenpawComposerEvents'))}`);
  try {
    await wait(`document.body.innerText.includes(${JSON.stringify(reply)})`, `Native ${title} reply did not render`);
  } catch (error) {
    const focus = await evaluate(`({ activeTag: document.activeElement?.tagName,
      documentFocused: document.hasFocus(), visibility: document.visibilityState,
      originalConnected: window.__qwenpawComposerOriginal?.isConnected,
      originalActive: document.activeElement === window.__qwenpawComposerOriginal,
      originalLength: window.__qwenpawComposerOriginal?.value?.length })`);
    throw new Error(`${error.message}: ${await evaluate('document.body.innerText.slice(-4000)')}; inputEvents=${JSON.stringify(await evaluate('window.__qwenpawComposerEvents'))}; focus=${JSON.stringify(focus)}`);
  }
  const openSteps = async () => {
    await wait(`document.body.innerText.includes('Completed 1 steps')`, "Original completed steps summary missing");
    await evaluate(`(() => {
      const summary = [...document.querySelectorAll('span, div, button')]
        .filter(el => el.innerText === 'Completed 1 steps')
        .sort((a, b) => a.childElementCount - b.childElementCount)[0]; summary.click();
    })()`);
    await wait(`Boolean([...document.querySelectorAll('summary')].find(el => el.innerText.includes('Read fixture.txt')))`,
      "Original Read File card did not render");
    await evaluate(`[...document.querySelectorAll('summary')].find(el => el.innerText.includes('Read fixture.txt')).click()`);
    await wait(`document.body.innerText.includes('fixture content')`, "Original tool output did not render");
  };
  await openSteps();
  const oldOrigin = await evaluate("performance.timeOrigin");
  await client.send("Page.reload");
  await wait(`performance.timeOrigin !== ${oldOrigin} && document.body.innerText.includes(${JSON.stringify(reply)})`,
    "Original chat history did not survive reload");
  await openSteps();
  if (gemini || responses) {
    await wait(`Boolean([...document.images].find(img => img.src.startsWith('data:image/png;base64,') && img.complete && img.naturalWidth > 0))`,
      "Immutable image history did not render after reload");
  }
  return { configured: true, modelAdded: true, agentSelected: true, reply: true, tool: true, reload: true,
    ...(gemini ? { multimodal: true, image: true } : {}), ...(responses ? { image: true } : {}) };
}
