#!/usr/bin/env bash
# Build the Rust-only QwenPaw Desktop package for macOS.
#
# Usage:
#   ./scripts/pack-tauri/build_macos_rust_core.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$REPO_ROOT"

VERSION=$(sed -n 's/^__version__[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' src/qwenpaw/__version__.py)

echo "========================================="
echo "QwenPaw Tauri Build - macOS (Rust Core)"
echo "========================================="
echo "Version: ${VERSION}"
echo ""

SIGN_MACOS_BUNDLE="${REPO_ROOT}/scripts/pack-tauri/sign_macos_bundle.sh"

# Step 0: Prerequisites
echo "== Step 0: Checking Prerequisites =="
missing=()

if command -v npm &>/dev/null; then
    echo "  [OK] npm ($(npm --version))"
else
    echo "  [MISSING] npm"
    echo "    Install Node.js: https://nodejs.org/"
    missing+=("npm")
fi

if command -v rustc &>/dev/null; then
    echo "  [OK] rustc ($(rustc --version))"
else
    echo "  [MISSING] rustc (Rust)"
    echo "    Install: https://rustup.rs"
    missing+=("rustc")
fi

if [ ${#missing[@]} -gt 0 ]; then
    echo ""
    echo "Missing prerequisites: ${missing[*]}"
    echo "Install the missing tools and re-run this script."
    exit 1
fi
echo ""

if [ ! -f "${SIGN_MACOS_BUNDLE}" ]; then
    echo "ERROR: macOS signing helper not found at ${SIGN_MACOS_BUNDLE}"
    exit 1
fi

if [ -z "${APPLE_SIGNING_IDENTITY:-}" ] && [ -z "${APPLE_CERTIFICATE:-}" ]; then
    # Keep the app, Rust Core, and native helper consistently ad-hoc signed
    # when a Developer ID certificate is not configured. This is suitable for
    # local validation only and is not a notarized release.
    export APPLE_SIGNING_IDENTITY="-"
    echo "Using ad-hoc macOS code signing"
fi
echo ""

# Step 1: Build console static assets
echo "== Step 1: Building Console Static Assets =="
cd console
npm ci
echo "Generating Tauri icons..."
npm exec -- tauri icon ../scripts/pack/assets/icon.svg
echo "Syncing Tauri version..."
node ../scripts/pack-tauri/sync_tauri_version.mjs
echo "Building console frontend..."
npm run build:prod
cd ..
echo "Console static assets built"
echo ""

echo "== Step 1b: Staging Rust Core sidecar =="
bash scripts/pack-tauri/stage_rust_core.sh
echo "Rust Core sidecar staged"
echo ""

# Step 2: Build Tauri app and its native Computer Use helper
echo "== Step 2: Building Tauri App =="
BUNDLE_DIR="${REPO_ROOT}/console/src-tauri/target/release/bundle"
rm -rf "${BUNDLE_DIR}/dmg" "${BUNDLE_DIR}/macos"
cd console
echo "Building for macOS..."
npm exec -- tauri build \
    --config src-tauri/tauri.version.conf.json \
    --bundles app
cd ..
echo "Tauri app built"
echo ""

APP_PATH="${BUNDLE_DIR}/macos/QwenPaw Desktop.app"
if [ ! -d "${APP_PATH}" ]; then
    echo "ERROR: No Tauri macOS app found at ${APP_PATH}"
    exit 1
fi
HELPER_PATH="${APP_PATH}/Contents/MacOS/qwenpaw-computer-use-helper"
if [ ! -x "${HELPER_PATH}" ]; then
    echo "ERROR: Computer Use helper was not bundled at ${HELPER_PATH}"
    exit 1
fi

echo "== Step 2b: Signing Final macOS App =="
bash "${SIGN_MACOS_BUNDLE}" \
    "${APP_PATH}" \
    "${APPLE_SIGNING_IDENTITY}"
echo "Final macOS app signed and verified"
echo ""

# Step 3: Collect distribution artifacts
echo "== Step 3: Collecting Distribution Artifacts =="
DIST="${DIST:-dist}"
if [[ "${DIST}" = /* ]]; then
    DIST_ROOT="${DIST}"
else
    DIST_ROOT="${REPO_ROOT}/${DIST}"
fi
DIST_DIR="${DIST_ROOT}/tauri-macos"
rm -rf "${DIST_DIR}"
mkdir -p "${DIST_DIR}"

# Match the legacy macOS package shape: one zip containing one .app bundle.
cp -R "${APP_PATH}" "${DIST_DIR}/"
STAGED_APP_PATH="${DIST_DIR}/$(basename "${APP_PATH}")"
echo ".app copied to ${STAGED_APP_PATH}"

# Create ZIP archive
ZIP_NAME="${DIST_ROOT}/QwenPaw-Tauri-${VERSION}-macOS.zip"
if [ -f "${ZIP_NAME}" ]; then
    rm -f "${ZIP_NAME}"
fi
if command -v ditto &>/dev/null; then
    ditto -c -k --sequesterRsrc --keepParent "${STAGED_APP_PATH}" "${ZIP_NAME}"
else
    cd "${DIST_DIR}"
    zip -r "${ZIP_NAME}" "$(basename "${STAGED_APP_PATH}")"
    cd "${REPO_ROOT}"
fi

if [ -f "${ZIP_NAME}" ]; then
    SIZE=$(du -sh "${ZIP_NAME}" | cut -f1)
    echo "Created ${ZIP_NAME} (${SIZE})"
else
    echo "ERROR: Failed to create ZIP archive"
    exit 1
fi
echo ""

UPDATER_NAME="${DIST_ROOT}/QwenPaw-Tauri-${VERSION}-macOS.app.tar.gz"
if [ -n "${TAURI_SIGNING_PRIVATE_KEY:-}" ]; then
    case "$(uname -m)" in
        arm64 | aarch64) UPDATER_TARGET="darwin-aarch64" ;;
        *) UPDATER_TARGET="darwin-x86_64" ;;
    esac
    python \
        "${REPO_ROOT}/scripts/pack-tauri/generate_update_manifest.py" \
        stage \
        --bundle-dir "${BUNDLE_DIR}/macos" \
        --pattern '*.app.tar.gz' \
        --target "${UPDATER_TARGET}" \
        --output "${UPDATER_NAME}" \
        --pubkey-config \
        "${REPO_ROOT}/console/src-tauri/tauri.version.conf.json"
    UPDATER_RESULT="${UPDATER_NAME}"
else
    UPDATER_RESULT="not generated (updater signing key is not set)"
    echo "Skipping Tauri updater artifact staging: ${UPDATER_RESULT}"
fi

echo ""
echo "========================================="
echo "Build Complete!"
echo "========================================="
echo "App:          ${APP_PATH}"
echo "Distribution: ${DIST_DIR}"
echo "Archive:      ${ZIP_NAME}"
echo "Updater:      ${UPDATER_RESULT}"
echo ""
echo "Test: open \"${STAGED_APP_PATH}\""
echo ""
