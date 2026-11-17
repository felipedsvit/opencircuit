OpenCircuit @VERSION@: an editor and simulator for industrial control circuits (contactors, relays, timers, motors). Each package has the window (`opencircuit-desktop`) and the command line (`opencircuit`).@BUILD@

## What is new

@CHANGES@

## Download

| System | Package |
| --- | --- |
| Windows (x86_64), installer | `OpenCircuit-@VERSION@-windows-x86_64.msi`: double-click it; it also puts `opencircuit` on the `PATH` and opens `.ocircuit` files with the window |
| Windows (x86_64), portable | `opencircuit-@VERSION@-windows-x86_64.zip`: unzip and run `opencircuit-desktop.exe` |
| macOS (Apple Silicon and Intel) | `OpenCircuit-@VERSION@-macos-universal.dmg`: drag `OpenCircuit.app` to Applications |
| Debian, Ubuntu (amd64) | `opencircuit_@VERSION@_amd64.deb`: `sudo apt install ./opencircuit_@VERSION@_amd64.deb` |
| Fedora (x86_64) | `opencircuit-@VERSION@-1.x86_64.rpm`: `sudo dnf install ./opencircuit-@VERSION@-1.x86_64.rpm` |
| Any Linux (x86_64) | `OpenCircuit-@VERSION@-x86_64.AppImage`: `chmod +x` it and run it; or `opencircuit-@VERSION@-linux-x86_64.tar.gz` |
| macOS, command line | `opencircuit-@VERSION@-macos-universal.tar.gz` |

Check a download against `SHA256SUMS` (`sha256sum -c SHA256SUMS --ignore-missing`).

## Not signed

This build is not signed or notarized. Windows may show SmartScreen (More info, then Run anyway); macOS asks to confirm the first start (right-click the application, Open). The binaries are built by the workflow in `.github/workflows/release.yml` from the tag.

## Open it

Open a project from the `examples` folder, for instance `examples/direct-starter/starter.ocircuit`, and press Edit to draw. On Linux the examples are in `/usr/share/doc/opencircuit/examples`; with the Windows installer they are in `C:\Program Files\OpenCircuit\examples`. In the window, type `help` in the command line, or see the Help menu.

Licences: MIT OR Apache-2.0; the font of the PDF export is under the SIL Open Font License.
