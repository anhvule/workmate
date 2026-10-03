#!/usr/bin/env bash
# Sign every executable in a directory with the hardened runtime.
# Usage: sign-binaries.sh <dir>
set -euo pipefail

DIR="${1:?usage: sign-binaries.sh <dir>}"
ENT="$(dirname "$0")/../apps/desktop/src-tauri/entitlements.plist"

if [ -z "${APPLE_SIGNING_IDENTITY:-}" ]; then
  echo "No APPLE_SIGNING_IDENTITY: leaving $DIR unsigned."
  exit 0
fi

for f in "$DIR"/*; do
  [ -f "$f" ] || continue
  case "$(file -b "$f")" in
    *Mach-O*) ;;
    *) continue ;;
  esac
  echo "signing $f"
  codesign --force --timestamp --options runtime \
    --entitlements "$ENT" --sign "$APPLE_SIGNING_IDENTITY" "$f"
  codesign --verify --strict --verbose=2 "$f"
done
