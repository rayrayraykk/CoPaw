#!/usr/bin/env node

import { access, mkdtemp, rm } from "node:fs/promises";
import { createServer } from "node:http";
import os from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";

const WAIT_AFTER_LOAD_MS = 8_000;
const NAVIGATION_WAIT_AFTER_LOAD_MS = 2_500;
const START_TIMEOUT_MS = 15_000;
const ALL_NAVIGATION_PATHS = [
  "/chat",
  "/files",
  "/inbox",
  "/market",
  "/channels",
  "/sessions",
  "/cron-jobs",
  "/heartbeat",
  "/skills",
  "/tools",
  "/mcp",
  "/acp",
  "/agent-config",
  "/agent-stats",
  "/checkpoints",
  "/agents",
  "/models",
  "/environments",
  "/offload-policy",
  "/security",
  "/token-usage",
  "/voice-transcription",
  "/debug",
  "/backups",
];

function delay(milliseconds) {
  return new Promise((resolve) => setTimeout(resolve, milliseconds));
}

async function waitForExit(child, timeoutMs = 5_000) {
  if (child.exitCode !== null) return;
  await Promise.race([
    new Promise((resolve) => child.once("exit", resolve)),
    delay(timeoutMs),
  ]);
}

async function firstExecutable(candidates) {
  for (const candidate of candidates) {
    if (!candidate) continue;
    try {
      await access(candidate);
      return candidate;
    } catch {
      // Try the next platform path.
    }
  }
  throw new Error(
    "Chrome was not found; set QWENPAW_CHROME to a Chrome/Chromium executable",
  );
}

async function chromeExecutable() {
  const configured = process.env.QWENPAW_CHROME;
  if (process.platform === "darwin") {
    return firstExecutable([
      configured,
      "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
      "/Applications/Chromium.app/Contents/MacOS/Chromium",
    ]);
  }
  if (process.platform === "win32") {
    return firstExecutable([
      configured,
      process.env.PROGRAMFILES &&
        path.join(
          process.env.PROGRAMFILES,
          "Google/Chrome/Application/chrome.exe",
        ),
      process.env["PROGRAMFILES(X86)"] &&
        path.join(
          process.env["PROGRAMFILES(X86)"],
          "Google/Chrome/Application/chrome.exe",
        ),
    ]);
  }
  return firstExecutable([
    configured,
    "/usr/bin/google-chrome",
    "/usr/bin/google-chrome-stable",
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
  ]);
}

function validateBaseUrl(value) {
  const parsed = new URL(value);
  const loopback =
    parsed.hostname === "localhost" ||
    parsed.hostname === "127.0.0.1" ||
    parsed.hostname === "[::1]";
  if (parsed.protocol !== "http:" || !loopback) {
    throw new Error("Console smoke URL must be a loopback HTTP origin");
  }
  return parsed.origin;
}

function navigationPaths(arguments_) {
  if (arguments_.length === 0) return ["/"];
  if (arguments_.length === 1 && arguments_[0] === "--all") {
    return ALL_NAVIGATION_PATHS;
  }
  return arguments_.map((value) => {
    const parsed = new URL(value, "http://localhost");
    if (
      parsed.origin !== "http://localhost" ||
      !parsed.pathname.startsWith("/")
    ) {
      throw new Error(`Console smoke path must be relative: ${value}`);
    }
    return `${parsed.pathname}${parsed.search}`;
  });
}

function smokeOptions(arguments_) {
  const codingGit = arguments_.includes("--coding-git");
  const mcpCrud = arguments_.includes("--mcp-crud");
  const securityCrud = arguments_.includes("--security-crud");
  const skillsCrud = arguments_.includes("--skills-crud");
  const agentsCrud = arguments_.includes("--agents-crud");
  const acpCrud = arguments_.includes("--acp-crud");
  const modelsCrud = arguments_.includes("--models-crud");
  const navigationArguments = arguments_.filter(
    (argument) =>
      argument !== "--coding-git" &&
      argument !== "--mcp-crud" &&
      argument !== "--security-crud" &&
      argument !== "--skills-crud" &&
      argument !== "--agents-crud" &&
      argument !== "--acp-crud" &&
      argument !== "--models-crud",
  );
  return {
    codingGit,
    mcpCrud,
    securityCrud,
    skillsCrud,
    agentsCrud,
    acpCrud,
    modelsCrud,
    paths: navigationPaths(navigationArguments),
  };
}

async function selectAgentFromSidebar(client, agentId) {
  const opened = await evaluateValue(
    client,
    `(() => {
      const selector = document.querySelector(
        '[class*="agentSelectorWrapper"] [class*="-select-selector"]'
      );
      selector?.dispatchEvent(new MouseEvent("mousedown", {
        bubbles: true,
        cancelable: true
      }));
      selector?.click();
      return Boolean(selector);
    })()`,
  );
  if (!opened) throw new Error("Agent selector did not render");
  await waitForValue(
    client,
    `[...document.querySelectorAll('[class*="-select-item-option"]')].some(
      (item) => (item.innerText ?? "").includes("ID: ${agentId}")
    )`,
    `Agent selector option did not render: ${agentId}`,
  );
  const selected = await evaluateValue(
    client,
    `(() => {
      const option = [...document.querySelectorAll(
        '[class*="-select-item-option"]'
      )].find((item) =>
        (item.innerText ?? "").includes("ID: ${agentId}")
      );
      option?.dispatchEvent(new MouseEvent("mousedown", {
        bubbles: true,
        cancelable: true
      }));
      option?.click();
      return Boolean(option);
    })()`,
  );
  if (!selected) throw new Error(`Could not select Agent: ${agentId}`);
  await waitForValue(
    client,
    `localStorage.getItem("qwenpaw-last-used-agent") === ${JSON.stringify(
      agentId,
    )}`,
    `Selected Agent was not persisted: ${agentId}`,
  );
}

async function clickAgentRowAction(client, agentId, actionIndex) {
  const clicked = await evaluateValue(
    client,
    `(() => {
      const row = document.querySelector(
        'tr[data-row-key=${JSON.stringify(agentId)}]'
      );
      const actionCell = row?.querySelector("td:last-child");
      const buttons = [...(actionCell?.querySelectorAll("button") ?? [])];
      const button = buttons[${actionIndex}];
      button?.click();
      return Boolean(button && !button.disabled);
    })()`,
  );
  if (!clicked) {
    throw new Error(
      `Agent row action ${actionIndex} did not render: ${agentId}`,
    );
  }
}

