ObjC.import("Foundation");

const current = Application.currentApplication();
current.includeStandardAdditions = true;

function safe(callable, fallback) {
  try {
    return callable();
  } catch (error) {
    return fallback;
  }
}

function children(element) {
  return safe(() => element.uiElements(), []);
}

function role(element) {
  return safe(() => element.role(), "");
}

function identifier(element) {
  return String(
    safe(() => element.attributes.byName("AXIdentifier").value(), "") || "",
  );
}

function value(element) {
  return String(safe(() => element.value(), "") || "").trim();
}

function findIdentifier(element, wanted, depth) {
  if (!element || depth < 0) {
    return null;
  }
  if (identifier(element) === wanted) {
    return element;
  }
  for (const child of children(element)) {
    const found = findIdentifier(child, wanted, depth - 1);
    if (found) {
      return found;
    }
  }
  return null;
}

function findRole(element, wanted, depth, skipLargeTables) {
  if (!element || depth < 0) {
    return null;
  }
  if (role(element) === wanted) {
    return element;
  }
  const items = children(element);
  if (
    skipLargeTables &&
    role(element) === "AXTable" &&
    items.length > 100
  ) {
    return null;
  }
  for (const child of items) {
    const found = findRole(
      child,
      wanted,
      depth - 1,
      skipLargeTables,
    );
    if (found) {
      return found;
    }
  }
  return null;
}

function collectText(element, depth, output) {
  if (!element || depth < 0) {
    return;
  }
  const elementRole = role(element);
  if (elementRole === "AXStaticText" || elementRole === "AXTextArea") {
    const text = value(element);
    if (text) {
      output.push({
        text,
      });
    }
  }
  for (const child of children(element)) {
    collectText(child, depth - 1, output);
  }
}

function processForBundle(bundleId) {
  const systemEvents = Application("System Events");
  const matches = systemEvents.processes.whose({
    bundleIdentifier: bundleId,
  })();
  return matches.length ? matches[0] : null;
}

function mainWindow(process) {
  const windows = safe(() => process.windows(), []);
  return windows.length ? windows[0] : null;
}

function chatSplit(process) {
  return findIdentifier(mainWindow(process), "ChatSplitView", 6);
}

function currentConversation(process) {
  const chat = chatSplit(process);
  if (!chat) {
    return "";
  }
  for (const child of children(chat)) {
    if (role(child) === "AXStaticText" && value(child)) {
      return value(child);
    }
  }
  return "";
}

function messageTable(process) {
  const chat = chatSplit(process);
  if (!chat) {
    return null;
  }
  let selected = null;
  let selectedCount = 0;
  function visit(element, depth) {
    if (!element || depth < 0) {
      return;
    }
    if (role(element) === "AXTable") {
      const count = safe(() => element.rows.length, 0);
      if (count > selectedCount) {
        selected = element;
        selectedCount = count;
      }
      return;
    }
    for (const child of children(element)) {
      visit(child, depth - 1);
    }
  }
  visit(chat, 4);
  return selected;
}

function messageFromRow(row) {
  const semantics = [];
  function collectSemantics(element, depth) {
    if (!element || depth < 0) {
      return;
    }
    const description = String(
      safe(() => element.description(), "") || "",
    ).toLowerCase();
    if (description) {
      semantics.push(description);
    }
    for (const child of children(element)) {
      collectSemantics(child, depth - 1);
    }
  }
  collectSemantics(row, 8);
  const receiving = semantics.some((item) =>
    item.includes("session msg receiving"),
  );
  const sending = semantics.some((item) =>
    item.includes("session msg sending"),
  );
  if (receiving === sending) {
    return null;
  }
  const textNodes = [];
  collectText(row, 8, textNodes);
  const meaningful = textNodes.filter(
    (item) =>
      item.text &&
      !/^\d{1,2}:\d{2}$/.test(item.text) &&
      !/^\d{4}[-/]\d{1,2}[-/]\d{1,2}/.test(item.text),
  );
  if (!meaningful.length) {
    return null;
  }
  meaningful.sort((left, right) => right.text.length - left.text.length);
  const message = meaningful[0];
  return {
    text: message.text,
    incoming: receiving,
  };
}

function messageAtOffset(process, requestedOffset) {
  const conversation = currentConversation(process);
  const table = messageTable(process);
  if (!conversation || !table) {
    return { conversation, message: null };
  }
  const count = safe(() => table.rows.length, 0);
  const offset = Math.max(0, Number(requestedOffset || 0));
  const index = count - 1 - offset;
  const message = index >= 0 ? messageFromRow(table.rows.at(index)) : null;
  if (currentConversation(process) !== conversation) {
    throw new Error("The visible DingTalk conversation changed while reading");
  }
  return { conversation, message };
}

function latestMessage(process) {
  const snapshot = messageAtOffset(process, 0);
  if (!snapshot.message) {
    return null;
  }
  return {
    conversation: snapshot.conversation,
    ...snapshot.message,
  };
}

function withConversation(process, title, callback) {
  if (!title || currentConversation(process) !== title) {
    throw new Error("The allowed DingTalk conversation is not visible");
  }
  return callback();
}

function sendMessage(process, title, text) {
  return withConversation(process, title, () => {
    const chat = chatSplit(process);
    const editor = findRole(chat, "AXTextArea", 8, true);
    if (!editor) {
      throw new Error("DingTalk message editor is unavailable");
    }
    process.frontmost = true;
    editor.value = text;
    safe(() => editor.attributes.byName("AXFocused").value(true), null);
    current.delay(0.1);
    Application("System Events").keyCode(36);
    return { sent: true };
  });
}

function readInput() {
  const handle = $.NSFileHandle.fileHandleWithStandardInput;
  const data = handle.readDataToEndOfFile;
  if (!data || Number(data.length) === 0) {
    return {};
  }
  const text = $.NSString.alloc.initWithDataEncoding(
    data,
    $.NSUTF8StringEncoding,
  ).js;
  return JSON.parse(text || "{}");
}

function runAction(input) {
  const bundleId = String(
    input.bundle_id || "dd.work.exclusive4aliding",
  );
  const process = processForBundle(bundleId);
  if (input.action === "status") {
    if (!process) {
      return { running: false, accessibility: true, logged_in: false };
    }
    const window = mainWindow(process);
    return {
      running: true,
      accessibility: Boolean(window),
      logged_in: Boolean(chatSplit(process) && currentConversation(process)),
      current_conversation: currentConversation(process),
      pid: Number(safe(() => process.unixId(), 0)),
    };
  }
  if (!process) {
    throw new Error("DingTalk is not running");
  }
  if (input.action === "current") {
    return { conversation: currentConversation(process) };
  }
  if (input.action === "row") {
    return withConversation(
      process,
      String(input.conversation || ""),
      () => messageAtOffset(process, input.offset),
    );
  }
  if (input.action === "read") {
    return withConversation(
      process,
      String(input.conversation || ""),
      () => ({ message: latestMessage(process) }),
    );
  }
  if (input.action === "send") {
    return sendMessage(
      process,
      String(input.conversation || ""),
      String(input.text || ""),
    );
  }
  throw new Error("Unsupported DingTalk desktop action");
}

let finalOutput;
try {
  finalOutput = JSON.stringify({ ok: true, result: runAction(readInput()) });
} catch (error) {
  finalOutput = JSON.stringify({
    ok: false,
    error: String(error && error.message ? error.message : error),
  });
}
finalOutput;
