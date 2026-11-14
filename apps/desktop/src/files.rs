//! Reading project files from disk.

#![cfg_attr(not(test), deny(clippy::arithmetic_side_effects))]

use std::fmt;
use std::fs::File;
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};

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
    Ok(text)
}

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
    fn file_that_is_not_utf8_is_refused() {
        let file = TempFile::with(&[0xff, 0xfe]);

        assert!(read_text(&file.0, 10).is_err());
    }
}
