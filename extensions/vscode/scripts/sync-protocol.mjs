import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDirectory = path.dirname(fileURLToPath(import.meta.url));
const extensionRoot = path.resolve(scriptDirectory, "..");
const defaultCoreRoot = path.resolve(scriptDirectory, "../../../qwenpaw-core");
const coreRoot = path.resolve(process.argv[2] ?? defaultCoreRoot);
const sdkSourceRoot = path.join(coreRoot, "sdk/typescript/src");
const sdkFiles = ["protocol.ts", "rpcClient.ts", "appServerClient.ts"];
const lockPath = path.join(extensionRoot, "protocol-lock.json");
const sources = await Promise.all(
  sdkFiles.map(async (name) => ({
    name,
    contents: await readFile(path.join(sdkSourceRoot, name)),
  })),
);
const protocol = sources[0]?.contents;
if (!protocol) {
  throw new Error(`Missing protocol.ts in ${sdkSourceRoot}`);
}
const versionMatch = protocol
  .toString("utf8")
  .match(/PROTOCOL_VERSION = (\d+) as const/);

if (!versionMatch) {
  throw new Error(`Missing PROTOCOL_VERSION in ${sdkSourceRoot}`);
}

const destinationRoot = path.join(extensionRoot, "src/generated");
await mkdir(destinationRoot, { recursive: true });
for (const source of sources) {
  await writeFile(path.join(destinationRoot, source.name), source.contents);
}
const sdkHash = createHash("sha256");
for (const source of sources) {
  sdkHash.update(source.name);
  sdkHash.update(source.contents);
}
await writeFile(
  lockPath,
  `${JSON.stringify(
    {
      protocolVersion: Number(versionMatch[1]),
      sha256: createHash("sha256").update(protocol).digest("hex"),
      sdkSha256: sdkHash.digest("hex"),
    },
    null,
    2,
  )}\n`,
);
console.log(`Synced TypeScript SDK from ${sdkSourceRoot}`);
