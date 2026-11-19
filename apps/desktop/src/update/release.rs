//! The latest release as GitHub describes it: its version, its notes, its page
//! and its files. Everything here is read from a response the program does not
//! trust: sizes are capped, unexpected fields are ignored, and a link is kept
//! only when it points into the project's own origin.

use std::path::Path;

use serde::Deserialize;

use super::UpdateError;
use super::version::Version;

/// Largest release description read, in bytes.
pub const MAX_RELEASE_BYTES: u64 = 1 << 20;
/// Most files of a release that are looked at.
const MAX_ASSETS: usize = 128;
/// Most characters of the notes kept for the dialog.
const MAX_NOTES_CHARS: usize = 4_000;

/// A file of a release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    /// The file name, with no directory in it.
    pub name: String,
    /// Where to download it. Starts with the trusted origin.
    pub url: String,
    /// Size announced by the release.
    pub size: u64,
}

/// A published release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    /// The version, from the tag.
    pub version: Version,
    /// The release page. Starts with the trusted origin.
    pub page: String,
    /// The notes as written, cut to a sensible length.
    pub notes: String,
    /// The files of the release.
    pub assets: Vec<Asset>,
}

/// The release fields this program reads. Others in the response are ignored.
#[derive(Deserialize)]
struct RawRelease {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    assets: Vec<RawAsset>,
}

/// One file of the response.
#[derive(Deserialize)]
struct RawAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

impl Release {
    /// Read the release from `bytes`, the response of the GitHub API. Links
    /// are kept only when they start with `origin` (for example
    /// `https://github.com/felipedsvit/opencircuit/`).
    ///
    /// # Errors
    ///
    /// [`UpdateError::TooLarge`] when `bytes` is over [`MAX_RELEASE_BYTES`],
    /// [`UpdateError::BadResponse`] when the fields this program needs are
    /// missing or wrong, or the release is a draft.
    pub fn from_json(bytes: &[u8], origin: &str) -> Result<Self, UpdateError> {
        if u64::try_from(bytes.len()).map_or(true, |len| len > MAX_RELEASE_BYTES) {
            return Err(UpdateError::TooLarge);
        }
        let raw: RawRelease =
            serde_json::from_slice(bytes).map_err(|_| UpdateError::BadResponse)?;
        if raw.draft {
            return Err(UpdateError::BadResponse);
        }
        let version = Version::parse(&raw.tag_name).ok_or(UpdateError::BadResponse)?;
        if !raw.html_url.starts_with(origin) {
            return Err(UpdateError::BadResponse);
        }
        let assets = raw
            .assets
            .into_iter()
            .take(MAX_ASSETS)
            .filter(|asset| {
                plain_name(&asset.name) && asset.browser_download_url.starts_with(origin)
            })
            .map(|asset| Asset {
                name: asset.name,
                url: asset.browser_download_url,
                size: asset.size,
            })
            .collect();
        let notes = raw
            .body
            .unwrap_or_default()
            .chars()
            .take(MAX_NOTES_CHARS)
            .collect();
        Ok(Self {
            version,
            page: raw.html_url,
            notes,
            assets,
        })
    }

    /// The file of this release for `installer`, when the release has one.
    #[must_use]
    pub fn asset_for(&self, installer: Installer) -> Option<&Asset> {
        let name = installer.file_name(&self.version);
        self.assets.iter().find(|asset| asset.name == name)
    }

    /// The `SHA256SUMS` file of the release, when it has one.
    #[must_use]
    pub fn checksums(&self) -> Option<&Asset> {
        self.assets.iter().find(|asset| asset.name == "SHA256SUMS")
    }
}

/// A file name with no directory part and nothing odd in it.
fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 255
        && name != "."
        && name != ".."
        && Path::new(name).file_name().is_some_and(|file| file == name)
}

/// The kind of file a system installs from. Each one has a file name in the
/// release, written by `.github/workflows/release.yml`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Installer {
    /// The AppImage of Linux x86-64.
    AppImage,
    /// The Debian package.
    Deb,
    /// The RPM package.
    Rpm,
    /// The disk image of macOS (both processors). Only macOS builds choose it.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    Dmg,
    /// The Windows installer. Only Windows builds choose it.
    #[cfg_attr(
        not(all(target_os = "windows", target_arch = "x86_64")),
        allow(dead_code)
    )]
    Msi,
}

