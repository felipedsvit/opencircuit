//! A project open in two windows (M9.1, issue #118).
//!
//! A window holds an operating-system lock, in the user's data folder, named
//! after the path of its project; a second window that opens the same
//! project finds it held and says that saving in both overwrites each
//! other's changes. The system releases the lock however the process ends,
//! so a crashed window never leaves a stale lock. Nothing is written next to
//! the project. Same user and machine only: two machines on one network
//! share do not see each other's locks.

#![cfg_attr(not(test), deny(clippy::arithmetic_side_effects))]

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use crate::recovery::{hold, private_dir, user_data_dir};

/// The folder of the locks of this user.
#[derive(Clone, Debug)]
pub struct ProjectLocks {
    dir: PathBuf,
}

/// The lock of one project, released when dropped.
#[derive(Debug)]
pub struct ProjectLock {
    project: PathBuf,
    path: PathBuf,
    file: Option<File>,
    elsewhere: bool,
}

/// FNV-1a: a stable 64-bit name for a path, the same in every process.
pub(crate) fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

impl ProjectLocks {
    /// The `locks` folder of the user's data folder.
    #[must_use]
    pub fn in_user_data() -> Option<Self> {
        Some(Self::at(user_data_dir()?.join("locks")))
    }

    /// Locks in `dir`, created when the first project is locked.
    #[must_use]
    pub const fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Lock `project` for this window. When another window holds it, the
    /// result says so and holds nothing; when locks are not available here,
    /// it holds nothing and says nothing.
    #[must_use]
    pub fn take(&self, project: &Path) -> ProjectLock {
        // One name for every way of writing the path: links and `..`
        // resolved, as far as the file exists.
        let real = fs::canonicalize(project).unwrap_or_else(|_| project.to_path_buf());
        let name = format!("{:016x}.lock", fnv1a(real.to_string_lossy().as_bytes()));
        let path = self.dir.join(name);
        let mut lock = ProjectLock {
            project: project.to_path_buf(),
            path,
            file: None,
            elsewhere: false,
        };
        if private_dir(&self.dir).is_err() {
            return lock;
        }
        match hold(&lock.path) {
            Ok(file) => lock.file = Some(file),
            Err(error) => lock.elsewhere = error.kind() == io::ErrorKind::WouldBlock,
        }
        lock
    }
}

impl ProjectLock {
    /// The project this lock is for, as the window named it.
    #[must_use]
    pub fn project(&self) -> &Path {
        &self.project
    }

    /// Whether another window holds this project.
    #[must_use]
    pub const fn open_elsewhere(&self) -> bool {
        self.elsewhere
    }
}

impl Drop for ProjectLock {
    fn drop(&mut self) {
        if self.file.take().is_some() {
            // Best effort: a leftover file is unlocked and taken again.
            let _ = fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!(
                "opencircuit-locks-test-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            fs::write(dir.join("a.ocircuit"), "{}").unwrap();
            Self(dir)
        }

        fn locks(&self) -> ProjectLocks {
            ProjectLocks::at(self.0.join("locks"))
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn a_second_window_finds_the_project_held_until_the_first_lets_go() {
        let scratch = Scratch::new();
        let project = scratch.0.join("a.ocircuit");

        let first = scratch.locks().take(&project);
        let second = scratch.locks().take(&project);
        assert!(!first.open_elsewhere());
        assert!(second.open_elsewhere());

        drop(first);
        assert!(!scratch.locks().take(&project).open_elsewhere());
    }

    #[test]
    fn another_spelling_of_the_path_is_the_same_project() {
        let scratch = Scratch::new();
        let _held = scratch.locks().take(&scratch.0.join("a.ocircuit"));

        let detour = scratch.0.join("locks").join("..").join("a.ocircuit");

        assert!(scratch.locks().take(&detour).open_elsewhere());
    }

    #[test]
    fn different_projects_do_not_block_each_other() {
        let scratch = Scratch::new();
        fs::write(scratch.0.join("b.ocircuit"), "{}").unwrap();
        let _a = scratch.locks().take(&scratch.0.join("a.ocircuit"));

        assert!(
            !scratch
                .locks()
                .take(&scratch.0.join("b.ocircuit"))
                .open_elsewhere()
        );
    }

    #[test]
    fn a_folder_that_cannot_be_made_holds_nothing_and_warns_of_nothing() {
        let scratch = Scratch::new();
        fs::write(scratch.0.join("locks"), "in the way").unwrap();

        let lock = scratch.locks().take(&scratch.0.join("a.ocircuit"));

        assert!(!lock.open_elsewhere());
    }

    #[test]
    fn the_name_of_a_path_is_stable() {
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    }
}
