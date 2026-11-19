# ADR-0020: Update notice and in-app updates

Status: Accepted for phase 1 (2026-11-19), implemented in issue #133. Phases 2
and 3 are still proposed. Not in a milestone yet. Work outside M9 starts
from an issue (AGENTS.md); this ADR is the plan for that issue.

## Context

Version 0.0.2 ships as a manual install from GitHub Releases
(`.github/workflows/release.yml`): `.deb`, `.rpm` and AppImage on Linux, a
universal `.dmg` (ad hoc signed) on macOS, and `.msi` and `.zip` on Windows. An
installed app does not know a newer release exists. `docs/releasing.md` lists
what is missing: signing, an apt or dnf repository, Flatpak.

The release already contains `SHA256SUMS`, written by the workflow. The app does
not read it.

Two facts shape the design:

- `SHA256SUMS` comes from the same release as the binary. It catches a corrupt
  or truncated download. It does not catch a replaced release. Only a signature
  whose key is kept outside the release protects against that.
- The app runs as the user. Replacing a file in `/usr` needs root, an MSI
  install needs elevation, and a macOS bundle may be read-only.

## Decision

1. The app asks GitHub for the latest published release:
   `GET https://api.github.com/repos/felipedsvit/opencircuit/releases/latest`.
   Drafts and prereleases are never offered.
2. Two install modes, chosen in the Updates window:
   - **Download and check (default):** the app downloads the asset for this
     platform into a private folder under the user data directory, checks its
     size and SHA-256 against `SHA256SUMS`, and shows the file. The user installs
     it.
   - **Install automatically:** the app installs the verified file and restarts.
     It is enabled in phase 3. Until then the option is shown disabled, with
     the reason.
3. Frequency: every 3 days, every 5 days (default), or never. "Never" turns off
   automatic checks and reminders. "Check now" still works.
4. Automatic checks are silent when the network is down. An error is shown only
   after the user asks for "Check now".
5. The code lives in `apps/desktop/src/update/`. Core crates do not change: no
   network, no clock, no egui in `circuit-*`.
6. Releases carry `SHA256SUMS.minisig` (minisign, Ed25519). The public key is
   compiled into the binary. Automatic install requires a valid signature and a
   version strictly newer than the running one, read from the signed trusted
   comment.

## User experience

A notice that never blocks work, and that can be set aside for one version
without losing the information.

- **Status bar chip.** A segment at the right end of the existing status bar,
  shown only while a newer version is known: "Version 0.0.3 available". A click
  opens the dialog. A window with no update known looks exactly as before.
- **Dialog** (the existing `dialog.rs` component, ADR-0019). It opens once when
  a version is found, and again every reminder interval while that version is
  still unhandled.
  - Title: "Version 0.0.3 available". Message: "You have 0.0.2." and a link to
    the release notes, which open in the browser.
  - Download and check mode: primary "Download 0.0.3". After the file is
    verified, "Show file" appears. Other choices: "Remind me in 3 days" and
    "Skip this version".
  - Install automatically mode: primary "Update and restart". If there are
    unsaved changes, the dialog says so and the restart waits until the user has
    saved.
- **Help > Updates…** (a new entry after "About OpenCircuit"):
  - Check every: 3 days / 5 days / Never (radio buttons).
  - Install: "Download and check" / "Install automatically" (disabled until
    phase 2, with its reason beside it).
  - "Check now" button, the installed version, and the date of the last check.
  - One note: "Downloads are checked against the published SHA-256." Before
    phase 2 it adds: "Signatures are checked from signed releases on." Later it
    says "Downloads are checked against the signed SHA-256 list."

Wording stays short. Technical terms (SHA-256, signature) appear only in the
Updates window, never in the dialog's primary path.

## State

`update.json` in `user_data_dir()`, the folder that already holds recent
projects and recovery files (`apps/desktop/src/recovery.rs`):

```json
{"interval_days": 5, "mode": "download", "last_check": 1795132800, "last_reminder": 1795132800, "skipped": "0.0.3"}
```

- A missing file means the defaults. A corrupt file means the defaults, and it
  is rewritten on the next save.
- Writes go to a temporary file, then rename over the target (the pattern of the
  save-systems skill). A failed write leaves the old file in place.
- Schedule, as a pure function of `now` passed in:
  - automatic check at startup when `interval_days > 0` and
    `now - last_check >= interval_days`;
  - dialog for a new version when it is not skipped and was not shown yet;
  - reminder when `now - last_reminder >= interval_days`;
  - "Check now" ignores the schedule.

## Platforms

| Platform | Download and check (phase 1) | Install automatically (phase 3) |
| --- | --- | --- |
| Linux AppImage | Download, verify, show file | Replace the file at `$APPIMAGE`, relaunch it |
| Linux `.deb` / `.rpm` | Download, verify, show file. The user runs `apt install ./file` | `pkexec apt-get install -y <verified file>` (asks for a password). Without `pkexec`, manual |
| Windows `.msi` | Download, verify, run `msiexec` on the user's click | `msiexec /i <verified file> /qb` after the app exits (UAC prompt) |
| Windows `.zip` | Download, verify, show file | Not planned: a portable folder, the user replaces it |
| macOS `.dmg` | Download, verify, show file | After tests on a real Mac: copy the bundle into `/Applications` when writable |
| Browser (wasm) | Not checked | Not applicable: the page is always the current build |

