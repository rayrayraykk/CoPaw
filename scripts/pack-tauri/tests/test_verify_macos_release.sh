#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
VERIFY_SCRIPT="${REPO_ROOT}/scripts/pack-tauri/verify_macos_release.sh"
TEST_ROOT="$(mktemp -d)"
trap 'rm -rf "${TEST_ROOT}"' EXIT

FAKE_BIN="${TEST_ROOT}/bin"
APP_PATH="${TEST_ROOT}/QwenPaw Desktop.app"
COMMAND_LOG="${TEST_ROOT}/commands.log"
mkdir -p "${FAKE_BIN}" "${APP_PATH}"
touch "${COMMAND_LOG}"

printf '%s\n' \
    '#!/usr/bin/env bash' \
    'echo Darwin' >"${FAKE_BIN}/uname"
printf '%s\n' \
    '#!/usr/bin/env bash' \
    'printf "codesign %s\\n" "$*" >>"${COMMAND_LOG}"' \
    'if [[ "$1" == "-dv" ]]; then' \
    '  echo "Authority=Developer ID Application: QwenPaw Test" >&2' \
    'fi' >"${FAKE_BIN}/codesign"
printf '%s\n' \
    '#!/usr/bin/env bash' \
    'printf "xcrun %s\\n" "$*" >>"${COMMAND_LOG}"' >"${FAKE_BIN}/xcrun"
printf '%s\n' \
    '#!/usr/bin/env bash' \
    'printf "spctl %s\\n" "$*" >>"${COMMAND_LOG}"' >"${FAKE_BIN}/spctl"
chmod +x "${FAKE_BIN}"/*

PATH="${FAKE_BIN}:${PATH}" \
COMMAND_LOG="${COMMAND_LOG}" \
APPLE_SIGNING_IDENTITY="Developer ID Application: QwenPaw Test" \
    bash "${VERIFY_SCRIPT}" "${APP_PATH}"

grep -Fq 'codesign --verify --deep --strict --verbose=2' "${COMMAND_LOG}"
grep -Fq 'codesign -dv --verbose=4' "${COMMAND_LOG}"
grep -Fq 'xcrun stapler validate' "${COMMAND_LOG}"
grep -Fq 'spctl --assess --type execute --verbose=4' "${COMMAND_LOG}"

if PATH="${FAKE_BIN}:${PATH}" \
    COMMAND_LOG="${COMMAND_LOG}" \
    APPLE_SIGNING_IDENTITY="-" \
    bash "${VERIFY_SCRIPT}" "${APP_PATH}" >/dev/null 2>&1; then
    echo "ERROR: ad-hoc signing identity passed production verification" >&2
    exit 1
fi

echo "macOS release verifier tests passed"
