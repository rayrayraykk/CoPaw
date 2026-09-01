#!/usr/bin/env bash

set -euo pipefail

CORE_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
CHECK_SCRIPT="${CORE_ROOT}/scripts/check_apple_release_secrets.sh"
TEST_ROOT="$(mktemp -d)"
trap 'rm -rf "${TEST_ROOT}"' EXIT

missing_output="${TEST_ROOT}/missing.log"
if env -i PATH="${PATH}" QWENPAW_REQUIRE_APPLE_RELEASE=1 \
    bash "${CHECK_SCRIPT}" \
    >"${missing_output}" 2>&1; then
    echo "ERROR: Core release passed without Apple secrets" >&2
    exit 1
fi
grep -Fq 'requires APPLE_CERTIFICATE_P12' "${missing_output}"

env -i PATH="${PATH}" QWENPAW_REQUIRE_APPLE_RELEASE=0 \
    bash "${CHECK_SCRIPT}" >/dev/null

success_output="${TEST_ROOT}/success.log"
env -i \
    PATH="${PATH}" \
    QWENPAW_REQUIRE_APPLE_RELEASE=1 \
    APPLE_CERTIFICATE_P12='secret-sentinel' \
    APPLE_CERTIFICATE_PASSWORD='secret-sentinel' \
    APPLE_SIGNING_IDENTITY='Developer ID Application: QwenPaw Test' \
    APPLE_ID='secret-sentinel' \
    APPLE_TEAM_ID='secret-sentinel' \
    APPLE_APP_PASSWORD='secret-sentinel' \
    bash "${CHECK_SCRIPT}" >"${success_output}" 2>&1
if grep -Fq 'secret-sentinel' "${success_output}"; then
    echo "ERROR: Core release preflight printed a secret value" >&2
    exit 1
fi

if env -i \
    PATH="${PATH}" \
    QWENPAW_REQUIRE_APPLE_RELEASE=1 \
    APPLE_CERTIFICATE_P12='test-value' \
    APPLE_CERTIFICATE_PASSWORD='test-value' \
    APPLE_SIGNING_IDENTITY='-' \
    APPLE_ID='test-value' \
    APPLE_TEAM_ID='test-value' \
    APPLE_APP_PASSWORD='test-value' \
    bash "${CHECK_SCRIPT}" >/dev/null 2>&1; then
    echo "ERROR: Core release accepted ad-hoc Apple signing" >&2
    exit 1
fi

if env -i PATH="${PATH}" QWENPAW_REQUIRE_APPLE_RELEASE=invalid \
    bash "${CHECK_SCRIPT}" >/dev/null 2>&1; then
    echo "ERROR: Core release accepted an invalid signing mode" >&2
    exit 1
fi

echo "Apple Core release preflight tests passed"
