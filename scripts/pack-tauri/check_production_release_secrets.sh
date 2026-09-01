#!/usr/bin/env bash
# Fail closed before a production Desktop build when release credentials or
# updater configuration are missing. Secret values are never printed.

set -euo pipefail

PRODUCTION_RELEASE="${QWENPAW_REQUIRE_APPLE_RELEASE:-0}"
if [[ "${PRODUCTION_RELEASE}" != "0" && "${PRODUCTION_RELEASE}" != "1" ]]; then
    echo "ERROR: QWENPAW_REQUIRE_APPLE_RELEASE must be 0 or 1" >&2
    exit 1
fi
if [[ "${PRODUCTION_RELEASE}" == "0" ]]; then
    echo "QA build: production signing is not required"
    exit 0
fi

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
    if [[ -z "${!name:-}" ]]; then
        echo "ERROR: production Desktop release requires ${name}" >&2
        exit 1
    fi
done

echo "Production Desktop release configuration is present"
