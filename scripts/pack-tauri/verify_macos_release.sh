#!/usr/bin/env bash
# Verify that a macOS app is Developer ID signed, notarized, and accepted by
# Gatekeeper. This script never signs or mutates the bundle.

set -euo pipefail

APP_PATH="${1:?Usage: verify_macos_release.sh <app-path>}"

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "ERROR: macOS release verification must run on Darwin" >&2
    exit 1
fi

if [[ ! -d "${APP_PATH}" || "${APP_PATH}" != *.app ]]; then
    echo "ERROR: macOS app bundle not found: ${APP_PATH}" >&2
    exit 1
fi

if [[ -z "${APPLE_SIGNING_IDENTITY:-}" || "${APPLE_SIGNING_IDENTITY}" == "-" ]]; then
    echo "ERROR: production release requires a Developer ID signing identity" >&2
    exit 1
fi

for command in codesign xcrun spctl; do
    if ! command -v "${command}" >/dev/null 2>&1; then
        echo "ERROR: ${command} is required for macOS release verification" >&2
        exit 1
    fi
done

codesign --verify --deep --strict --verbose=2 "${APP_PATH}"
signature_info="$(codesign -dv --verbose=4 "${APP_PATH}" 2>&1)"
if ! grep -q '^Authority=Developer ID Application:' <<<"${signature_info}"; then
    echo "ERROR: macOS app is not signed with a Developer ID Application certificate" >&2
    exit 1
fi
xcrun stapler validate "${APP_PATH}"
spctl --assess --type execute --verbose=4 "${APP_PATH}"

echo "macOS production signature, notarization ticket, and Gatekeeper verified"
