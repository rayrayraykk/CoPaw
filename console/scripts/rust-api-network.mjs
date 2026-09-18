import ts from "typescript";

function networkKind(node) {
  if (!ts.isIdentifier(node.expression)) return null;
  if (ts.isCallExpression(node) && node.expression.text === "fetch") {
    return "fetch";
  }
  if (
    ts.isNewExpression(node) &&
    ["EventSource", "WebSocket"].includes(node.expression.text)
  ) {
    return node.expression.text;
  }
  return null;
}

/** Resolve actual consumers, including const url = getApiUrl(...); fetch(url). */
export function networkCallsForUrl(node, sourceFile) {
  let current = node.parent;
  while (current && !ts.isStatement(current)) {
    if (ts.isCallExpression(current) || ts.isNewExpression(current)) {
      const kind = networkKind(current);
      if (kind) return [{ kind, node: current }];
    }
    current = current.parent;
  }
  const declaration = node.parent;
  if (
    !ts.isVariableDeclaration(declaration) ||
    declaration.initializer !== node ||
    !ts.isIdentifier(declaration.name) ||
    !(declaration.parent.flags & ts.NodeFlags.Const)
  ) {
    return [];
  }
  // A local-only checker distinguishes repeated `url` names and shadowed locals.
  const options = { noLib: true, noResolve: true };
  const host = ts.createCompilerHost(options);
  host.getSourceFile = (name) =>
    name === sourceFile.fileName ? sourceFile : undefined;
  const program = ts.createProgram([sourceFile.fileName], options, host);
  const checker = program.getTypeChecker();
  const symbol = checker.getSymbolAtLocation(declaration.name);
  if (!symbol) return [];
  const consumers = [];
  function visit(candidate) {
    if (ts.isCallExpression(candidate) || ts.isNewExpression(candidate)) {
      const argument = candidate.arguments?.[0];
      const kind = networkKind(candidate);
      if (
        kind &&
        argument &&
        ts.isIdentifier(argument) &&
        checker.getSymbolAtLocation(argument) === symbol
      ) {
        consumers.push({ kind, node: candidate });
      }
    }
    ts.forEachChild(candidate, visit);
  }
  visit(sourceFile);
  return consumers;
}
