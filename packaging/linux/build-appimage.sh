#!/usr/bin/env bash
# Build the AppImage of the desktop application and the command line.
#
#   packaging/linux/build-appimage.sh VERSION OUTPUT_DIRECTORY
#
# Run from the root of the repository, after `cargo build --release`.
# `appimagetool` must be on the PATH (CI downloads a pinned release).
set -euo pipefail

version="${1:?usage: build-appimage.sh VERSION OUTPUT_DIRECTORY}"
output="${2:?usage: build-appimage.sh VERSION OUTPUT_DIRECTORY}"
root="$(pwd)"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
appdir="$work/OpenCircuit.AppDir"

install -Dm755 target/release/opencircuit-desktop "$appdir/usr/bin/opencircuit-desktop"
install -Dm755 target/release/opencircuit "$appdir/usr/bin/opencircuit"
install -Dm644 packaging/linux/opencircuit.desktop "$appdir/usr/share/applications/opencircuit.desktop"
install -Dm644 packaging/linux/opencircuit.desktop "$appdir/opencircuit.desktop"
install -Dm644 assets/icons/opencircuit.svg "$appdir/usr/share/icons/hicolor/scalable/apps/opencircuit.svg"
for size in 16 32 48 64 128 256 512; do
  install -Dm644 "assets/icons/png/opencircuit-$size.png" \
    "$appdir/usr/share/icons/hicolor/${size}x${size}/apps/opencircuit.png"
done
# The icon the AppImage tools look for beside the desktop file.
install -Dm644 assets/icons/png/opencircuit-256.png "$appdir/opencircuit.png"
ln -s opencircuit.png "$appdir/.DirIcon"
install -Dm644 LICENSE-MIT "$appdir/usr/share/doc/opencircuit/LICENSE-MIT"
install -Dm644 LICENSE-APACHE "$appdir/usr/share/doc/opencircuit/LICENSE-APACHE"
install -Dm644 assets/fonts/liberation-mono/LICENSE "$appdir/usr/share/doc/opencircuit/LICENSE-LiberationMono"

cat > "$appdir/AppRun" <<'RUN'
#!/bin/sh
# With a command-line argument that is a subcommand of the command line
# tool, run that tool; otherwise open the window.
here="$(dirname "$(readlink -f "$0")")"
case "${1:-}" in
  validate|simulate|test|export|new|edit|serve)
    exec "$here/usr/bin/opencircuit" "$@" ;;
  *)
    exec "$here/usr/bin/opencircuit-desktop" "$@" ;;
esac
RUN
chmod 755 "$appdir/AppRun"

mkdir -p "$output"
target="$output/OpenCircuit-$version-x86_64.AppImage"
ARCH=x86_64 appimagetool --appimage-extract-and-run "$appdir" "$target"
echo "$target"
cd "$root"
