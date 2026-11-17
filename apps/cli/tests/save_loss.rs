//! A command never replaces a file it reads with what it writes: a trace,
//! a snapshot or an edited project written over the project, the scenario,
//! the edit file or a library would lose the person's work.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root exists")
}

fn opencircuit(directory: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_opencircuit"))
        .args(args)
        .current_dir(directory)
        .env_remove("CI")
        .output()
        .expect("the binary runs")
}

/// A scratch copy of the direct-online starter, its scenario and the
/// library example.
fn scratch(name: &str) -> PathBuf {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("save-loss-{name}"));
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("scratch directory");
    for file in [
        "examples/direct-starter/starter.ocircuit",
        "examples/direct-starter/starter.scenario.json",
        "examples/direct-starter/starter.trace",
        "examples/library/acme.oclib",
        "examples/edit/starter-sheets.edit.json",
    ] {
        let source = root().join(file);
        let name = source.file_name().expect("file name");
        fs::copy(&source, directory.join(name)).expect("copy");
    }
    directory
}

/// The bytes of every file in `directory`, to compare before and after.
fn snapshot(directory: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<(String, Vec<u8>)> = fs::read_dir(directory)
        .expect("directory")
        .map(|entry| {
            let path = entry.expect("entry").path();
            (
                path.file_name()
                    .expect("name")
                    .to_string_lossy()
                    .into_owned(),
                fs::read(&path).expect("readable"),
            )
        })
        .collect();
    files.sort();
    files
}

fn refused(output: &Output) -> bool {
    output.status.code() == Some(2)
        && String::from_utf8_lossy(&output.stderr).contains("which this command reads")
}

const SIMULATE: [&str; 4] = [
    "simulate",
    "starter.ocircuit",
    "--scenario",
    "starter.scenario.json",
];

#[test]
fn a_trace_is_never_written_over_the_project_or_the_scenario() {
    for target in [
        "starter.ocircuit",
        "starter.scenario.json",
        "./starter.ocircuit",
    ] {
        let directory = scratch("trace");
        let before = snapshot(&directory);

        let mut args = SIMULATE.to_vec();
        args.extend(["--trace", target]);
        let output = opencircuit(&directory, &args);

        assert!(refused(&output), "{target}: {output:?}");
        assert_eq!(snapshot(&directory), before, "{target}");
    }
}

#[test]
fn a_snapshot_is_never_written_over_an_input_or_the_trace() {
    let directory = scratch("snapshot");
    let before = snapshot(&directory);

    for extra in [
        &["--snapshot-json", "starter.scenario.json"][..],
        &["--snapshot-json", "starter.ocircuit"],
        &["--trace", "out.trace", "--snapshot-json", "out.trace"],
        &["--library", "acme.oclib", "--snapshot-json", "acme.oclib"],
    ] {
        let mut args = SIMULATE.to_vec();
        args.extend(extra);
        let output = opencircuit(&directory, &args);

        assert!(refused(&output), "{extra:?}: {output:?}");
        assert_eq!(snapshot(&directory), before, "{extra:?}");
    }
}

#[cfg(unix)]
#[test]
fn another_name_of_the_project_is_recognized() {
    let directory = scratch("links");
    fs::hard_link(
        directory.join("starter.ocircuit"),
        directory.join("hard.json"),
    )
    .expect("link");
    std::os::unix::fs::symlink("starter.ocircuit", directory.join("soft.json")).expect("link");
    let before = snapshot(&directory);

    for target in ["hard.json", "soft.json"] {
        let mut args = SIMULATE.to_vec();
        args.extend(["--snapshot-json", target]);
        let output = opencircuit(&directory, &args);

        assert!(refused(&output), "{target}: {output:?}");
        assert_eq!(snapshot(&directory), before, "{target}");
    }
}

#[test]
fn separate_outputs_are_still_written() {
    let directory = scratch("separate");

    let mut args = SIMULATE.to_vec();
    args.extend(["--trace", "run.trace", "--snapshot-json", "run.json"]);
    let output = opencircuit(&directory, &args);

    assert_eq!(output.status.code(), Some(0), "{output:?}");
    assert_eq!(
        fs::read_to_string(directory.join("run.trace")).expect("trace"),
        fs::read_to_string(directory.join("starter.trace")).expect("golden")
    );
    assert!(directory.join("run.json").is_file());
}

#[test]
fn an_edit_never_writes_over_its_edit_file_or_a_library() {
    let directory = scratch("edit");
    let created = opencircuit(&directory, &["new", "fresh.ocircuit"]);
    assert_eq!(created.status.code(), Some(0), "{created:?}");
    let before = snapshot(&directory);

    for extra in [
        &["--output", "starter-sheets.edit.json"][..],
        &["--library", "acme.oclib", "--output", "acme.oclib"],
    ] {
        let mut args = vec![
            "edit",
            "fresh.ocircuit",
            "--apply",
            "starter-sheets.edit.json",
        ];
        args.extend(extra);
        let output = opencircuit(&directory, &args);

        assert!(refused(&output), "{extra:?}: {output:?}");
        assert_eq!(snapshot(&directory), before, "{extra:?}");
    }
}

#[test]
fn an_edit_still_writes_over_its_project_by_default() {
    let directory = scratch("edit-default");
    let created = opencircuit(&directory, &["new", "fresh.ocircuit"]);
    assert_eq!(created.status.code(), Some(0), "{created:?}");

    let output = opencircuit(
        &directory,
        &[
            "edit",
            "fresh.ocircuit",
            "--apply",
            "starter-sheets.edit.json",
            "--output",
            "fresh.ocircuit",
        ],
    );

    assert_eq!(output.status.code(), Some(0), "{output:?}");
}

#[test]
fn update_golden_never_writes_the_trace_over_the_project() {
    let directory = scratch("golden");
    let scenario = fs::read_to_string(directory.join("starter.scenario.json")).expect("scenario");
    fs::write(
        directory.join("starter.scenario.json"),
        scenario.replace("\"starter.trace\"", "\"starter.ocircuit\""),
    )
    .expect("tamper");
    let before = snapshot(&directory);

    let output = opencircuit(&directory, &["test", ".", "--update-golden"]);

    assert!(refused(&output), "{output:?}");
    assert_eq!(snapshot(&directory), before);
}
