import assert from "node:assert/strict";
import { access, mkdtemp } from "node:fs/promises";
import { spawn } from "node:child_process";
import { tmpdir } from "node:os";
import path from "node:path";
import { DevToolsClient, closeBrowser } from "./console_devtools.mjs";

const base = new URL(process.argv[2]);
assert(base.protocol === "http:" && ["localhost", "127.0.0.1", "[::1]"].includes(base.hostname));
const candidates = [process.env.QWENPAW_CHROME, ...(process.platform === "darwin"
  ? ["/Applications/Google Chrome.app/Contents/MacOS/Google Chrome", "/Applications/Chromium.app/Contents/MacOS/Chromium"]
  : process.platform === "win32"
    ? [process.env.PROGRAMFILES && path.join(process.env.PROGRAMFILES, "Google/Chrome/Application/chrome.exe")]
    : ["/usr/bin/google-chrome", "/usr/bin/chromium", "/usr/bin/chromium-browser"])];
let executable;
for (const candidate of candidates.filter(Boolean)) {
  try { await access(candidate); executable = candidate; break; } catch {}
}
assert(executable, "Chrome is required for original frontend acceptance");
const profile = await mkdtemp(path.join(tmpdir(), "qwenpaw-plugin-browser-"));
const child = spawn(executable, ["--headless=new", "--no-first-run", "--no-default-browser-check",
  "--remote-debugging-port=0", `--user-data-dir=${profile}`, "about:blank"],
{ stdio: ["ignore", "ignore", "pipe"] });
let page, browser;
const pause = ms => new Promise(resolve => setTimeout(resolve, ms));
async function connect(url) {
  const socket = new WebSocket(url);
  await new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });
  return new DevToolsClient(socket);
}
try {
  const debuggerUrl = await new Promise((resolve, reject) => {
    let stderr = "";
    const timer = setTimeout(() => reject(new Error("DevTools startup timeout")), 15000);
    child.on("error", error => {clearTimeout(timer); reject(error);});
    child.on("exit", code => {clearTimeout(timer); reject(new Error(`Chrome exited ${code}`));});
    child.stderr.on("data", bytes => {
      stderr += bytes;
      const url = stderr.match(/DevTools listening on (ws:\/\/[^\s]+)/)?.[1];
      if (url) {clearTimeout(timer); resolve(url);}
    });
  });
  browser = await connect(debuggerUrl);
  const { targetId } = await browser.send("Target.createTarget", {url: "about:blank"});
  const targets = await (await fetch(`http://127.0.0.1:${new URL(debuggerUrl).port}/json/list`)).json();
  page = await connect(targets.find(target => target.id === targetId).webSocketDebuggerUrl);
  const exceptions = [], requests = [];
  page.on("Runtime.exceptionThrown", value => exceptions.push(value.exceptionDetails.text));
  page.on("Network.responseReceived", ({response}) => {
    if (new URL(response.url).pathname.startsWith("/api/frontend_plugin"))
      requests.push({path: new URL(response.url).pathname, status: response.status});
  });
  await page.send("Page.enable"); await page.send("Runtime.enable"); await page.send("Network.enable");
  await page.send("Page.addScriptToEvaluateOnNewDocument", {source: `
    window.__fixtureEvents=[];
    for (const name of ['popstate','pageshow','click','pointerdown','pointerup']) {
      window.addEventListener(name, event => {
        window.__fixtureEvents.push({type:name,url:location.pathname,time:performance.now(),
          text:event.target?.textContent?.slice(0,80),trusted:event.isTrusted});
        if(name==='popstate') window.__fixturePopHref=location.pathname;
      },true);
    }
  `});
  await page.send("Emulation.setDeviceMetricsOverride", {width:1440,height:1000,deviceScaleFactor:1,mobile:false});
  const evaluate = async expression => {
    const result = await page.send("Runtime.evaluate", {expression,returnByValue:true,awaitPromise:true});
    assert(!result.exceptionDetails, JSON.stringify(result.exceptionDetails));
    return result.result.value;
  };
  async function wait(expression) {
    const deadline = Date.now() + 20000;
    while (Date.now() < deadline) {const value=await evaluate(expression); if(value) return value; await pause(100);}
    throw new Error(`Browser condition timed out: ${expression}; ${await evaluate("document.body.innerText")}; ${JSON.stringify(await evaluate("window.__fixtureEvents"))}`);
  }
  async function click(selector) {
    const probe = `(async () => {
      const element=document.querySelector(${JSON.stringify(selector)});
      if(!element || document.readyState!=='complete') return null;
      const before=element.getBoundingClientRect();
      if(!before.width || !before.height) return null;
      await new Promise(requestAnimationFrame);
      const after=element.getBoundingClientRect();
      const point={x:after.x+after.width/2,y:after.y+after.height/2};
      return element.isConnected && before.x===after.x && before.y===after.y &&
        before.width===after.width && before.height===after.height &&
        element.contains(document.elementFromPoint(point.x,point.y)) ? point : null;
    })()`;
    let point = await wait(probe);
    await page.send("Input.dispatchMouseEvent", {type:"mouseMoved",...point});
    point = await wait(probe);
    for (const type of ["mousePressed", "mouseReleased"])
      await page.send("Input.dispatchMouseEvent", {type,button:"left",clickCount:1,...point});
  }
  await page.send("Page.navigate", {url: new URL("/market", base).href});
  await click('[role="button"][aria-label="Demo"]');
  await wait('location.pathname === "/apps/demo" && document.querySelector("#fixture-count")?.textContent === "Count: 0"');
  await click("#fixture-increment");
  await wait('document.querySelector("#fixture-count")?.textContent === "Count: 1"');
  const before = await evaluate("performance.timeOrigin");
  await page.send("Page.reload");
  await wait(`performance.timeOrigin !== ${before} && location.pathname === "/apps/demo" && document.querySelector("#fixture-count")?.textContent === "Count: 0"`);
  await click('button[title*="ESC"]');
  await wait('window.__fixturePopHref === "/market" && location.pathname === "/market" && !document.querySelector("#fixture-count")');
  await click('[role="button"][aria-label="Demo"]');
  await wait('document.querySelector("#fixture-count")?.textContent === "Count: 0"');
  assert.deepEqual(exceptions, []);
  assert(requests.some(r => r.path === "/api/frontend_plugin/demo/files/ui/main.js"));
  assert(requests.every(r => r.status === 200 || r.status === 304));
  await closeBrowser(browser, child);
  console.log(JSON.stringify({ok:true,open:true,interaction:true,reload:true,back:true,reopen:true,requests,profile}));
} finally {
  page?.close(); browser?.close();
  if(child.exitCode === null && child.signalCode === null) {
    child.kill("SIGTERM");
    await Promise.race([new Promise(resolve=>child.once("exit",resolve)),pause(2000)]);
    if(child.exitCode === null && child.signalCode === null) {
      child.kill("SIGKILL");
      await new Promise(resolve=>child.once("exit",resolve));
    }
  }
  // Keep the isolated profile for diagnosis; never touch a daily profile.
}
