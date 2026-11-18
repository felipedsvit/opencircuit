//! Recent projects (M9.2, issue #118): the files this user opened or saved
//! last, newest first, kept in the user's data folder.

#![cfg_attr(not(test), deny(clippy::arithmetic_side_effects))]

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::files::{read_text, write_atomic};
use crate::recovery::{private_dir, user_data_dir};

/// Most projects the list keeps.
pub const MAX_RECENT: usize = 10;
/// Largest list file read.
const MAX_LIST_BYTES: usize = 64 * 1024;

/// The list of recent projects, in a file.
#[derive(Clone, Debug)]
pub struct RecentProjects {
    file: PathBuf,
}

#[derive(Debug, Default, Deserialize, Serialize)]
struct List {
    format: u32,
    projects: Vec<PathBuf>,
}

impl RecentProjects {
    /// The list in the user's data folder.
    #[must_use]
    pub fn in_user_data() -> Option<Self> {
        Some(Self::at(user_data_dir()?.join("recent.json")))
    }

    /// The list in `file`, created with the first project.
    #[must_use]
    pub const fn at(file: PathBuf) -> Self {
        Self { file }
    }

    /// The projects, newest first. A list that cannot be read is empty.
    #[must_use]
    pub fn list(&self) -> Vec<PathBuf> {
        read_text(&self.file, MAX_LIST_BYTES)
            .ok()
            .and_then(|text| serde_json::from_str::<List>(&text).ok())
            .filter(|list| list.format == 1)
            .map(|list| {
                list.projects
                    .into_iter()
                    .filter(|path| path.is_absolute())
                    .take(MAX_RECENT)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Put `project` first, once, and keep the newest [`MAX_RECENT`].
    ///
    /// # Errors
    ///
    /// Returns the error of creating the folder or writing the list.
    pub fn record(&self, project: &Path) -> io::Result<()> {
        let project = std::fs::canonicalize(project).unwrap_or_else(|_| project.to_path_buf());
        let mut projects = self.list();
        projects.retain(|path| path != &project);
        projects.insert(0, project);
        projects.truncate(MAX_RECENT);
        self.write(projects)
    }

    /// Take `project` off the list.
    ///
    /// # Errors
    ///
    /// Returns the error of writing the list.
    pub fn forget(&self, project: &Path) -> io::Result<()> {
        let mut projects = self.list();
        let before = projects.len();
        projects.retain(|path| path != project);
        if projects.len() == before {
            return Ok(());
        }
        self.write(projects)
    }

    /// Empty the list.
    ///
    /// # Errors
    ///
    /// Returns the error of writing the list.
    pub fn clear(&self) -> io::Result<()> {
        self.write(Vec::new())
    }

    fn write(&self, projects: Vec<PathBuf>) -> io::Result<()> {
        if let Some(dir) = self.file.parent() {
            private_dir(dir)?;
        }
        let text = serde_json::to_string_pretty(&List {
            format: 1,
            projects,
        })
        .map_err(io::Error::other)?;
        write_atomic(&self.file, text).map_err(|error| error.source)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU32, Ordering};

    use super::*;

    struct Scratch(PathBuf);

    impl Scratch {
        fn new() -> Self {
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let dir = std::env::temp_dir().join(format!(
                "opencircuit-recent-test-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Self(fs::canonicalize(dir).unwrap())
        }

        fn project(&self, name: &str) -> PathBuf {
            let path = self.0.join(name);
            fs::write(&path, "{}").unwrap();
            path
        }

        fn recent(&self) -> RecentProjects {
            RecentProjects::at(self.0.join("data/recent.json"))
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_newest_project_comes_first_once() {
        let scratch = Scratch::new();
        let (a, b) = (scratch.project("a.ocircuit"), scratch.project("b.ocircuit"));
        let recent = scratch.recent();

        recent.record(&a).unwrap();
        recent.record(&b).unwrap();
        recent.record(&a).unwrap();

        assert_eq!(recent.list(), [a.clone(), b.clone()]);
        recent.forget(&a).unwrap();
        assert_eq!(recent.list(), [b]);
        recent.clear().unwrap();
        assert!(recent.list().is_empty());
    }

    #[test]
    fn only_the_newest_are_kept() {
        let scratch = Scratch::new();
        let recent = scratch.recent();
        for n in 0..(MAX_RECENT + 3) {
            recent
                .record(&scratch.project(&format!("{n}.ocircuit")))
                .unwrap();
        }

        let list = recent.list();
        assert_eq!(list.len(), MAX_RECENT);
        assert_eq!(
            list[0],
            scratch.0.join(format!("{}.ocircuit", MAX_RECENT + 2))
        );
    }

    #[test]
    fn a_damaged_or_missing_list_is_empty_and_is_replaced() {
        let scratch = Scratch::new();
        let recent = scratch.recent();
        assert!(recent.list().is_empty());
        fs::create_dir_all(scratch.0.join("data")).unwrap();
        fs::write(scratch.0.join("data/recent.json"), "not json").unwrap();
        assert!(recent.list().is_empty());

        let a = scratch.project("a.ocircuit");
        recent.record(&a).unwrap();
        assert_eq!(recent.list(), [a]);
    }

    #[test]
    fn relative_paths_in_the_file_are_ignored() {
        let scratch = Scratch::new();
        fs::create_dir_all(scratch.0.join("data")).unwrap();
        fs::write(
            scratch.0.join("data/recent.json"),
            r#"{ "format": 1, "projects": ["relative.ocircuit"] }"#,
        )
        .unwrap();

        assert!(scratch.recent().list().is_empty());
    }
}
