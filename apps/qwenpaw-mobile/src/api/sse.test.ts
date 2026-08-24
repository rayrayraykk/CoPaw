import assert from "node:assert/strict";
import test from "node:test";

import { eventText, SseParser } from "./sse";

test("parses events split across network chunks", () => {
  const parser = new SseParser();
  assert.deepEqual(
    parser.push("event: message\ndata: {\"object\":\"content\","),
    [],
  );
  const events = parser.push("\"delta\":true,\"text\":\"hello\"}\n\n");
  assert.equal(events.length, 1);
  assert.equal(events[0].event, "message");
  assert.equal(eventText(events[0]), "hello");
});

test("joins multi-line event data", () => {
  const parser = new SseParser();
  const [event] = parser.push("data: hello\ndata: world\n\n");
  assert.equal(event.data, "hello\nworld");
});

test("returns only incremental AgentScope text", () => {
  assert.equal(eventText({
    data: JSON.stringify({
      object: "content",
      delta: true,
      text: "next",
    }),
  }), "next");
  assert.equal(eventText({
    data: JSON.stringify({
      object: "response",
      status: "completed",
      output: [{ content: [{ text: "complete answer" }] }],
    }),
  }), "");
});
