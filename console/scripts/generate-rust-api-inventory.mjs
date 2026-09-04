import { readFile, readdir, writeFile } from "node:fs/promises";
import { dirname, extname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import ts from "typescript";

const scriptDirectory = dirname(fileURLToPath(import.meta.url));
const consoleDirectory = resolve(scriptDirectory, "..");
const repositoryDirectory = resolve(consoleDirectory, "..");
const sourceDirectory = join(consoleDirectory, "src");
const outputPath = join(
  repositoryDirectory,
  "qwenpaw-core",
  "docs",
  "api-contract",
  "console-call-sites.json",
);
const rustSourceDirectory = join(
  repositoryDirectory,
  "qwenpaw-core",
  "crates",
  "qwenpaw-app-server",
  "src",
);
const writeSnapshot = process.argv.includes("--write");
const checkSnapshot = process.argv.includes("--check");

if (writeSnapshot && checkSnapshot) {
  throw new Error("Use either --write or --check, not both.");
}

async function walk(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...(await walk(path)));
    } else {
      files.push(path);
    }
  }
  return files;
}

function isProductionSource(path) {
  const extension = extname(path);
  const normalized = path.replaceAll("\\", "/");
  return (
    (extension === ".ts" || extension === ".tsx") &&
    !normalized.endsWith(".d.ts") &&
    !normalized.includes(".test.") &&
    !normalized.includes("/__tests__/") &&
    !normalized.includes("/test/") &&
    !normalized.includes("/tests/") &&
    !normalized.endsWith("/api/request.ts")
  );
}

function compact(source) {
  return source.replaceAll(/\s+/g, " ").trim();
}

function extractRouteArguments(source) {
  const argumentsList = [];
  let offset = 0;
  while ((offset = source.indexOf(".route(", offset)) !== -1) {
    const start = offset + ".route(".length;
    let depth = 1;
    let quote = null;
    let escaped = false;
    let cursor = start;
    for (; cursor < source.length; cursor += 1) {
      const character = source[cursor];
      if (quote) {
        if (escaped) {
          escaped = false;
        } else if (character === "\\") {
          escaped = true;
        } else if (character === quote) {
          quote = null;
        }
        continue;
      }
      if (character === '"' || character === "'") {
        quote = character;
      } else if (character === "(") {
        depth += 1;
      } else if (character === ")") {
        depth -= 1;
        if (depth === 0) {
          argumentsList.push(source.slice(start, cursor));
          break;
        }
      }
    }
    offset = cursor + 1;
  }
  return argumentsList;
}

async function rustRoutes() {
  const routes = [];
  const files = (await walk(rustSourceDirectory))
    .filter((path) => extname(path) === ".rs")
    .sort((left, right) => left.localeCompare(right));
  for (const path of files) {
    const source = await readFile(path, "utf-8");
    for (const routeArguments of extractRouteArguments(source)) {
      const match = routeArguments.match(/^\s*"([^"]+)"\s*,([\s\S]*)$/);
      if (!match) {
        continue;
      }
      const methods = [
        ...match[2].matchAll(
          /\b(get|post|put|patch|delete|any)\s*\(\s*([A-Za-z_][A-Za-z0-9_]*)/g,
        ),
      ].map((methodMatch) => ({
        method: methodMatch[1].toUpperCase(),
        handler: methodMatch[2],
      }));
      for (const { method, handler } of methods) {
        const routeSource = relative(repositoryDirectory, path).replaceAll(
          "\\",
          "/",
        );
        routes.push({
          method,
          path: match[1],
          handler,
          placeholder:
            handler.startsWith("empty_") ||
            routeSource.endsWith("/desktop_navigation.rs"),
          source: routeSource,
        });
      }
    }
  }
  routes.sort(
    (left, right) =>
      left.path.localeCompare(right.path) ||
      left.method.localeCompare(right.method),
  );
  return routes;
}

function routeShape(path) {
  return path
    .replace(/\{choice:\?\{query\}\|\}$/, "")
    .replace(/\{query\}$/, "")
    .split("?", 1)[0]
    .split("/")
    .map((segment) => (segment.includes("{") ? "{}" : segment))
    .join("/");
}

function routeMatches(routePath, callPath) {
  const normalize = (path) =>
    path
      .replace(/\{choice:\?\{query\}\|\}$/, "")
      .replace(/\{query\}$/, "")
      .split("?", 1)[0]
      .split("/");
  const routeSegments = normalize(routePath);
  const callSegments = normalize(callPath);
  for (let index = 0; index < routeSegments.length; index += 1) {
    const routeSegment = routeSegments[index];
    if (routeSegment.startsWith("{*")) {
      return index === routeSegments.length - 1;
    }
    if (callSegments[index] === undefined) {
      return false;
    }
    if (!routeSegment.startsWith("{") && routeSegment !== callSegments[index]) {
      return false;
    }
  }
  return routeSegments.length === callSegments.length;
}

