import { createHash } from "node:crypto";
import { existsSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { join } from "node:path";

export type CoreExecutableSource = "configured" | "bundled" | "path";

export interface CoreExecutable {
  readonly path: string;
  readonly source: CoreExecutableSource;
}

interface CoreRelease {
  readonly version: string;
  readonly protocolVersion: number;
}

interface BundledCoreManifest extends CoreRelease {
  readonly target: string;
  readonly executable: string;
  readonly sha256: string;
}

interface ResolveCoreExecutableOptions {
  readonly configuredPath: string | undefined;
  readonly extensionPath: string;
  readonly platform?: NodeJS.Platform;
  readonly arch?: string;
}

const SUPPORTED_TARGETS = new Set([
  "darwin-arm64",
  "darwin-x64",
  "linux-x64",
  "win32-x64",
]);

export async function resolveCoreExecutable(
  options: ResolveCoreExecutableOptions,
): Promise<CoreExecutable> {
  const configuredPath = options.configuredPath?.trim();
  if (configuredPath) {
    return { path: configuredPath, source: "configured" };
  }

  const platform = options.platform ?? process.platform;
  const arch = options.arch ?? process.arch;
  const target = `${platform}-${arch}`;
  const executable = platform === "win32" ? "qwenpaw-core.exe" : "qwenpaw-core";
  const coreRoot = join(options.extensionPath, "resources", "core");
  const manifestPath = join(coreRoot, "manifest.json");
  if (!SUPPORTED_TARGETS.has(target) || !existsSync(manifestPath)) {
    return { path: executable, source: "path" };
  }

  const release = await readJson<CoreRelease>(
    join(options.extensionPath, "core-release.json"),
    "Core release",
  );
  const manifest = await readJson<BundledCoreManifest>(
    manifestPath,
    "bundled Core manifest",
  );
  validateRelease(release);
  validateManifest(manifest, release, target, executable);

  const bundledPath = join(coreRoot, target, executable);
  let binary: Buffer;
  try {
    binary = await readFile(bundledPath);
  } catch (error) {
    throw new Error(
      `Bundled QwenPaw Core is missing at ${bundledPath}: ${String(error)}`,
    );
  }
  const digest = createHash("sha256").update(binary).digest("hex");
  if (digest !== manifest.sha256) {
    throw new Error(`Bundled QwenPaw Core checksum mismatch for ${target}`);
  }
  return { path: bundledPath, source: "bundled" };
}

async function readJson<T>(path: string, label: string): Promise<T> {
  try {
    return JSON.parse(await readFile(path, "utf8")) as T;
  } catch (error) {
    throw new Error(`Invalid ${label} at ${path}: ${String(error)}`);
  }
}

function validateRelease(value: CoreRelease): void {
  if (
    typeof value.version !== "string" ||
    !value.version ||
    !Number.isInteger(value.protocolVersion)
  ) {
    throw new Error("Invalid Core release metadata");
  }
}

function validateManifest(
  manifest: BundledCoreManifest,
  release: CoreRelease,
  target: string,
  executable: string,
): void {
  if (
    manifest.version !== release.version ||
    manifest.protocolVersion !== release.protocolVersion ||
    manifest.target !== target ||
    manifest.executable !== executable ||
    !/^[0-9a-f]{64}$/.test(manifest.sha256)
  ) {
    throw new Error(`Bundled QwenPaw Core manifest does not match ${target}`);
  }
}
