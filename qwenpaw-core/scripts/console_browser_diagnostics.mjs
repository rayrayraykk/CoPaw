// Supplemental evidence only: this must never decide whether a page passes.
export function attachBrowserDiagnostics(client, origin) {
  const requests = new Map();
  const contexts = new Map();
  let navigationPath;
  let report;
  const localPath = (url) => {
    try {
      const parsed = new URL(url);
      return parsed.origin === origin ? parsed.pathname : null;
    } catch {
      return null;
    }
  };
  client.on("Network.requestWillBeSent", (event) => {
    const path = localPath(event.request.url);
    if (!path?.startsWith("/api/")) return;
    requests.set(event.requestId, {
      requestId: event.requestId,
      path,
      navigationPath,
      documentPath: localPath(event.documentURL),
      loaderId: event.loaderId,
      type: event.type,
    });
  });
  client.on("Network.loadingFinished", ({ requestId }) => {
    requests.delete(requestId);
  });
  client.on("Network.loadingFailed", (event) => {
    const request = requests.get(event.requestId);
    if (request) {
      report?.failedRequests.push({
        ...request,
        errorText: event.errorText,
        canceled: event.canceled ?? false,
      });
    }
    requests.delete(event.requestId);
  });
  client.on("Page.frameNavigated", ({ frame }) => {
    if (frame.parentId) return;
    report?.documents.push({
      path: localPath(frame.url),
      frameId: frame.id,
      loaderId: frame.loaderId,
    });
  });
  client.on("Runtime.executionContextCreated", ({ context }) => {
    contexts.set(context.id, {
      contextId: context.id,
      navigationPath,
      frameId: context.auxData?.frameId,
      isDefault: context.auxData?.isDefault ?? false,
    });
    // Retain recent destroyed contexts to identify errors during navigation.
    if (contexts.size > 128) contexts.delete(contexts.keys().next().value);
  });
  const recordContext = (kind, contextId) => {
    report?.errorContexts.push({
      kind,
      ...(contexts.get(contextId) ?? { contextId }),
    });
  };
  client.on("Runtime.consoleAPICalled", ({ type, executionContextId }) => {
    if (type === "error") recordContext("console", executionContextId);
  });
  client.on("Runtime.exceptionThrown", ({ exceptionDetails }) => {
    recordContext("exception", exceptionDetails.executionContextId);
  });
  return {
    begin(path) {
      navigationPath = path;
      report = {
        pendingBeforeNavigation: [...requests.values()],
        documents: [],
        failedRequests: [],
        errorContexts: [],
      };
      return report;
    },
  };
}
