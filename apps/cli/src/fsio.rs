//! File access of the command line: bounded reads and atomic writes.

use std::fs::{self, File};
use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use circuit_model::ComponentCatalog;
use circuit_model::library::MAX_LIBRARY_BYTES;

use crate::CliError;

/// Extension of library files found in a directory.
const LIBRARY_EXTENSION: &str = "oclib";
/// Most library files one directory may provide.
const MAX_LIBRARY_FILES: usize = 256;

/// Deepest directory nesting followed when looking for scenarios.
const MAX_DIRECTORY_DEPTH: u32 = 16;
/// Most scenario files one `test` run accepts.
const MAX_SCENARIO_FILES: usize = 10_000;

fn file_error(path: &Path, source: io::Error) -> CliError {
    CliError::File {
        path: path.to_path_buf(),
        source,
    }
}

/// The built-in catalog with the given libraries added in order. A
/// directory stands for its `*.oclib` files, sorted by name, without
/// descending into subdirectories.
pub(crate) fn load_catalog(paths: &[PathBuf]) -> Result<ComponentCatalog, CliError> {
    let mut files: Vec<PathBuf> = Vec::new();
    for path in paths {
        if path.is_dir() {
            let mut found: Vec<PathBuf> = fs::read_dir(path)
                .map_err(|source| file_error(path, source))?
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|file| {
                    file.is_file()
                        && file
                            .extension()
                            .is_some_and(|extension| extension == LIBRARY_EXTENSION)
                })
                .take(MAX_LIBRARY_FILES.saturating_add(1))
                .collect();
            if found.len() > MAX_LIBRARY_FILES {
                return Err(CliError::Usage(format!(
                    "{}: more than {MAX_LIBRARY_FILES} library files",
                    path.display()
                )));
            }
            found.sort();
            files.extend(found);
        } else {
            files.push(path.clone());
        }
    }
    let mut catalog = ComponentCatalog::builtin();
    for file in files {
        let text = read_text(&file, MAX_LIBRARY_BYTES)?;
        let library = circuit_project::library::from_str(&text)
            .map_err(|error| CliError::Usage(format!("{}: {error}", file.display())))?;
        catalog = catalog
            .with_library(library)
            .map_err(|error| CliError::Usage(format!("{}: {error}", file.display())))?;
    }
    Ok(catalog)
}

/// Read a text file, refusing one larger than `limit` bytes before reading
/// it into memory.
pub(crate) fn read_text(path: &Path, limit: usize) -> Result<String, CliError> {
    let file = File::open(path).map_err(|source| file_error(path, source))?;
    let mut text = String::new();
    // Read at most one byte past the limit: enough to know it was exceeded
    // without trusting the reported file size.
    let cap = u64::try_from(limit).unwrap_or(u64::MAX).saturating_add(1);
    file.take(cap)
        .read_to_string(&mut text)
        .map_err(|source| file_error(path, source))?;
    if text.len() > limit {
        return Err(file_error(
            path,
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("the file is larger than {limit} bytes"),
            ),
        ));
    }
    Ok(text)
}

/// Write a file atomically: write a temporary file in the same directory,
/// flush it to disk, then rename it over the destination. A failure leaves
/// the destination untouched and removes the temporary file.
pub(crate) fn write_atomic(path: &Path, contents: impl AsRef<[u8]>) -> Result<(), CliError> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let name = path
        .file_name()
        .ok_or_else(|| {
            file_error(
                path,
                io::Error::new(io::ErrorKind::InvalidInput, "the path has no file name"),
            )
        })?
        .to_string_lossy();
    let temporary = path.with_file_name(format!(
        ".{name}.tmp.{}.{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));

    let written = (|| -> io::Result<()> {
        let mut file = File::create(&temporary)?;
        file.write_all(contents.as_ref())?;
        file.sync_all()?;
        fs::rename(&temporary, path)
    })();
    written.map_err(|source| {
        // Best effort: the write already failed and that is what is reported.
        let _ = fs::remove_file(&temporary);
        file_error(path, source)
    })
}

/// Expand files and directories into the scenario files they contain, sorted
/// and without repetition. A file is taken as given; a directory is searched
/// recursively for `*.scenario.json`.
pub(crate) fn find_scenarios(paths: &[PathBuf]) -> Result<Vec<PathBuf>, CliError> {
    let mut found = Vec::new();
    for path in paths {
        let metadata = fs::metadata(path).map_err(|source| file_error(path, source))?;
        if metadata.is_dir() {
            collect_scenarios(path, MAX_DIRECTORY_DEPTH, &mut found)?;
        } else {
            found.push(path.clone());
        }
        if found.len() > MAX_SCENARIO_FILES {
            return Err(CliError::Usage(format!(
                "more than {MAX_SCENARIO_FILES} scenario files"
            )));
        }
    }
    found.sort();
    found.dedup();
    Ok(found)
}

fn collect_scenarios(
    directory: &Path,
    depth_left: u32,
    found: &mut Vec<PathBuf>,
) -> Result<(), CliError> {
    let Some(depth_left) = depth_left.checked_sub(1) else {
        return Err(CliError::Usage(format!(
            "{} is nested deeper than {MAX_DIRECTORY_DEPTH} directories",
            directory.display()
        )));
    };
    let entries = fs::read_dir(directory).map_err(|source| file_error(directory, source))?;
    for entry in entries {
        let path = entry
            .map_err(|source| file_error(directory, source))?
            .path();
        if path.is_dir() {
            collect_scenarios(&path, depth_left, found)?;
        } else if path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with(".scenario.json"))
        {
            found.push(path);
        }
        if found.len() > MAX_SCENARIO_FILES {
            return Err(CliError::Usage(format!(
                "more than {MAX_SCENARIO_FILES} scenario files"
            )));
        }
    }
    Ok(())
}
