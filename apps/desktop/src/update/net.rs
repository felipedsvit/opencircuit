//! The network side of the update: the release lookup and the download of one
//! installer, with a size cap, a SHA-256 check, and a file that appears in its
//! place only when it is complete and correct.
//!
//! The connection uses the certificates of the system (the store of Windows,
//! macOS or the Linux distribution), so a company or school certificate
//! installed there works. The request names the program and its version, and
//! nothing about the person.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use sha2::{Digest, Sha256};

use super::UpdateError;
use super::release::{Asset, Installer, MAX_RELEASE_BYTES, Release, checksum_for};

/// The latest published release of the project.
pub const API_URL: &str = "https://api.github.com/repos/felipedsvit/opencircuit/releases/latest";
/// Where the project's links live. A link from a response is used only when it
/// starts with this.
pub const ORIGIN: &str = "https://github.com/felipedsvit/opencircuit/";

/// Largest installer accepted, in bytes.
pub const MAX_INSTALLER_BYTES: u64 = 512 << 20;
/// Largest `SHA256SUMS` file accepted, in bytes.
const MAX_SUMS_BYTES: u64 = 64 << 10;
/// How long the release lookup may take.
const CHECK_TIMEOUT: Duration = Duration::from_secs(20);
/// How long a whole download may take.
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// Size of the buffer a download is copied through.
const CHUNK_BYTES: usize = 64 << 10;

/// Look up the latest release at `api_url`, keeping only links under `origin`.
///
/// # Errors
///
/// When the request fails, the answer is not a release, or it is too large.
pub fn check(api_url: &str, origin: &str) -> Result<Release, UpdateError> {
    let agent = agent(CHECK_TIMEOUT)?;
    fetch_release(&agent, api_url, origin)
}

/// Look up the latest release, download its file for `installer` into
/// `folder`, and check it against the published SHA-256.
///
/// # Errors
///
/// When the release has no file for the system, its checksums are missing or
/// do not match, or the download fails.
pub fn download_latest(
    api_url: &str,
    origin: &str,
    installer: Installer,
    folder: &Path,
) -> Result<PathBuf, UpdateError> {
    let agent = agent(DOWNLOAD_TIMEOUT)?;
    let release = fetch_release(&agent, api_url, origin)?;
    let asset = release
        .asset_for(installer)
        .ok_or(UpdateError::NoInstaller)?;
    let sums = release.checksums().ok_or(UpdateError::NoChecksum)?;
    let listing = get(
        &agent,
        &sums.url,
        "application/octet-stream",
        MAX_SUMS_BYTES,
    )?;
    let listing = String::from_utf8(listing).map_err(|_| UpdateError::BadResponse)?;
    let sha256 = checksum_for(&listing, &asset.name).ok_or(UpdateError::NoChecksum)?;
    download(&agent, asset, &sha256, folder)
}

/// The agent that makes every request: the system certificates, a clear user
/// agent, and one timeout for the whole request.
fn agent(timeout: Duration) -> Result<ureq::Agent, UpdateError> {
    let loaded = rustls_native_certs::load_native_certs();
    let roots: Vec<ureq::tls::Certificate<'static>> = loaded
        .certs
        .iter()
        .map(|der| ureq::tls::Certificate::from_der(der.as_ref()).to_owned())
        .collect();
    let tls = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::from(roots))
        .build();
    let user_agent = format!(
        "OpenCircuit/{} ({})",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS
    );
    let config = ureq::Agent::config_builder()
        .tls_config(tls)
        .user_agent(user_agent)
        .timeout_global(Some(timeout))
        .build();
    Ok(ureq::Agent::new_with_config(config))
}

/// Read the release at `url`.
fn fetch_release(agent: &ureq::Agent, url: &str, origin: &str) -> Result<Release, UpdateError> {
    let bytes = get(agent, url, "application/vnd.github+json", MAX_RELEASE_BYTES)?;
    Release::from_json(&bytes, origin)
}

/// Read the body of `url`, which must be at most `cap` bytes.
fn get(agent: &ureq::Agent, url: &str, accept: &str, cap: u64) -> Result<Vec<u8>, UpdateError> {
    let mut response = agent
        .get(url)
        .header("Accept", accept)
        .call()
        .map_err(status_error)?;
    let mut reader = response
        .body_mut()
        .with_config()
        .limit(cap.saturating_add(1))
        .reader();
    let mut bytes = Vec::new();
    let read = (&mut reader)
        .take(cap.saturating_add(1))
        .read_to_end(&mut bytes);
    if too_long(bytes.len(), cap) {
        return Err(UpdateError::TooLarge);
    }
    read.map_err(|_| UpdateError::Connection)?;
    Ok(bytes)
}