## Security

Threat summary. Assets: the user's machine and privileges, the user's project
files, the integrity of releases, and the signing key.

| # | Threat | Mitigation |
| --- | --- | --- |
| T1 | The GitHub account, a release or a workflow is compromised, and a malicious binary is served | Minisign signature with a key kept outside GitHub, in a protected CI environment. Automatic install refuses unsigned or badly signed files |
| T2 | An old, signed, vulnerable release is replayed | The version is in the signed trusted comment. Install only when strictly newer than the running version |
| T3 | Man in the middle on the download | HTTPS with the operating system's certificate store (`rustls-native-certs`). The signature check, from phase 2, does not depend on TLS |
| T4 | Truncated or corrupt download | Size from the release asset and SHA-256 from `SHA256SUMS`. The file lives in a fresh private folder and is renamed into place only when complete |
| T5 | Oversized response exhausts memory or disk | Caps: release JSON 1 MiB, asset 512 MiB. Read in chunks; refuse when `Content-Length` is over the cap |
| T6 | Hostile release JSON | Typed structs, no indexing, unexpected types refused as errors. A test feeds random and oversized bytes and expects no panic |
| T7 | Privilege abuse through `pkexec` or `msiexec` | Run only a file that passed signature and checksum, from a folder the app created with private permissions. Never a path taken from the response |
| T8 | A restart loses unsaved work | Flush autosave before installing. "Update and restart" waits for a save or asks. No restart while an edit is open |
| T9 | Tracking | The request sends a generic User-Agent (`OpenCircuit/<version> (<os>)`) and nothing else: no account, project or machine identifier |
| T10 | GitHub rate limit (60 unauthenticated requests per hour per IP) | At most one automatic check every 3 days. A 403 or 429 answer gives a quiet retry later |

Out of scope: a compromised user machine; sandboxing the installers; any
automatic install without a signature.

Until phase 2, a manual download is checked only against `SHA256SUMS`, which
the same release provides. That is an integrity check, and the Updates window
says so.

## Implementation

New dependencies, all in `apps/desktop` only. Each is checked against
`deny.toml` before it is added (allowlist: Apache-2.0, BSD-2/3, BSL-1.0, ISC,
MIT, Unicode-3.0, Unlicense, Zlib):

- `ureq` 3, synchronous, with `rustls-no-provider` and the `ring` provider of
  `rustls`. No async runtime.
- `rustls-native-certs` for the roots of the operating system. The webpki roots
  (`webpki-roots`, and `rustls-platform-verifier` through `webpki-root-certs`)
  are under CDLA-Permissive-2.0, which is outside the allowlist, so they are not
  used. A licence exception would need the owner's approval; the OS store avoids
  it. `native-tls` was rejected because it needs OpenSSL headers at build time.
- `sha2` for SHA-256.
- Version comparison is a small parser in `update/version.rs`, not a dependency.
- Phase 2 (signatures): `minisign-verify`, Ed25519; its file format was checked
  in the minisign documentation (ctx7, `/jedisct1/minisign`).
- Phase 3 (automatic install): `self-replace` for the running executable.

Modules:

- `update/mod.rs`: state machine (idle, checking, available, downloading,
  verified, installing, failed). Transitions are pure and tested.
- `update/check.rs`: request and release JSON parsing; asset choice per
  platform, from the file names of `release.yml`.
- `update/verify.rs`: SHA-256 and minisign checks, and the version rule.
- `update/store.rs`: `update.json` with atomic writes.
- `update/install.rs`: per-platform install (phase 3).
- `update/ui.rs`: chip, dialog, Updates window.

A worker thread does the network and file work and sends results over a
channel. The UI reads the channel each frame. The worker requests a repaint only
when a result arrives, so an idle window does not redraw.

Errors are a `UpdateError` enum. Users see short sentences, for example "Could
not check for updates. Check the connection and try again."

## Conservative interface (AGENTS.md)

- One entry added to the existing Help menu. No existing control moved, renamed
  or restyled.
- One status bar segment, shown only when an update is known. No new panel, no
  new palette tab.
- The dialog uses the existing component of ADR-0019.
- New strings go through `tr()` and into `apps/desktop/src/i18n/pt_br.rs`,
  sorted by the English text (ADR-0017).
- New tests go in a new file, `apps/desktop/tests/updates.rs`. Existing tests
  are not edited.
- The check runs off the UI thread. The startup window and frame timing do not
  change.

## Tests

Unit tests, inline in each module:

- Version order: `0.0.2 < 0.0.3`, `0.0.2-dev < 0.0.2`. The tag `v0.0.3` parses;
  text that is not a version is refused.
- Asset choice from a fixture release JSON for each platform. A missing asset
  gives "no installer for this platform".
