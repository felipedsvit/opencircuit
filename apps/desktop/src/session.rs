//! One open project: the document, its compilation and its simulator.
//!
//! Nothing here knows about egui. The window shows what a session holds and
//! forwards operator actions to it; every electrical decision is made by the
//! core crates.

#![cfg_attr(not(test), deny(clippy::arithmetic_side_effects))]

use std::fmt;
use std::path::{Path, PathBuf};

use circuit_model::{ComponentCatalog, Diagnostic, Schematic, SimSnapshot};
use circuit_project::{MAX_PROJECT_BYTES, ProjectError};
use circuit_renderer::{Scene, SceneOptions, build_scene};
use circuit_sim::{SimConfig, Simulator, compile};

use crate::files::{FileError, read_text};

/// Why a project could not be opened.
#[derive(Debug)]
pub enum LoadError {
    /// The file could not be read.
    File(FileError),
    /// The text is not a valid project.
    Project {
        /// Where the text came from, for the message.
        origin: String,
        /// What is wrong with it.
        error: ProjectError,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(error) => error.fmt(f),
            Self::Project { origin, error } => write!(f, "{origin}: {error}"),
        }
    }
}

impl std::error::Error for LoadError {}

/// An open project.
#[derive(Debug)]
pub struct Session {
    path: Option<PathBuf>,
    doc: Schematic,
    catalog: ComponentCatalog,
    /// Compilation diagnostics, then configuration diagnostics.
    diagnostics: Vec<Diagnostic>,
    /// `None` when the project does not compile.
    simulator: Option<Simulator>,
    snapshot: Option<SimSnapshot>,
}

impl Session {
    /// Open a project file.
    ///
    /// # Errors
    ///
    /// Returns [`LoadError`] when the file cannot be read or is not a valid
    /// project. A project that loads but does not compile is not an error:
    /// it opens without a simulator and with its diagnostics.
    pub fn open(path: &Path) -> Result<Self, LoadError> {
        let text = read_text(path, MAX_PROJECT_BYTES).map_err(LoadError::File)?;
        Self::load(&text, Some(path.to_path_buf()))
    }

    /// Load a project from its text.
    ///
    /// # Errors
    ///
    /// Returns [`LoadError::Project`] when the text is not a valid project.
    pub fn load(text: &str, path: Option<PathBuf>) -> Result<Self, LoadError> {
        let doc = circuit_project::from_str(text).map_err(|error| LoadError::Project {
            origin: path
                .as_deref()
                .map_or_else(|| "project".to_owned(), |p| p.display().to_string()),
            error,
        })?;
        let catalog = ComponentCatalog::builtin();
        let output = compile(&doc, &catalog);
        let mut diagnostics = output.diagnostics;
        let simulator = match output
            .circuit
            .map(|c| Simulator::new(c, SimConfig::default()))
        {
            Some(Ok(simulator)) => {
                diagnostics.extend_from_slice(simulator.diagnostics());
                Some(simulator)
            }
            Some(Err(error)) => {
                diagnostics.push(error.diagnostic());
                None
            }
            None => None,
        };
        Ok(Self {
            path,
            doc,
            catalog,
            diagnostics,
            simulator,
            snapshot: None,
        })
    }

    /// The file the project came from, if any.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// The document.
    #[must_use]
    pub const fn document(&self) -> &Schematic {
        &self.doc
    }

    /// The component catalog the document was compiled against.
    #[must_use]
    pub const fn catalog(&self) -> &ComponentCatalog {
        &self.catalog
    }

    /// Compilation and configuration diagnostics, in canonical order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Whether the project compiled and can be simulated.
    #[must_use]
    pub const fn can_simulate(&self) -> bool {
        self.simulator.is_some()
    }

    /// The state of the last solved tick, or `None` before the first one.
    #[must_use]
    pub const fn snapshot(&self) -> Option<&SimSnapshot> {
        self.snapshot.as_ref()
    }

    /// The scene of the document in its current state.
    #[must_use]
    pub fn scene(&self, options: &SceneOptions) -> Scene {
        build_scene(&self.doc, &self.catalog, self.snapshot.as_ref(), options)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMAND: &str = include_str!("../../../examples/direct-starter/command.ocircuit");
    const SERIES_LOADS: &str =
        include_str!("../../../tests/fixtures/series-loads/series-loads.ocircuit");

    fn codes(session: &Session) -> Vec<&'static str> {
        session
            .diagnostics()
            .iter()
            .map(|d| d.code().as_str())
            .collect()
    }

    #[test]
    fn example_that_compiles_can_be_simulated() {
        let session = Session::load(COMMAND, None).unwrap();

        assert!(session.can_simulate());
    }

    #[test]
    fn example_that_compiles_has_no_diagnostic() {
        let session = Session::load(COMMAND, None).unwrap();

        assert_eq!(codes(&session), Vec::<&str>::new());
    }

    #[test]
    fn project_that_does_not_compile_opens_with_its_diagnostics() {
        let session = Session::load(SERIES_LOADS, None).unwrap();

        assert!(!session.can_simulate());
        assert_eq!(codes(&session), ["OC-MOD-001", "OC-SEM-006"]);
    }

    #[test]
    fn project_that_does_not_compile_is_still_drawn() {
        let session = Session::load(SERIES_LOADS, None).unwrap();

        assert!(!session.scene(&SceneOptions::default()).items.is_empty());
    }

    #[test]
    fn text_that_is_not_a_project_is_refused_with_its_origin() {
        let error = Session::load("{", Some(PathBuf::from("broken.ocircuit"))).unwrap_err();

        assert!(
            error.to_string().starts_with("broken.ocircuit: "),
            "{error}"
        );
    }

    #[test]
    fn nothing_is_solved_before_the_first_tick() {
        let session = Session::load(COMMAND, None).unwrap();

        assert_eq!(session.snapshot(), None);
    }
}
