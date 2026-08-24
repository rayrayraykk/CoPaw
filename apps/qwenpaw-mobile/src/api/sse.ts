export interface SseEvent {
  event?: string;
  data: string;
}

export class SseParser {
  private buffer = "";

  push(chunk: string): SseEvent[] {
    this.buffer += chunk.replace(/\r\n/g, "\n");
    const blocks = this.buffer.split("\n\n");
    this.buffer = blocks.pop() ?? "";
    return blocks.flatMap(parseBlock);
  }

  finish(): SseEvent[] {
    const block = this.buffer;
    this.buffer = "";
    return block.trim() ? parseBlock(block) : [];
  }
}

function parseBlock(block: string): SseEvent[] {
  let event: string | undefined;
  const data: string[] = [];
  for (const line of block.split("\n")) {
    if (line.startsWith(":")) continue;
    const separator = line.indexOf(":");
    const field = separator < 0 ? line : line.slice(0, separator);
    const value = separator < 0
      ? ""
      : line.slice(separator + 1).replace(/^ /, "");
    if (field === "event") event = value;
    if (field === "data") data.push(value);
  }
  return data.length ? [{ event, data: data.join("\n") }] : [];
}

export function eventText(event: SseEvent): string {
  if (event.data === "[DONE]") return "";
  try {
    const payload = JSON.parse(event.data) as Record<string, unknown>;
    if (payload.object === "content" && payload.delta === true) {
      return typeof payload.text === "string" ? payload.text : "";
    }
    if (payload.object === "message" && payload.status === "in_progress") {
      return messageDeltaText(payload.content);
    }
    return "";
  } catch {
    return event.data;
  }
}

function messageDeltaText(content: unknown): string {
  if (!Array.isArray(content)) return "";
  return content.map((item) => {
    if (!item || typeof item !== "object") return "";
    const part = item as Record<string, unknown>;
    return part.delta === true && typeof part.text === "string"
      ? part.text
      : "";
  }).join("");
}