async function runAgentsCrudScenario(client) {
  const agentId = "browser-agent";
  const initialAgentName = "Browser Agent";
  const updatedAgentName = "Browser Agent Updated";
  const copiedAgentName = `${updatedAgentName} Copy`;
  await waitForValue(
    client,
    `document.body.innerText.includes("Create Agent")`,
    "Agents management page did not render",
  );
  const existing = await evaluateValue(
    client,
    `fetch("/api/agents").then((response) => response.json()).then(
      (value) => value.agents.some((agent) => agent.id === ${JSON.stringify(
        agentId,
      )})
    )`,
  );
  if (!existing) {
    await clickButton(client, "Create Agent");
    await waitForValue(
      client,
      `document.body.innerText.includes("Create New Agent")`,
      "Create Agent modal did not open",
    );
    await setInputByPlaceholder(client, "e.g.: my-agent", agentId);
    await setInputByPlaceholder(client, "e.g.: My Agent", initialAgentName);
    await clickButton(client, "Save");
    await waitForValue(
      client,
      `fetch("/api/agents").then((response) => response.json()).then(
        (value) => value.agents.some((agent) =>
          agent.id === ${JSON.stringify(agentId)} &&
          agent.name === ${JSON.stringify(initialAgentName)}
        )
      )`,
      "Agent created through the Console did not persist",
    );
  }
  await waitForValue(
    client,
    `Boolean(document.querySelector(
      'tr[data-row-key=${JSON.stringify(agentId)}]'
    ))`,
    "Created Agent row did not render",
  );

  await clickAgentRowAction(client, agentId, 0);
  await waitForValue(
    client,
    `fetch("/api/agents").then((response) => response.json()).then(
      (value) => value.agents.some((agent) =>
        agent.id === ${JSON.stringify(agentId)} && agent.pinned === true
      )
    )`,
    "Agent pin action did not persist",
  );

  await clickAgentRowAction(client, agentId, 1);
  await waitForValue(
    client,
    `document.body.innerText.includes("Edit Agent -")`,
    "Edit Agent modal did not open",
  );
  await setInputByPlaceholder(client, "e.g.: My Agent", updatedAgentName);
  await clickButton(client, "Save");
  await waitForValue(
    client,
    `fetch("/api/agents").then((response) => response.json()).then(
      (value) => value.agents.some((agent) =>
        agent.id === ${JSON.stringify(agentId)} &&
        agent.name === ${JSON.stringify(updatedAgentName)}
      )
    )`,
    "Agent edit action did not persist",
  );

  const isolatedFiles = await evaluateValue(
    client,
    `fetch("/api/workspace/files/PROFILE.md", {
        method: "PUT",
        headers: {
          "Content-Type": "application/json",
          "X-Agent-Id": ${JSON.stringify(agentId)}
        },
        body: JSON.stringify({content: "Browser Agent profile"})
      }).then(async (saved) => {
        const [agentFile, defaultFile] = await Promise.all([
          fetch("/api/workspace/files/PROFILE.md", {
            headers: {"X-Agent-Id": ${JSON.stringify(agentId)}}
          }).then((response) => response.json()),
          fetch("/api/workspace/files/PROFILE.md", {
            headers: {"X-Agent-Id": "default"}
          }).then((response) => response.json())
        ]);
        return {
          saved: saved.ok,
          agentContent: agentFile.content,
          defaultContent: defaultFile.content
        };
      })`,
  );
  if (
    !isolatedFiles.saved ||
    isolatedFiles.agentContent !== "Browser Agent profile" ||
    isolatedFiles.defaultContent === isolatedFiles.agentContent
  ) {
    throw new Error("Agent workspace files were not isolated");
  }

  await clickAgentRowAction(client, agentId, 2);
  await waitForValue(
    client,
    `document.body.innerText.includes("Copy Agent Configuration")`,
    "Copy Agent modal did not open",
  );
  await clickButton(client, "Confirm");
  await waitForValue(
    client,
    `fetch("/api/agents").then((response) => response.json()).then(
      (value) => value.agents.some((agent) =>
        agent.name === ${JSON.stringify(copiedAgentName)}
      )
    )`,
    "Agent copy action did not persist",
  );
  const copiedAgentId = await evaluateValue(
    client,
    `fetch("/api/agents").then((response) => response.json()).then(
      (value) => value.agents.find((agent) =>
        agent.name === ${JSON.stringify(copiedAgentName)}
      )?.id
    )`,
  );
  const copiedProfile = await evaluateValue(
    client,
    `fetch("/api/workspace/files/PROFILE.md", {
      headers: {"X-Agent-Id": ${JSON.stringify(copiedAgentId)}}
    }).then((response) => response.json()).then((value) => value.content)`,
  );
  if (copiedProfile !== "Browser Agent profile") {
    throw new Error("Agent copy did not preserve selected Markdown files");
  }

  await selectAgentFromSidebar(client, agentId);
  const selectedLabel = await evaluateValue(
    client,
    `document.querySelector(
      '[class*="agentSelectorWrapper"] [class*="-select-selection-item"]'
    )?.innerText ?? ""`,
  );
  if (!selectedLabel.includes(updatedAgentName)) {
    throw new Error("Original Agent selector did not show the selected Agent");
  }

  await clickAgentRowAction(client, agentId, 3);
  await clickButton(client, "Confirm");
  await waitForValue(
    client,
    `fetch("/api/agents").then((response) => response.json()).then(
      (value) => value.agents.some((agent) =>
        agent.id === ${JSON.stringify(agentId)} && agent.enabled === false
      )
    )`,
    "Agent disable action did not persist",
  );
  await waitForValue(
    client,
    `localStorage.getItem("qwenpaw-last-used-agent") === "default"`,
    "Disabling the selected Agent did not switch the original UI to default",
  );
  await clickAgentRowAction(client, agentId, 3);
  await clickButton(client, "Confirm");
  await waitForValue(
    client,
    `fetch("/api/agents").then((response) => response.json()).then(
      (value) => value.agents.some((agent) =>
        agent.id === ${JSON.stringify(agentId)} && agent.enabled === true
      )
    )`,
    "Agent enable action did not persist",
  );
  await selectAgentFromSidebar(client, agentId);

  await clickAgentRowAction(client, copiedAgentId, 4);
  await clickButton(client, "Confirm");
  await waitForValue(
    client,
    `fetch("/api/agents").then((response) => response.json()).then(
      (value) => !value.agents.some((agent) =>
        agent.id === ${JSON.stringify(copiedAgentId)}
      )
    )`,
    "Agent delete action did not persist",
  );

  return {
    createdThroughOriginalModal: true,
    editedThroughOriginalModal: true,
    copiedThroughOriginalModal: true,
    pinnedThroughOriginalTable: true,
    toggledThroughOriginalTable: true,
    deletedThroughOriginalTable: true,
    selectedThroughOriginalSidebar: true,
    selectedAgent: agentId,
    workspaceFilesIsolated: true,
  };
}

async function clickAcpCard(client, agentKey) {
  const clicked = await evaluateValue(
    client,
    `(() => {
      const title = [...document.querySelectorAll('[class*="cardTitle"]')].find(
        (item) => (item.innerText ?? "").trim() === ${JSON.stringify(agentKey)}
      );
      title?.click();
      return Boolean(title);
    })()`,
  );
  if (!clicked) throw new Error(`ACP card did not render: ${agentKey}`);
}

