//! Compatibility with files written by released builds.
//!
//! `tests/compat/v0.0.1` holds the projects, libraries, scenarios, golden
//! traces and edit file of the `v0.0.1` release, frozen as that release
//! wrote them (schema version 2). A person who saved a project with a
//! released build must be able to open it, simulate it and get the same
//! trace; these tests fail when a change breaks that promise.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const V0_0_1: &str = "tests/compat/v0.0.1";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root exists")
}

fn opencircuit(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_opencircuit"))
        .args(args)
        .current_dir(root())
        .env_remove("CI")
        .output()
        .expect("the binary runs")
}

fn report(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Files below `directory` with the given suffix, sorted.
fn files_ending_with(directory: &Path, suffix: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(next) = pending.pop() {
        for entry in fs::read_dir(&next).expect("directory is readable") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else if path.to_string_lossy().ends_with(suffix) {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

/// Project files of the release and of the current examples.
fn every_project() -> Vec<PathBuf> {
    let mut projects = files_ending_with(&root().join(V0_0_1), ".ocircuit");
    projects.extend(files_ending_with(&root().join("examples"), ".ocircuit"));
    projects
}

#[test]
fn scenarios_of_0_0_1_reproduce_their_golden_traces() {
    let output = opencircuit(&["test", V0_0_1]);

    assert_eq!(output.status.code(), Some(0), "{}", report(&output));
    assert!(
        report(&output).ends_with("20 scenario(s), 0 failed\n"),
        "{}",
        report(&output)
    );
}

#[test]
fn projects_of_0_0_1_validate_without_errors() {
    let library = root().join(V0_0_1).join("library/acme.oclib");
    let projects = files_ending_with(&root().join(V0_0_1), ".ocircuit");
    assert_eq!(projects.len(), 10);

    for project in projects {
        let project = project.to_string_lossy().into_owned();
        let library = library.to_string_lossy().into_owned();
        let output = opencircuit(&["validate", &project, "--library", &library]);

        assert_eq!(
            output.status.code(),
            Some(0),
            "{project}: {}",
            report(&output)
        );
    }
}

#[test]
fn projects_of_0_0_1_migrate_to_one_stable_current_document() {
    for path in files_ending_with(&root().join(V0_0_1), ".ocircuit") {
        let text = fs::read_to_string(&path).expect("project is readable");
        assert!(text.contains("\"schema_version\": 2"), "{}", path.display());

        let migrated = circuit_project::from_str(&text).expect("an old project loads");
        let saved = circuit_project::to_string(&migrated).expect("it saves");
        let reloaded = circuit_project::from_str(&saved).expect("the saved text loads");

        assert!(
            saved.contains(&format!(
                "\"schema_version\": {}",
                circuit_model::CURRENT_SCHEMA_VERSION
            )),
            "{}",
            path.display()
        );
        assert_eq!(reloaded, migrated, "{}", path.display());
        assert_eq!(
            circuit_project::to_string(&reloaded).expect("it saves again"),
            saved,
            "{}: a second save changed the bytes",
            path.display()
        );
    }
}

#[test]
fn edit_file_of_0_0_1_still_builds_a_valid_project() {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("compat-edit-0.0.1");
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("scratch directory");
    let project = directory.join("sheets.ocircuit");
    let project = project.to_str().expect("UTF-8 path");
    let edit = format!("{V0_0_1}/edit/starter-sheets.edit.json");

    let created = opencircuit(&["new", project]);
    let edited = opencircuit(&["edit", project, "--apply", &edit]);
    let validated = opencircuit(&["validate", project]);

    assert_eq!(created.status.code(), Some(0), "{}", report(&created));
    assert_eq!(edited.status.code(), Some(0), "{}", report(&edited));
    assert_eq!(validated.status.code(), Some(0), "{}", report(&validated));
}

/// A save cut short (a full disk, a crash before the rename of an editor that
/// writes in place, a copy that stopped) must never load as a smaller but
/// valid document: every proper prefix of a project is refused, or is the
/// whole document without its final newline.
#[test]
fn a_truncated_project_is_refused_never_read_as_a_different_document() {
    for path in every_project() {
        let text = fs::read_to_string(&path).expect("project is readable");
        let whole = circuit_project::from_str(&text).expect("the whole project loads");

        // Every line boundary, and every 61st byte in between, keeps the run
        // short in a debug build while still cutting inside strings, numbers
        // and keys.
        let mut cuts: Vec<usize> = text.match_indices('\n').map(|(at, _)| at).collect();
        cuts.extend((0..text.len()).step_by(61));
        for cut in cuts {
            let Some(prefix) = text.get(..cut) else {
                continue;
            };
            if let Ok(loaded) = circuit_project::from_str(prefix) {
                assert_eq!(
                    loaded,
                    whole,
                    "{} cut at byte {cut} loads as a different document",
                    path.display()
                );
            }
        }
    }
}

#[test]
fn compatibility_files_contain_no_carriage_return() {
    // The same rule as the examples: goldens are compared byte for byte.
    let offenders: Vec<PathBuf> = files_ending_with(&root().join(V0_0_1), "")
        .into_iter()
        .filter(|path| fs::read(path).expect("file is readable").contains(&b'\r'))
        .collect();

    assert_eq!(offenders, Vec::<PathBuf>::new());
}
