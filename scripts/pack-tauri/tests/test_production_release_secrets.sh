#!/usr/bin/env bash

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
CHECK_SCRIPT="${REPO_ROOT}/scripts/pack-tauri/check_production_release_secrets.sh"
TEST_ROOT="$(mktemp -d)"
trap 'rm -rf "${TEST_ROOT}"' EXIT

QWENPAW_REQUIRE_APPLE_RELEASE=0 bash "${CHECK_SCRIPT}" >/dev/null

missing_output="${TEST_ROOT}/missing.log"
if QWENPAW_REQUIRE_APPLE_RELEASE=1 \
    bash "${CHECK_SCRIPT}" >"${missing_output}" 2>&1; then
    echo "ERROR: production release passed without secrets" >&2
    exit 1
fi
grep -Fq 'requires QWENPAW_DASHSCOPE_API_KEY' "${missing_output}"

required=(
    QWENPAW_DASHSCOPE_API_KEY
    APPLE_CERTIFICATE_P12
    APPLE_CERTIFICATE_PASSWORD
    APPLE_SIGNING_IDENTITY
    APPLE_ID
    APPLE_TEAM_ID
    APPLE_APP_PASSWORD
    TAURI_SIGNING_PRIVATE_KEY
    TAURI_UPDATER_PUBKEY
    TAURI_UPDATER_ENDPOINTS
)
for name in "${required[@]}"; do
    export "${name}=test-value"
done
QWENPAW_REQUIRE_APPLE_RELEASE=1 bash "${CHECK_SCRIPT}" >/dev/null

if QWENPAW_REQUIRE_APPLE_RELEASE=invalid \
    bash "${CHECK_SCRIPT}" >/dev/null 2>&1; then
    echo "ERROR: invalid production release mode passed validation" >&2
    exit 1
fi

echo "production release secret preflight tests passed"
