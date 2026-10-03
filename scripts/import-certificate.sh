#!/usr/bin/env bash
# Import the Developer ID certificate into a throwaway keychain on the runner.
set -euo pipefail

if [ -z "${APPLE_CERTIFICATE:-}" ]; then
  echo "No APPLE_CERTIFICATE: building unsigned. A release made this way will not pass Gatekeeper."
  exit 0
fi

KEYCHAIN="$RUNNER_TEMP/workmate-signing.keychain-db"
PASSWORD="$(uuidgen)"
echo "$APPLE_CERTIFICATE" | base64 --decode > "$RUNNER_TEMP/cert.p12"
security create-keychain -p "$PASSWORD" "$KEYCHAIN"
security set-keychain-settings -lut 21600 "$KEYCHAIN"
security unlock-keychain -p "$PASSWORD" "$KEYCHAIN"
security import "$RUNNER_TEMP/cert.p12" -P "$APPLE_CERTIFICATE_PASSWORD" -A -t cert -f pkcs12 -k "$KEYCHAIN"
security set-key-partition-list -S apple-tool:,apple: -k "$PASSWORD" "$KEYCHAIN" >/dev/null
security list-keychains -d user -s "$KEYCHAIN" login.keychain-db
rm -f "$RUNNER_TEMP/cert.p12"
