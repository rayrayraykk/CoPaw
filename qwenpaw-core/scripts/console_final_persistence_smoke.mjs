import assert from "node:assert/strict";
import { createInterface } from "node:readline";

// Only the isolated Rust test may acknowledge the journal inspection.
export async function runFinalPersistenceScenario(client, { evaluateValue, waitForValue }) {
  const evaluate = expression => evaluateValue(client, expression);
  const wait = (expression, message) => waitForValue(client, expression, message);
  const reply = "Cron fixture finished";
  const failure = "Failed to persist the final turn; the latest state may not survive restart.";
  const composer = `[...document.querySelectorAll('[contenteditable="true"], textarea')]
    .find(el => el.offsetWidth && el.offsetHeight && !el.disabled && !el.readOnly)`;
  const control = createInterface({ input: process.stdin });
  const acknowledge = phase => new Promise((resolve, reject) => {
    const timer = setTimeout(() => finish(new Error(`No journal acknowledgement: ${phase}`)), 10_000);
    const line = value => finish(value === "continue" ? undefined : new Error(`Unexpected acknowledgement: ${value}`));
    const finish = error => {
      clearTimeout(timer);
      control.off("line", line);
      if (error) reject(error); else resolve();
    };
    control.once("line", line);
    process.stderr.write(`QWENPAW_FINAL_PERSISTENCE_${phase}\n`);
  });
  const send = async text => {
    await client.send("Page.bringToFront");
    await wait(`document.hasFocus() && Boolean(${composer})`, "Original Chat composer not ready");
    await evaluate(`${composer}.focus()`);
    // A failed turn may leave its draft in the unchanged sender after reload.
    const modifiers = process.platform === "darwin" ? 4 : 2;
    await client.send("Input.dispatchKeyEvent", { type: "keyDown", key: "a", code: "KeyA", windowsVirtualKeyCode: 65, modifiers });
    await client.send("Input.dispatchKeyEvent", { type: "keyUp", key: "a", code: "KeyA", windowsVirtualKeyCode: 65, modifiers });
    await client.send("Input.insertText", { text });
    await client.send("Input.dispatchKeyEvent", { type: "keyDown", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
    await client.send("Input.dispatchKeyEvent", { type: "keyUp", key: "Enter", code: "Enter", windowsVirtualKeyCode: 13 });
  };
  const visibleFailure = `document.body.innerText.includes(${JSON.stringify(reply)})
    && document.body.innerText.includes(${JSON.stringify(failure)})`;
  try {
    await send("write fixture with final persistence failure");
    await wait(visibleFailure, "Original Chat did not retain its reply and show the persistence failure");
    assert.equal(await evaluate("document.body.innerText.includes('fixture completion write failure')"), false);
    await acknowledge("VISIBLE");
    const previous = await evaluate("performance.timeOrigin");
    await client.send("Page.reload");
    await wait(`performance.timeOrigin !== ${previous} && (${visibleFailure})`,
      "Reload lost the reply or the persistence failure");
    await acknowledge("RELOADED");
    await send("write fixture after storage recovery");
    await wait(`document.body.innerText.split(${JSON.stringify(reply)}).length === 3
      && document.body.innerText.includes('write fixture after storage recovery')`,
    "Original Chat could not complete the next turn after storage recovery");
    const nextOrigin = await evaluate("performance.timeOrigin");
    await client.send("Page.reload");
    await wait(`performance.timeOrigin !== ${nextOrigin}
      && document.body.innerText.split(${JSON.stringify(reply)}).length === 3
      && document.body.innerText.includes(${JSON.stringify(failure)})`,
    "Reload lost the recovered conversation or its earlier error");
    return { replyRetained: true, failureVisible: true, reload: true,
      journalUnchanged: true, nextTurn: true, recoveredReload: true };
  } finally {
    control.close();
    process.stdin.pause();
  }
}
