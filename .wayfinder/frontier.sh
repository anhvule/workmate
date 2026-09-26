#!/usr/bin/env bash
# Lists the frontier: open, unclaimed tickets whose blockers are all closed.
set -uo pipefail
cd "$(dirname "$0")/tickets"

field() { sed -n "s/^$2: *//p" "$1" | head -1; }
status_of() { sed -n 's/^status: *//p' "$(printf '%03d' "$1")"-*.md 2>/dev/null | head -1; }

for f in *.md; do
  [ -e "$f" ] || continue
  [ "$(field "$f" status)" = "open" ] || continue
  [ -z "$(field "$f" assignee)" ] || continue
  blockers=$(sed -n 's/^blocked-by: *\[\(.*\)\]/\1/p' "$f" | head -1 | tr -d ' ')
  ready=1
  if [ -n "$blockers" ]; then
    IFS=','
    for b in $blockers; do
      [ "$(status_of "$b")" = "closed" ] || ready=0
    done
    unset IFS
  fi
  if [ "$ready" = 1 ]; then
    printf '%-5s %-10s %s\n' "$(field "$f" id)" "$(field "$f" type)" "$(field "$f" title)"
  fi
done
exit 0