async function runAcpCrudScenario(client) {
  const agentKey = "browser_acp";
  const initialCommand = "browser-acp";
  const updatedCommand = "browser-acp-updated";
  await waitForValue(
    client,
    `document.body.innerText.includes("codex") &&
      document.body.innerText.includes("Add Custom Agent")`,
    "ACP page and built-in cards did not render",
  );
  const selectedAgent =
    (await evaluateValue(
      client,
      `localStorage.getItem("qwenpaw-last-used-agent") ?? "default"`,
    )) || "default";
  const requestHeaders = JSON.stringify({ "X-Agent-Id": selectedAgent });
  const existing = await evaluateValue(
    client,
    `fetch("/api/config/acp", {headers: ${requestHeaders}})
      .then((response) => response.json())
      .then((value) => Boolean(value.agents?.[${JSON.stringify(agentKey)}]))`,
  );
  if (!existing) {
    await clickButton(client, "Add Custom Agent");
    await waitForValue(
      client,
      `document.body.innerText.includes("Create ACP Agent") &&
        Boolean(document.querySelector('input[placeholder="my_custom_runner"]'))`,
      "ACP create drawer did not open",
    );
    await setInputByPlaceholder(client, "my_custom_runner", agentKey);
    await setInputByPlaceholder(client, "qwen", initialCommand);
    await clickButton(client, "Save");
    await waitForValue(
      client,
      `fetch("/api/config/acp", {headers: ${requestHeaders}})
        .then((response) => response.json())
        .then((value) =>
          value.agents?.[${JSON.stringify(agentKey)}]?.command ===
            ${JSON.stringify(initialCommand)}
        )`,
      "ACP Agent created through the original drawer did not persist",
    );
  }
  await waitForValue(
    client,
    `[...document.querySelectorAll('[class*="cardTitle"]')].some(
      (item) => (item.innerText ?? "").trim() === ${JSON.stringify(agentKey)}
    )`,
    "Created ACP Agent card did not render",
  );

  let agentIsolationVerified = selectedAgent === "default";
  if (selectedAgent !== "default") {
    await selectAgentFromSidebar(client, "default");
    await waitForValue(
      client,
      `fetch("/api/config/acp", {headers: {"X-Agent-Id": "default"}})
        .then((response) => response.json())
        .then((value) => !value.agents?.[${JSON.stringify(agentKey)}])`,
      "Default Agent unexpectedly shared the custom ACP Agent",
    );
    await selectAgentFromSidebar(client, selectedAgent);
    await waitForValue(
      client,
      `[...document.querySelectorAll('[class*="cardTitle"]')].some(
        (item) => (item.innerText ?? "").trim() === ${JSON.stringify(agentKey)}
      )`,
      "Custom ACP Agent did not return after switching Agents",
    );
    agentIsolationVerified = true;
  }

  await clickAcpCard(client, agentKey);
  await waitForValue(
    client,
    `document.body.innerText.includes("Edit ACP Configuration: ${agentKey}")`,
    "ACP edit drawer did not open",
  );
  await setInputByPlaceholder(client, "qwen", updatedCommand);
  await clickButton(client, "Save");
  await waitForValue(
    client,
    `fetch("/api/config/acp/${agentKey}", {headers: ${requestHeaders}})
      .then((response) => response.json())
      .then((value) => value.command === ${JSON.stringify(updatedCommand)})`,
    "ACP Agent edit through the original drawer did not persist",
  );

  await clickButton(client, "Node Settings");
  await waitForValue(
    client,
    `document.body.innerText.includes("Node path") &&
      [...document.querySelectorAll('[role="combobox"]')].some(
        (element) => element.offsetWidth || element.offsetHeight ||
          element.getClientRects().length
      )`,
    "ACP Node runtime modal did not render",
  );
  const effectiveNodePath = await evaluateValue(
    client,
    `fetch("/api/config/acp/node-runtime")
      .then((response) => response.json())
      .then((value) => value.effective_node_path)`,
  );
  let nodeSavedThroughOriginalModal = false;
  if (effectiveNodePath) {
    await evaluateValue(
      client,
      `(() => {
        window.prompt = () => ${JSON.stringify(effectiveNodePath)};
        const visible = (element) => Boolean(
          element.offsetWidth || element.offsetHeight || element.getClientRects().length
        );
        const select = [...document.querySelectorAll('[role="combobox"]')]
          .filter(visible).at(-1);
        const selector = select?.closest('[class*="-select"]')?.querySelector(
          '[class*="-select-selector"]'
        ) ?? select;
        selector?.dispatchEvent(new MouseEvent("mousedown", {
          bubbles: true,
          cancelable: true,
          view: window
        }));
        selector?.click();
        return Boolean(selector);
      })()`,
    );
    await waitForValue(
      client,
      `[...document.querySelectorAll('[class*="-select-item-option"]')].some(
        (item) => (item.innerText ?? "").includes("Choose another Node...")
      )`,
      "ACP custom Node option did not open",
    );
    const selected = await evaluateValue(
      client,
      `(() => {
        const option = [...document.querySelectorAll(
          '[class*="-select-item-option"]'
        )].find((item) =>
          (item.innerText ?? "").includes("Choose another Node...")
        );
        option?.dispatchEvent(new MouseEvent("mousedown", {
          bubbles: true,
          cancelable: true,
          view: window
        }));
        option?.click();
        return Boolean(option);
      })()`,
    );
    if (!selected) throw new Error("ACP custom Node option was not selectable");
    await waitForValue(
      client,
      `fetch("/api/config/acp/node-runtime")
        .then((response) => response.json())
        .then((value) => value.node_path === ${JSON.stringify(
          effectiveNodePath,
        )})`,
      "Node path selected through the original modal did not persist",
    );
    nodeSavedThroughOriginalModal = true;
  }
  await client.send("Input.dispatchKeyEvent", {
    type: "keyDown",
    key: "Escape",
    code: "Escape",
  });
  await client.send("Input.dispatchKeyEvent", {
    type: "keyUp",
    key: "Escape",
    code: "Escape",
  });
  await delay(300);

  await clickAcpCard(client, agentKey);
  await clickButton(client, "Delete");
  await waitForValue(
    client,
    `document.body.innerText.includes("Delete ${agentKey}")`,
    "ACP delete confirmation did not open",
  );
  await clickButton(client, "Delete");
  await waitForValue(
    client,
    `fetch("/api/config/acp", {headers: ${requestHeaders}})
      .then((response) => response.json())
      .then((value) => !value.agents?.[${JSON.stringify(agentKey)}])`,
    "ACP Agent deletion through the original drawer did not persist",
  );

  return {
    builtinsRendered: true,
    createdThroughOriginalDrawer: true,
    editedThroughOriginalDrawer: true,
    deletedThroughOriginalDrawer: true,
    agentIsolationVerified,
    nodeRuntimeDetected: Boolean(effectiveNodePath),
    nodeSavedThroughOriginalModal,
  };
}

async function startModelApiMock() {
  const requests = [];
  let discoveryEnabled = false;
  const server = createServer((request, response) => {
    const chunks = [];
    request.on("data", (chunk) => chunks.push(chunk));
    request.on("end", () => {
      if (request.method === "GET" && request.url?.startsWith("/v1/models")) {
        requests.push({
          method: request.method,
          path: request.url,
          body: null,
        });
        response.writeHead(200, { "Content-Type": "application/json" });
        response.end(
          JSON.stringify({
            data: discoveryEnabled
              ? [
                  {
                    id: "browser/discovered",
                    name: "Browser Discovered",
                    context_length: 131072,
                    max_output_tokens: 4096,
                  },
                ]
              : [],
            has_more: false,
          }),
        );
        return;
      }
      let body;
      try {
        body = JSON.parse(Buffer.concat(chunks).toString("utf8"));
      } catch {
        response.writeHead(400, { "Content-Type": "application/json" });
        response.end(JSON.stringify({ error: "invalid JSON" }));
        return;
      }
      requests.push({ method: request.method, path: request.url, body });
      const encoded = JSON.stringify(body);
      const content = encoded.includes("image_url")
        ? "red"
        : encoded.includes("video_url")
        ? "blue"
        : "pong";
      response.writeHead(200, { "Content-Type": "application/json" });
      response.end(
        JSON.stringify({
          id: "browser-model-response",
          choices: [{ message: { role: "assistant", content } }],
        }),
      );
    });
  });
  await new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  if (!address || typeof address === "string") {
    server.close();
    throw new Error("Model API mock did not expose a TCP address");
  }
  return {
    baseUrl: `http://127.0.0.1:${address.port}/v1`,
    requests,
    enableDiscovery: () => {
      discoveryEnabled = true;
    },
    close: () => new Promise((resolve) => server.close(resolve)),
  };
}

async function clickModelsTab(client, prefix) {
  const clicked = await evaluateValue(
    client,
    `(() => {
      const visible = (element) => Boolean(
        element.offsetWidth || element.offsetHeight || element.getClientRects().length
      );
      const tab = [...document.querySelectorAll("div")]
        .filter((item) => {
          const text = (item.innerText ?? "").trim();
          return visible(item) && text.includes(${JSON.stringify(prefix)});
        })
        .sort((left, right) =>
          (left.innerText ?? "").length - (right.innerText ?? "").length
        )[0];
      tab?.click();
      return Boolean(tab);
    })()`,
  );
  if (!clicked) throw new Error(`Models tab did not render: ${prefix}`);
}

async function clickProviderCardAction(client, providerName, action) {
  const clicked = await evaluateValue(
    client,
    `(() => {
      const visible = (element) => Boolean(
        element.offsetWidth || element.offsetHeight || element.getClientRects().length
      );
      const name = [...document.querySelectorAll('[class*="groupCardName"]')].find(
        (item) => visible(item) &&
          (item.innerText ?? "").trim() === ${JSON.stringify(providerName)}
      );
      const card = name?.closest('[class*="groupCardGlass"]');
      const button = [...(card?.querySelectorAll("button") ?? [])].find(
        (item) => visible(item) &&
          (item.innerText ?? "").trim() === ${JSON.stringify(action)}
      );
      button?.click();
      return Boolean(button);
    })()`,
  );
  if (!clicked) {
    throw new Error(
      `Provider action did not render: ${providerName} / ${action}`,
    );
  }
}

