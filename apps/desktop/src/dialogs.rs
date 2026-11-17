//! Native file dialogs, on the platforms that have them.
//!
//! In a browser (`wasm32`) there is no synchronous file dialog: the pickers
//! answer `None` and files reach the application by drag and drop, and
//! saving or exporting becomes a download (see `web`).

use std::path::PathBuf;

#[cfg(not(target_arch = "wasm32"))]
use crate::i18n::tr;

/// Extension of project files.
pub const PROJECT_EXTENSION: &str = "ocircuit";
/// Extension of library files.
pub const LIBRARY_EXTENSION: &str = "oclib";

/// Ask for a project to open. `None` when the user cancels or no file
/// dialog is available.
#[must_use]
pub fn pick_project() -> Option<PathBuf> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        rfd::FileDialog::new()
            .add_filter(tr("OpenCircuit project"), &[PROJECT_EXTENSION])
            .pick_file()
    }
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
}

/// Ask for a macro (an edit file, `.json`) to play. `None` when the user
/// cancels or no file dialog is available.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn pick_macro() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(tr("OpenCircuit macro (edit file)"), &["json"])
        .pick_file()
}

/// Ask where to save a macro. A browser downloads instead.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn choose_macro_path() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(tr("OpenCircuit macro (edit file)"), &["json"])
        .set_file_name("macro.json")
        .save_file()
}

/// Ask where to save a project named `name`. The extension is added when
/// the user leaves it out. A browser downloads instead.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn choose_save_path(name: &str) -> Option<PathBuf> {
    save_path(tr("OpenCircuit project"), name, PROJECT_EXTENSION)
}

/// Ask where to write a drawing in the format of `extension` (`svg` or
/// `pdf`). The extension is added when the user leaves it out. A browser
/// downloads instead.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn choose_export_path(name: &str, extension: &str) -> Option<PathBuf> {
    let filter = if extension == "pdf" {
        tr("PDF document")
    } else {
        tr("SVG drawing")
    };
    save_path(filter, name, extension)
}

/// Ask where to write the list of problems of a project named `name`. The
/// extension is added when the user leaves it out. A browser downloads
/// instead.
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn choose_problems_path(name: &str) -> Option<PathBuf> {
    save_path(
        tr("Comma-separated values"),
        &format!("{name}-problems"),
        "csv",
    )
}

/// Ask for library files to load; empty when the user cancels or no file
/// dialog is available.
#[must_use]
pub fn pick_libraries() -> Vec<PathBuf> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        rfd::FileDialog::new()
            .add_filter(tr("OpenCircuit library"), &[LIBRARY_EXTENSION])
            .pick_files()
            .unwrap_or_default()
    }
    #[cfg(target_arch = "wasm32")]
    {
        Vec::new()
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn save_path(filter: &str, name: &str, extension: &str) -> Option<PathBuf> {
    let mut path = rfd::FileDialog::new()
        .add_filter(filter, &[extension])
        .set_file_name(format!("{name}.{extension}"))
        .save_file()?;
    if path.extension().is_none() {
        path.set_extension(extension);
    }
    Some(path)
}
