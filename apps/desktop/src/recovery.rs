//! Autosave and recovery after an unexpected exit (M9.1, issue #118).
//!
//! While a project has unsaved changes, the window writes a copy of it into a
//! per-user data folder at most once per interval, never next to the person's
//! file. The copy is removed when the project is saved, replaced or closed on
//! purpose. A copy still there when a window starts belongs to a window that
//! ended without closing (a crash, a killed process, a power loss) and is
//! offered for recovery, unless its window still runs. A running window holds
//! an operating-system lock on a file next to its copy, which the system
//! releases however the process ends; where locks are not available, a
//! heartbeat renewed once per interval tells instead. A panic in the window
//! writes the copy at once (see [`PanicCopy`]).
//!
//! The folder is chosen in `main` and handed to the window, so that a window
//! made by a test never sees the copies of a real session.

#![cfg_attr(not(test), deny(clippy::arithmetic_side_effects))]

use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use circuit_model::Schematic;
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
/// Extension of the file a running window keeps locked.
const LOCK_EXTENSION: &str = "lock";

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

/// Create `dir` and its parents, the last one readable by this user only.
pub(crate) fn private_dir(dir: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        builder.mode(0o700);
    }
    builder.create(dir)
}

/// Whether another process holds the lock of the file at `path`: `None` when
/// the file is missing or cannot be locked here.
pub(crate) fn held_elsewhere(path: &Path) -> Option<bool> {
    let file = OpenOptions::new().read(true).write(true).open(path).ok()?;
    match file.try_lock() {
        // Released when `file` is dropped.
        Ok(()) => Some(false),
        Err(fs::TryLockError::WouldBlock) => Some(true),
        Err(fs::TryLockError::Error(_)) => None,
    }
}

/// Open (creating it) and lock the file at `path` for as long as the result
/// lives.
pub(crate) fn hold(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.try_lock().map_err(|error| match error {
        fs::TryLockError::WouldBlock => io::Error::new(
            io::ErrorKind::WouldBlock,
            format!("{} is locked by another process", path.display()),
        ),
        fs::TryLockError::Error(error) => error,
    })?;
    Ok(file)
}

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
    lock: PathBuf,
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
        let _ = fs::remove_file(&self.lock);
    }
}

/// Seconds since 1970 now; 0 on a clock set before it.
fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

impl RecoveryStore {
    /// The `recovery` folder of [`user_data_dir`].
    #[must_use]
    pub fn in_user_data() -> Option<Self> {
        Some(Self::at(user_data_dir()?.join("recovery")))
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

    fn lock_path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.{LOCK_EXTENSION}"))
    }

    /// Create the folder, readable by this user only.
    fn ensure_dir(&self) -> io::Result<()> {
        private_dir(&self.dir)
    }

    /// The copies whose window no longer runs, oldest first, leaving out
    /// `own`. A window runs while it holds the lock next to its copy; where
    /// that cannot be asked, while its heartbeat is younger than
    /// `stale_after` seconds. A description that cannot be read is skipped,
    /// not removed.
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
                if meta.format != 1 {
                    return None;
                }
                let lock = self.lock_path(&key);
                let running = held_elsewhere(&lock)
                    .unwrap_or_else(|| now.saturating_sub(meta.heartbeat) < stale_after);
                if running {
                    return None;
                }
                let (copy, meta_path) = self.paths(&key);
                copy.is_file().then_some(Entry {
                    original: meta.original,
                    name: meta.name,
                    saved_at: meta.saved_at,
                    copy,
                    meta: meta_path,
                    lock,
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
    /// The lock that tells other windows this one runs, taken with the
    /// first copy.
    lock: Option<File>,
    /// The project a panic writes.
    panic: PanicCopy,
}

impl Drop for Autosave {
    fn drop(&mut self) {
        if self.lock.take().is_some() {
            // Best effort: a leftover lock file is unlocked and harmless.
            let _ = fs::remove_file(self.store.lock_path(&self.key));
        }
    }
}

impl Autosave {
    /// The autosave of this window, writing at most once per `interval`
    /// seconds of `now` (the window's clock).
    #[must_use]
    pub fn new(store: RecoveryStore, interval: f64, now: f64) -> Self {
        let key = format!("window-{}-{}", std::process::id(), unix_now());
        let panic = PanicCopy {
            slot: Arc::default(),
            store: store.clone(),
            key: key.clone(),
        };
        Self {
            store,
            key,
            interval: interval.max(0.0),
            written: None,
            meta: None,
            last_write: now,
            failure: None,
            lock: None,
            panic,
        }
    }

    /// What a panic hook needs to write the project of this window.
    #[must_use]
    pub fn panic_copy(&self) -> PanicCopy {
        self.panic.clone()
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
        self.panic.keep(session);
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
        if self.lock.is_none() {
            // Without a lock, the heartbeat tells other windows instead.
            self.lock = hold(&self.store.lock_path(&self.key)).ok();
        }
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
        self.panic.forget();
    }
}

/// The project of a window with unsaved changes, kept where a panic hook can
/// write it: a panic in the window writes the recovery copy at once, so that
/// even changes newer than the last copy survive.
#[derive(Clone, Debug)]
pub struct PanicCopy {
    slot: Arc<Mutex<Option<Kept>>>,
    store: RecoveryStore,
    key: String,
}

/// A document kept for [`PanicCopy`].
#[derive(Debug)]
struct Kept {
    doc: Schematic,
    original: Option<PathBuf>,
    revision: u64,
}

impl PanicCopy {
    /// Keep the document of `session` unless this revision is kept already.
    fn keep(&self, session: &Session) {
        let revision = session.history().revision();
        let mut slot = self.slot.lock().unwrap_or_else(PoisonError::into_inner);
        if slot.as_ref().is_some_and(|kept| kept.revision == revision) {
            return;
        }
        *slot = Some(Kept {
            doc: session.document().clone(),
            original: session.path().map(Path::to_path_buf),
            revision,
        });
    }

    fn forget(&self) {
        *self.slot.lock().unwrap_or_else(PoisonError::into_inner) = None;
    }

    /// Write the kept document as a recovery copy whose window has stopped,
    /// so that the next window offers it at once. Nothing happens without a
    /// kept document, or while another thread holds it.
    pub fn write_now(&self) {
        let slot = match self.slot.try_lock() {
            Ok(slot) => slot,
            Err(std::sync::TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(std::sync::TryLockError::WouldBlock) => return,
        };
        let Some(kept) = slot.as_ref() else {
            return;
        };
        let Ok(text) = circuit_project::to_string(&kept.doc) else {
            return;
        };
        if self.store.ensure_dir().is_err() {
            return;
        }
        let (copy, meta_path) = self.store.paths(&self.key);
        if write_atomic(&copy, &text).is_err() {
            return;
        }
        let meta = Meta {
            format: 1,
            original: kept.original.clone(),
            name: kept.doc.project.name.clone(),
            saved_at: unix_now(),
            // The window is going away: no heartbeat to wait for.
            heartbeat: 0,
        };
        if let Ok(text) = serde_json::to_string_pretty(&meta) {
            let _ = write_atomic(&meta_path, text);
        }
    }

    /// Write the copy before the panic is reported, then report it as
    /// before. For `main`; the hook is global to the process.
    pub fn install_hook(self) {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            self.write_now();
            previous(info);
        }));
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
            [
                format!("{key}.json"),
                format!("{key}.lock"),
                format!("{key}.ocircuit")
            ]
        );
        assert!(autosave.is_current(Some(&session)));