async function clickModelRowAction(client, modelId, ariaLabel) {
  const clicked = await evaluateValue(
    client,
    `(() => {
      const visible = (element) => Boolean(
        element.offsetWidth || element.offsetHeight || element.getClientRects().length
      );
      const id = [...document.querySelectorAll('[class*="modelListItemId"]')].find(
        (item) => visible(item) &&
          (item.innerText ?? "").trim() === ${JSON.stringify(modelId)}
      );
      let row = id;
      while (row && ![...row.querySelectorAll("button")].some(
        (item) => item.getAttribute("aria-label") === ${JSON.stringify(
          ariaLabel,
        )}
      )) {
        row = row.parentElement;
      }
      const button = [...(row?.querySelectorAll("button") ?? [])].find(
        (item) => visible(item) &&
          item.getAttribute("aria-label") === ${JSON.stringify(ariaLabel)}
      );
      button?.click();
      return Boolean(button);
    })()`,
  );
  if (!clicked) {
    throw new Error(`Model action did not render: ${modelId} / ${ariaLabel}`);
  }
}

async function setFirstVisibleSpinButton(client, value) {
  const focused = await evaluateValue(
    client,
    `(() => {
      const visible = (element) => Boolean(
        element.offsetWidth || element.offsetHeight || element.getClientRects().length
      );
      const input = [...document.querySelectorAll('input[role="spinbutton"]')]
        .find(visible);
      input?.focus();
      input?.select();
      return Boolean(input);
    })()`,
  );
  if (!focused) throw new Error("Model Config max tokens input did not render");
  await client.send("Input.insertText", { text: String(value) });
}

async function setModelIdInput(client, value) {
  const focused = await evaluateValue(
    client,
    `(() => {
      const visible = (element) => Boolean(
        element.offsetWidth || element.offsetHeight || element.getClientRects().length
      );
      const nameInput = [...document.querySelectorAll("input")].find(
        (input) => visible(input) &&
          input.placeholder === "e.g. GPT-4o, Gemini 2.0 Flash"
      );
      const inputs = [...(nameInput?.closest("form")?.querySelectorAll("input") ?? [])]
        .filter(visible);
      const input = inputs[inputs.indexOf(nameInput) - 1];
      input?.focus();
      input?.select();
      return Boolean(input);
    })()`,
  );
  if (!focused) throw new Error("Model ID input did not render");
  await client.send("Input.insertText", { text: value });
}

async function runModelsCrudScenario(client) {
  const providerId = "browser-provider";
  const providerName = "Browser Provider";
  const modelId = "browser/model-1";
  const modelName = "Browser Model One";
  const mock = await startModelApiMock();
  try {
    await waitForValue(
      client,
      `document.body.innerText.includes("Add Provider") &&
        document.body.innerText.includes("Cloud Providers")`,
      "Models page did not render",
    );
    const existing = await evaluateValue(
      client,
      `fetch("/api/models").then((response) => response.json()).then(
        (providers) => providers.some((provider) =>
          provider.id === ${JSON.stringify(providerId)}
        )
      )`,
    );
    if (existing) {
      const removed = await evaluateValue(
        client,
        `fetch("/api/models/custom-providers/${providerId}", {
          method: "DELETE"
        }).then((response) => response.ok)`,
      );
      if (!removed) throw new Error("Models smoke cleanup failed");
    }

    await clickModelsTab(client, "Local & Custom");
    await clickButton(client, "Add Provider");
    await waitForValue(
      client,
      `document.body.innerText.includes("Add Custom Provider")`,
      "Custom Provider modal did not open",
    );
    await setInputByPlaceholder(
      client,
      "e.g. openai, google, anthropic",
      providerId,
    );
    await setInputByPlaceholder(
      client,
      "e.g. OpenAI, Google Gemini",
      providerName,
    );
    await setInputByPlaceholder(
      client,
      "e.g. https://api.example.com",
      mock.baseUrl,
    );
    await clickButton(client, "Create");
    await waitForValue(
      client,
      `fetch("/api/models").then((response) => response.json()).then(
        (providers) => providers.some((provider) =>
          provider.id === ${JSON.stringify(providerId)} &&
          provider.name === ${JSON.stringify(providerName)} &&
          provider.base_url === ${JSON.stringify(mock.baseUrl)}
        )
      )`,
      "Provider created through the original modal did not persist",
    );

    await client.send("Page.reload");
    await delay(1_500);
    await clickModelsTab(client, "Local & Custom");

    await waitForValue(
      client,
      `document.body.innerText.includes(${JSON.stringify(providerName)})`,
      "Custom Provider card did not render",
    );
    await clickProviderCardAction(client, providerName, "Models");
    await waitForValue(
      client,
      `document.body.innerText.includes(${JSON.stringify(
        `${providerName} — Model Management`,
      )})`,
      "Model Management modal did not open",
    );
    await clickButton(client, "Add Model");
    await delay(300);
    const addModelForm = await evaluateValue(
      client,
      `(() => {
        const visible = (element) => Boolean(
          element.offsetWidth || element.offsetHeight || element.getClientRects().length
        );
        return {
          rendered: [...document.querySelectorAll("input")].some(
            (input) => visible(input) &&
              input.placeholder === "e.g. GPT-4o, Gemini 2.0 Flash"
          ),
          inputs: [...document.querySelectorAll("input")].filter(visible).map(
            (input) => input.placeholder
          ),
          buttons: [...document.querySelectorAll("button")].filter(visible).map(
            (button) => (button.innerText ?? "").trim()
          )
        };
      })()`,
    );
    if (!addModelForm.rendered) {
      throw new Error(
        `Add Model form did not render: ${JSON.stringify(addModelForm)}`,
      );
    }
    await setModelIdInput(client, modelId);
    await setInputByPlaceholder(
      client,
      "e.g. GPT-4o, Gemini 2.0 Flash",
      modelName,
    );
    await clickButton(client, "Add Model");
    await waitForValue(
      client,
      `fetch("/api/models").then((response) => response.json()).then(
        (providers) => providers.find((provider) =>
          provider.id === ${JSON.stringify(providerId)}
        )?.extra_models.some((model) =>
          model.id === ${JSON.stringify(modelId)} &&
          model.name === ${JSON.stringify(modelName)}
        )
      )`,
      "Model tested and added through the original modal did not persist",
    );
    const liveProbe = mock.requests.find(
      (request) => request.method === "POST" && request.body?.model === modelId,
    );
    if (!liveProbe || liveProbe.path !== "/v1/chat/completions") {
      throw new Error(
        `Original Add Model flow did not issue the expected live probe: ${JSON.stringify(
          mock.requests,
        )}`,
      );
    }

    await clickModelRowAction(client, modelId, "Model Config");
    await setFirstVisibleSpinButton(client, 3072);
    await clickButton(client, "Save");
    await waitForValue(
      client,
      `fetch("/api/models").then((response) => response.json()).then(
        (providers) => providers.find((provider) =>
          provider.id === ${JSON.stringify(providerId)}
        )?.extra_models.find((model) =>
          model.id === ${JSON.stringify(modelId)}
        )?.generate_kwargs.max_tokens === 3072
      )`,
      "Model configuration saved through the original editor did not persist",
    );

    await clickModelRowAction(client, modelId, "Remove");
    await waitForValue(
      client,
      `document.body.innerText.includes(${JSON.stringify(
        `Remove model "${modelName}" from ${providerName}?`,
      )})`,
      "Model removal confirmation did not open",
    );
    await clickButton(client, "Delete");
    await waitForValue(
      client,
      `fetch("/api/models").then((response) => response.json()).then(
        (providers) => !providers.find((provider) =>
          provider.id === ${JSON.stringify(providerId)}
        )?.extra_models.some((model) => model.id === ${JSON.stringify(modelId)})
      )`,
      "Model removal through the original modal did not persist",
    );

    mock.enableDiscovery();
    await clickButton(client, "Auto Discover Models");
    await waitForValue(
      client,
      `fetch("/api/models").then((response) => response.json()).then(
        (providers) => providers.find((provider) =>
          provider.id === ${JSON.stringify(providerId)}
        )?.discovered_models.some((model) =>
          model.id === "browser/discovered" &&
          model.max_input_length === 131072
        )
      )`,
      "Remote model discovery through the original modal did not persist",
    );
    await clickButton(client, "Add Model");
    await setModelIdInput(client, "browser/discovered");
    await setInputByPlaceholder(
      client,
      "e.g. GPT-4o, Gemini 2.0 Flash",
      "Browser Discovered",
    );
    await clickButton(client, "Add Model");
    await waitForValue(
      client,
      `fetch("/api/models").then((response) => response.json()).then(
        (providers) => providers.find((provider) =>
          provider.id === ${JSON.stringify(providerId)}
        )?.extra_models.some((model) => model.id === "browser/discovered")
      )`,
      "Discovered candidate added through the original modal did not persist",
    );
    await clickModelRowAction(client, "browser/discovered", "Test Multimodal");
    await waitForValue(
      client,
      `fetch("/api/models").then((response) => response.json()).then(
        (providers) => providers.find((provider) =>
          provider.id === ${JSON.stringify(providerId)}
        )?.extra_models.find((model) =>
          model.id === "browser/discovered"
        )?.supports_image === true &&
        providers.find((provider) =>
          provider.id === ${JSON.stringify(providerId)}
        )?.extra_models.find((model) =>
          model.id === "browser/discovered"
        )?.supports_video === true
      )`,
      "Multimodal probe through the original modal did not persist",
    );

    await client.send("Input.dispatchKeyEvent", {
      type: "keyDown",
      key: "Escape",
      code: "Escape",
    });
    await client.send("Input.dispatchKeyEvent", {
      type: "keyUp",
      key: "Escape",
      code: "Escape",
    });
    await evaluateValue(client, `location.assign("/chat")`);
    await waitForValue(
      client,
      `Boolean(document.querySelector('button[aria-label="Select model"]'))`,
      "Chat Model Selector did not render",
    );
    const selectorOpened = await evaluateValue(
      client,
      `(() => {
        const button = document.querySelector('button[aria-label="Select model"]');
        button?.click();
        return Boolean(button);
      })()`,
    );
    if (!selectorOpened) throw new Error("Chat Model Selector did not open");
    await setInputByPlaceholder(
      client,
      "Search models...",
      "Browser Discovered",
    );
    await clickButton(client, "Browser Discovered");
    await waitForValue(
      client,
      `fetch("/api/models/active?scope=agent&agent_id=default")
        .then((response) => response.json())
        .then((active) =>
          active.active_llm?.provider_id === ${JSON.stringify(providerId)} &&
          active.active_llm?.model === "browser/discovered"
        )`,
      "Chat Model Selector did not persist the Agent-scoped model",
    );

    await evaluateValue(client, `location.assign("/models")`);
    await waitForValue(
      client,
      `document.body.innerText.includes("Cloud Providers")`,
      "Models page did not render after Chat model selection",
    );
    await clickModelsTab(client, "Local & Custom");
    await delay(300);
    await clickProviderCardAction(client, providerName, "Delete");
    await waitForValue(
      client,
      `document.body.innerText.includes(${JSON.stringify(
        `Delete custom provider "${providerName}" and all its models? This cannot be undone.`,
      )})`,
      "Provider deletion confirmation did not open",
    );
    await clickButton(client, "Delete");
    await waitForValue(
      client,
      `fetch("/api/models").then((response) => response.json()).then(
        (providers) => !providers.some((provider) =>
          provider.id === ${JSON.stringify(providerId)}
        )
      )`,
      "Provider deletion through the original card did not persist",
    );

    return {
      providerCreatedThroughOriginalModal: true,
      modelLiveProbePath: liveProbe.path,
      modelAddedThroughOriginalModal: true,
      modelConfiguredThroughOriginalEditor: true,
      modelRemovedThroughOriginalModal: true,
      modelsDiscoveredThroughOriginalModal: true,
      discoveredCandidateAddedThroughOriginalModal: true,
      multimodalProbePersistedThroughOriginalModal: true,
      agentModelSelectedThroughOriginalChatSelector: true,
      providerDeletedThroughOriginalCard: true,
    };
  } finally {
    await mock.close();
  }
}

