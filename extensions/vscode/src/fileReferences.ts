import { type UserInput } from "./generated/protocol";

const MAX_FILE_REFERENCES = 32;

interface ReferenceLike {
  readonly value: unknown;
}

interface UriLike {
  readonly scheme: string;
  readonly fsPath: string;
}

interface PositionLike {
  readonly line: number;
  readonly character: number;
}

interface RangeLike {
  readonly start: PositionLike;
  readonly end: PositionLike;
}

interface LocationLike {
  readonly uri: UriLike;
  readonly range: RangeLike;
}

export function buildUserInput(
  prompt: string,
  references: readonly ReferenceLike[],
): UserInput[] {
  const input: UserInput[] = [{ type: "text", text: prompt }];
  const seen = new Set<string>();
  for (const reference of references) {
    const file = fileReference(reference.value);
    if (!file) {
      continue;
    }
    const key = JSON.stringify(file);
    if (seen.has(key)) {
      continue;
    }
    if (seen.size === MAX_FILE_REFERENCES) {
      throw new Error(
        `QwenPaw supports at most ${MAX_FILE_REFERENCES} file references per request`,
      );
    }
    seen.add(key);
    input.push(file);
  }
  return input;
}

function fileReference(value: unknown): UserInput | undefined {
  if (isLocation(value)) {
    const lines = inclusiveLines(value.range);
    if (!lines || value.uri.scheme !== "file" || !value.uri.fsPath) {
      return undefined;
    }
    return {
      type: "fileReference",
      path: value.uri.fsPath,
      startLine: lines.start,
      endLine: lines.end,
    };
  }
  if (!isUri(value) || value.scheme !== "file" || !value.fsPath) {
    return undefined;
  }
  return {
    type: "fileReference",
    path: value.fsPath,
    startLine: null,
    endLine: null,
  };
}

function inclusiveLines(
  range: RangeLike,
): { readonly start: number; readonly end: number } | undefined {
  const { start, end } = range;
  if (
    !isPosition(start) ||
    !isPosition(end) ||
    end.line < start.line ||
    (end.line === start.line && end.character < start.character)
  ) {
    return undefined;
  }
  const startLine = start.line + 1;
  const endLine =
    end.line > start.line && end.character === 0 ? end.line : end.line + 1;
  return { start: startLine, end: Math.max(startLine, endLine) };
}

function isLocation(value: unknown): value is LocationLike {
  return (
    isRecord(value) &&
    isUri(value.uri) &&
    isRecord(value.range) &&
    isPosition(value.range.start) &&
    isPosition(value.range.end)
  );
}

function isUri(value: unknown): value is UriLike {
  return (
    isRecord(value) &&
    typeof value.scheme === "string" &&
    typeof value.fsPath === "string"
  );
}

function isPosition(value: unknown): value is PositionLike {
  return (
    isRecord(value) &&
    typeof value.line === "number" &&
    Number.isSafeInteger(value.line) &&
    value.line >= 0 &&
    typeof value.character === "number" &&
    Number.isSafeInteger(value.character) &&
    value.character >= 0
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}
