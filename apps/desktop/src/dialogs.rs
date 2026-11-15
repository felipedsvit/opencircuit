//! Native file dialogs, on the platforms that have them.
//!
//! In a browser (`wasm32`) there is no synchronous file dialog: these
//! functions answer `None`, and files reach the application by drag and
//! drop instead, as bytes.

use std::path::PathBuf;

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
            .add_filter("OpenCircuit project", &[PROJECT_EXTENSION])
            .pick_file()
    }
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
}

/// Ask where to save a project named `name`. The extension is added when
/// the user leaves it out.
#[must_use]
pub fn choose_save_path(name: &str) -> Option<PathBuf> {
    save_path("OpenCircuit project", name, PROJECT_EXTENSION)
}

/// Ask where to write a drawing in the format of `extension` (`svg` or
/// `pdf`). The extension is added when the user leaves it out.
#[must_use]
pub fn choose_export_path(name: &str, extension: &str) -> Option<PathBuf> {
    let filter = if extension == "pdf" {
        "PDF document"
    } else {
        "SVG drawing"
    };
    save_path(filter, name, extension)
}

/// Ask for library files to load; empty when the user cancels or no file
/// dialog is available.
#[must_use]
pub fn pick_libraries() -> Vec<PathBuf> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        rfd::FileDialog::new()
            .add_filter("OpenCircuit library", &[LIBRARY_EXTENSION])
            .pick_files()
            .unwrap_or_default()
    }
    #[cfg(target_arch = "wasm32")]
    {
        Vec::new()
    }
}

#[cfg_attr(target_arch = "wasm32", allow(unused_variables))]
fn save_path(filter: &str, name: &str, extension: &str) -> Option<PathBuf> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let mut path = rfd::FileDialog::new()
            .add_filter(filter, &[extension])
            .set_file_name(format!("{name}.{extension}"))
            .save_file()?;
        if path.extension().is_none() {
            path.set_extension(extension);
        }
        Some(path)
    }
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
}