async function clickSkillCard(client, name) {
  const clicked = await evaluateValue(
    client,
    `(() => {
      const title = [...document.querySelectorAll("h3")].find(
        (item) => (item.innerText ?? "").trim().startsWith(${JSON.stringify(
          name,
        )})
      );
      title?.click();
      return Boolean(title);
    })()`,
  );
  if (!clicked) throw new Error(`Skills card did not render: ${name}`);
}

async function runSkillsCrudScenario(client, navigationPath) {
  const skillName = "browser_weather";
  const initialContent = `---
name: browser_weather
description: Browser smoke weather
---

Return a concise weather summary.`;
  const editedContent = `${initialContent}\n\nAlways include temperature units.`;
  if (navigationPath === "/skills") {
    await waitForValue(
      client,
      `document.body.innerText.includes("Create First Skill") ||
        document.body.innerText.includes(${JSON.stringify(skillName)})`,
      "Skills empty state did not render",
    );
    const exists = await evaluateValue(
      client,
      `document.body.innerText.includes(${JSON.stringify(skillName)})`,
    );
    if (!exists) {
      await clickButton(client, "Create First Skill");
      await waitForValue(
        client,
        `document.body.innerText.includes("Create Skill")`,
        "Create Skill drawer did not open",
      );
      await setInputByPlaceholder(client, "e.g., weather_query", skillName);
      await setVisibleTextarea(client, initialContent, "");
      await clickButton(client, "Create");
      await waitForValue(
        client,
        `document.body.innerText.includes(${JSON.stringify(skillName)})`,
        "Created Skill card did not appear",
      );
    }
    await clickSkillCard(client, skillName);
    await waitForValue(
      client,
      `document.body.innerText.includes("View Skill: ${skillName}")`,
      "Skill edit drawer did not open",
    );
    await setVisibleTextarea(client, editedContent, "Browser smoke weather");
    await clickButton(client, "Save");
    await waitForValue(
      client,
      `fetch("/api/skills/${skillName}").then((response) => response.json())
        .then((value) => value.content.includes("Always include temperature units."))`,
      "Edited Skill content did not persist",
    );
    const disabled = await evaluateValue(
      client,
      `fetch("/api/skills/${skillName}/disable", {method: "POST"})
        .then((response) => response.ok)`,
    );
    if (!disabled) throw new Error("Skill disable request failed");
    const uploaded = await evaluateValue(
      client,
      `fetch("/api/skills/pool/upload", {
        method: "POST",
        headers: {"Content-Type": "application/json"},
        body: JSON.stringify({
          workspace_id: "default",
          skill_name: ${JSON.stringify(skillName)},
          overwrite: true
        })
      }).then((response) => response.ok)`,
    );
    if (!uploaded) throw new Error("Skill Pool upload request failed");
    return evaluateValue(
      client,
      `Promise.all([
        fetch("/api/skills/${skillName}").then((response) => response.json()),
        fetch("/api/skills/pool").then((response) => response.json())
      ]).then(([skill, pool]) => ({
        created: skill.name === ${JSON.stringify(skillName)},
        edited: skill.content.includes("Always include temperature units."),
        disabled: skill.enabled === false,
        uploaded: pool.some((item) => item.name === ${JSON.stringify(
          skillName,
        )})
      }))`,
    );
  }

  await waitForValue(
    client,
    `document.body.innerText.includes(${JSON.stringify(skillName)})`,
    "Skill Pool item did not render",
  );
  await clickSkillCard(client, skillName);
  await waitForValue(
    client,
    `document.body.innerText.includes("Edit ${skillName}")`,
    "Skill Pool editor did not open",
  );
  return evaluateValue(
    client,
    `Promise.all([
      fetch("/api/skills/pool/builtin-sources").then((response) => response.json()),
      fetch("/api/skills/pool/${skillName}").then((response) => response.json())
    ]).then(([builtins, skill]) => ({
      itemRendered: document.body.innerText.includes(${JSON.stringify(
        skillName,
      )}),
      editorOpened: document.body.innerText.includes("Edit ${skillName}"),
      builtinCount: builtins.length,
      detailName: skill.name
    }))`,
  );
}

async function evaluateValue(client, expression) {
  const evaluation = await client.send("Runtime.evaluate", {
    expression,
    awaitPromise: true,
    returnByValue: true,
  });
  if (evaluation.exceptionDetails) {
    throw new Error(evaluation.exceptionDetails.text);
  }
  return evaluation.result.value;
}

