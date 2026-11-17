#!/usr/bin/env bash
# Build OpenCircuit.app and its disk image, and a tar.gz, from the two
# release builds of the Mac targets.
#
#   packaging/macos/package.sh VERSION
#
# Run on macOS from the root of the repository, after
#   cargo build --release --target aarch64-apple-darwin -p opencircuit-desktop -p opencircuit-cli
#   cargo build --release --target x86_64-apple-darwin  -p opencircuit-desktop -p opencircuit-cli
# The binaries are joined into universal ones and signed ad hoc (no
# certificate): an Apple Silicon Mac does not run an unsigned binary at all.
# The result is not notarized; the first start needs a confirmation.
set -euo pipefail

version="${1:?usage: package.sh VERSION}"
# The version of the bundle is numbers and dots; a pre-release part (0.0.2-dev)
# stays in the names of the files only.
bundle_version="${version%%-*}"
arm="target/aarch64-apple-darwin/release"
intel="target/x86_64-apple-darwin/release"
out="dist"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

mkdir -p "$out" "$work/bin"
for name in opencircuit-desktop opencircuit; do
  lipo -create -output "$work/bin/$name" "$arm/$name" "$intel/$name"
  lipo -archs "$work/bin/$name"
done

# OpenCircuit.app
app="$work/stage/OpenCircuit.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$work/bin/opencircuit-desktop" "$work/bin/opencircuit" "$app/Contents/MacOS/"
sed "s/@VERSION@/$bundle_version/g" packaging/macos/Info.plist.in > "$app/Contents/Info.plist"

iconset="$work/OpenCircuit.iconset"
mkdir -p "$iconset"
png="assets/icons/png/opencircuit"
cp "$png-16.png"   "$iconset/icon_16x16.png"
cp "$png-32.png"   "$iconset/icon_16x16@2x.png"
cp "$png-32.png"   "$iconset/icon_32x32.png"
cp "$png-64.png"   "$iconset/icon_32x32@2x.png"
cp "$png-128.png"  "$iconset/icon_128x128.png"
cp "$png-256.png"  "$iconset/icon_128x128@2x.png"
cp "$png-256.png"  "$iconset/icon_256x256.png"
cp "$png-512.png"  "$iconset/icon_256x256@2x.png"
cp "$png-512.png"  "$iconset/icon_512x512.png"
cp "$png-1024.png" "$iconset/icon_512x512@2x.png"
iconutil -c icns "$iconset" -o "$app/Contents/Resources/OpenCircuit.icns"

codesign --force --sign - "$app/Contents/MacOS/opencircuit"
codesign --force --sign - "$app"

# Disk image: the application, a link to /Applications and the licences.
mkdir -p "$work/stage/Licenses"
cp LICENSE-MIT LICENSE-APACHE "$work/stage/Licenses/"
cp assets/fonts/liberation-mono/LICENSE "$work/stage/Licenses/LICENSE-LiberationMono"
ln -s /Applications "$work/stage/Applications"
dmg="$out/OpenCircuit-$version-macos-universal.dmg"
rm -f "$dmg"
hdiutil create -volname "OpenCircuit $version" -srcfolder "$work/stage" -ov -format UDZO "$dmg"

# tar.gz with both programs, for the command line.
name="opencircuit-$version-macos-universal"
mkdir -p "$work/$name"
cp "$work/bin/opencircuit-desktop" "$work/bin/opencircuit" "$work/$name/"
codesign --force --sign - "$work/$name/opencircuit"
codesign --force --sign - "$work/$name/opencircuit-desktop"
cp LICENSE-MIT LICENSE-APACHE "$work/$name/"
cp assets/fonts/liberation-mono/LICENSE "$work/$name/LICENSE-LiberationMono"
sed "s/@VERSION@/$version/g" packaging/README.txt > "$work/$name/README.txt"
cp -R examples "$work/$name/examples"
tar -C "$work" -czf "$out/$name.tar.gz" "$name"

ls -la "$out"
