# Build and stage the Rust Core plus the unchanged Console for Tauri.

param()

$ErrorActionPreference = "Stop"
$REPO_ROOT = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$CORE_ROOT = Join-Path $REPO_ROOT "qwenpaw-core"
$CORE_TARGET_DIR = if ($env:QWENPAW_CORE_TARGET_DIR) {
    $env:QWENPAW_CORE_TARGET_DIR
} else {
    Join-Path $CORE_ROOT "target"
}
if (-not [System.IO.Path]::IsPathRooted($CORE_TARGET_DIR)) {
    $CORE_TARGET_DIR = Join-Path $REPO_ROOT $CORE_TARGET_DIR
}
$DEST = Join-Path $REPO_ROOT "console\src-tauri\binaries\qwenpaw-core"
$CONSOLE_DIST = Join-Path $REPO_ROOT "console\dist"

if (-not (Test-Path (Join-Path $CONSOLE_DIST "index.html") -PathType Leaf)) {
    throw "Console production build is missing; run npm run build:prod first"
}

Write-Host "== Building Rust Core sidecar ==" -ForegroundColor Yellow
$previousTargetDir = $env:CARGO_TARGET_DIR
try {
    $env:CARGO_TARGET_DIR = $CORE_TARGET_DIR
    & cargo build `
        --manifest-path (Join-Path $CORE_ROOT "Cargo.toml") `
        --release `
        --locked `
        -p qwenpaw-cli
    if ($LASTEXITCODE -ne 0) {
        throw "Rust Core release build failed"
    }
} finally {
    $env:CARGO_TARGET_DIR = $previousTargetDir
}

$CORE_BINARY = Join-Path $CORE_TARGET_DIR "release\qwenpaw-core.exe"
if (-not (Test-Path $CORE_BINARY -PathType Leaf)) {
    throw "Rust Core executable not found at $CORE_BINARY"
}
& $CORE_BINARY --version
if ($LASTEXITCODE -ne 0) {
    throw "Rust Core release executable failed its version check"
}

if (Test-Path $DEST) {
    Remove-Item -Recurse -Force $DEST
}
New-Item -ItemType Directory -Force -Path (Join-Path $DEST "console") | Out-Null
Copy-Item -Force $CORE_BINARY (Join-Path $DEST "qwenpaw-core.exe")
Copy-Item -Recurse -Force (Join-Path $CONSOLE_DIST "*") (Join-Path $DEST "console")
$sourceHash = (Get-FileHash -Algorithm SHA256 $CORE_BINARY).Hash
$stagedHash = (
    Get-FileHash -Algorithm SHA256 (Join-Path $DEST "qwenpaw-core.exe")
).Hash
if ($sourceHash -ne $stagedHash) {
    throw "Staged Rust Core does not match the release executable"
}
Write-Host "Rust Core Tauri resource staged at $DEST" -ForegroundColor Green