/// Stream `asset` into `folder` under its own name, hashing it on the way.
/// The file is named `<name>.part` until its checksum matches, then it is
/// renamed into place. A failed download leaves nothing behind.
fn download(
    agent: &ureq::Agent,
    asset: &Asset,
    sha256: &str,
    folder: &Path,
) -> Result<PathBuf, UpdateError> {
    if asset.size > MAX_INSTALLER_BYTES {
        return Err(UpdateError::TooLarge);
    }
    create_private_dir(folder)?;
    let part = folder.join(format!("{}.part", asset.name));
    let target = folder.join(&asset.name);
    if let Err(error) = stream(agent, asset, sha256, &part) {
        let _ = fs::remove_file(&part);
        return Err(error);
    }
    fs::rename(&part, &target).map_err(|_| UpdateError::Disk)?;
    Ok(target)
}

/// Copy the body of `asset` into `part`, and check its size and SHA-256.
fn stream(
    agent: &ureq::Agent,
    asset: &Asset,
    sha256: &str,
    part: &Path,
) -> Result<(), UpdateError> {
    let mut response = agent.get(&asset.url).call().map_err(status_error)?;
    let mut reader = response
        .body_mut()
        .with_config()
        .limit(MAX_INSTALLER_BYTES.saturating_add(1))
        .reader();
    let mut file = File::create(part).map_err(|_| UpdateError::Disk)?;
    let mut hasher = Sha256::new();
    let mut total: u64 = 0;
    let mut buffer = vec![0_u8; CHUNK_BYTES];
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|_| UpdateError::Connection)?;
        if read == 0 {
            break;
        }
        let chunk = buffer.get(..read).ok_or(UpdateError::Connection)?;
        total = total.saturating_add(u64::try_from(read).unwrap_or(u64::MAX));
        if total > MAX_INSTALLER_BYTES {
            return Err(UpdateError::TooLarge);
        }
        hasher.update(chunk);
        file.write_all(chunk).map_err(|_| UpdateError::Disk)?;
    }
    file.sync_all().map_err(|_| UpdateError::Disk)?;
    if total != asset.size {
        return Err(UpdateError::Incomplete);
    }
    let digest: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if digest != sha256 {
        return Err(UpdateError::ChecksumMismatch);
    }
    Ok(())
}

/// Whether `len` bytes are more than `cap`.
fn too_long(len: usize, cap: u64) -> bool {
    u64::try_from(len).map_or(true, |len| len > cap)
}

/// The error of a request that failed. A refused request names the reason
/// only when it is the rate limit.
fn status_error(error: ureq::Error) -> UpdateError {
    match error {
        ureq::Error::StatusCode(403 | 429) => UpdateError::RateLimited,
        ureq::Error::StatusCode(_) => UpdateError::Unavailable,
        _ => UpdateError::Connection,
    }
}

