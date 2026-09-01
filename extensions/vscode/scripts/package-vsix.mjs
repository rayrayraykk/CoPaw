import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const extensionRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const target = process.env.QWENPAW_VSCODE_TARGET;
if (!target) {
  throw new Error("QWENPAW_VSCODE_TARGET is required");
}
const vsce = join(extensionRoot, "node_modules", "@vscode", "vsce", "vsce");
const result = spawnSync(
  process.execPath,
  [vsce, "package", "--no-dependencies", "--target", target],
  { cwd: extensionRoot, stdio: "inherit" },
);
if (result.error) {
  throw result.error;
}
if (result.status !== 0) {
  process.exit(result.status ?? 1);
}
