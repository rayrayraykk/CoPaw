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
const profile = await mkdtemp(path.join(tmpdir(), "qwenpaw-plugin-market-browser-"));
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
    if (new URL(response.url).pathname.startsWith("/api/plugins"))
      requests.push({path: new URL(response.url).pathname, query: new URL(response.url).search, status: response.status});
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
    let point = await wait(probe);
    await page.send("Input.dispatchMouseEvent", {type:"mouseMoved",...point});
    point = await wait(probe);
    for (const type of ["mousePressed", "mouseReleased"])
      await page.send("Input.dispatchMouseEvent", {type,button:"left",clickCount:1,...point});
  }


  const waitState = expression => wait("Boolean(" + expression + ")");
  const card = name => "document.querySelector('article[aria-label=" + JSON.stringify(name) + "]')";
  async function response(params, from, status=200) {
    const deadline=Date.now()+20000;
    while(Date.now()<deadline) {
      const found=requests.slice(from).find(r=>{
        if(r.path!=="/api/plugins/market/search" || r.status!==status) return false;
        const query=new URLSearchParams(r.query);
        return Object.entries(params).every(([key,value])=>value===null ? !query.has(key) : query.get(key)===value);
      });
      if(found) return found;
      await pause(100);
    }
    assert.fail("Market response missing: "+JSON.stringify({params,from,status,requests}));
  }
  async function search(value) {
    await click('input[placeholder="Search plugins..."]');
    const modifiers=process.platform==="darwin"?4:2;
    await page.send("Input.dispatchKeyEvent",{type:"keyDown",key:"a",code:"KeyA",modifiers,windowsVirtualKeyCode:65,commands:["selectAll"]});
    await page.send("Input.dispatchKeyEvent",{type:"keyUp",key:"a",code:"KeyA",modifiers,windowsVirtualKeyCode:65});
    for(const type of ["keyDown","keyUp"])
      await page.send("Input.dispatchKeyEvent",{type,key:"Backspace",code:"Backspace",windowsVirtualKeyCode:8});
    if(value) await page.send("Input.insertText",{text:value});
    await waitState("document.querySelector('input[placeholder=\"Search plugins...\"]').value === "+JSON.stringify(value));
    for(const type of ["keyDown","keyUp"])
      await page.send("Input.dispatchKeyEvent",{type,key:"Enter",code:"Enter",windowsVirtualKeyCode:13});
  }
  async function openMarket() {
    await click('[role="tab"]',"Plugin Market");
    await waitState(card("First Plugin")+" && "+card("Second Plugin"));
  }
  await page.send("Page.navigate",{url:new URL("/market?tab=plugins",base).href});
  await openMarket();
  await response({page_number:"2"},0);
  let start=requests.length;
  await search("Needle");
  await response({search:"Needle",page_number:"1"},start);
  await waitState(card("Needle Plugin")+" && !"+card("First Plugin"));
  start=requests.length;
  await click("button","Agent Tool");
  await response({category:"agent-tool",search:"Needle"},start);
  await waitState(card("Fixture Plugin"));
  start=requests.length;
  await click("button","Featured");
  await response({category:null,is_featured:"true"},start);
  start=requests.length;
  await click("button","Trending");
  await response({is_featured:null,is_trending:"true"},start);
  start=requests.length;
  await click('[aria-label="Sort plugins"]');
  await click(".qwenpaw-select-item-option-content","Recently updated");
  await response({sort_by:"updated_time"},start);
  await waitState(card("Fixture Plugin"));
  await click('span[aria-label="List view"]');
  await waitState("document.querySelector('[class*=\"catalogRow\"]') && !"+card("Fixture Plugin"));
  await click('span[aria-label="Grid view"]');
  await waitState(card("Fixture Plugin"));
  start=requests.length;
  await click('button[aria-label="Refresh"]');
  await response({sort_by:"updated_time"},start);
  await waitState(card("Fixture Plugin"));
  await click("button","All");
  await click('[aria-label="Sort plugins"]');
  await click(".qwenpaw-select-item-option-content","Most downloaded");
  start=requests.length;
  await search("fail");
  await response({search:"fail",sort_by:"downloads"},start,502);
  await waitState('document.body.innerText.includes("Plugin Market is currently under maintenance, please try again later")');
  start=requests.length;
  await search("Needle");
  await response({search:"Needle"},start);
  await waitState(card("Needle Plugin")+' && !document.body.innerText.includes("Plugin Market is currently under maintenance")');
  const before=await evaluate("performance.timeOrigin");
  await page.send("Page.reload");
  await waitState("performance.timeOrigin !== "+before);
  await openMarket();
  assert.deepEqual(exceptions,[]);
  assert(requests.every(r=>r.status===200||r.path==="/api/plugins/market/search"&&r.status===502));
  assert(requests.some(r=>r.status===502));
  await closeBrowser(browser,child);
  console.log(JSON.stringify({ok:true,pagination:true,search:true,filters:true,sort:true,views:true,refresh:true,recovery:true,reload:true,requests,profile}));

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
