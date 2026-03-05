#!/usr/bin/env bash
# Build CoPaw macOS .app and optionally create DMG. Run from repo root.
# Usage: ./scripts/pack/build_macos.sh [--dmg]
set -e
cd "$(dirname "$0")/../.."
ENTRY=scripts/pack/copaw_desktop_main.py
OUT=dist
APP_NAME=CoPaw

pip install -e ".[desktop]" nuitka ordered-set -q
echo "Building .app with Nuitka..."
python -m nuitka \
  --standalone \
  --macos-create-app-bundle \
  --macos-app-mode=gui \
  --output-dir="$OUT" \
  --output-filename="$APP_NAME" \
  --include-package=copaw \
  --include-package-data=copaw \
  --assume-yes-for-downloads \
  "$ENTRY"

echo "Built: $OUT/$APP_NAME.app"
if [[ "${1:-}" == "--dmg" ]]; then
  mkdir -p dmg_src
  cp -R "$OUT/$APP_NAME.app" dmg_src/
  if [[ ! -d create_dmg_repo ]]; then
    git clone --depth 1 https://github.com/create-dmg/create-dmg.git create_dmg_repo
  fi
  ./create_dmg_repo/create-dmg \
    --volname "$APP_NAME" \
    --window-size 600 400 \
    --icon "$APP_NAME.app" 150 190 \
    --app-drop-link 450 190 \
    "$OUT/CoPaw.dmg" dmg_src/
  echo "Built: $OUT/CoPaw.dmg"
fi