impl Installer {
    /// The name of this installer's file in a release of `version`.
    #[must_use]
    pub fn file_name(self, version: &Version) -> String {
        match self {
            Self::AppImage => format!("OpenCircuit-{version}-x86_64.AppImage"),
            Self::Deb => format!("opencircuit_{version}_amd64.deb"),
            Self::Rpm => format!("opencircuit-{version}-1.x86_64.rpm"),
            Self::Dmg => format!("OpenCircuit-{version}-macos-universal.dmg"),
            Self::Msi => format!("OpenCircuit-{version}-windows-x86_64.msi"),
        }
    }
}

/// The installer for the system this runs on, or `None` when the project
/// publishes none for it.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[must_use]
pub fn current_installer() -> Option<Installer> {
    if std::env::var_os("APPIMAGE").is_some() {
        return Some(Installer::AppImage);
    }
    if Path::new("/etc/debian_version").exists() {
        Some(Installer::Deb)
    } else if Path::new("/etc/redhat-release").exists() || Path::new("/etc/fedora-release").exists()
    {
        Some(Installer::Rpm)
    } else {
        Some(Installer::AppImage)
    }
}

/// The installer for the system this runs on, or `None` when the project
/// publishes none for it.
#[cfg(target_os = "macos")]
#[must_use]
pub fn current_installer() -> Option<Installer> {
    Some(Installer::Dmg)
}

/// The installer for the system this runs on, or `None` when the project
/// publishes none for it.
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
#[must_use]
pub fn current_installer() -> Option<Installer> {
    Some(Installer::Msi)
}

/// The installer for the system this runs on, or `None` when the project
/// publishes none for it.
#[cfg(not(any(
    all(target_os = "linux", target_arch = "x86_64"),
    target_os = "macos",
    all(target_os = "windows", target_arch = "x86_64")
)))]
#[must_use]
pub fn current_installer() -> Option<Installer> {
    None
}

/// The SHA-256 written for `file` in `sums`, a `sha256sum` listing (one
/// `<hex>  <name>` line per file). `None` when the listing has no such line.
#[must_use]
pub fn checksum_for(sums: &str, file: &str) -> Option<String> {
    sums.lines().take(MAX_SUM_LINES).find_map(|line| {
        let mut parts = line.split_whitespace();
        let hex = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        let well_formed = hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit());
        (name == file && well_formed).then(|| hex.to_ascii_lowercase())
    })
}

/// Most lines of a checksum listing that are read.
const MAX_SUM_LINES: usize = 256;

#[cfg(test)]
mod tests {
    use super::{Installer, MAX_RELEASE_BYTES, Release, UpdateError, checksum_for};
    use crate::update::version::Version;

    const ORIGIN: &str = "https://github.com/felipedsvit/opencircuit/";