async function waitForValue(client, expression, message, timeoutMs = 10_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await evaluateValue(client, expression)) return;
    await delay(100);
  }
  throw new Error(message);
}

async function clickButton(client, text) {
  const clicked = await evaluateValue(
    client,
    `(() => {
      const visible = (element) => Boolean(
        element.offsetWidth || element.offsetHeight || element.getClientRects().length
      );
      const buttons = [...document.querySelectorAll("button")].filter(visible);
      const button = buttons.findLast(
        (item) => (item.innerText ?? "").trim() === ${JSON.stringify(text)}
      );
      button?.click();
      return Boolean(button);
    })()`,
  );
  if (!clicked) throw new Error(`MCP UI button did not render: ${text}`);
}

async function clickMcpClientButton(client, text) {
  const clicked = await evaluateValue(
    client,
    `(() => {
      const title = [...document.querySelectorAll("h3")].find(
        (item) => item.innerText.trim() === "Browser MCP Updated"
      );
      let card = title;
      while (card && ![...card.querySelectorAll("button")].some(
        (button) => (button.innerText ?? "").trim() === "Delete"
      )) card = card.parentElement;
      const button = [...(card?.querySelectorAll("button") ?? [])].find(
        (item) => (item.innerText ?? "").trim() === ${JSON.stringify(text)}
      );
      button?.click();
      return Boolean(button);
    })()`,
  );
  if (!clicked) throw new Error(`MCP client button did not render: ${text}`);
}

async function setVisibleTextarea(client, value, currentValueIncludes) {
  const focused = await evaluateValue(
    client,
    `(() => {
      const visible = (element) => Boolean(
        element.offsetWidth || element.offsetHeight || element.getClientRects().length
      );
      const textareas = [...document.querySelectorAll("textarea")].filter(visible);
      const textarea = textareas.find(
        (item) => item.value.includes(${JSON.stringify(currentValueIncludes)})
      ) ?? textareas.at(-1);
      if (!textarea) return false;
      textarea.focus();
      textarea.select();
      return true;
    })()`,
  );
  if (!focused) throw new Error("MCP UI textarea did not render");
  await client.send("Input.insertText", { text: value });
}

async function runMcpCrudScenario(client) {
  const testServer = process.env.QWENPAW_MCP_TEST_SERVER ?? "/usr/bin/false";
  const createdConfig = JSON.stringify(
    {
      key: "browser-mcp",
      name: "Browser MCP",
      description: "Created through the unchanged Console",
      enabled: true,
      transport: "stdio",
      command: testServer,
      env: { TOKEN: "browser-mcp-secret" },
    },
    null,
    2,
  );
  await clickButton(client, "Create Client");
  await waitForValue(
    client,
    `document.body.innerText.includes("Supported formats")`,
    "MCP create modal did not open",
  );
  await setVisibleTextarea(client, createdConfig, "mcpServers");
  await delay(200);
  await clickButton(client, "Create");
  await waitForValue(
    client,
    `document.body.innerText.includes("Browser MCP")`,
    "MCP client card did not appear",
  );

  const cardOpened = await evaluateValue(
    client,
    `(() => {
      const title = [...document.querySelectorAll("h3")].find(
        (item) => item.innerText.trim() === "Browser MCP"
      );
      title?.click();
      return Boolean(title);
    })()`,
  );
  if (!cardOpened) throw new Error("MCP client card did not render");
  await waitForValue(
    client,
    `document.body.innerText.includes("Browser MCP - Configuration")`,
    "MCP configuration modal did not open",
  );
  await clickButton(client, "Edit");
  const updatedConfig = await evaluateValue(
    client,
    `(() => {
      const textarea = [...document.querySelectorAll("textarea")].findLast(
        (item) => item.offsetWidth || item.offsetHeight || item.getClientRects().length
      );
      if (!textarea) return "";
      const config = JSON.parse(textarea.value);
      config.name = "Browser MCP Updated";
      config.description = "Edited through the unchanged Console";
      return JSON.stringify(config, null, 2);
    })()`,
  );
  if (!updatedConfig) throw new Error("MCP configuration JSON did not render");
  await setVisibleTextarea(client, updatedConfig, "Browser MCP");
  await delay(200);
  await clickButton(client, "Save");
  await waitForValue(
    client,
    `document.body.innerText.includes("Browser MCP Updated")`,
    "MCP edited name did not appear",
  );

  await clickMcpClientButton(client, "Tools & Permissions");
  await waitForValue(
    client,
    `document.body.innerText.includes("Tool Access") &&
      document.body.innerText.includes("echo")`,
    "MCP access modal did not load the real MCP tool",
  );
  const denied = await evaluateValue(
    client,
    `(() => {
      const visible = (element) => Boolean(
        element.offsetWidth || element.offsetHeight || element.getClientRects().length
      );
      const label = [...document.querySelectorAll("body *")].find(
        (element) => visible(element) &&
          element.children.length === 0 &&
          (element.innerText ?? "").trim() === "Deny"
      );
      const control = label?.closest("label") ?? label?.parentElement ?? label;
      control?.click();
      return Boolean(control);
    })()`,
  );
  if (!denied) throw new Error("MCP Deny policy control did not render");
  await clickButton(client, "Save");
  await delay(500);
  await client.send("Input.dispatchKeyEvent", {
    type: "keyDown",
    key: "Escape",
    code: "Escape",
  });
  await client.send("Input.dispatchKeyEvent", {
    type: "keyUp",
    key: "Escape",
    code: "Escape",
  });
  await delay(300);
  const savedDefaultEffect = await evaluateValue(
    client,
    `fetch("/api/mcp/policy/browser-mcp")
      .then((response) => response.json())
      .then((policy) => policy.default_effect)`,
  );
  if (savedDefaultEffect !== "deny") {
    throw new Error(
      "MCP access policy did not persist the selected Deny value",
    );
  }

  await clickMcpClientButton(client, "Disable");
  await waitForValue(
    client,
    `[...document.querySelectorAll("h3")].some(
      (item) => item.innerText.trim() === "Browser MCP Updated"
    ) && document.body.innerText.includes("Disabled")`,
    "MCP client did not disable",
  );
  await clickMcpClientButton(client, "Enable");
  await waitForValue(
    client,
    `document.body.innerText.includes("Enabled")`,
    "MCP client did not enable",
  );
  if (process.env.QWENPAW_MCP_KEEP === "1") {
    return {
      created: true,
      editedWithMaskedSecret: true,
      discoveredTool: "echo",
      policySaved: savedDefaultEffect,
      toggled: true,
      deleted: false,
    };
  }
  await clickMcpClientButton(client, "Delete");
  await waitForValue(
    client,
    `document.body.innerText.includes("Are you sure you want to delete")`,
    "MCP delete confirmation did not open",
  );
  await clickButton(client, "Confirm");
  await waitForValue(
    client,
    `document.body.innerText.includes("No MCP clients configured yet")`,
    "MCP client did not disappear after deletion",
  );
  return {
    created: true,
    editedWithMaskedSecret: true,
    discoveredTool: "echo",
    policySaved: savedDefaultEffect,
    toggled: true,
    deleted: true,
  };
}

async function clickTab(client, text) {
  const clicked = await evaluateValue(
    client,
    `(() => {
      const tab = [...document.querySelectorAll('[role="tab"]')].find(
        (item) => (item.innerText ?? "").trim() === ${JSON.stringify(text)}
      );
      tab?.click();
      return Boolean(tab);
    })()`,
  );
  if (!clicked) throw new Error(`Security UI tab did not render: ${text}`);
  await delay(400);
}

async function setInputByPlaceholder(client, placeholder, value) {
  const focused = await evaluateValue(
    client,
    `(() => {
      const input = [...document.querySelectorAll("input")].find(
        (item) => item.placeholder === ${JSON.stringify(placeholder)}
      );
      input?.focus();
      input?.select();
      return Boolean(input);
    })()`,
  );
  if (!focused) {
    throw new Error(`Security UI input did not render: ${placeholder}`);
  }
  await client.send("Input.insertText", { text: value });
}

