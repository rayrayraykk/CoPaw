#!/usr/bin/env bash
# Fail before native Core builds when Apple release configuration is missing.
# Secret values are never printed.

set -euo pipefail

PRODUCTION_RELEASE="${QWENPAW_REQUIRE_APPLE_RELEASE:-1}"
if [[ "${PRODUCTION_RELEASE}" != "0" && "${PRODUCTION_RELEASE}" != "1" ]]; then
    echo "ERROR: QWENPAW_REQUIRE_APPLE_RELEASE must be 0 or 1" >&2
    exit 1
fi
if [[ "${PRODUCTION_RELEASE}" == "0" ]]; then
    echo "QA Core build: Apple release signing is not required"
    exit 0
fi

required=(
    APPLE_CERTIFICATE_P12
    APPLE_CERTIFICATE_PASSWORD
    APPLE_SIGNING_IDENTITY
    APPLE_ID
    APPLE_TEAM_ID
    APPLE_APP_PASSWORD
)
for name in "${required[@]}"; do
    if [[ -z "${!name:-}" ]]; then
        echo "ERROR: Core release requires ${name}" >&2
        exit 1
    fi
done

if [[ "${APPLE_SIGNING_IDENTITY}" == "-" ]]; then
    echo "ERROR: Core release rejects ad-hoc Apple signing" >&2
    exit 1
fi

echo "Apple Core release configuration is present"
