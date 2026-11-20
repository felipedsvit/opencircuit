//! The update of OpenCircuit (ADR-0020): the latest published release, its
//! installer for this system, and the check of that installer against the
//! signed list of checksums. The window and the command line use the same
//! functions, so a download means the same thing in both.
//!
//! This crate does the network work and is not a core crate (`circuit-*`):
//! the core stays free of sockets, files and clocks.

#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]
#![cfg_attr(not(test), deny(clippy::indexing_slicing))]

pub mod net;
pub mod release;
pub mod verify;
pub mod version;

use std::path::{Path, PathBuf};

pub use release::{Installer, Release, current_installer};
pub use version::Version;

/// Why an update could not be checked or downloaded. The messages are in
/// English; the window translates them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateError {
    /// The request did not reach GitHub, or the connection broke.
    Connection,
    /// GitHub refused the request because of its rate limit.
    RateLimited,
    /// GitHub answered with an error.
    Unavailable,
    /// The answer is not a release this program can read.
    BadResponse,
    /// A response or a file is larger than the program accepts.
    TooLarge,
    /// The release has no file for this system.
    NoInstaller,
    /// The release has no SHA-256 for its file.
    NoChecksum,
    /// The download ended before the size the release announces.
    Incomplete,
    /// The download does not have the SHA-256 the release publishes.
    ChecksumMismatch,
    /// A file could not be written, or a folder could not be made.
    Disk,
    /// The folder of the download could not be opened in the file manager.
    Open,
    /// The release has no signature for its checksums.
    NotSigned,
    /// The signature of the checksums does not verify, or names another version.
    SignatureInvalid,
}

impl UpdateError {
    /// What the person is told, in English.
    #[must_use]
    pub const fn message(self) -> &'static str {
        match self {
            Self::Connection => "Could not reach GitHub. Check the connection and try again.",
            Self::RateLimited => "GitHub is limiting checks for now. Try again later.",
            Self::Unavailable => "GitHub did not answer. Try again later.",
            Self::BadResponse => "The release information is not valid.",
            Self::TooLarge => "The download is larger than expected. It was discarded.",
            Self::NoInstaller => "The latest release has no installer for this system.",
            Self::NoChecksum => "The release has no published SHA-256 for this file.",
            Self::Incomplete => "The download is incomplete. It was discarded.",
            Self::ChecksumMismatch => {
                "The download does not match the published SHA-256. It was discarded."
            }
            Self::Disk => "The file could not be saved. Check the disk space and try again.",
            Self::Open => "The folder could not be opened. Open it by hand.",
            Self::NotSigned => {
                "This release is not signed. Download it from the release page and check it by hand."
            }
            Self::SignatureInvalid => {
                "The signature of this release does not match. The download was not started."
            }
        }
    }
}

/// The answer of an upgrade: the program is the latest version, or the
/// installer of a newer version was downloaded and checked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Upgrade {
    /// No published release is newer than the program.
    UpToDate {
        /// The latest published version, which is not newer.
        version: Version,
    },
    /// A newer release was downloaded into the folder, and checked.
    Downloaded {
        /// The version of the installer.
        version: Version,
        /// The file of the installer.
        path: PathBuf,
    },
}

/// Look for a release newer than `current` and download its installer for this
/// system into the updates folder, checked against the signed list. This is
/// what "Download and check" does in the window.
///
/// # Errors
///
/// When the release cannot be read, has no installer for this system, or its
/// download does not verify. See [`UpdateError`].
pub fn upgrade(current: &Version, folder: &Path) -> Result<Upgrade, UpdateError> {
    upgrade_from(
        net::API_URL,
        net::ORIGIN,
        verify::PUBLIC_KEY_FILE,
        current,
        folder,
        current_installer(),
    )
}

/// [`upgrade`] with the places and the installer given, so that tests can use a
/// local server and a test key.
///
/// # Errors
///
/// As [`upgrade`].
pub fn upgrade_from(
    api_url: &str,
    origin: &str,
    key_file: &str,
    current: &Version,
    folder: &Path,
    installer: Option<Installer>,
) -> Result<Upgrade, UpdateError> {
    let release = net::check(api_url, origin)?;
    if release.version <= *current {
        return Ok(Upgrade::UpToDate {
            version: release.version,
        });
    }
    let installer = installer.ok_or(UpdateError::NoInstaller)?;
    let path = net::download_latest(api_url, origin, key_file, installer, folder)?;
    Ok(Upgrade::Downloaded {
        version: release.version,
        path,
    })
}

