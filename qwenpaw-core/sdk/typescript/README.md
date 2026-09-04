# QwenPaw TypeScript SDK

Typed Node.js client for `qwenpaw-core app-server`. The SDK owns App Protocol
transport, initialization, request correlation, and notifications. Agent logic
continues to run exclusively in Rust Core.

```typescript
import { QwenPaw } from "@qwenpaw/sdk";

const qwenpaw = await QwenPaw.start({
  clientInfo: {
    name: "example",
    title: "QwenPaw SDK Example",
    version: "0.2.0",
  },
});

const thread = await qwenpaw.startThread({ workspaceRoot: process.cwd() });
const result = await thread.run("Summarize this repository");
console.log(result.finalResponse);
qwenpaw.dispose();
```

Run `npm run check` to build the package and verify it against the shared App
Protocol fixtures.
