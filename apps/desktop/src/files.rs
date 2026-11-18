//! Reading and writing project files.

#![cfg_attr(not(test), deny(clippy::arithmetic_side_effects))]

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// A file that could not be read.
#[derive(Debug)]
pub struct FileError {
    /// The file.
    pub path: PathBuf,
    /// What went wrong.
    pub source: io::Error,
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.source)
    }
}

impl std::error::Error for FileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// Read a text file, refusing one larger than `limit` bytes before reading
/// it into memory. The same rule as the command line.
///
/// # Errors
///
/// Returns [`FileError`] when the file cannot be opened or read, is not
/// UTF-8, or is larger than `limit`.
pub fn read_text(path: &Path, limit: usize) -> Result<String, FileError> {
    let error = |source| FileError {
        path: path.to_path_buf(),
        source,
    };
    let file = File::open(path).map_err(error)?;
    let mut text = String::new();
    // Read at most one byte past the limit: enough to know it was exceeded
    // without trusting the reported file size.
    let cap = u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1);
    file.take(cap).read_to_string(&mut text).map_err(error)?;
    if text.len() > limit {
        return Err(error(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("the file is larger than {limit} bytes"),
        )));
    }
    // Drop a byte order mark at the start: some editors on Windows write one
    // in front of UTF-8 text, and JSON does not allow it. A file is never
    // saved with one.
    if text.starts_with(BOM) {
        text.drain(..BOM.len_utf8());
    }
    Ok(text)
}

/// The byte order mark.
const BOM: char = '\u{feff}';

/// Write a file atomically: write a temporary file in the same directory,
/// flush it to disk, then rename it over the destination. A failure leaves
/// the destination as it was and removes the temporary file. Through a link,
/// the file it points to is replaced and the link kept; a replaced file
/// keeps its permissions and group. The same rule as the command line.
///
/// # Errors
///
/// Returns [`FileError`] when the path has no file name or the file cannot
/// be written, synced or renamed.
pub fn write_atomic(path: &Path, contents: impl AsRef<[u8]>) -> Result<(), FileError> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let error = |source| FileError {
        path: path.to_path_buf(),
        source,
    };

    if path.file_name().is_none() {
        return Err(error(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the path has no file name",
        )));
    }
    // Saving through a link updates the file it points to and keeps the link.
    let target = link_target(path).map_err(error)?;
    let temporary = temporary_next_to(&target, COUNTER.fetch_add(1, Ordering::Relaxed));
    replace_through(&temporary, &target, contents.as_ref()).map_err(error)
}

/// Write `contents` to `temporary`, which must not exist yet, and rename it
/// over `path`, then flush the directory. The same rule as the command line.
fn replace_through(temporary: &Path, path: &Path, contents: &[u8]) -> io::Result<()> {
    // `create_new` refuses a file or a link already at the temporary name:
    // writing through it could change a file elsewhere, and it is not ours
    // to remove.
    let existing = fs::metadata(path).ok();
    let written = {
        let mut file = create_temporary(temporary, existing.is_some())?;
        file.write_all(contents)
            .and_then(|()| keep_access(&file, existing.as_ref()))
            .and_then(|()| file.sync_all())
    }
    // The file is closed here, before the rename.
    .and_then(|()| fs::rename(temporary, path));
    if let Err(error) = written {
        // Best effort: the write already failed and that is what is reported.
        let _ = fs::remove_file(temporary);
        return Err(error);
    }
    sync_parent(path);
    Ok(())
}

/// Most links followed to find the file a path names.
const MAX_LINKS: u32 = 16;

/// The file `path` names once its links are followed, at most [`MAX_LINKS`]
/// of them. A path that is not a link, or that does not exist, is returned
/// as it is; a link to a missing file gives that file's path.
fn link_target(path: &Path) -> io::Result<PathBuf> {
    let mut current = path.to_path_buf();
    for _ in 0..MAX_LINKS {
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let next = fs::read_link(&current)?;
                current = match current.parent() {
                    Some(parent) if next.is_relative() => parent.join(next),
                    _ => next,
                };
            }
            _ => return Ok(current),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("more than {MAX_LINKS} links to follow"),
    ))
}

/// A name for the temporary file in the directory of `target`. It is short
/// and does not repeat the name of the target, which may already be close
/// to the 255-byte limit of most file systems.
fn temporary_next_to(target: &Path, count: u64) -> PathBuf {
    target.with_file_name(format!(".opencircuit-{}-{count}.tmp", std::process::id()))
}

/// Create the temporary file, refusing one that exists. When it replaces a
/// file, only its owner can read it until it takes the access of that file.
#[cfg(unix)]
fn create_temporary(temporary: &Path, replaces: bool) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt as _;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    if replaces {
        options.mode(0o600);
    }
    options.open(temporary)
}

#[cfg(not(unix))]
fn create_temporary(temporary: &Path, _replaces: bool) -> io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary)
}

/// Give the new file the permissions and the group of the file it
/// replaces, so that a save never opens a private project to others. When
/// this user cannot give the old group, the group gets no access rather
/// than the access meant for another group.
#[cfg(unix)]
fn keep_access(file: &File, replaced: Option<&fs::Metadata>) -> io::Result<()> {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
    let Some(replaced) = replaced else {
        return Ok(());
    };
    let mut mode = replaced.mode() & 0o777;
    if std::os::unix::fs::fchown(file, None, Some(replaced.gid())).is_err() {
        mode &= !0o070;
    }
    file.set_permissions(fs::Permissions::from_mode(mode))
}

