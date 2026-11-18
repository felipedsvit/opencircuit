//! Autosave and recovery after an unexpected exit (M9.1, issue #118).
//!
//! While a project has unsaved changes, the window writes a copy of it into a
//! per-user data folder at most once per interval, never next to the person's
//! file. The copy is removed when the project is saved, replaced or closed on
//! purpose. A copy still there when a window starts belongs to a window that
//! ended without closing (a crash, a killed process, a power loss) and is
//! offered for recovery, unless its window still runs: a running window
//! renews the heartbeat of its copy once per interval.
//!
//! The folder is chosen in `main` and handed to the window, so that a window
//! made by a test never sees the copies of a real session.

#![cfg_attr(not(test), deny(clippy::arithmetic_side_effects))]

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use circuit_project::MAX_PROJECT_BYTES;
use serde::{Deserialize, Serialize};

use crate::files::{FileError, read_text, write_atomic};
use crate::session::Session;

/// How often a copy is written while there are unsaved changes, in seconds.
pub const DEFAULT_INTERVAL_SECONDS: f64 = 30.0;
/// Most copies a window looks at when it starts.
const MAX_ENTRIES: usize = 64;
/// Largest description file read.
const MAX_META_BYTES: usize = 64 * 1024;
/// Extension of the copy of a project.
const COPY_EXTENSION: &str = "ocircuit";
/// Extension of the description of a copy.
const META_EXTENSION: &str = "json";

/// The folder of the recovery copies of this user.
#[derive(Clone, Debug)]
pub struct RecoveryStore {
    dir: PathBuf,
}

/// What is known about a copy, next to it.
#[derive(Debug, Deserialize, Serialize)]
struct Meta {
    /// Format of this description.
    format: u32,
    /// The file the project came from, if it had one.
    original: Option<PathBuf>,
    /// The name of the project.
    name: String,
    /// When the copy was written, in seconds since 1970.
    saved_at: u64,
    /// When its window last said it is still running, in seconds since 1970.
    heartbeat: u64,
}

/// A copy left by a window that is no longer running.
#[derive(Clone, Debug)]
pub struct Entry {
    /// The file the project came from, if it had one.
    pub original: Option<PathBuf>,
    /// The name of the project.
    pub name: String,
    /// When the copy was written, in seconds since 1970.
    pub saved_at: u64,
    copy: PathBuf,
    meta: PathBuf,
}

impl Entry {
    /// The text of the copy.
    ///
    /// # Errors
    ///
    /// Returns [`FileError`] when the copy cannot be read.
    pub fn read(&self) -> Result<String, FileError> {
        read_text(&self.copy, MAX_PROJECT_BYTES)
    }

    /// Remove the copy and its description.
    pub fn discard(&self) {
        // Best effort: a copy that cannot be removed is offered again.
        let _ = fs::remove_file(&self.copy);
        let _ = fs::remove_file(&self.meta);
    }
}

/// Seconds since 1970 now; 0 on a clock set before it.
fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

impl RecoveryStore {
    /// The folder in the data folder of this user: `$XDG_DATA_HOME` or
    /// `~/.local/share` on Linux and other Unix systems, `~/Library/Application
    /// Support` on macOS and `%LOCALAPPDATA%` on Windows. `None` when the
    /// environment names none.
    #[must_use]
    pub fn in_user_data() -> Option<Self> {
        let env = |name: &str| {
            std::env::var_os(name)
                .map(PathBuf::from)
                .filter(|path| path.is_absolute())
        };
        let base = if cfg!(windows) {
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
        };
        Some(Self::at(base.join("recovery")))
    }

    /// A store in `dir`, created when the first copy is written.
    #[must_use]
    pub const fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// The folder.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn paths(&self, key: &str) -> (PathBuf, PathBuf) {
        (
            self.dir.join(format!("{key}.{COPY_EXTENSION}")),
            self.dir.join(format!("{key}.{META_EXTENSION}")),
        )
    }

    /// Create the folder, readable by this user only.
    fn ensure_dir(&self) -> io::Result<()> {
        let mut builder = fs::DirBuilder::new();
        builder.recursive(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt as _;
            builder.mode(0o700);
        }
        builder.create(&self.dir)
    }

    /// The copies whose window has not renewed its heartbeat for
    /// `stale_after` seconds, oldest first, leaving out `own`. A description
    /// that cannot be read is skipped, not removed.
    #[must_use]
    pub fn orphans(&self, own: &str, stale_after: u64) -> Vec<Entry> {
        let Ok(listing) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let now = unix_now();
        let mut entries: Vec<Entry> = listing
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .is_some_and(|extension| extension == META_EXTENSION)
            })
            .take(MAX_ENTRIES)
            .filter_map(|meta_path| {
                let key = meta_path.file_stem()?.to_str()?.to_owned();
                if key == own {
                    return None;
                }
                let text = read_text(&meta_path, MAX_META_BYTES).ok()?;
                let meta: Meta = serde_json::from_str(&text).ok()?;
                if meta.format != 1 || now.saturating_sub(meta.heartbeat) < stale_after {
                    return None;
                }
                let (copy, meta_path) = self.paths(&key);
                copy.is_file().then_some(Entry {
                    original: meta.original,
                    name: meta.name,
                    saved_at: meta.saved_at,
                    copy,
                    meta: meta_path,
                })
            })
            .collect();
        entries.sort_by_key(|entry| entry.saved_at);
        entries
    }
}

