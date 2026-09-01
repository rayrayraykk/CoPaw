export interface CursorPage<T> {
  readonly data: readonly T[];
  readonly nextCursor: string | null;
}

export interface CursorLimits {
  readonly maxItems: number;
  readonly maxPages: number;
}

const DEFAULT_LIMITS: CursorLimits = {
  maxItems: 10_000,
  maxPages: 100,
};

export async function collectCursorPages<T>(
  fetchPage: (cursor: string | null) => Promise<CursorPage<T>>,
  limits: CursorLimits = DEFAULT_LIMITS,
): Promise<readonly T[]> {
  const data: T[] = [];
  const seenCursors = new Set<string>();
  let cursor: string | null = null;
  for (let pageNumber = 0; pageNumber < limits.maxPages; pageNumber += 1) {
    const page = await fetchPage(cursor);
    if (page.data.length > limits.maxItems - data.length) {
      throw new Error(
        `QwenPaw Core pagination exceeded ${limits.maxItems} items`,
      );
    }
    data.push(...page.data);
    if (page.nextCursor === null) {
      return data;
    }
    if (seenCursors.has(page.nextCursor)) {
      throw new Error(
        `QwenPaw Core returned a repeated cursor: ${page.nextCursor}`,
      );
    }
    seenCursors.add(page.nextCursor);
    cursor = page.nextCursor;
  }
  throw new Error(
    `QwenPaw Core pagination exceeded ${limits.maxPages} pages`,
  );
}
