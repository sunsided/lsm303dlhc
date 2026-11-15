#!/usr/bin/env sh
# Prints the CHANGELOG.md section of a version.
# Fails if the changelog has no section for the version.
# Usage: release-notes.sh <version>
set -eu
awk -v heading="## [$1]" '
  index($0, heading) == 1 { found = 1; started = 0; next }
  /^## \[/ { found = 0 }
  found && !started && $0 == "" { next }
  found { started = 1; print }
  END { if (!started) exit 1 }
' CHANGELOG.md || {
  echo "release-notes.sh: no changelog section for $1 in CHANGELOG.md" >&2
  exit 1
}
