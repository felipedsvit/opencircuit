#!/usr/bin/env bash
# The notes of a release: packaging/release-notes.md with the version and the
# section of CHANGELOG.md of that version filled in. Run from the root of the
# repository; it prints the notes and fails when CHANGELOG.md has no section
# `## [VERSION]` or the section is empty, so a release never goes out without
# its list of changes.
#
#   packaging/release-notes.sh 0.0.2-dev > notes.md
set -euo pipefail

version="${1:?usage: release-notes.sh VERSION}"

# The lines of the section, without the reference links at the end of the file.
changes="$(
  awk -v header="## [$version]" '
    index($0, header) == 1 { on = 1; next }
    /^## \[/ { on = 0 }
    /^\[[^]]+\]: / { next }
    on { print }
  ' CHANGELOG.md
)"
# Trim the blank lines at both ends.
changes="$(printf '%s\n' "$changes" | sed -e '/./,$!d' | sed -e ':a' -e '/^\n*$/{$d;N;ba' -e '}')"
if [ -z "$changes" ]; then
  echo "release-notes.sh: CHANGELOG.md has no list of changes for version $version" >&2
  exit 1
fi

build=""
case "$version" in
  *-*) build=$'\n\nThis is a development build: it is published as a pre-release, and it may change before the release it leads to.' ;;
esac

notes="$(cat packaging/release-notes.md)"
notes="${notes//@VERSION@/$version}"
notes="${notes//@BUILD@/$build}"
notes="${notes//@CHANGES@/$changes}"
printf '%s\n' "$notes"