/// Create `path` and its parents, readable by this user only where the system
/// has permissions.
fn create_private_dir(path: &Path) -> Result<(), UpdateError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
            .map_err(|_| UpdateError::Disk)
    }
    #[cfg(not(unix))]
    {
        fs::create_dir_all(path).map_err(|_| UpdateError::Disk)
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::thread;
    use std::time::Duration;

    use sha2::{Digest, Sha256};

    use super::{agent, download, fetch_release, get, too_long};
    use crate::update::UpdateError;
    use crate::update::release::Asset;

    /// A free port on this machine, and the origin that names it.
    fn bind() -> (TcpListener, String) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("a free port");
        let port = listener.local_addr().expect("an address").port();
        (listener, format!("http://127.0.0.1:{port}/"))
    }

    /// Serve `routes` (path, body) over plain HTTP on `listener`, for the
    /// requests a test makes.
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

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("oc-update-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn release_json(origin: &str, installer: &[u8], sums: &str) -> Vec<u8> {
        format!(
            r#"{{"tag_name": "v0.0.3",
                "html_url": "{origin}releases/tag/v0.0.3",
                "body": "Notes",
                "assets": [
                  {{"name": "opencircuit_0.0.3_amd64.deb",
                    "browser_download_url": "{origin}asset.deb",
                    "size": {}}},
                  {{"name": "SHA256SUMS",
                    "browser_download_url": "{origin}sums",
                    "size": {}}}
                ]}}"#,
            installer.len(),
            sums.len()
        )
        .into_bytes()
    }

    #[test]
    fn a_release_is_read_from_a_server_with_the_project_origin() {
        let (listener, origin) = bind();
        let body = release_json(&origin, &[0_u8; 7], "abc");
        serve(listener, vec![("/release".to_owned(), body)], 1);
        let agent = agent(Duration::from_secs(10)).expect("an agent");

        let release =
            fetch_release(&agent, &format!("{origin}release"), &origin).expect("a release");

        assert_eq!(release.version.to_string(), "0.0.3");
        assert_eq!(release.assets.len(), 2);
    }

    #[test]
    fn a_release_whose_links_are_outside_the_origin_is_refused() {
        let (listener, origin) = bind();
        let body = release_json(&origin, &[0_u8; 7], "abc");
        serve(listener, vec![("/release".to_owned(), body)], 1);
        let agent = agent(Duration::from_secs(10)).expect("an agent");

        let refused = fetch_release(
            &agent,
            &format!("{origin}release"),
            "https://github.com/felipedsvit/opencircuit/",
        );

        assert_eq!(refused, Err(UpdateError::BadResponse));
    }

    #[test]
    fn a_body_over_its_cap_is_refused() {
        let (listener, origin) = bind();
        serve(listener, vec![("/big".to_owned(), vec![b'a'; 100])], 1);
        let agent = agent(Duration::from_secs(10)).expect("an agent");

        assert_eq!(
            get(&agent, &format!("{origin}big"), "*/*", 10),
            Err(UpdateError::TooLarge)
        );
    }

    #[test]
    fn a_length_test_has_no_off_by_one() {
        assert!(!too_long(10, 10));
        assert!(too_long(11, 10));
    }

    #[test]
    fn an_installer_with_the_right_checksum_is_kept() {
        let payload = b"an installer".to_vec();
        let digest: String = Sha256::digest(&payload)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let (listener, origin) = bind();
        serve(
            listener,
            vec![("/asset.deb".to_owned(), payload.clone())],
            1,
        );
        let agent = agent(Duration::from_secs(10)).expect("an agent");
        let asset = Asset {
            name: "opencircuit_0.0.3_amd64.deb".to_owned(),
            url: format!("{origin}asset.deb"),
            size: u64::try_from(payload.len()).unwrap_or(0),
        };
        let folder = scratch("ok");

        let path = download(&agent, &asset, &digest, &folder).expect("downloaded");

        assert_eq!(std::fs::read(&path).expect("the file"), payload);
        assert!(!folder.join(format!("{}.part", asset.name)).exists());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn an_installer_that_does_not_match_its_checksum_leaves_nothing_behind() {
        let payload = b"tampered".to_vec();
        let (listener, origin) = bind();
        serve(
            listener,
            vec![("/asset.deb".to_owned(), payload.clone())],
            1,
        );
        let agent = agent(Duration::from_secs(10)).expect("an agent");
        let asset = Asset {
            name: "opencircuit_0.0.3_amd64.deb".to_owned(),
            url: format!("{origin}asset.deb"),
            size: u64::try_from(payload.len()).unwrap_or(0),
        };
        let folder = scratch("bad");
        let wrong = "0".repeat(64);

        let result = download(&agent, &asset, &wrong, &folder);

        assert_eq!(result, Err(UpdateError::ChecksumMismatch));
        assert!(!folder.join(&asset.name).exists());
        assert!(!folder.join(format!("{}.part", asset.name)).exists());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_download_shorter_than_announced_is_incomplete() {
        let (listener, origin) = bind();
        serve(
            listener,
            vec![("/asset.deb".to_owned(), b"short".to_vec())],
            1,
        );
        let agent = agent(Duration::from_secs(10)).expect("an agent");
        let asset = Asset {
            name: "opencircuit_0.0.3_amd64.deb".to_owned(),
            url: format!("{origin}asset.deb"),
            size: 500,
        };
        let folder = scratch("short");

        let result = download(&agent, &asset, &"0".repeat(64), &folder);

        assert_eq!(result, Err(UpdateError::Incomplete));
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn an_announced_installer_over_the_cap_is_refused_before_connecting() {
        let asset = Asset {
            name: "OpenCircuit-0.0.3-x86_64.AppImage".to_owned(),
            // Nothing listens on port 1: a connection attempt would fail
            // differently, so `TooLarge` shows that no request was made.
            url: "http://127.0.0.1:1/asset".to_owned(),
            size: super::MAX_INSTALLER_BYTES + 1,
        };
        let folder = scratch("oversized");

        let result = download(
            &agent(Duration::from_secs(1)).expect("an agent"),
            &asset,
            &"0".repeat(64),
            &folder,
        );

        assert_eq!(result, Err(UpdateError::TooLarge));
        assert!(
            !folder.exists(),
            "nothing is created for a refused download"
        );
    }
}
