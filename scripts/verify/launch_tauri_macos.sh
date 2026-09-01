#!/usr/bin/env bash
# Unpack, launch the Tauri macOS shell, and wait for the backend to be ready.
# Outputs BASE_URL to $GITHUB_ENV for subsequent steps.
set -euo pipefail

# 1. Unpack the freshly built Tauri zip.
echo "[launch_tauri_macos] Unpacking zip..."
mkdir -p dist/verify-tauri
unzip -q dist/QwenPaw-Tauri-*-macOS.zip -d dist/verify-tauri
APP="$(find dist/verify-tauri -maxdepth 3 -name '*.app' -type d | head -1)"
if [ -z "$APP" ]; then
  echo "::error::Tauri .app not found inside zip"
  exit 1
fi
echo "[launch_tauri_macos] Found app: $APP"

# 2. Verify the app contains the Rust-only payload before launch.
CORE="$APP/Contents/Resources/binaries/qwenpaw-core/qwenpaw-core"
HELPER="$APP/Contents/MacOS/qwenpaw-computer-use-helper"
for required in "$CORE" "$HELPER"; do
  if [ ! -x "$required" ]; then
    echo "::error::Required Rust Desktop executable is missing: $required"
    exit 1
  fi
done
for legacy in \
  "$APP/Contents/Resources/binaries/qwenpaw-backend" \
  "$APP/Contents/Resources/binaries/python-runtime" \
  "$APP/Contents/Resources/binaries/node-runtime"; do
  if [ -e "$legacy" ]; then
    echo "::error::Legacy runtime must not be present: $legacy"
    exit 1
  fi
done
echo "[launch_tauri_macos] Rust-only Desktop payload verified"

# 3. Remove macOS quarantine (CI download marks it).
xattr -dr com.apple.quarantine "$APP" 2>/dev/null || true

# 4. Launch the full Tauri shell (matches real user double-click).
echo "[launch_tauri_macos] Launching Tauri shell..."
open "$APP"
echo "[launch_tauri_macos] open exit=$?"
sleep 3
echo "[launch_tauri_macos] Process snapshot after launch:"
ps -ef | grep -iE "qwenpaw|tauri" | grep -v grep || echo "  (no matching processes)"

# 5. Wait for Rust Core to publish its port in the versioned fresh-start data
#    directory and respond.
CORE_HOME="$HOME/Library/Application Support/com.qwenpaw.desktop/rust-core-v1"
PORT_FILE="$CORE_HOME/desktop_port"
PORT=""
for i in $(seq 1 60); do
  if [ -f "$PORT_FILE" ]; then
    PORT="$(cat "$PORT_FILE" | tr -d '[:space:]')"
    if [ -n "$PORT" ] && curl -sf "http://127.0.0.1:$PORT/api/version" >/dev/null; then
      echo "[launch_tauri_macos] Tauri app ready on port $PORT after ~$((i*2))s"
      break
    fi
  fi
  if [ "$i" = "60" ]; then
    echo "::error::Tauri app did not start within 120s"
    echo "[debug] PORT_FILE=$PORT_FILE exists=$([ -f "$PORT_FILE" ] && echo yes || echo no)"
    echo "[debug] Rust Core data directory contents:"
    ls -la "$CORE_HOME/" 2>/dev/null || echo "  (missing)"
    echo "[debug] Rust Core files (top 30):"
    find "$CORE_HOME" -maxdepth 4 -type f 2>/dev/null | head -30 || true
    echo "[debug] desktop.log tail (if exists):"
    tail -50 "$HOME/Library/Logs/com.qwenpaw.desktop/qwenpaw-desktop.log" \
      2>/dev/null || echo "  (no desktop log)"
    echo "[debug] Process list:"
    ps -ef | grep -iE "qwenpaw|tauri" | grep -v grep || echo "  (no matching processes)"
    exit 1
  fi
  sleep 2
done

# 6. Auto-init creates BOOTSTRAP.md during startup. Remove it afterwards so
#    the verifier can drive the agent in normal QA mode.
rm -f "$CORE_HOME/workspace/BOOTSTRAP.md"

export BASE_URL="http://127.0.0.1:$PORT"
echo "BASE_URL=$BASE_URL" >> "$GITHUB_ENV"
echo "$BASE_URL"
