# Releasing

A release is a `vX.Y.Z` tag. The workflow `.github/workflows/release.yml`
builds the packages on the three systems and creates a **draft** release with
them; nothing is public until someone presses Publish.

## What a release has

Each package carries the window (`opencircuit-desktop`) and the command line
(`opencircuit`), the licences and the examples.

| File | System | Built by |
| --- | --- | --- |
| `opencircuit-X.Y.Z-windows-x86_64.zip` | Windows 10 and later | PowerShell `Compress-Archive` |
| `OpenCircuit-X.Y.Z-macos-universal.dmg` | macOS 11 and later, Apple Silicon and Intel | `packaging/macos/package.sh` |
| `opencircuit-X.Y.Z-macos-universal.tar.gz` | the same, for the command line | the same script |
| `opencircuit_X.Y.Z_amd64.deb` | Debian 12, Ubuntu 22.04 and later | nfpm, `packaging/linux/nfpm.yaml` |
| `opencircuit-X.Y.Z-1.x86_64.rpm` | Fedora | nfpm, the same file |
| `OpenCircuit-X.Y.Z-x86_64.AppImage` | any Linux with a C library as new as 2.35 | `packaging/linux/build-appimage.sh` |
| `opencircuit-X.Y.Z-linux-x86_64.tar.gz` | any Linux | the workflow |
| `SHA256SUMS` | all of the above | the release job |

The Linux packages are built on Ubuntu 22.04 so that they run on older
systems; the programs open the graphics, keyboard and window-system
libraries when they start, so the `.deb` and the `.rpm` recommend them and do
not require them.

## Making a release

1. Set the version in the root `Cargo.toml` (`[workspace.package]` and the
   `version` of each workspace dependency), update `Cargo.lock`
   (`cargo update --workspace`), and merge it through a pull request.
2. Tag the merge commit and push the tag:

   ```sh
   git tag -a v0.0.1 -m "OpenCircuit 0.0.1"
   git push origin v0.0.1
   ```

   The workflow stops at once if the tag is not `v` plus the version of
   `Cargo.toml`.
3. Open the draft release on GitHub, read the notes (`packaging/release-notes.md`,
   with the version filled in), try a package, and publish it.

A pull request that changes `packaging/`, `assets/icons/` or the workflow runs
the same builds and package checks without creating a release, so a change is
tested before a tag. `workflow_dispatch` runs them by hand on a branch.

## What is checked

Each job installs or opens what it built and runs the command line on an
example: the `.deb` is installed, the `.rpm` is listed, the AppImage runs
`validate`, the `.dmg` is mounted (the `Info.plist` is linted and the
programs must be universal), the `.zip` is unpacked.

## What a release does not have yet

- **Signing.** Nothing is signed or notarized, because that needs
  certificates and secrets. The macOS binaries are signed ad hoc (no
  certificate), which Apple Silicon requires to run them at all; the first
  start still needs a confirmation. Windows shows SmartScreen.
- An `.msi` installer, Linux and Windows on ARM, Flatpak, a repository for
  `apt` or `dnf`, an icon inside the Windows executable.
- A list of the licences of the Rust dependencies. The project licences and
  the font licence are in every package; `cargo deny check` keeps the
  dependencies to MIT, Apache-2.0 and the other licences of `deny.toml`.

## The icon

`assets/icons/opencircuit.svg` is drawn for this project (a coil, the contact
it closes and the linkage between them); the PNG sizes in
`assets/icons/png/` are rendered from it:

```sh
for n in 16 32 48 64 128 256 512 1024; do
  convert -background none -density 384 assets/icons/opencircuit.svg \
    -resize ${n}x${n} assets/icons/png/opencircuit-$n.png
done
```
