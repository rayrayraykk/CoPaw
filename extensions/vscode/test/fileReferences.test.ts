import assert from "node:assert/strict";
import test from "node:test";

import { buildUserInput } from "../src/fileReferences";

test("converts file URIs and locations without reading content", () => {
  assert.deepEqual(
    buildUserInput("Review these files", [
      { value: { scheme: "file", fsPath: "/workspace/README.md" } },
      {
        value: {
          uri: { scheme: "file", fsPath: "/workspace/src/lib.rs" },
          range: {
            start: { line: 9, character: 2 },
            end: { line: 19, character: 8 },
          },
        },
      },
    ]),
    [
      { type: "text", text: "Review these files" },
      {
        type: "fileReference",
        path: "/workspace/README.md",
        startLine: null,
        endLine: null,
      },
      {
        type: "fileReference",
        path: "/workspace/src/lib.rs",
        startLine: 10,
        endLine: 20,
      },
    ],
  );
});

test("converts an exclusive line-boundary end to an inclusive range", () => {
  assert.deepEqual(
    buildUserInput("Review", [
      {
        value: {
          uri: { scheme: "file", fsPath: "/workspace/src/lib.rs" },
          range: {
            start: { line: 4, character: 0 },
            end: { line: 8, character: 0 },
          },
        },
      },
    ]),
    [
      { type: "text", text: "Review" },
      {
        type: "fileReference",
        path: "/workspace/src/lib.rs",
        startLine: 5,
        endLine: 8,
      },
    ],
  );
});

test("deduplicates files and ignores unsupported references", () => {
  const file = { scheme: "file", fsPath: "/workspace/src/lib.rs" };
  assert.deepEqual(
    buildUserInput("Review", [
      { value: file },
      { value: file },
      { value: { scheme: "https", fsPath: "/not-local" } },
      { value: "variable output" },
      { value: { future: true } },
      {
        value: {
          uri: file,
          range: {
            start: { line: 3, character: 5 },
            end: { line: 2, character: 1 },
          },
        },
      },
    ]),
    [
      { type: "text", text: "Review" },
      {
        type: "fileReference",
        path: "/workspace/src/lib.rs",
        startLine: null,
        endLine: null,
      },
    ],
  );
});

test("rejects more than 32 unique local file references", () => {
  const references = Array.from({ length: 33 }, (_, index) => ({
    value: {
      scheme: "file",
      fsPath: `/workspace/file-${String(index)}.txt`,
    },
  }));

  assert.throws(
    () => buildUserInput("Review", references),
    /at most 32 file references/,
  );
});