async function setSecuritySwitch(client, label, checked) {
  const changed = await evaluateValue(
    client,
    `(() => {
      const visible = (element) => Boolean(
        element.offsetWidth || element.offsetHeight || element.getClientRects().length
      );
      const label = [...document.querySelectorAll("body *")].find(
        (item) => visible(item) &&
          (item.innerText ?? "").trim() === ${JSON.stringify(label)}
      );
      let container = label;
      let control = null;
      for (let depth = 0; container && depth < 8 && !control; depth += 1) {
        control = container.querySelector(
          '[role="switch"], .ant-switch, input[type="checkbox"]'
        );
        container = container.parentElement;
      }
      if (!control) return false;
      const current = control.matches('input[type="checkbox"]')
        ? control.checked
        : control.getAttribute("aria-checked") === "true" ||
          control.classList.contains("ant-switch-checked");
      if (current !== ${checked}) {
        control.click();
      }
      return true;
    })()`,
  );
  if (!changed) throw new Error(`Security switch did not render: ${label}`);
}

async function runSecurityCrudScenario(client) {
  await waitForValue(
    client,
    `document.body.innerText.includes("TOOL_CMD_DANGEROUS_RM")`,
    "Security built-in rules did not render",
  );
  await setSecuritySwitch(client, "Enable Sandbox Execution", true);
  await setSecuritySwitch(client, "Hidden Newlines", true);
  await clickButton(client, "Save");
  await waitForValue(
    client,
    `fetch("/api/config/security/tool-guard").then((response) => response.json())
      .then((value) => value.shell_evasion_checks.newlines === true)`,
    "Tool Guard settings did not persist",
  );

  await clickTab(client, "File Guard");
  const protectedPath = "/tmp/qwenpaw-browser-security";
  await setInputByPlaceholder(
    client,
    "Enter file or directory path (e.g. ~/.ssh/ or /etc/passwd)",
    protectedPath,
  );
  await clickButton(client, "Add");
  await clickButton(client, "Save");
  await waitForValue(
    client,
    `fetch("/api/config/security/file-guard").then((response) => response.json())
      .then((value) => value.paths.includes(${JSON.stringify(protectedPath)}))`,
    "File Guard path did not persist",
  );

  await clickTab(client, "Skill Scanner");
  const selectOpened = await evaluateValue(
    client,
    `(() => {
      const visible = (element) => Boolean(
        element.offsetWidth || element.offsetHeight || element.getClientRects().length
      );
      const select = [...document.querySelectorAll('[role="combobox"]')].find(visible);
      select?.focus();
      const selector = select?.closest(".qwenpaw-select")?.querySelector(
        ".qwenpaw-select-selector"
      ) ?? select;
      selector?.dispatchEvent(new MouseEvent("mousedown", {
        bubbles: true,
        cancelable: true,
        view: window
      }));
      selector?.click();
      return Boolean(select);
    })()`,
  );
  if (!selectOpened)
    throw new Error("Skill Scanner mode selector did not render");
  await waitForValue(
    client,
    `[...document.querySelectorAll('body *')].some(
      (item) => (item.innerText ?? "").trim().toLowerCase() === "block"
    )`,
    "Skill Scanner Block option did not render",
  );
  const selected = await evaluateValue(
    client,
    `(() => {
      const visible = (element) => Boolean(
        element.offsetWidth || element.offsetHeight || element.getClientRects().length
      );
      const option = [...document.querySelectorAll('body *')].filter(
        (item) => visible(item) &&
          (item.innerText ?? "").trim().toLowerCase() === "block"
      ).at(-1);
      option?.dispatchEvent(new MouseEvent("mousedown", {
        bubbles: true,
        cancelable: true,
        view: window
      }));
      option?.click();
      return Boolean(option);
    })()`,
  );
  if (!selected)
    throw new Error("Skill Scanner Block option was not selectable");
  await waitForValue(
    client,
    `fetch("/api/config/security/skill-scanner").then((response) => response.json())
      .then((value) => value.mode === "block")`,
    "Skill Scanner mode did not persist",
  );

  await clickTab(client, "Allow No Auth Hosts");
  const allowedHost = "10.20.30.40";
  await setInputByPlaceholder(
    client,
    "Enter IP address (e.g., 192.168.1.100 or ::1)",
    allowedHost,
  );
  await clickButton(client, "Add");
  await clickButton(client, "Save");
  await waitForValue(
    client,
    `fetch("/api/config/security/allow-no-auth-hosts")
      .then((response) => response.json())
      .then((value) => value.hosts.includes(${JSON.stringify(allowedHost)}))`,
    "Allow No Auth Hosts value did not persist",
  );

  return evaluateValue(
    client,
    `Promise.all([
      fetch("/api/config/security/tool-guard").then((response) => response.json()),
      fetch("/api/config/security/tool-guard/builtin-rules").then((response) => response.json()),
      fetch("/api/config/security/sandbox").then((response) => response.json()),
      fetch("/api/config/security/file-guard").then((response) => response.json()),
      fetch("/api/config/security/skill-scanner").then((response) => response.json()),
      fetch("/api/config/security/allow-no-auth-hosts").then((response) => response.json())
    ]).then(([guard, rules, sandbox, fileGuard, scanner, hosts]) => ({
      builtinRules: rules.length,
      hiddenNewlines: guard.shell_evasion_checks.newlines,
      sandboxEnabled: sandbox.enabled,
      sandboxEffective: sandbox.effective,
      protectedPath: fileGuard.paths.includes(${JSON.stringify(protectedPath)}),
      scannerMode: scanner.mode,
      allowedHost: hosts.hosts.includes(${JSON.stringify(allowedHost)})
    }))`,
  );
}

async function enableCodingMode(origin) {
  const response = await fetch(`${origin}/api/coding-mode`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ enabled: true }),
  });
  if (!response.ok) {
    throw new Error(`Coding Mode setup returned ${response.status}`);
  }
  const body = await response.json();
  if (body.enabled !== true) {
    throw new Error("Coding Mode setup did not enable the Rust capability");
  }
}

function waitForDevTools(child) {
  return new Promise((resolve, reject) => {
    let stderr = "";
    const timer = setTimeout(() => {
      reject(
        new Error(`Chrome DevTools did not start: ${stderr.slice(-2_000)}`),
      );
    }, START_TIMEOUT_MS);
    child.stderr.on("data", (chunk) => {
      stderr += chunk.toString();
      const match = stderr.match(/DevTools listening on (ws:\/\/[^\s]+)/);
      if (match) {
        clearTimeout(timer);
        resolve(match[1]);
      }
    });
    child.once("exit", (code) => {
      clearTimeout(timer);
      reject(new Error(`Chrome exited before DevTools was ready (${code})`));
    });
  });
}

class DevToolsClient {
  constructor(socket) {
    this.socket = socket;
    this.nextId = 1;
    this.pending = new Map();
    this.listeners = new Map();
    socket.addEventListener("message", (event) => {
      const message = JSON.parse(event.data);
      if (message.id) {
        const pending = this.pending.get(message.id);
        if (!pending) return;
        this.pending.delete(message.id);
        if (message.error) pending.reject(new Error(message.error.message));
        else pending.resolve(message.result);
        return;
      }
      for (const listener of this.listeners.get(message.method) ?? []) {
        listener(message.params);
      }
    });
  }

  on(method, listener) {
    const listeners = this.listeners.get(method) ?? [];
    listeners.push(listener);
    this.listeners.set(method, listeners);
  }

  send(method, params = {}) {
    const id = this.nextId++;
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.socket.send(JSON.stringify({ id, method, params }));
    });
  }

  close() {
    this.socket.close();
  }
}

async function connectDevTools(webSocketUrl) {
  const socket = new WebSocket(webSocketUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });
  return new DevToolsClient(socket);
}

async function createPage(browserWebSocketUrl) {
  const browserUrl = new URL(browserWebSocketUrl);
  const response = await fetch(
    `http://${browserUrl.host}/json/new?${encodeURIComponent("about:blank")}`,
    { method: "PUT" },
  );
  if (!response.ok) {
    throw new Error(`Chrome target creation failed with ${response.status}`);
  }
  return response.json();
}

