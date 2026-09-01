#!/usr/bin/env bash
# Build and stage the Rust Core plus the unchanged Console for Tauri.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
CORE_ROOT="${REPO_ROOT}/qwenpaw-core"
CORE_TARGET_DIR="${QWENPAW_CORE_TARGET_DIR:-${CORE_ROOT}/target}"
DEST="${REPO_ROOT}/console/src-tauri/binaries/qwenpaw-core"
EXPECTED_DEST="${REPO_ROOT}/console/src-tauri/binaries/qwenpaw-core"

if [[ "${DEST}" != "${EXPECTED_DEST}" ]]; then
    echo "ERROR: refusing to stage Rust Core outside the Tauri resource directory" >&2
    exit 1
fi
if [[ ! -f "${REPO_ROOT}/console/dist/index.html" ]]; then
    echo "ERROR: Console production build is missing; run npm run build:prod first" >&2
    exit 1
fi

echo "== Building Rust Core sidecar =="
CARGO_TARGET_DIR="${CORE_TARGET_DIR}" cargo build \
    --manifest-path "${CORE_ROOT}/Cargo.toml" \
    --release \
    --locked \
    -p qwenpaw-cli

CORE_BINARY="${CORE_TARGET_DIR}/release/qwenpaw-core"
if [[ ! -x "${CORE_BINARY}" ]]; then
    echo "ERROR: Rust Core executable not found at ${CORE_BINARY}" >&2
    exit 1
fi
"${CORE_BINARY}" --version

rm -rf "${DEST}"
mkdir -p "${DEST}/console"
cp "${CORE_BINARY}" "${DEST}/qwenpaw-core"
cp -R "${REPO_ROOT}/console/dist/." "${DEST}/console/"
chmod +x "${DEST}/qwenpaw-core"
if ! cmp -s "${CORE_BINARY}" "${DEST}/qwenpaw-core"; then
    echo "ERROR: staged Rust Core does not match the release binary" >&2
    exit 1
fi
echo "Rust Core Tauri resource staged at ${DEST}"
