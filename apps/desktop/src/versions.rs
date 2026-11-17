//! Version history (M9.1, issue #118): a copy of the project at every save,
//! kept in the user's data folder, never next to the project.
//!
//! The copies of one project live in a folder named after its path. A save
//! that changes nothing adds no copy, and only the newest
//! [`MAX_VERSIONS`] are kept. A version is opened as a new, unsaved project:
//! restoring never writes over the file.

#![cfg_attr(not(test), deny(clippy::arithmetic_side_effects))]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use circuit_project::MAX_PROJECT_BYTES;

use crate::files::{FileError, read_text, write_atomic};
use crate::project_lock::fnv1a;
use crate::recovery::{private_dir, user_data_dir};

/// Most versions kept for one project.
pub const MAX_VERSIONS: usize = 20;
/// Most files looked at in the folder of one project.
const MAX_FILES: usize = 256;
/// Extension of a version.
const EXTENSION: &str = "ocircuit";

/// The folder of the versions of this user.
#[derive(Clone, Debug)]
pub struct VersionStore {
    dir: PathBuf,
}

/// One saved version of a project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Version {
    /// When it was saved, in seconds since 1970 (UTC).
    pub saved_at: u64,
    path: PathBuf,
}

impl Version {
    /// The text of the version.
    ///
    /// # Errors
    ///
    /// Returns [`FileError`] when the copy cannot be read.
    pub fn read(&self) -> Result<String, FileError> {
        read_text(&self.path, MAX_PROJECT_BYTES)
    }

    /// When it was saved, as `2026-11-17 18:05 UTC`.
    #[must_use]
    pub fn label(&self) -> String {
        utc_label(self.saved_at)
    }
}

/// `YYYY-MM-DD HH:MM UTC` for seconds since 1970, from the civil calendar
/// (the algorithm of Howard Hinnant's `civil_from_days`).
#[must_use]
pub fn utc_label(seconds: u64) -> String {
    // Clamped to the year 9999 so that the arithmetic below stays in range.
    let seconds = seconds.min(253_402_300_799);
    let days = seconds / 86_400;
    let minute_of_day = (seconds % 86_400) / 60;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        minute_of_day / 60,
        minute_of_day % 60
    )
}

/// The year, month and day of a day count since 1970-01-01. The inputs are
/// below the year 10000, where none of these operations can overflow.
#[allow(clippy::arithmetic_side_effects)]
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

impl VersionStore {
    /// The `versions` folder of the user's data folder.
    #[must_use]
    pub fn in_user_data() -> Option<Self> {
        Some(Self::at(user_data_dir()?.join("versions")))
    }

    /// Versions in `dir`, created with the first one.
    #[must_use]
    pub const fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// The folder of the versions of `project`, the same for every way of
    /// writing its path.
    fn folder(&self, project: &Path) -> PathBuf {
        let real = fs::canonicalize(project).unwrap_or_else(|_| project.to_path_buf());
        self.dir
            .join(format!("{:016x}", fnv1a(real.to_string_lossy().as_bytes())))
    }

    /// Keep `text` as the newest version of `project`, unless it is the
    /// newest already, and drop the oldest beyond [`MAX_VERSIONS`].
    ///
    /// # Errors
    ///
    /// Returns the error of creating the folder or writing the copy.
    pub fn record(&self, project: &Path, text: &str) -> io::Result<()> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let folder = self.folder(project);
        let versions = self.list(project);
        if versions
            .first()
            .and_then(|newest| newest.read().ok())
            .is_some_and(|newest| newest == text)
        {
            return Ok(());
        }
        private_dir(&self.dir)?;
        private_dir(&folder)?;
        let name = format!(
            "{:020}-{:010}-{:020}.{EXTENSION}",
            unix_now(),
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        write_atomic(&folder.join(name), text).map_err(|error| error.source)?;
        for old in self.list(project).iter().skip(MAX_VERSIONS) {
            // Best effort: an old version left behind is dropped next time.
            let _ = fs::remove_file(&old.path);
        }
        Ok(())
    }

    /// The versions of `project`, newest first.
    #[must_use]
    pub fn list(&self, project: &Path) -> Vec<Version> {
        let Ok(listing) = fs::read_dir(self.folder(project)) else {
            return Vec::new();
        };
        let mut versions: Vec<(String, Version)> = listing
            .filter_map(Result::ok)
            .take(MAX_FILES)
            .filter_map(|entry| {
                let path = entry.path();
                if path
                    .extension()
                    .is_none_or(|extension| extension != EXTENSION)
                {
                    return None;
                }
                let name = path.file_stem()?.to_str()?.to_owned();
                let saved_at = name.split('-').next()?.parse().ok()?;
                Some((name, Version { saved_at, path }))
            })
            .collect();
        // The names are the time, the process and a counter, each padded
        // with zeros: in name order, oldest first.
        versions.sort_by(|a, b| b.0.cmp(&a.0));
        versions.into_iter().map(|(_, version)| version).collect()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicU32;

    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!(
                "opencircuit-versions-test-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("a.ocircuit"), "{}").unwrap();
            Self(dir)
        }

        fn store(&self) -> VersionStore {
            VersionStore::at(self.0.join("versions"))
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn each_save_that_changes_something_is_a_version_newest_first() {
        let scratch = Scratch::new();
        let project = scratch.0.join("a.ocircuit");
        let store = scratch.store();

        store.record(&project, "one").unwrap();
        store.record(&project, "one").unwrap();
        store.record(&project, "two").unwrap();

        let texts: Vec<String> = store
            .list(&project)
            .iter()
            .map(|version| version.read().unwrap())
            .collect();
        assert_eq!(texts, ["two", "one"]);
    }

    #[test]
    fn only_the_newest_versions_are_kept() {
        let scratch = Scratch::new();
        let project = scratch.0.join("a.ocircuit");
        let store = scratch.store();

        for n in 0..(MAX_VERSIONS + 5) {
            store.record(&project, &format!("version {n}")).unwrap();
        }

        let versions = store.list(&project);
        assert_eq!(versions.len(), MAX_VERSIONS);
        assert_eq!(
            versions.first().unwrap().read().unwrap(),
            format!("version {}", MAX_VERSIONS + 4)
        );
    }

    #[test]
    fn projects_have_their_own_versions() {
        let scratch = Scratch::new();
        fs::write(scratch.0.join("b.ocircuit"), "{}").unwrap();
        let store = scratch.store();

        store.record(&scratch.0.join("a.ocircuit"), "a").unwrap();

        assert_eq!(store.list(&scratch.0.join("a.ocircuit")).len(), 1);
        assert!(store.list(&scratch.0.join("b.ocircuit")).is_empty());
    }

    #[test]
    fn a_stray_file_in_the_folder_is_ignored() {
        let scratch = Scratch::new();
        let project = scratch.0.join("a.ocircuit");
        let store = scratch.store();
        store.record(&project, "one").unwrap();
        let folder = store.folder(&project);
        fs::write(folder.join("notes.txt"), "x").unwrap();
        fs::write(folder.join("not-a-time.ocircuit"), "x").unwrap();

        assert_eq!(store.list(&project).len(), 1);
    }

    #[test]
    fn times_are_written_as_utc_dates() {
        assert_eq!(utc_label(0), "1970-01-01 00:00 UTC");
        assert_eq!(utc_label(951_782_400), "2000-02-29 00:00 UTC");
        assert_eq!(utc_label(1_794_939_900), "2026-11-17 18:25 UTC");
        assert_eq!(utc_label(u64::MAX), "9999-12-31 23:59 UTC");
    }
}