function apiPath(path) {
  const normalized = path.startsWith("/") ? path : `/${path}`;
  return `/api${normalized}`;
}

function importedNames(sourceFile, moduleSuffix, importedName) {
  const names = new Set();
  for (const statement of sourceFile.statements) {
    if (
      !ts.isImportDeclaration(statement) ||
      !ts.isStringLiteral(statement.moduleSpecifier) ||
      !statement.moduleSpecifier.text.endsWith(moduleSuffix)
    ) {
      continue;
    }
    for (const element of statement.importClause?.namedBindings?.elements ??
      []) {
      if ((element.propertyName ?? element.name).text === importedName) {
        names.add(element.name.text);
      }
    }
  }
  return names;
}

function localConstants(sourceFile) {
  const values = new Map();
  const duplicates = new Set();
  function visit(node) {
    if (
      ts.isVariableDeclaration(node) &&
      ts.isIdentifier(node.name) &&
      node.initializer
    ) {
      if (values.has(node.name.text)) {
        duplicates.add(node.name.text);
      } else {
        values.set(node.name.text, node.initializer);
      }
    }
    ts.forEachChild(node, visit);
  }
  visit(sourceFile);
  for (const duplicate of duplicates) {
    values.delete(duplicate);
  }
  return values;
}

function placeholder(expression, sourceFile) {
  if (ts.isIdentifier(expression)) {
    return expression.text;
  }
  if (ts.isPropertyAccessExpression(expression)) {
    return expression.name.text;
  }
  if (
    ts.isCallExpression(expression) &&
    ts.isIdentifier(expression.expression) &&
    expression.expression.text === "encodeURIComponent" &&
    expression.arguments[0]
  ) {
    return placeholder(expression.arguments[0], sourceFile);
  }
  const source = compact(expression.getText(sourceFile));
  if (/query|params|search|suffix|dateQuery|buildQuery/.test(source)) {
    return "query";
  }
  return "value";
}

function renderPath(expression, sourceFile, constants, seen = new Set()) {
  if (
    ts.isStringLiteral(expression) ||
    ts.isNoSubstitutionTemplateLiteral(expression)
  ) {
    return expression.text;
  }
  if (ts.isTemplateExpression(expression)) {
    let rendered = expression.head.text;
    for (const span of expression.templateSpans) {
      const constant = ts.isIdentifier(span.expression)
        ? constants.get(span.expression.text)
        : null;
      rendered += constant
        ? renderPath(constant, sourceFile, constants, seen)
        : `{${placeholder(span.expression, sourceFile)}}`;
      rendered += span.literal.text;
    }
    return rendered;
  }
  if (ts.isParenthesizedExpression(expression)) {
    return renderPath(expression.expression, sourceFile, constants, seen);
  }
  if (ts.isIdentifier(expression)) {
    if (!seen.has(expression.text) && constants.has(expression.text)) {
      const nextSeen = new Set(seen).add(expression.text);
      return renderPath(
        constants.get(expression.text),
        sourceFile,
        constants,
        nextSeen,
      );
    }
    return `{dynamic:${expression.text}}`;
  }
  if (
    ts.isCallExpression(expression) &&
    ts.isIdentifier(expression.expression) &&
    expression.expression.text === "workspaceQuery" &&
    expression.arguments[0]
  ) {
    return `${renderPath(
      expression.arguments[0],
      sourceFile,
      constants,
      seen,
    )}{query}`;
  }
  if (
    ts.isBinaryExpression(expression) &&
    expression.operatorToken.kind === ts.SyntaxKind.PlusToken
  ) {
    return `${renderPath(
      expression.left,
      sourceFile,
      constants,
      seen,
    )}${renderPath(expression.right, sourceFile, constants, seen)}`;
  }
  if (ts.isConditionalExpression(expression)) {
    const whenTrue = renderPath(
      expression.whenTrue,
      sourceFile,
      constants,
      seen,
    );
    const whenFalse = renderPath(
      expression.whenFalse,
      sourceFile,
      constants,
      seen,
    );
    return whenTrue === whenFalse
      ? whenTrue
      : `{choice:${whenTrue}|${whenFalse}}`;
  }
  return `{dynamic:${compact(expression.getText(sourceFile))}}`;
}

function methodFromOptions(options, sourceFile, fallback) {
  if (!options || !ts.isObjectLiteralExpression(options)) {
    return fallback;
  }
  for (const property of options.properties) {
    if (
      ts.isPropertyAssignment(property) &&
      property.name.getText(sourceFile) === "method" &&
      (ts.isStringLiteral(property.initializer) ||
        ts.isNoSubstitutionTemplateLiteral(property.initializer))
    ) {
      return property.initializer.text.toUpperCase();
    }
  }
  return fallback;
}

