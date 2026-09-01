import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const extensionRoot = path.resolve(scriptDirectory, "..");
const defaultCoreRoot = path.resolve(scriptDirectory, "../../../qwenpaw-core");
const coreRoot = path.resolve(process.argv[2] ?? defaultCoreRoot);
const source = path.join(coreRoot, "sdk/typescript/src/protocol.ts");
const destination = path.join(
  extensionRoot,
  "src/generated/protocol.ts",
);
const lockPath = path.join(extensionRoot, "protocol-lock.json");
const contents = await readFile(source);
const versionMatch = contents
  .toString("utf8")
  .match(/PROTOCOL_VERSION = (\d+) as const/);

if (!versionMatch) {
  throw new Error(`Missing PROTOCOL_VERSION in ${source}`);
}

await mkdir(path.dirname(destination), { recursive: true });
await writeFile(destination, contents);
await writeFile(
  lockPath,
  `${JSON.stringify(
    {
      protocolVersion: Number(versionMatch[1]),
      sha256: createHash("sha256").update(contents).digest("hex"),
    },
    null,
    2,
  )}\n`,
);
console.log(`Synced App Protocol from ${source}`);
