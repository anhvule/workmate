#!/usr/bin/env bash
# Check what actually shipped, not what the config says should have.
# Usage: verify-bundle.sh <bundle dir>
set -euo pipefail

APP="$(find "${1:?usage: verify-bundle.sh <bundle dir>}" -name '*.app' -maxdepth 3 | head -1)"
[ -n "$APP" ] || { echo "no .app found"; exit 1; }
RES="$APP/Contents/Resources"

fail=0
need() { [ -e "$1" ] || { echo "MISSING: $1"; fail=1; }; }

# The engine, the sidecar, and the notices that MIT requires travel with them.
need "$RES/binaries/opencode"
need "$RES/binaries/workmate-sidecar"
need "$RES/NOTICE"
need "$RES/LICENSE"
need "$RES/packs/solo/pack.json"
need "$RES/skills/conventional-commits/SKILL.md"

grep -q "OpenCode" "$RES/NOTICE" || { echo "NOTICE does not mention the bundled engine"; fail=1; }

SIZE_MB=$(du -sm "$APP" | cut -f1)
echo "app size: ${SIZE_MB} MB"

if [ -n "${APPLE_SIGNING_IDENTITY:-}" ]; then
  codesign --verify --deep --strict --verbose=2 "$APP"
  spctl --assess --type execute --verbose=2 "$APP" || { echo "Gatekeeper would refuse this app"; fail=1; }
fi

exit $fail