    fn release_json(tag: &str, extra: &str) -> String {
        format!(
            r#"{{
                "tag_name": "{tag}",
                "html_url": "https://github.com/felipedsvit/opencircuit/releases/tag/{tag}",
                "body": "Fixes the wires.",
                "draft": false,
                "published_at": "2026-11-19T10:00:00Z",
                "assets": [
                    {{"name": "opencircuit_0.0.3_amd64.deb",
                      "browser_download_url": "https://github.com/felipedsvit/opencircuit/releases/download/{tag}/opencircuit_0.0.3_amd64.deb",
                      "size": 6143}},
                    {{"name": "OpenCircuit-0.0.3-x86_64.AppImage",
                      "browser_download_url": "https://github.com/felipedsvit/opencircuit/releases/download/{tag}/OpenCircuit-0.0.3-x86_64.AppImage",
                      "size": 90000}},
                    {{"name": "SHA256SUMS",
                      "browser_download_url": "https://github.com/felipedsvit/opencircuit/releases/download/{tag}/SHA256SUMS",
                      "size": 300}}
                ]{extra}
            }}"#
        )
    }

    #[test]
    fn a_release_is_read_with_its_files_and_notes() {
        let release =
            Release::from_json(release_json("v0.0.3", "").as_bytes(), ORIGIN).expect("a release");

        assert_eq!(release.version.to_string(), "0.0.3");
        assert_eq!(release.notes, "Fixes the wires.");
        assert_eq!(release.assets.len(), 3);
        assert!(release.checksums().is_some());
    }

    #[test]
    fn the_installer_of_each_system_has_its_file_name() {
        let version = Version::parse("0.0.3").expect("a version");

        assert_eq!(
            Installer::Deb.file_name(&version),
            "opencircuit_0.0.3_amd64.deb"
        );
        assert_eq!(
            Installer::AppImage.file_name(&version),
            "OpenCircuit-0.0.3-x86_64.AppImage"
        );
        assert_eq!(
            Installer::Rpm.file_name(&version),
            "opencircuit-0.0.3-1.x86_64.rpm"
        );
        assert_eq!(
            Installer::Dmg.file_name(&version),
            "OpenCircuit-0.0.3-macos-universal.dmg"
        );
        assert_eq!(
            Installer::Msi.file_name(&version),
            "OpenCircuit-0.0.3-windows-x86_64.msi"
        );
    }

    #[test]
    fn a_release_without_the_installer_of_a_system_has_none() {
        let release =
            Release::from_json(release_json("v0.0.3", "").as_bytes(), ORIGIN).expect("a release");

        assert!(release.asset_for(Installer::Deb).is_some());
        assert!(release.asset_for(Installer::Msi).is_none());
    }

    #[test]
    fn a_link_outside_the_project_is_dropped_and_a_page_outside_refuses_the_release() {
        let outside = release_json("v0.0.3", "").replace(
            "https://github.com/felipedsvit/opencircuit/releases/download/v0.0.3/SHA256SUMS",
            "https://example.com/SHA256SUMS",
        );
        let release = Release::from_json(outside.as_bytes(), ORIGIN).expect("a release");
        assert!(release.checksums().is_none());

        let page = release_json("v0.0.3", "").replace(
            "https://github.com/felipedsvit/opencircuit/releases/tag/v0.0.3",
            "https://example.com/page",
        );
        assert_eq!(
            Release::from_json(page.as_bytes(), ORIGIN),
            Err(UpdateError::BadResponse)
        );
    }

    #[test]
    fn a_file_name_with_a_directory_in_it_is_dropped() {
        let hostile = release_json("v0.0.3", "")
            .replace("OpenCircuit-0.0.3-x86_64.AppImage", "../../bin/opencircuit");
        let release = Release::from_json(hostile.as_bytes(), ORIGIN).expect("a release");

        assert!(
            release
                .assets
                .iter()
                .all(|asset| asset.name != "../../bin/opencircuit")
        );
    }

    #[test]
    fn hostile_responses_are_refused_without_a_panic() {
        let oversized = vec![b' '; usize::try_from(MAX_RELEASE_BYTES + 1).unwrap_or(0)];
        assert_eq!(
            Release::from_json(&oversized, ORIGIN),
            Err(UpdateError::TooLarge)
        );

        let mut seed: u64 = 0x9e37_79b9_7f4a_7c15;
        for _ in 0..500 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            let len = usize::try_from(seed % 300).unwrap_or(0);
            let bytes: Vec<u8> = (0..len)
                .map(|index| u8::try_from((seed >> (index % 56)) & 0xff).unwrap_or(0))
                .collect();
            assert!(Release::from_json(&bytes, ORIGIN).is_err());
        }

        for body in [
            r#"{"tag_name": 3}"#,
            r#"{"tag_name": "v0.0.3", "html_url": 5}"#,
            r#"{"tag_name": "nope", "html_url": "https://github.com/felipedsvit/opencircuit/x"}"#,
            r#"[1, 2, 3]"#,
            r#"{"tag_name": "v0.0.3", "html_url": "https://github.com/felipedsvit/opencircuit/x", "assets": [{"name": 1}]}"#,
        ] {
            assert!(
                Release::from_json(body.as_bytes(), ORIGIN).is_err(),
                "{body}"
            );
        }
    }

    #[test]
    fn the_checksum_of_a_file_is_read_from_a_sha256sum_listing() {
        let sums = format!(
            "{}  opencircuit_0.0.3_amd64.deb\n{} *OpenCircuit-0.0.3-x86_64.AppImage\n",
            "A".repeat(64),
            "b".repeat(64)
        );

        assert_eq!(
            checksum_for(&sums, "opencircuit_0.0.3_amd64.deb"),
            Some("a".repeat(64))
        );
        assert_eq!(
            checksum_for(&sums, "OpenCircuit-0.0.3-x86_64.AppImage"),
            Some("b".repeat(64))
        );
        assert_eq!(checksum_for(&sums, "missing.msi"), None);
        assert_eq!(
            checksum_for("short  opencircuit.deb", "opencircuit.deb"),
            None
        );
    }
}