function apiPath(url, origin) {
  const parsed = new URL(url);
  if (parsed.origin !== origin || !parsed.pathname.startsWith("/api/")) {
    return null;
  }
  return `${parsed.pathname}${parsed.search}`;
}

async function inspectConsole(client, origin, options) {
  const {
    codingGit,
    mcpCrud,
    securityCrud,
    skillsCrud,
    agentsCrud,
    acpCrud,
    modelsCrud,
    paths,
  } = options;
  let observation;
  client.on("Network.responseReceived", ({ response }) => {
    const requestPath = apiPath(response.url, origin);
    if (requestPath)
      observation?.apiResponses.set(requestPath, response.status);
  });
  client.on("Network.loadingFailed", ({ errorText, type }) => {
    if (type !== "EventSource" && errorText !== "net::ERR_ABORTED") {
      observation?.requestFailures.push(`${type}: ${errorText}`);
    }
  });
  client.on("Runtime.exceptionThrown", ({ exceptionDetails }) => {
    observation?.browserErrors.push(
      exceptionDetails.exception?.description ?? exceptionDetails.text,
    );
  });
  client.on("Runtime.consoleAPICalled", ({ type, args }) => {
    if (type !== "error") return;
    observation?.browserErrors.push(
      args.map((argument) => argument.value ?? argument.description).join(" "),
    );
  });
  await Promise.all([
    client.send("Network.enable"),
    client.send("Page.enable"),
    client.send("Runtime.enable"),
  ]);
  const pages = [];
  for (const navigationPath of paths) {
    observation = {
      apiResponses: new Map(),
      requestFailures: [],
      browserErrors: [],
    };
    await client.send("Page.navigate", { url: `${origin}${navigationPath}` });
    await delay(
      paths.length === 1 ? WAIT_AFTER_LOAD_MS : NAVIGATION_WAIT_AFTER_LOAD_MS,
    );
    let mcpCrudResult;
    let securityCrudResult;
    let skillsCrudResult;
    let agentsCrudResult;
    let acpCrudResult;
    let modelsCrudResult;
    if (mcpCrud && navigationPath === "/mcp") {
      try {
        mcpCrudResult = await runMcpCrudScenario(client);
      } catch (error) {
        observation.browserErrors.push(error.stack ?? String(error));
      }
    }
    if (securityCrud && navigationPath === "/security") {
      try {
        securityCrudResult = await runSecurityCrudScenario(client);
      } catch (error) {
        observation.browserErrors.push(error.stack ?? String(error));
      }
    }
    if (
      skillsCrud &&
      (navigationPath === "/skills" || navigationPath === "/skill-pool")
    ) {
      try {
        skillsCrudResult = await runSkillsCrudScenario(client, navigationPath);
      } catch (error) {
        observation.browserErrors.push(error.stack ?? String(error));
      }
    }
    if (agentsCrud && navigationPath === "/agents") {
      try {
        agentsCrudResult = await runAgentsCrudScenario(client);
      } catch (error) {
        observation.browserErrors.push(error.stack ?? String(error));
      }
    }
    if (acpCrud && navigationPath === "/acp") {
      try {
        acpCrudResult = await runAcpCrudScenario(client);
      } catch (error) {
        observation.browserErrors.push(error.stack ?? String(error));
      }
    }
    if (modelsCrud && navigationPath === "/models") {
      try {
        modelsCrudResult = await runModelsCrudScenario(client);
      } catch (error) {
        observation.browserErrors.push(error.stack ?? String(error));
      }
    }
    if (codingGit && navigationPath === "/files") {
      const activated = await client.send("Runtime.evaluate", {
        expression: `(() => {
          const sourceControl = [...document.querySelectorAll("button")].find(
            (button) => /source control/i.test(
              button.getAttribute("aria-label") ?? ""
            )
          );
          sourceControl?.click();
          return Boolean(sourceControl);
        })()`,
        returnByValue: true,
      });
      if (!activated.result.value) {
        observation.browserErrors.push(
          "Coding Mode Source Control control did not render",
        );
      } else {
        await delay(NAVIGATION_WAIT_AFTER_LOAD_MS);
      }
    }
    const evaluation = await client.send("Runtime.evaluate", {
      expression: `JSON.stringify({
        bodyText: document.body?.innerText ?? "",
        rootChildren: document.querySelector("#root")?.childElementCount ?? 0,
        hasComposer: Boolean(
          document.querySelector("textarea") ||
          document.querySelector('[contenteditable="true"]')
        ),
        loadingError: Boolean(
          [...document.querySelectorAll("body *")].some((element) =>
            /backend detection failed|authentication failed/i.test(
              element.textContent ?? ""
            )
          )
        )
      })`,
      returnByValue: true,
    });
    const page = JSON.parse(evaluation.result.value);
    const responses = [...observation.apiResponses]
      .map(([requestPath, status]) => ({ path: requestPath, status }))
      .sort((left, right) => left.path.localeCompare(right.path));
    const failedApi = responses.filter(({ status }) => status >= 400);
    const failures = [];
    if (page.rootChildren === 0) failures.push("#root did not render");
    if (
      (navigationPath === "/" || navigationPath.startsWith("/chat")) &&
      !page.hasComposer
    ) {
      failures.push("chat composer did not render");
    }
    if (page.loadingError) failures.push("backend loading/error page rendered");
    failures.push(
      ...observation.requestFailures,
      ...observation.browserErrors,
      ...failedApi.map(
        ({ path: requestPath, status }) => `${requestPath} returned ${status}`,
      ),
    );
    pages.push({
      path: navigationPath,
      ok: failures.length === 0,
      page: {
        rootChildren: page.rootChildren,
        hasComposer: page.hasComposer,
        textSample: page.bodyText.slice(0, 300),
      },
      mcpCrud: mcpCrudResult,
      securityCrud: securityCrudResult,
      skillsCrud: skillsCrudResult,
      agentsCrud: agentsCrudResult,
      acpCrud: acpCrudResult,
      modelsCrud: modelsCrudResult,
      apiResponses: responses,
      failedApi,
      failures,
    });
  }
  observation = undefined;
  return {
    ok: pages.every(({ ok }) => ok),
    pages,
  };
}

async function main() {
  const origin = validateBaseUrl(process.argv[2] ?? "");
  const options = smokeOptions(process.argv.slice(3));
  if (options.codingGit) await enableCodingMode(origin);
  const executable = await chromeExecutable();
  const profile = await mkdtemp(
    path.join(os.tmpdir(), "qwenpaw-chrome-smoke-"),
  );
  const child = spawn(
    executable,
    [
      "--headless=new",
      "--disable-gpu",
      "--no-first-run",
      "--no-default-browser-check",
      "--no-sandbox",
      "--remote-debugging-port=0",
      `--user-data-dir=${profile}`,
      "about:blank",
    ],
    { stdio: ["ignore", "ignore", "pipe"] },
  );
  let pageClient;
  let browserClient;
  try {
    const browserWebSocketUrl = await waitForDevTools(child);
    const target = await createPage(browserWebSocketUrl);
    pageClient = await connectDevTools(target.webSocketDebuggerUrl);
    await pageClient.send("Emulation.setDeviceMetricsOverride", {
      width: 1440,
      height: 1000,
      deviceScaleFactor: 1,
      mobile: false,
    });
    const result = await inspectConsole(pageClient, origin, options);
    process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
    browserClient = await connectDevTools(browserWebSocketUrl);
    await browserClient.send("Browser.close");
    if (!result.ok) process.exitCode = 1;
  } finally {
    pageClient?.close();
    browserClient?.close();
    if (child.exitCode === null) child.kill("SIGTERM");
    await waitForExit(child, 2_000);
    if (child.exitCode === null) {
      child.kill("SIGKILL");
      await waitForExit(child, 2_000);
    }
    await rm(profile, {
      recursive: true,
      force: true,
      maxRetries: 10,
      retryDelay: 200,
    });
  }
}

main().catch((error) => {
  process.stderr.write(`${error.stack ?? error}\n`);
  process.exitCode = 1;
});