        session.mark_saved();
        autosave.tick(32.0, Some(&session));
        assert_eq!(
            scratch.files(),
            [format!("{key}.lock")],
            "held while running"
        );
        drop(autosave);
        assert!(scratch.files().is_empty());
    }

    #[test]
    fn a_running_window_is_not_offered_and_a_stopped_one_is() {
        let scratch = Scratch::new();
        let mut running = Autosave::new(scratch.0.clone(), 0.0, 0.0);
        running.tick(1.0, Some(&edited()));
        let other = format!("{}-other", running.key());

        // Its lock says it runs, even with a heartbeat that looks stale.
        assert!(scratch.0.orphans(&other, 0).is_empty());

        // The process ends without closing: the system drops the lock and
        // the file stays. The copy is offered at once, heartbeat or not.
        drop(running.lock.take());
        let found = scratch.0.orphans(&other, 3600);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Recovered pump");
        assert_eq!(found[0].original, None);
        let recovered = Session::load(&found[0].read().unwrap(), None).unwrap();
        assert_eq!(recovered.document().project.name, "Recovered pump");

        assert!(scratch.0.orphans(running.key(), 0).is_empty(), "own copy");

        found[0].discard();
        assert!(scratch.files().is_empty(), "{:?}", scratch.files());
    }

    #[test]
    fn without_a_lock_file_the_heartbeat_decides() {
        let scratch = Scratch::new();
        let mut running = Autosave::new(scratch.0.clone(), 0.0, 0.0);
        running.tick(1.0, Some(&edited()));
        let other = format!("{}-other", running.key());
        drop(running.lock.take());
        fs::remove_file(scratch.0.lock_path(running.key())).unwrap();

        assert!(
            scratch.0.orphans(&other, 3600).is_empty(),
            "fresh heartbeat"
        );
        assert_eq!(scratch.0.orphans(&other, 0).len(), 1, "stale heartbeat");
    }

    #[test]
    fn a_panic_writes_the_newest_changes_and_they_are_offered_at_once() {
        let scratch = Scratch::new();
        // A long interval: no regular copy is written yet.
        let mut autosave = Autosave::new(scratch.0.clone(), 3600.0, 0.0);
        autosave.tick(1.0, Some(&edited()));
        assert!(scratch.files().is_empty());

        autosave.panic_copy().write_now();

        let other = format!("{}-other", autosave.key());
        let found = scratch.0.orphans(&other, 3600);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "Recovered pump");
    }

    #[test]
    fn a_panic_with_nothing_unsaved_writes_nothing() {
        let scratch = Scratch::new();
        let mut autosave = Autosave::new(scratch.0.clone(), 0.0, 0.0);
        let mut session = edited();
        autosave.tick(1.0, Some(&session));
        session.mark_saved();
        autosave.tick(2.0, Some(&session));

        autosave.panic_copy().write_now();

        let copies: Vec<String> = scratch
            .files()
            .into_iter()
            .filter(|name| !name.ends_with(".lock"))
            .collect();
        assert!(copies.is_empty(), "{copies:?}");
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
