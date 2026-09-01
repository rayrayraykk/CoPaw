import assert from "node:assert/strict";
import test from "node:test";

import { collectCursorPages } from "../src/pagination";

test("collects all cursor pages in server order", async () => {
  const cursors: Array<string | null> = [];
  const pages = new Map<
    string | null,
    {
      data: readonly number[];
      nextCursor: string | null;
    }
  >([
    [null, { data: [1, 2], nextCursor: "page-2" }],
    ["page-2", { data: [3], nextCursor: "page-3" }],
    ["page-3", { data: [4, 5], nextCursor: null }],
  ]);

  const data = await collectCursorPages(async (cursor) => {
    cursors.push(cursor);
    const page = pages.get(cursor);
    if (!page) {
      throw new Error("unexpected cursor");
    }
    return page;
  });

  assert.deepEqual(data, [1, 2, 3, 4, 5]);
  assert.deepEqual(cursors, [null, "page-2", "page-3"]);
});

test("rejects a repeated cursor", async () => {
  await assert.rejects(
    collectCursorPages(async (cursor) => ({
      data: [],
      nextCursor: cursor ?? "repeated",
    })),
    /repeated cursor: repeated/,
  );
});

test("rejects a result over the item limit", async () => {
  await assert.rejects(
    collectCursorPages(async () => ({ data: [1, 2, 3], nextCursor: null }), {
      maxItems: 2,
      maxPages: 2,
    }),
    /exceeded 2 items/,
  );
});

test("rejects endless empty pages", async () => {
  let nextCursor = 0;
  await assert.rejects(
    collectCursorPages(
      async () => {
        nextCursor += 1;
        return { data: [], nextCursor: String(nextCursor) };
      },
      { maxItems: 2, maxPages: 2 },
    ),
    /exceeded 2 pages/,
  );
});
