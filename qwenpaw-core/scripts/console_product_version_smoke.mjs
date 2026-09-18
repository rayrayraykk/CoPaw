import assert from "node:assert/strict";
import { access, mkdtemp, writeFile } from "node:fs/promises";
import { spawn } from "node:child_process";
import { tmpdir } from "node:os";
import path from "node:path";
import { DevToolsClient, closeBrowser } from "./console_devtools.mjs";
import { attachShutdownDiagnostics } from "./console_browser_shutdown_diagnostics.mjs";

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
const profile = await mkdtemp(path.join(tmpdir(), "qwenpaw-product-version-browser-"));
const child = spawn(executable, ["--headless=new", "--no-first-run", "--no-default-browser-check",
  "--remote-debugging-port=0", `--user-data-dir=${profile}`, "about:blank"],
{ stdio: ["ignore", "ignore", "pipe"] });
const diagnosticRoot = process.env.QWENPAW_BROWSER_DIAGNOSTICS_DIR;
const shutdownDiagnostics = diagnosticRoot ? attachShutdownDiagnostics(child) : null;
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
  const exceptions = [], requests = [], installs = [];
  page.on("Network.requestWillBeSent", ({request}) => {
    if (new URL(request.url).pathname === "/api/plugins/install") installs.push(request.url);
  });
  page.on("Runtime.exceptionThrown", value => exceptions.push(value.exceptionDetails.text));
  page.on("Network.responseReceived", ({response}) => {
    if (new URL(response.url).pathname.startsWith("/api/plugins"))
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
  async function click(selector, label = null) {
    const probe = `(async () => {
      const element=[...document.querySelectorAll(${JSON.stringify(selector)})].find(e=>e.getClientRects().length && (${JSON.stringify(label)} === null || e.textContent.trim() === ${JSON.stringify(label)}));
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
    await evaluate(`(() => {
      const element=[...document.querySelectorAll(${JSON.stringify(selector)})].find(e=>e.getClientRects().length && (${JSON.stringify(label)} === null || e.textContent.trim() === ${JSON.stringify(label)}));
      element?.scrollIntoView({block:"center",inline:"nearest"});
    })()`);
    let point;
    try { point = await wait(probe); } catch (error) {
      const targets = await evaluate(`[...document.querySelectorAll(${JSON.stringify(selector)})].map(e=>{
        const r=e.getBoundingClientRect();
        return {text:e.textContent,rect:r.toJSON(),hit:document.elementFromPoint(r.x+r.width/2,r.y+r.height/2)?.outerHTML};
      })`);
      throw new Error(`${error.message}; click targets: ${JSON.stringify(targets)}`);
    }
    await page.send("Input.dispatchMouseEvent", {type:"mouseMoved",...point});
    point = await wait(probe);
    for (const type of ["mousePressed", "mouseReleased"])
      await page.send("Input.dispatchMouseEvent", {type,button:"left",clickCount:1,...point});
  }


  const waitState = expression => wait("Boolean(" + expression + ")");
  async function badges() {
    await waitState(`document.querySelector('article[aria-label="Current Plugin"]') &&
      document.querySelector('article[aria-label="Old Plugin"]')`);
    await waitState(`document.querySelector('article[aria-label="Current Plugin"] .qwenpaw-tag-green')?.textContent.includes("2.x") &&
      document.querySelector('article[aria-label="Old Plugin"] .qwenpaw-tag-orange')?.textContent.includes("1.x")`);
  }
  await page.send("Page.navigate", {url: new URL("/market?tab=plugins", base).href});
  await click('[role="tab"]', "Plugin Market");
  await badges();
  // The original cards reveal their action buttons only on hover/focus.
  const cardPoint = await wait(`(() => {
    const card=document.querySelector('article[aria-label="Old Plugin"]');
    const r=card.getBoundingClientRect();
    const point={x:r.x+r.width/2,y:r.y+r.height/2};
    return card.contains(document.elementFromPoint(point.x,point.y)) ? point : null;
  })()`);
  await page.send("Input.dispatchMouseEvent", {type:"mouseMoved",...cardPoint});
  await click('article[aria-label="Old Plugin"] button', "Install");
  await waitState(`[...document.querySelectorAll('[role="dialog"]')].some(e=>
    e.getClientRects().length && e.innerText.includes("Compatibility Warning") &&
    e.innerText.includes("Your QwenPaw version is 2.2.0b5") &&
    e.innerText.includes("1.x") && e.innerText.includes("Install anyway"))`);
  await click('[role="dialog"] button', "Cancel");
  await waitState(`![...document.querySelectorAll('[role="dialog"]')].some(e=>e.getClientRects().length)`);
  assert.deepEqual(installs, []);
  const before = await evaluate("performance.timeOrigin");
  await page.send("Page.reload");
  await waitState(`performance.timeOrigin !== ${before}`);
  await click('[role="tab"]', "Plugin Market");
  await badges();
  assert.deepEqual(installs, []);
  assert.deepEqual(exceptions, []);
  assert(requests.some(r=>r.path === "/api/plugins/market/search"));
  assert(requests.every(r=>r.status === 200));
  shutdownDiagnostics?.watchClose(browser);
  try {
    await closeBrowser(browser, child);
    shutdownDiagnostics?.mark("close-verified");
  } catch (error) {
    shutdownDiagnostics?.mark("close-rejected");
    throw error;
  }
  console.log(JSON.stringify({ok:true,classification:true,warning:true,cancel:true,
    reload:true,noInstall:true,requests,profile}));

} finally {
  shutdownDiagnostics?.mark("cleanup-start");
  page?.close(); browser?.close();
  if(child.exitCode === null && child.signalCode === null) {
    shutdownDiagnostics?.mark("cleanup-sigterm");
    child.kill("SIGTERM");
    await Promise.race([new Promise(resolve=>child.once("exit",resolve)),pause(2000)]);
    if(child.exitCode === null && child.signalCode === null) {
      shutdownDiagnostics?.mark("cleanup-sigkill");
      child.kill("SIGKILL");
      await new Promise(resolve=>child.once("exit",resolve));
    }
  }
  if (shutdownDiagnostics) {
    shutdownDiagnostics.mark("cleanup-complete");
    shutdownDiagnostics.dispose();
    try {
      const directory = await mkdtemp(path.join(diagnosticRoot, "browser-shutdown-"));
      await writeFile(path.join(directory, "timeline.json"),
        JSON.stringify({ ...shutdownDiagnostics.snapshot(), profile }, null, 2) + "\n", { flag:"wx" });
    } catch (error) {
      console.error(`Shutdown diagnostics could not be saved (${error.code ?? "unknown"})`);
    }
  }
  // Keep the isolated profile for diagnosis; never touch a daily profile.
}
