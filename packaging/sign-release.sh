#!/bin/sh
# Sign the release checksums with the project's minisign key (ADR-0020).
#
# Usage: packaging/sign-release.sh <SHA256SUMS> <version>
#
# The secret key is read from MINISIGN_SECRET and its password from
# MINISIGN_PASSWORD; both are GitHub secrets of the environment
# `release-signing`. The signature goes next to the file as
# <SHA256SUMS>.minisig. Its trusted comment is `version:<version>`: the app
# refuses a signed list whose version is not the release it is installing,
# so an old signed list cannot be replayed.
#
# The signature is checked with the public key of the project before the
# script ends. PUBLIC_KEY overrides the key file (for tests only).
set -eu

sums=${1:?usage: sign-release.sh <SHA256SUMS> <version>}
version=${2:?usage: sign-release.sh <SHA256SUMS> <version>}
public_key=${PUBLIC_KEY:-packaging/update-public-key.minisign}
: "${MINISIGN_SECRET:?MINISIGN_SECRET is not set}"
: "${MINISIGN_PASSWORD:?MINISIGN_PASSWORD is not set}"

case "$version" in
  [0-9]*.[0-9]*.[0-9]*) ;;
  *)
    echo "sign-release: not a version: $version" >&2
    exit 1
    ;;
esac
if [ ! -f "$sums" ]; then
  echo "sign-release: no file $sums" >&2
  exit 1
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
umask 077
printf '%s\n' "$MINISIGN_SECRET" > "$work/minisign.key"

# The password goes in on standard input, never on the command line.
printf '%s\n' "$MINISIGN_PASSWORD" |
  minisign -S -s "$work/minisign.key" -m "$sums" \
    -x "$sums.minisig" -t "version:$version"

# Check what was written, with the public key of the project.
minisign -Vm "$sums" -p "$public_key" -x "$sums.minisig" -q
comment=$(minisign -Vm "$sums" -p "$public_key" -x "$sums.minisig" -Q)
if [ "$comment" != "version:$version" ]; then
  echo "sign-release: trusted comment is '$comment', expected version:$version" >&2
  exit 1
fi
echo "signed $sums for version $version"