/// Elsewhere a new file takes the access of its directory.
#[cfg(not(unix))]
fn keep_access(_file: &File, _replaced: Option<&fs::Metadata>) -> io::Result<()> {
    Ok(())
}

/// Flush the directory that holds `path`, so that the rename survives a
/// power loss and not only the contents of the file. Best effort: some file
/// systems refuse to sync a directory, and the file is complete either way.
#[cfg(unix)]
fn sync_parent(path: &Path) {
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    if let Ok(directory) = File::open(parent) {
        let _ = directory.sync_all();
    }
}

/// The standard library has no portable way to flush a directory elsewhere;
/// the rename is as durable as the file system makes it.
#[cfg(not(unix))]
fn sync_parent(_path: &Path) {}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    /// A file in the temporary directory, removed when dropped.
    struct TempFile(PathBuf);

    impl TempFile {
        fn with(contents: &[u8]) -> Self {
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let path = std::env::temp_dir().join(format!(
                "opencircuit-desktop-test-{}-{}.ocircuit",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            std::fs::write(&path, contents).unwrap();
            Self(path)
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn file_within_the_limit_is_read_whole() {
        let file = TempFile::with(b"{}");

        assert_eq!(read_text(&file.0, 2).unwrap(), "{}");
    }

    #[test]
    fn file_over_the_limit_is_refused() {
        let file = TempFile::with(b"{ }");

        let error = read_text(&file.0, 2).unwrap_err();

        assert_eq!(error.source.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn missing_file_names_the_path() {
        let path = std::env::temp_dir().join("opencircuit-desktop-test-does-not-exist.ocircuit");

        let error = read_text(&path, 10).unwrap_err();

        assert!(error.to_string().contains("does-not-exist"), "{error}");
    }

    #[test]
    fn atomic_write_replaces_the_whole_file() {
        let file = TempFile::with(b"old contents that are longer");

        write_atomic(&file.0, "new").unwrap();

        assert_eq!(read_text(&file.0, 100).unwrap(), "new");
    }

    #[test]
    fn atomic_write_into_a_missing_directory_fails_and_leaves_nothing() {
        let directory = std::env::temp_dir().join(format!(
            "opencircuit-desktop-missing-{}",
            std::process::id()
        ));
        let path = directory.join("project.ocircuit");

        assert!(write_atomic(&path, "x").is_err());
        assert!(!directory.exists());
    }

    #[test]
    fn atomic_write_leaves_no_temporary_file_behind() {
        // A directory of its own: other tests of this process save into the
        // shared temporary directory at the same time.
        let directory = std::env::temp_dir().join(format!(
            "opencircuit-desktop-leftovers-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("project.ocircuit");

        write_atomic(&path, "first").unwrap();
        write_atomic(&path, "content").unwrap();

        let names: Vec<String> = std::fs::read_dir(&directory)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        let _ = std::fs::remove_dir_all(&directory);
        assert_eq!(names, ["project.ocircuit"]);
    }

    #[test]
    fn a_file_already_at_the_temporary_name_is_neither_overwritten_nor_removed() {
        let file = TempFile::with(b"saved");
        let other = TempFile::with(b"someone else's");

        let error = replace_through(&other.0, &file.0, b"new").unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(read_text(&file.0, 100).unwrap(), "saved");
        assert_eq!(read_text(&other.0, 100).unwrap(), "someone else's");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_at_the_temporary_name_is_not_followed() {
        let file = TempFile::with(b"saved");
        let victim = TempFile::with(b"precious");
        let link = TempFile(file.0.with_extension("link"));
        let _ = std::fs::remove_file(&link.0);
        std::os::unix::fs::symlink(&victim.0, &link.0).unwrap();

        assert!(replace_through(&link.0, &file.0, b"new").is_err());

        assert_eq!(read_text(&victim.0, 100).unwrap(), "precious");
        assert_eq!(read_text(&file.0, 100).unwrap(), "saved");
    }

    #[test]
    fn a_byte_order_mark_is_dropped() {
        let file = TempFile::with(b"\xef\xbb\xbf{}");

        assert_eq!(read_text(&file.0, 10).unwrap(), "{}");
    }

    #[cfg(unix)]
    #[test]
    fn saving_through_a_link_updates_its_target_and_keeps_the_link() {
        let target = TempFile::with(b"old");
        let link = TempFile(target.0.with_extension("via-link"));
        let _ = std::fs::remove_file(&link.0);
        std::os::unix::fs::symlink(&target.0, &link.0).unwrap();

        write_atomic(&link.0, "new").unwrap();

        assert!(
            std::fs::symlink_metadata(&link.0)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(read_text(&target.0, 10).unwrap(), "new");
    }

    #[cfg(unix)]
    #[test]
    fn a_private_file_stays_private_after_a_save() {
        use std::os::unix::fs::PermissionsExt as _;
        let file = TempFile::with(b"old");
        std::fs::set_permissions(&file.0, std::fs::Permissions::from_mode(0o600)).unwrap();

        write_atomic(&file.0, "new").unwrap();

        let mode = std::fs::metadata(&file.0).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[test]
    fn file_that_is_not_utf8_is_refused() {
        let file = TempFile::with(&[0xff, 0xfe]);

        assert!(read_text(&file.0, 10).is_err());
    }
}