- SHA-256: match, mismatch, truncated file.
- Minisign: a valid signature passes; one changed byte fails; a signature whose
  trusted comment names another version fails the rule of T2. Tests use a key
  pair made for the test, never the production key.
- Schedule: table tests with an injected `now` for 3 days, 5 days, and never;
  skipped version; reminder interval.
- Store: missing file gives the defaults; corrupt JSON gives the defaults and is
  rewritten; a failed write leaves the old file.
- Hostile input: random bytes and oversized JSON never panic. This test fails
  without the caps of T5 and T6.

Integration: a local HTTP server in the test serves the release JSON and an
asset. Download, then verify, then the file is in the private folder. A checksum
mismatch keeps nothing.

UI (`egui_kittest`, `tests/updates.rs`):

- A fresh window has no chip, and Help has the "Updates…" entry.
- With a mocked newer version the chip shows. Its click opens the dialog with
  its choices. "Skip this version" hides the chip for that version only.
- "Never" makes no automatic request.

Manual checks, on real machines (CI runners cannot judge the interface): Linux
AppImage and `.deb`, Windows `.msi` and `.zip`, macOS `.dmg`. Not run yet.

Gates before every commit: `cargo fmt --check`, clippy with `-D warnings`,
rustdoc with warnings denied, `cargo test --workspace --locked`, `cargo deny
check`, `cargo run -p xtask -- check-architecture`.

## Phases

0. Issue opened, this ADR accepted, owner of the signing key decided.
1. Notice and download-and-check, native builds only. Tests as above.
   Release notes of the first release with the feature say how to get the next
   notice. Installs older than this release update by hand once.
2. Signing, in a separate pull request (AGENTS.md: CI changes are separate).
   `release.yml` signs `SHA256SUMS` into `SHA256SUMS.minisig`. The public key
   `packaging/update-public-key.minisign` is embedded with `include_str!`. The
   private key sits in a protected CI environment. The app verifies signatures
   in both modes. The "Install automatically" option stays disabled until
   phase 3, which is where it does something; enabling it earlier would offer
   a button with nothing behind it.

   Decided with the owner on 2026-11-19: the key is kept with a password (the
   secret `MINISIGN_PASSWORD` in the environment), which CI feeds on standard
   input to minisign 0.12, and the trusted comment of each signature is
   `version:X.Y.Z`. A signed list is refused when that version is not the one
   of the release.
3. Automatic install, one platform at a time: AppImage, then `.deb` (`pkexec`),
   then Windows `.msi`. macOS only after tests on hardware. Each platform needs
   its manual check before it is enabled.
4. Later, outside this ADR: apt or dnf repository, Flatpak, Homebrew cask, winget,
   a beta channel, delta downloads.

Key rotation: the app accepts a short list of trusted keys. A release that
introduces a new key ships both keys first and switches afterwards.

## Alternatives considered

- **A link on the About dialog only.** Simple, but nobody sees new versions.
  Rejected.
- **Only OS package repositories (apt, winget, Homebrew).** Good for the users
  of each store, but it does not cover the Windows `.zip` or the macOS `.dmg`
  users, and it needs signing anyway. Kept as phase 4.
- **Checksum only, no signature, for automatic install.** Cannot detect a
  replaced release. Rejected for automatic install. Accepted for manual mode,
  with the label in the Updates window.
- **A timer thread instead of a startup check.** Extra wakeups for no gain; the
  stored `last_check` covers it.
- **A native message box.** Rejected in ADR-0019.

## Consequences

- Installs before this release get no notice. They update by hand once.
- The desktop binary gains an HTTP and TLS stack. Its size and build time must
  be measured and recorded in `docs/benchmarks.md`.
- The GitHub API sees the user's IP address at most every few days. This is
  stated in the Updates window and in the release notes.
- Automatic install on Windows and macOS depends on SmartScreen and Gatekeeper
  behaviour. It is tested on hardware before it is enabled there.

## Threat model note

This ADR has a short threat model. The full model file (the
security-threat-model workflow) waits for confirmation of two assumptions: that
the update source is the public GitHub API, and that the app is single-user with
no network service of its own. The holder of the signing key is decided (see
Decisions).

## Decisions

Taken on 2026-11-19 by the project owner:

1. Check interval: 5 days.
2. Default install mode: download and check.
3. Signing key holder: the project owner, who answers for it. The private key
   is a GitHub Actions secret in an environment named `release-signing`. The
   environment accepts only tags `v*` and has a required reviewer. Environments,
   environment secrets and required reviewers are free for public repositories,
   and this repository is public. The owner keeps an offline encrypted backup of
   the key in a free password manager. Losing the key stops signed releases; the
   key rotation above recovers from it.
4. Signature scope: all platforms. `SHA256SUMS` covers every release asset, and
   the signature covers `SHA256SUMS`.

Residual risk, accepted: the owner is the only reviewer, so self-review must be
allowed. A stolen owner account can therefore approve a release. The defence is
two-factor authentication with a passkey on that account, required before
phase 2.

Still open:

5. First platform for automatic install: AppImage only (proposed), or all.
