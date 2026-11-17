# Releasing

A release is a `vX.Y.Z` tag (or `vX.Y.Z-dev`, a pre-release). The workflow
`.github/workflows/release.yml` builds the packages on the three systems and
creates a **draft** release with them, with the list of changes of the version
as its notes; nothing is public until someone presses Publish.

## What a release has

Each package carries the window (`opencircuit-desktop`) and the command line
(`opencircuit`), the licences and the examples.

| File | System | Built by |
| --- | --- | --- |
| `OpenCircuit-X.Y.Z-windows-x86_64.msi` | Windows 10 and later, installer for all users | WiX v5, `packaging/windows/opencircuit.wxs` |
| `opencircuit-X.Y.Z-windows-x86_64.zip` | Windows 10 and later, portable | PowerShell `Compress-Archive` |
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

The `.msi` installs in `C:\Program Files\OpenCircuit` for every user: the
window, the command line (added to the `PATH`), the licences, the README and
the examples, a Start menu entry, and the association of `.ocircuit` files with
the window. A newer installer replaces an older one (the `UpgradeCode` in the
`.wxs` never changes), and an installer version has no pre-release part, so
`0.0.2-dev` and `0.0.2` are both `0.0.2` to Windows and may replace each other.
In the `.deb` and the `.rpm` a pre-release version is written `0.0.2~dev`, so
that it sorts before `0.0.2`.

## Making a release

1. Add a section `## [X.Y.Z]` to `CHANGELOG.md` with what changed (Added,
   Changed, Fixed). The release notes are made from it
   (`packaging/release-notes.sh`), and the workflow fails, on a pull request
   too, when the version of `Cargo.toml` has no section.
2. Set the version in the root `Cargo.toml` (`[workspace.package]` and the
   `version` of each workspace dependency), update `Cargo.lock`
   (`cargo update --workspace`), and merge both through a pull request.
3. Tag the merge commit and push the tag:

   ```sh
   git tag -a v0.0.2-dev -m "OpenCircuit 0.0.2-dev"
   git push origin v0.0.2-dev
   ```

   The workflow stops at once if the tag is not `v` plus the version of
   `Cargo.toml`. A version with a `-` is published as a pre-release.
4. Open the draft release on GitHub, read the notes (the template is
   `packaging/release-notes.md`; the "What is new" part is the section of
   `CHANGELOG.md`), try a package, and publish it.

A pull request that changes `packaging/`, `assets/icons/`, `CHANGELOG.md` or the
workflow runs the same builds and package checks without creating a release, so
a change is tested before a tag. `workflow_dispatch` runs them by hand on a branch.

## What is checked

Each job installs or opens what it built and runs the command line on an
example: the `.deb` is installed, the `.rpm` is listed, the AppImage runs
`validate`, the `.dmg` is mounted (the `Info.plist` is linted and the
programs must be universal), the `.zip` is unpacked, and the `.msi` is installed
(the programs, licences, README and examples are there, the command line runs on
an example, `opencircuit` is on the `PATH`, the `.ocircuit` association and the
Start menu entry exist) and then removed.

## What a release does not have yet

- **Signing.** Nothing is signed or notarized, because that needs
  certificates and secrets. The macOS binaries are signed ad hoc (no
  certificate), which Apple Silicon requires to run them at all; the first
  start still needs a confirmation. Windows shows SmartScreen.
- Linux and Windows on ARM, Flatpak, a repository for `apt` or `dnf`, an icon
  inside the Windows executable, and a wizard in the `.msi` (it installs
  without questions).
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
