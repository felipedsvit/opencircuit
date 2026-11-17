//! File access of the command line: bounded reads and atomic writes.

use std::fs::{self, File, OpenOptions};
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

/// The built-in catalog and the standard library, with the given libraries
/// added in order. A
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
    let mut catalog = circuit_project::stdlib::catalog();
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
    strip_bom(&mut text);
    Ok(text)
}

/// Drop a byte order mark at the start: some editors on Windows write one in
/// front of UTF-8 text, and JSON does not allow it. A file is never saved
/// with one.
pub(crate) fn strip_bom(text: &mut String) {
    if text.starts_with(BOM) {
        text.drain(..BOM.len_utf8());
    }
}

/// The byte order mark.
const BOM: char = '\u{feff}';

/// Write a file atomically: write a temporary file in the same directory,
/// flush it to disk, then rename it over the destination and flush the
/// directory. A failure leaves the destination untouched and removes the
/// temporary file. Through a link, the file it points to is replaced and the
/// link kept; a replaced file keeps its permissions and group.
pub(crate) fn write_atomic(path: &Path, contents: impl AsRef<[u8]>) -> Result<(), CliError> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    if path.file_name().is_none() {
        return Err(file_error(
            path,
            io::Error::new(io::ErrorKind::InvalidInput, "the path has no file name"),
        ));
    }
    // Saving through a link updates the file it points to and keeps the link.
    let target = link_target(path).map_err(|source| file_error(path, source))?;
    let temporary = temporary_next_to(&target, COUNTER.fetch_add(1, Ordering::Relaxed));
    replace_through(&temporary, &target, contents.as_ref())
        .map_err(|source| file_error(path, source))
}

/// Write `contents` to `temporary`, which must not exist yet, and rename it
/// over `path`.
fn replace_through(temporary: &Path, path: &Path, contents: &[u8]) -> io::Result<()> {
    // `create_new` refuses a file or a link already at the temporary name:
    // writing through it could change a file elsewhere, and it is not ours
    // to remove.
    let existing = fs::metadata(path).ok();
    let written = {
        let mut file = create_temporary(temporary, existing.is_some())?;
        file.write_all(contents)
            .and_then(|()| keep_access(&file, existing.as_ref()))
            .and_then(|()| file.sync_all())
    }
    // The file is closed here, before the rename.
    .and_then(|()| fs::rename(temporary, path));
    if let Err(error) = written {
        // Best effort: the write already failed and that is what is reported.
        let _ = fs::remove_file(temporary);
        return Err(error);
    }
    sync_parent(path);
    Ok(())
}

/// Most links followed to find the file a path names.
const MAX_LINKS: u32 = 16;

/// The file `path` names once its links are followed, at most [`MAX_LINKS`]
/// of them. A path that is not a link, or that does not exist, is returned
/// as it is; a link to a missing file gives that file's path.
fn link_target(path: &Path) -> io::Result<PathBuf> {
    let mut current = path.to_path_buf();
    for _ in 0..MAX_LINKS {
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                let next = fs::read_link(&current)?;
                current = match current.parent() {
                    Some(parent) if next.is_relative() => parent.join(next),
                    _ => next,
                };
            }
            _ => return Ok(current),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("more than {MAX_LINKS} links to follow"),
    ))
}

/// A name for the temporary file in the directory of `target`. It is short
/// and does not repeat the name of the target, which may already be close
/// to the 255-byte limit of most file systems.
fn temporary_next_to(target: &Path, count: u64) -> PathBuf {
    target.with_file_name(format!(".opencircuit-{}-{count}.tmp", std::process::id()))
}

/// Create the temporary file, refusing one that exists. When it replaces a
/// file, only its owner can read it until it takes the access of that file.
#[cfg(unix)]
fn create_temporary(temporary: &Path, replaces: bool) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt as _;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    if replaces {
        options.mode(0o600);
    }
    options.open(temporary)
}

#[cfg(not(unix))]
fn create_temporary(temporary: &Path, _replaces: bool) -> io::Result<File> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary)
}

