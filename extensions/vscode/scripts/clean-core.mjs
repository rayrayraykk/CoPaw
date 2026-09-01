import { rm } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const extensionRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
await rm(join(extensionRoot, "resources", "core"), {
  recursive: true,
  force: true,
});