/// The autosave of one window: one copy at a time, of the open project.
#[derive(Debug)]
pub struct Autosave {
    store: RecoveryStore,
    key: String,
    interval: f64,
    /// The revision of the copy on disk, if there is one.
    written: Option<u64>,
    /// The project of the copy on disk.
    meta: Option<Meta>,
    /// When the copy or its heartbeat was last written, in window seconds.
    last_write: f64,
    /// Why the last write failed.
    failure: Option<String>,
}

impl Autosave {
    /// The autosave of this window, writing at most once per `interval`
    /// seconds of `now` (the window's clock).
    #[must_use]
    pub fn new(store: RecoveryStore, interval: f64, now: f64) -> Self {
        let key = format!("window-{}-{}", std::process::id(), unix_now());
        Self {
            store,
            key,
            interval: interval.max(0.0),
            written: None,
            meta: None,
            last_write: now,
            failure: None,
        }
    }

    /// The name of this window's copy, which [`RecoveryStore::orphans`]
    /// leaves out.
    #[must_use]
    pub fn key(&self) -> &str {
        &self.key
    }

    /// The store.
    #[must_use]
    pub const fn store(&self) -> &RecoveryStore {
        &self.store
    }

    /// Seconds after which a copy whose heartbeat stopped belongs to a
    /// window that is gone: three intervals, and never under a minute.
    #[must_use]
    pub fn stale_after(&self) -> u64 {
        let seconds = (self.interval * 3.0).clamp(60.0, 86_400.0);
        // In range by the clamp above.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let seconds = seconds as u64;
        seconds
    }

    /// Whether the copy on disk is the project as it is now.
    #[must_use]
    pub fn is_current(&self, session: Option<&Session>) -> bool {
        session.is_some_and(|session| self.written == Some(session.history().revision()))
    }

    /// Seconds until the next copy or heartbeat is due.
    #[must_use]
    pub fn due_in(&self, now: f64) -> f64 {
        self.interval - (now - self.last_write)
    }