/// Give the new file the permissions and the group of the file it
/// replaces, so that a save never opens a private project to others. When
/// this user cannot give the old group, the group gets no access rather
/// than the access meant for another group.
#[cfg(unix)]
fn keep_access(file: &File, replaced: Option<&fs::Metadata>) -> io::Result<()> {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
    let Some(replaced) = replaced else {
        return Ok(());
    };
    let mut mode = replaced.mode() & 0o777;
    if std::os::unix::fs::fchown(file, None, Some(replaced.gid())).is_err() {
        mode &= !0o070;
    }
    file.set_permissions(fs::Permissions::from_mode(mode))
}

/// Elsewhere a new file takes the access of its directory.
#[cfg(not(unix))]
fn keep_access(_file: &File, _replaced: Option<&fs::Metadata>) -> io::Result<()> {
    Ok(())
}

/// Flush the directory that holds `path`, so that the rename survives a
/// power loss and not only the contents of the file. Best effort: some file
/// systems refuse to sync a directory, and the file is complete either way.
#[cfg(unix)]
fn sync_parent(path: &Path) {
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    if let Ok(directory) = File::open(parent) {
        let _ = directory.sync_all();
    }
}

/// The standard library has no portable way to flush a directory elsewhere;
/// the rename is as durable as the file system makes it.
#[cfg(not(unix))]
fn sync_parent(_path: &Path) {}

/// Refuse an output file that is one of the files the command reads: writing
/// it would replace a project, a scenario, an edit file or a library with a
/// trace or a snapshot, and lose it. An input directory stands for the
/// library files in it. Other outputs of the same command are checked as
/// inputs too, so that two outputs never name one file.
pub(crate) fn refuse_input_as_output(
    flag: &str,
    output: &Path,
    inputs: &[&Path],
) -> Result<(), CliError> {
    let target = resolved(output);
    for input in inputs {
        let source = resolved(input);
        let clash = if source.is_dir() {
            target.parent() == Some(source.as_path())
                && target
                    .extension()
                    .is_some_and(|extension| extension == LIBRARY_EXTENSION)
        } else {
            target == source || same_file(&target, &source)
        };
        if clash {
            return Err(CliError::Usage(format!(
                "`{flag}` names {}, which this command reads or writes already; choose another file",
                output.display()
            )));
        }
    }
    Ok(())
}

/// The path with its links and `..` resolved as far as it exists: the whole
/// path for an existing file, else its directory joined with its name.
fn resolved(path: &Path) -> PathBuf {
    if let Ok(real) = fs::canonicalize(path) {
        return real;
    }
    let parent = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    match (fs::canonicalize(parent), path.file_name()) {
        (Ok(directory), Some(name)) => directory.join(name),
        _ => path.to_path_buf(),
    }
}

/// Two names of one file: a hard link has its own path but the same device
/// and inode.
#[cfg(unix)]
fn same_file(a: &Path, b: &Path) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    match (fs::metadata(a), fs::metadata(b)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    }
}