/// The data folder of OpenCircuit for this user: `$XDG_DATA_HOME/opencircuit`
/// or `~/.local/share/opencircuit` on Linux and other Unix systems,
/// `~/Library/Application Support/OpenCircuit` on macOS and
/// `%LOCALAPPDATA%\OpenCircuit` on Windows. `None` when the environment names
/// none.
#[must_use]
pub fn user_data_dir() -> Option<PathBuf> {
    let env = |name: &str| {
        std::env::var_os(name)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    Some(if cfg!(windows) {
        env("LOCALAPPDATA")
            .or_else(|| env("APPDATA"))?
            .join("OpenCircuit")
    } else if cfg!(target_os = "macos") {
        env("HOME")?
            .join("Library/Application Support")
            .join("OpenCircuit")
    } else {
        env("XDG_DATA_HOME")
            .or_else(|| env("HOME").map(|home| home.join(".local/share")))?
            .join("opencircuit")
    })
}

/// The folder the installers are downloaded to: `updates` in the data folder.
#[must_use]
pub fn updates_folder() -> Option<PathBuf> {
    user_data_dir().map(|dir| dir.join("updates"))
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::thread;

    use super::{Installer, UpdateError, Upgrade, Version, upgrade_from};
    use crate::verify::fixtures::{INSTALLER, KEY_FILE, LIST, SIGNATURE};

    /// A free port on this machine, and the origin that names it.
    fn bind() -> (TcpListener, String) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
        let port = listener.local_addr().expect("an address").port();
        (listener, format!("http://127.0.0.1:{port}/"))
    }

    /// Serve `routes` (path, body) over plain HTTP on `listener`, for `requests`
    /// requests.
    fn serve(listener: TcpListener, routes: Vec<(String, Vec<u8>)>, requests: usize) {
        thread::spawn(move || {
            for _ in 0..requests {
                let Ok((mut stream, _)) = listener.accept() else {
                    return;
                };
                let mut request = [0_u8; 2048];
                let read = stream.read(&mut request).unwrap_or(0);
                let head = String::from_utf8_lossy(request.get(..read).unwrap_or(&[])).into_owned();
                let path = head.split_whitespace().nth(1).unwrap_or("/").to_owned();
                let (status, body) = match routes.iter().find(|(route, _)| *route == path) {
                    Some((_, body)) => ("200 OK", body.clone()),
                    None => ("404 Not Found", Vec::new()),
                };
                let reply = format!(
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(reply.as_bytes());
                let _ = stream.write_all(&body);
            }
        });
    }

    /// The release `v0.0.3`, with its installer, its checksums and, when
    /// `signed`, the signature of the checksums; all served at `origin`.
    fn release_json(origin: &str, signed: bool) -> Vec<u8> {
        let signature = if signed {
            format!(
                r#", {{"name": "SHA256SUMS.minisig", "browser_download_url": "{origin}sums.minisig", "size": {}}}"#,
                SIGNATURE.len()
            )
        } else {
            String::new()
        };
        format!(
            r#"{{"tag_name": "v0.0.3", "html_url": "{origin}releases/tag/v0.0.3", "body": "Notes",
                "assets": [
                  {{"name": "opencircuit_0.0.3_amd64.deb", "browser_download_url": "{origin}asset.deb", "size": {}}},
                  {{"name": "SHA256SUMS", "browser_download_url": "{origin}sums", "size": {}}}{signature}
                ]}}"#,
            INSTALLER.len(),
            LIST.len()
        )
        .into_bytes()
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oc-upgrade-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn version(text: &str) -> Version {
        match Version::parse(text) {
            Some(version) => version,
            None => panic!("{text} is a version"),
        }
    }

    #[test]
    fn a_newer_signed_release_is_downloaded_and_checked() {
        let (listener, origin) = bind();
        serve(
            listener,
            vec![
                ("/release".to_owned(), release_json(&origin, true)),
                ("/asset.deb".to_owned(), INSTALLER.to_vec()),
                ("/sums".to_owned(), LIST.as_bytes().to_vec()),
                ("/sums.minisig".to_owned(), SIGNATURE.as_bytes().to_vec()),
            ],
            5,
        );
        let folder = scratch("newer");

        let answer = upgrade_from(
            &format!("{origin}release"),
            &origin,
            KEY_FILE,
            &version("0.0.2"),
            &folder,
            Some(Installer::Deb),
        );

        let Ok(Upgrade::Downloaded { version, path }) = answer else {
            panic!("a newer release is downloaded, found {answer:?}");
        };
        assert_eq!(version.to_string(), "0.0.3");
        assert_eq!(std::fs::read(&path).expect("the installer"), INSTALLER);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn the_latest_release_is_not_downloaded_when_the_program_is_up_to_date() {
        let (listener, origin) = bind();
        serve(
            listener,
            vec![("/release".to_owned(), release_json(&origin, true))],
            1,
        );
        let folder = scratch("current");

        let answer = upgrade_from(
            &format!("{origin}release"),
            &origin,
            KEY_FILE,
            &version("0.0.3"),
            &folder,
            Some(Installer::Deb),
        );

        assert_eq!(
            answer,
            Ok(Upgrade::UpToDate {
                version: version("0.0.3")
            })
        );
        assert!(!folder.exists(), "nothing is downloaded");
    }

    #[test]
    fn a_newer_release_without_an_installer_for_this_system_is_an_error() {
        let (listener, origin) = bind();
        serve(
            listener,
            vec![("/release".to_owned(), release_json(&origin, true))],
            1,
        );
        let folder = scratch("no-installer");

        let answer = upgrade_from(
            &format!("{origin}release"),
            &origin,
            KEY_FILE,
            &version("0.0.2"),
            &folder,
            None,
        );

        assert_eq!(answer, Err(UpdateError::NoInstaller));
    }

    #[test]
    fn an_unsigned_newer_release_is_not_downloaded() {
        let (listener, origin) = bind();
        serve(
            listener,
            vec![("/release".to_owned(), release_json(&origin, false))],
            2,
        );
        let folder = scratch("unsigned");

        let answer = upgrade_from(
            &format!("{origin}release"),
            &origin,
            KEY_FILE,
            &version("0.0.2"),
            &folder,
            Some(Installer::Deb),
        );

        assert_eq!(answer, Err(UpdateError::NotSigned));
        assert!(!folder.exists());
    }
}