    /// Why the last copy could not be written, until one is.
    #[must_use]
    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }

    /// Called once per frame: write a copy of a project with unsaved
    /// changes when the interval has passed since the last one, renew the
    /// heartbeat of an unchanged copy, and remove the copy of a project
    /// that has none.
    pub fn tick(&mut self, now: f64, session: Option<&Session>) {
        let Some(session) = session.filter(|session| session.is_dirty()) else {
            self.clear();
            return;
        };
        if now - self.last_write < self.interval {
            return;
        }
        self.last_write = now;
        let revision = session.history().revision();
        let result = if self.written == Some(revision) {
            self.beat()
        } else {
            self.write(session, revision)
        };
        self.failure = result.err();
    }

    fn write(&mut self, session: &Session, revision: u64) -> Result<(), String> {
        let text = session.text().map_err(|error| error.to_string())?;
        self.store
            .ensure_dir()
            .map_err(|error| format!("{}: {error}", self.store.dir.display()))?;
        let (copy, _) = self.store.paths(&self.key);
        write_atomic(&copy, &text).map_err(|error| error.to_string())?;
        let now = unix_now();
        self.meta = Some(Meta {
            format: 1,
            original: session.path().map(Path::to_path_buf),
            name: session.document().project.name.clone(),
            saved_at: now,
            heartbeat: now,
        });
        self.write_meta()?;
        self.written = Some(revision);
        Ok(())
    }

    fn beat(&mut self) -> Result<(), String> {
        if let Some(meta) = &mut self.meta {
            meta.heartbeat = unix_now();
        }
        self.write_meta()
    }

    fn write_meta(&self) -> Result<(), String> {
        let Some(meta) = &self.meta else {
            return Ok(());
        };
        let (_, path) = self.store.paths(&self.key);
        let text = serde_json::to_string_pretty(meta).map_err(|error| error.to_string())?;
        write_atomic(&path, text).map_err(|error| error.to_string())
    }

    /// Remove this window's copy: the project was saved, replaced or closed
    /// on purpose.
    pub fn clear(&mut self) {
        if self.written.take().is_some() || self.meta.is_some() {
            let (copy, meta) = self.store.paths(&self.key);
            // Best effort: a leftover copy is offered once and can be discarded.
            let _ = fs::remove_file(copy);
            let _ = fs::remove_file(meta);
        }
        self.meta = None;
        self.failure = None;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    /// A fresh store in the temporary directory, removed when dropped.
    struct Scratch(RecoveryStore);

    impl Scratch {
        fn new() -> Self {
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!(
                "opencircuit-recovery-test-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&dir);
            Self(RecoveryStore::at(dir))
        }

        fn files(&self) -> Vec<String> {
            let mut names: Vec<String> = fs::read_dir(self.0.dir())
                .map(|listing| {
                    listing
                        .filter_map(Result::ok)
                        .map(|entry| entry.file_name().to_string_lossy().into_owned())
                        .collect()
                })
                .unwrap_or_default();
            names.sort();
            names
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(self.0.dir());
        }
    }

    /// An untitled project with one edit, so it has unsaved changes.
    fn edited() -> Session {
        let mut session = Session::untitled();
        let tx = circuit_commands::sheets::set_project_info(
            session.document(),
            "Recovered pump",
            None,
            None,
            None,
            None,
        )
        .unwrap();
        session.edit(tx).unwrap();
        session
    }

    #[test]
    fn nothing_is_written_for_a_project_without_unsaved_changes() {
        let scratch = Scratch::new();
        let mut autosave = Autosave::new(scratch.0.clone(), 0.0, 0.0);

        autosave.tick(1.0, Some(&Session::untitled()));
        autosave.tick(2.0, None);

        assert!(scratch.files().is_empty());
    }

    #[test]
    fn a_copy_is_written_after_the_interval_and_removed_once_saved() {
        let scratch = Scratch::new();
        let mut autosave = Autosave::new(scratch.0.clone(), 30.0, 0.0);
        let mut session = edited();

        autosave.tick(10.0, Some(&session));
        assert!(scratch.files().is_empty(), "before the interval");

        autosave.tick(31.0, Some(&session));
        let key = autosave.key().to_owned();
        assert_eq!(
            scratch.files(),
            [format!("{key}.json"), format!("{key}.ocircuit")]
        );
        assert!(autosave.is_current(Some(&session)));

        session.mark_saved();
        autosave.tick(32.0, Some(&session));
        assert!(scratch.files().is_empty());
    }

    #[test]
    fn a_running_window_is_not_offered_and_a_stopped_one_is() {
        let scratch = Scratch::new();
        let mut running = Autosave::new(scratch.0.clone(), 0.0, 0.0);
        running.tick(1.0, Some(&edited()));

        let other = format!("{}-other", running.key());
        assert!(
            scratch.0.orphans(&other, 60).is_empty(),
            "heartbeat is fresh"
        );

        let found = scratch.0.orphans(&other, 0);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Recovered pump");
        assert_eq!(found[0].original, None);
        let recovered = Session::load(&found[0].read().unwrap(), None).unwrap();
        assert_eq!(recovered.document().project.name, "Recovered pump");

        assert!(scratch.0.orphans(running.key(), 0).is_empty(), "own copy");

        found[0].discard();
        assert!(scratch.files().is_empty());
    }

    #[test]
    fn a_damaged_description_is_skipped_not_removed() {
        let scratch = Scratch::new();
        fs::create_dir_all(scratch.0.dir()).unwrap();
        fs::write(scratch.0.dir().join("broken.json"), "{ not json").unwrap();
        fs::write(scratch.0.dir().join("broken.ocircuit"), "{}").unwrap();

        assert!(scratch.0.orphans("mine", 0).is_empty());
        assert_eq!(scratch.files(), ["broken.json", "broken.ocircuit"]);
    }

    #[test]
    fn a_folder_that_cannot_be_made_is_reported_and_retried() {
        let scratch = Scratch::new();
        // A file where the folder should be.
        fs::write(scratch.0.dir(), "in the way").unwrap();
        let mut autosave = Autosave::new(scratch.0.clone(), 0.0, 0.0);
        let session = edited();

        autosave.tick(1.0, Some(&session));
        assert!(autosave.failure().is_some());
        assert!(!autosave.is_current(Some(&session)));

        fs::remove_file(scratch.0.dir()).unwrap();
        autosave.tick(2.0, Some(&session));
        assert!(autosave.failure().is_none());
        assert!(autosave.is_current(Some(&session)));
    }

    #[cfg(unix)]
    #[test]
    fn the_folder_is_private_to_its_user() {
        use std::os::unix::fs::PermissionsExt as _;
        let scratch = Scratch::new();
        let mut autosave = Autosave::new(scratch.0.clone(), 0.0, 0.0);

        autosave.tick(1.0, Some(&edited()));

        let mode = fs::metadata(scratch.0.dir()).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }

    #[test]
    fn the_user_folder_is_absolute_and_named_for_the_program() {
        if let Some(store) = RecoveryStore::in_user_data() {
            assert!(store.dir().is_absolute());
            assert!(
                store
                    .dir()
                    .to_string_lossy()
                    .to_lowercase()
                    .contains("opencircuit")
            );
        }
    }
}