/// Without a portable file identity, only equal resolved paths match.
#[cfg(not(unix))]
fn same_file(_a: &Path, _b: &Path) -> bool {
    false
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

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh directory for one test, removed when dropped.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let directory = std::env::temp_dir()
                .join(format!("opencircuit-fsio-{name}-{}", std::process::id()));
            let _ = fs::remove_dir_all(&directory);
            fs::create_dir_all(&directory).unwrap();
            Self(directory)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                let _ = fs::set_permissions(&self.0, fs::Permissions::from_mode(0o755));
            }
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn entries(directory: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn atomic_write_replaces_the_whole_file_and_leaves_nothing_else() {
        let scratch = Scratch::new("replace");
        let path = scratch.0.join("a.ocircuit");
        fs::write(&path, "old contents that are longer").unwrap();

        write_atomic(&path, "new").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(entries(&scratch.0), ["a.ocircuit"]);
    }

    #[test]
    fn a_file_already_at_the_temporary_name_is_neither_overwritten_nor_removed() {
        let scratch = Scratch::new("taken");
        let path = scratch.0.join("a.ocircuit");
        let temporary = scratch.0.join(".a.ocircuit.tmp");
        fs::write(&path, "saved").unwrap();
        fs::write(&temporary, "someone else's").unwrap();

        let error = replace_through(&temporary, &path, b"new").unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&path).unwrap(), "saved");
        assert_eq!(fs::read_to_string(&temporary).unwrap(), "someone else's");
    }

    #[cfg(unix)]
    #[test]
    fn a_link_at_the_temporary_name_is_not_followed() {
        let scratch = Scratch::new("link");
        let path = scratch.0.join("a.ocircuit");
        let victim = scratch.0.join("victim.txt");
        let temporary = scratch.0.join(".a.ocircuit.tmp");
        fs::write(&path, "saved").unwrap();
        fs::write(&victim, "precious").unwrap();
        std::os::unix::fs::symlink(&victim, &temporary).unwrap();

        assert!(replace_through(&temporary, &path, b"new").is_err());

        assert_eq!(fs::read_to_string(&victim).unwrap(), "precious");
        assert_eq!(fs::read_to_string(&path).unwrap(), "saved");
    }

    #[test]
    fn a_directory_at_the_destination_is_refused_and_no_temporary_file_is_left() {
        let scratch = Scratch::new("directory");
        let path = scratch.0.join("a.ocircuit");
        fs::create_dir(&path).unwrap();

        assert!(write_atomic(&path, "new").is_err());

        assert!(path.is_dir());
        assert_eq!(entries(&scratch.0), ["a.ocircuit"]);
    }

    #[test]
    fn a_path_without_a_file_name_is_refused() {
        assert!(write_atomic(Path::new("/"), "x").is_err());
        assert!(write_atomic(Path::new(".."), "x").is_err());
    }

    /// Permission denied, as on a read-only share: the saved file stays as
    /// it was. Skipped where permissions are not enforced (a root user, as
    /// in a CI container run by `act`).
    #[cfg(unix)]
    #[test]
    fn a_read_only_directory_leaves_the_saved_file_as_it_was() {
        use std::os::unix::fs::PermissionsExt as _;

        let scratch = Scratch::new("read-only");
        let path = scratch.0.join("a.ocircuit");
        fs::write(&path, "saved").unwrap();
        fs::set_permissions(&scratch.0, fs::Permissions::from_mode(0o555)).unwrap();
        if File::create(scratch.0.join("probe")).is_ok() {
            return;
        }

        let error = write_atomic(&path, "new").unwrap_err();

        assert!(error.to_string().contains("a.ocircuit"), "{error}");
        assert_eq!(fs::read_to_string(&path).unwrap(), "saved");
        assert_eq!(entries(&scratch.0), ["a.ocircuit"]);
    }

    /// A failure after the temporary file was written (here the rename; a
    /// full disk fails `write_all` or `sync_all` and takes the same branch)
    /// removes the temporary file and leaves the saved file as it was.
    #[test]
    fn a_failure_after_writing_removes_the_temporary_file() {
        let scratch = Scratch::new("late-failure");
        let path = scratch.0.join("a.ocircuit");
        let temporary = scratch.0.join(".a.ocircuit.tmp");
        fs::write(&path, "saved").unwrap();
        let elsewhere = scratch.0.join("missing").join("a.ocircuit");

        assert!(replace_through(&temporary, &elsewhere, b"new").is_err());

        assert_eq!(entries(&scratch.0), ["a.ocircuit"]);
        assert_eq!(fs::read_to_string(&path).unwrap(), "saved");
    }

    #[test]
    fn a_byte_order_mark_is_dropped_and_only_at_the_start() {
        let scratch = Scratch::new("bom");
        let path = scratch.0.join("bom.json");
        fs::write(&path, "\u{feff}{\"a\": \"\u{feff}\"}").unwrap();

        assert_eq!(read_text(&path, 100).unwrap(), "{\"a\": \"\u{feff}\"}");
    }

    /// Unix only: Windows counts the whole path against its own limit.
    #[cfg(unix)]
    #[test]
    fn a_name_at_the_limit_of_the_file_system_can_be_saved() {
        let scratch = Scratch::new("long-name");
        // 255 bytes, the limit of a name on ext4 and APFS.
        let name = format!("{}.ocircuit", "a".repeat(255 - ".ocircuit".len()));
        let path = scratch.0.join(&name);

        write_atomic(&path, "first").unwrap();
        write_atomic(&path, "second").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
        assert_eq!(entries(&scratch.0), [name]);
    }

    #[cfg(unix)]
    mod unix {
        use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _, symlink};

        use super::*;

        fn mode(path: &Path) -> u32 {
            fs::metadata(path).unwrap().permissions().mode() & 0o777
        }

        #[test]
        fn a_saved_file_keeps_its_permissions() {
            let scratch = Scratch::new("modes");
            for wanted in [0o600, 0o640, 0o644, 0o400] {
                let path = scratch.0.join(format!("m{wanted:o}.ocircuit"));
                fs::write(&path, "old").unwrap();
                fs::set_permissions(&path, fs::Permissions::from_mode(wanted)).unwrap();

                write_atomic(&path, "new").unwrap();

                assert_eq!(fs::read_to_string(&path).unwrap(), "new");
                assert_eq!(mode(&path), wanted, "{wanted:o}");
            }
        }

        #[test]
        fn a_saved_file_keeps_its_group() {
            let scratch = Scratch::new("group");
            let path = scratch.0.join("g.ocircuit");
            fs::write(&path, "old").unwrap();
            let group = fs::metadata(&path).unwrap().gid();

            write_atomic(&path, "new").unwrap();

            assert_eq!(fs::metadata(&path).unwrap().gid(), group);
        }

        #[test]
        fn a_new_file_takes_the_usual_permissions() {
            let scratch = Scratch::new("fresh");
            let path = scratch.0.join("new.ocircuit");
            let reference = scratch.0.join("reference");
            File::create(&reference).unwrap();

            write_atomic(&path, "new").unwrap();

            assert_eq!(mode(&path), mode(&reference));
        }

        #[test]
        fn saving_through_a_link_updates_its_target_and_keeps_the_link() {
            let scratch = Scratch::new("through-link");
            let real = scratch.0.join("real.ocircuit");
            let link = scratch.0.join("link.ocircuit");
            fs::write(&real, "old").unwrap();
            fs::set_permissions(&real, fs::Permissions::from_mode(0o600)).unwrap();
            symlink("real.ocircuit", &link).unwrap();

            write_atomic(&link, "new").unwrap();

            assert!(
                fs::symlink_metadata(&link)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
            assert_eq!(fs::read_to_string(&real).unwrap(), "new");
            assert_eq!(mode(&real), 0o600);
            assert_eq!(entries(&scratch.0), ["link.ocircuit", "real.ocircuit"]);
        }

        #[test]
        fn a_chain_of_links_into_another_directory_is_followed() {
            let scratch = Scratch::new("link-chain");
            let elsewhere = scratch.0.join("elsewhere");
            fs::create_dir(&elsewhere).unwrap();
            fs::write(elsewhere.join("real.ocircuit"), "old").unwrap();
            symlink("elsewhere/real.ocircuit", scratch.0.join("one")).unwrap();
            symlink("one", scratch.0.join("two")).unwrap();

            write_atomic(&scratch.0.join("two"), "new").unwrap();

            assert_eq!(
                fs::read_to_string(elsewhere.join("real.ocircuit")).unwrap(),
                "new"
            );
            assert_eq!(entries(&elsewhere), ["real.ocircuit"]);
            assert_eq!(entries(&scratch.0), ["elsewhere", "one", "two"]);
        }

        #[test]
        fn a_link_to_a_missing_file_creates_that_file() {
            let scratch = Scratch::new("dangling");
            let link = scratch.0.join("link.ocircuit");
            symlink("later.ocircuit", &link).unwrap();

            write_atomic(&link, "new").unwrap();

            assert!(
                fs::symlink_metadata(&link)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
            assert_eq!(
                fs::read_to_string(scratch.0.join("later.ocircuit")).unwrap(),
                "new"
            );
        }

        #[test]
        fn a_loop_of_links_is_an_error_not_a_hang() {
            let scratch = Scratch::new("link-loop");
            symlink("b", scratch.0.join("a")).unwrap();
            symlink("a", scratch.0.join("b")).unwrap();

            let error = write_atomic(&scratch.0.join("a"), "new").unwrap_err();

            assert!(error.to_string().contains("links"), "{error}");
            assert_eq!(entries(&scratch.0), ["a", "b"]);
        }
    }

    #[test]
    fn read_text_refuses_a_file_over_the_limit_and_one_that_is_not_utf8() {
        let scratch = Scratch::new("read");
        let big = scratch.0.join("big");
        let binary = scratch.0.join("binary");
        fs::write(&big, "{ }").unwrap();
        fs::write(&binary, [0xff, 0xfe]).unwrap();

        assert_eq!(read_text(&big, 3).unwrap(), "{ }");
        assert!(read_text(&big, 2).is_err());
        assert!(read_text(&binary, 10).is_err());
    }
}