function containingNetworkCall(node) {
  let current = node.parent;
  while (current && !ts.isStatement(current)) {
    if (
      ts.isCallExpression(current) &&
      ts.isIdentifier(current.expression) &&
      current.expression.text === "fetch"
    ) {
      return { kind: "fetch", node: current };
    }
    if (
      ts.isNewExpression(current) &&
      ts.isIdentifier(current.expression) &&
      ["EventSource", "WebSocket"].includes(current.expression.text)
    ) {
      return { kind: current.expression.text, node: current };
    }
    current = current.parent;
  }
  return null;
}

function collectCallSites(path, source) {
  const sourceFile = ts.createSourceFile(
    path,
    source,
    ts.ScriptTarget.Latest,
    true,
  );
  const requestNames = importedNames(sourceFile, "/request", "request");
  const apiUrlNames = importedNames(sourceFile, "/config", "getApiUrl");
  const constants = localConstants(sourceFile);
  const callSites = [];

  function record(node, expression, method, transport) {
    const location = sourceFile.getLineAndCharacterOfPosition(node.getStart());
    const rendered = renderPath(expression, sourceFile, constants);
    callSites.push({
      method,
      transport,
      path: rendered,
      expression: compact(expression.getText(sourceFile)),
      source: relative(repositoryDirectory, path).replaceAll("\\", "/"),
      line: location.line + 1,
      resolved: !rendered.includes("{dynamic:"),
    });
  }

  function visit(node) {
    if (
      ts.isCallExpression(node) &&
      ts.isIdentifier(node.expression) &&
      requestNames.has(node.expression.text) &&
      node.arguments[0]
    ) {
      record(
        node,
        node.arguments[0],
        methodFromOptions(node.arguments[1], sourceFile, "GET"),
        "http",
      );
    } else if (
      ts.isCallExpression(node) &&
      ts.isIdentifier(node.expression) &&
      apiUrlNames.has(node.expression.text) &&
      node.arguments[0]
    ) {
      const networkCall = containingNetworkCall(node);
      let method = "GET";
      let transport = "url";
      if (networkCall?.kind === "fetch") {
        method = methodFromOptions(
          networkCall.node.arguments[1],
          sourceFile,
          "GET",
        );
        transport = "http";
      } else if (networkCall?.kind === "EventSource") {
        transport = "sse";
      } else if (networkCall?.kind === "WebSocket") {
        transport = "websocket";
      }
      record(node, node.arguments[0], method, transport);
    }
    ts.forEachChild(node, visit);
  }

  visit(sourceFile);
  return callSites;
}

const paths = (await walk(sourceDirectory))
  .filter(isProductionSource)
  .sort((left, right) => left.localeCompare(right));
const callSites = [];
for (const path of paths) {
  callSites.push(...collectCallSites(path, await readFile(path, "utf-8")));
}
callSites.sort(
  (left, right) =>
    left.source.localeCompare(right.source) || left.line - right.line,
);
const routes = await rustRoutes();
for (const call of callSites) {
  call.apiPath = apiPath(call.path);
  const route = routes.find(
    (candidate) =>
      candidate.handler !== "api_not_found" &&
      (candidate.method === "ANY" || candidate.method === call.method) &&
      routeMatches(candidate.path, call.apiPath),
  );
  call.registered = call.resolved && route !== undefined;
  call.placeholder = route?.placeholder ?? false;
}

const missingCalls = callSites.filter((call) => !call.registered);
const placeholderCalls = callSites.filter((call) => call.placeholder);

const inventory = {
  version: 1,
  source: "console/src production TypeScript call sites",
  callCount: callSites.length,
  unresolvedCount: callSites.filter((call) => !call.resolved).length,
  rustRouteCount: routes.length,
  registeredCallCount: callSites.length - missingCalls.length,
  placeholderCallCount: placeholderCalls.length,
  nonPlaceholderRegisteredCallCount:
    callSites.length - missingCalls.length - placeholderCalls.length,
  missingCallCount: missingCalls.length,
  rustRoutes: routes,
  calls: callSites,
};
const serialized = `${JSON.stringify(inventory, null, 2)}\n`;

if (writeSnapshot) {
  await writeFile(outputPath, serialized, "utf-8");
  console.log(
    `Wrote ${callSites.length} call sites and ${routes.length} Rust routes ` +
      `to ${outputPath}.`,
  );
} else if (checkSnapshot) {
  const expected = await readFile(outputPath, "utf-8");
  if (expected !== serialized) {
    throw new Error(
      "Console API call-site snapshot is stale. Run npm run api:inventory.",
    );
  }
  console.log(
    `Console API inventory is current: ${callSites.length} calls, ` +
      `${missingCalls.length} without a registered Rust route.`,
  );
} else {
  process.stdout.write(serialized);
}
