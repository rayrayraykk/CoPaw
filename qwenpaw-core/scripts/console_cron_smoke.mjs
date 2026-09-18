import assert from "node:assert/strict";

// All mutations use the unchanged page; fetch only checks persisted results.
export async function runCronScenario(client, { evaluateValue, waitForValue, clickButton, agent = false, existing = false, actor = "default" }) {
  const evaluate = expression => evaluateValue(client, expression);
  const wait = (expression, message) => waitForValue(client, expression, message);
  const headers = JSON.stringify({ "X-Agent-Id": actor });
  const fetchScoped = path => `fetch(${path}, {headers:${headers}})`;
  const defaultJobs = actor === "default" ? null : await evaluate("fetch('/api/cron/jobs').then(r => r.json())");
  const fill = async (id, text) => {
    await wait(`Boolean(document.getElementById(${JSON.stringify(id)}))`, `Missing Cron input ${id}`);
    await evaluate(`(() => { const input = document.getElementById(${JSON.stringify(id)}); input.focus(); input.select(); })()`);
    await client.send("Input.insertText", { text });
  };
  const select = async (id, text, custom = false) => {
    const selector = `document.getElementById(${JSON.stringify(id)})?.closest('[class*="-select-single"]')?.querySelector('[class*="-select-selector"]')`;
    assert.equal(await evaluate(`(() => { const el = ${selector}; el?.dispatchEvent(new MouseEvent('mousedown', {bubbles:true})); return Boolean(el); })()`), true);
    if (custom) await fill(id, text);
    const option = `[...document.querySelectorAll('[class*="-select-item-option-content"]')].find(el => el.offsetHeight && el.innerText === ${JSON.stringify(text)})`;
    await wait(`Boolean(${option})`, `Missing Cron option ${text}`);
    if (agent && id === "dispatch_target_session_id") {
      const visible = await evaluate(`[...document.querySelectorAll('[class*="-select-item-option-content"]')].filter(el=>el.offsetHeight).map(el=>el.innerText)`);
      assert.ok(!visible.includes("wrong-user-session"));
      assert.ok(!visible.includes("hidden-agent-session"));
      assert.ok(visible.every(value => !value.includes("\u0000")));
    }
    await evaluate(`(${option}).click()`);
  };
  if (!existing) {
    await clickButton(client, "+ Create Job");
    await fill("name", "Cron browser fixture");
    await select("task_type", agent ? "agent" : "text");
    if (agent) {
      await fill("request_input", JSON.stringify([{ role: "user", content: [{ type: "text", text: "Write the Cron fixture" }] }]));
      await select("dispatch_mode", "final");
      for (const [id, checked] of [["runtime_share_session", false], ["runtime_tool_safety", false], ["dispatch_silent", true], ["save_result_to_inbox", true]]) {
        await wait(`Boolean(document.getElementById(${JSON.stringify(id)}))`, `Missing original switch ${id}`);
        await evaluate(`(() => { const el = document.getElementById(${JSON.stringify(id)}); if ((el.getAttribute('aria-checked') === 'true') !== ${checked}) el.click(); })()`);
      }
    } else {
      await fill("text", "Cron original page reminder");
    }
    await select("dispatch_target_user_id", "admin", !agent);
    await select("dispatch_target_session_id", "cron-browser-session", !agent);
    await clickButton(client, "Save");
  }
  const jobs = `${fetchScoped("'/api/cron/jobs'")}.then(r => r.json())`;
  await wait(`${jobs}.then(jobs => jobs.some(job => job.name === 'Cron browser fixture'))`, "Original Cron create did not persist");
  const id = await evaluate(`${jobs}.then(jobs => jobs.find(job => job.name === 'Cron browser fixture').id)`);
  const item = JSON.stringify(`/api/cron/jobs/${id}`);
  const row = `document.querySelector(${JSON.stringify(`tr[data-row-key="${id}"]`)})`;
  const rowButton = async text => {
    const button = `[...(${row})?.querySelectorAll('button') ?? []].find(el => el.innerText.trim() === ${JSON.stringify(text)})`;
    await wait(`Boolean(${button})`, `Missing Cron row action ${text}`);
    await evaluate(`(${button}).click()`);
  };
  await rowButton("Enable");
  await wait(`${fetchScoped(item)}.then(r => r.json()).then(v => v.spec.enabled && Boolean(v.state.next_run_at))`, "Enabled Cron did not gain its real next run time");
  await rowButton("Disable");
  await wait(`${fetchScoped(item)}.then(r => r.json()).then(v => !v.spec.enabled && v.state.next_run_at === null)`, "Disabled Cron still has a next run");
  await rowButton("Execute Now");
  await wait(`Boolean(document.querySelector('[class*="-modal-confirm"]'))`, "Original execute confirmation missing");
  await clickButton(client, "Execute Now");
  await wait(`${fetchScoped(item + " + '/history'")}.then(r => r.json()).then(v => v.length === 1 && v[0].status === 'success' && v[0].trigger === 'manual')`, "Original manual trigger did not execute");
  if (agent) {
    const event = await evaluate(`fetch(${JSON.stringify(`/api/console/inbox/events?agent_id=${actor}`)}).then(r => r.json()).then(v => v.events.find(event => event.payload.job_id === ${JSON.stringify(id)}))`);
    assert.ok(event?.payload?.run_id, "Agent Cron did not create a real Inbox trace");
    const trace = await evaluate(`fetch(${JSON.stringify(`/api/console/inbox/traces/${event.payload.run_id}`)}).then(r => r.json())`);
    assert.equal(trace.status, "success");
    assert.equal(trace.meta.agent_id, actor);
    assert.equal(trace.meta.silent, true);
    assert.equal(trace.meta.session_id, `cron-browser-session:cron:${id}`);
    assert.equal(trace.events[1].event.tool_name, "write_file");
    assert.deepEqual(trace.events[3].event.content, [{ type: "text", text: "Cron fixture finished" }]);
  }
  await rowButton("History");
  await wait(`Boolean(document.querySelector('[class*="historyItemStatusSuccess"]')) && document.body.innerText.includes('Triggered manually')`, "Original Cron history did not render");
  const close = `[...document.querySelectorAll('[class*="-modal-title-close"]')].find(el => el.getBoundingClientRect().width > 0)`;
  await wait(`Boolean(${close})`, "Original history close icon missing");
  await evaluate(`(${close}).dispatchEvent(new MouseEvent('click', {bubbles:true}))`);
  await wait(`![...document.querySelectorAll('[class*="historyItemStatusSuccess"]')].some(el => el.offsetHeight)`, "Original history modal did not close");
  const menu = async action => {
    const point = await evaluate(`(() => { const button = (${row}).querySelector('[aria-label="more"]')?.closest('button');
      if (!button) return null; button.scrollIntoView({block:'center',inline:'center'}); const box = button.getBoundingClientRect(); return {x:box.x+box.width/2,y:box.y+box.height/2}; })()`);
    assert.ok(point, "Original Cron more menu missing");
    await client.send("Input.dispatchMouseEvent", { type: "mouseMoved", ...point });
    const entry = `[...document.querySelectorAll('[role="menuitem"]')].find(el => el.offsetHeight && el.innerText === ${JSON.stringify(action)})`;
    await wait(`Boolean(${entry})`, `Original Cron menu ${action} missing`);
    await evaluate(`(${entry}).click()`);
  };
  await menu("Edit");
  await fill("name", "Cron browser edited");
  await clickButton(client, "Save");
  await wait(`${fetchScoped(item)}.then(r => r.json()).then(v => v.spec.name === 'Cron browser edited')`, "Original Cron edit did not persist");
  const oldOrigin = await evaluate("performance.timeOrigin");
  await client.send("Page.reload");
  await wait(`performance.timeOrigin !== ${oldOrigin} && (${row})?.innerText.includes('Cron browser edited')`, "Cron edit did not survive reload");
  await menu("Delete");
  await clickButton(client, "Delete");
  await wait(`${jobs}.then(jobs => !jobs.some(job => job.id === ${JSON.stringify(id)}))`, "Original Cron delete did not persist");
  await wait(`!(${row})`, "Deleted Cron still appears in original table");
  if (defaultJobs !== null) assert.deepEqual(await evaluate("fetch('/api/cron/jobs').then(r => r.json())"), defaultJobs);
  return { created: !existing, toggled: true, manual: true, history: true, edited: true, reload: true, deleted: true, ...(actor === "default" ? {} : { scoped: actor }), ...(agent ? { agent: true, ...(existing ? { mapped: true } : { knownTargets: true }) } : {}) };
}
