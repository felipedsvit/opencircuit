//! `opencircuit diff`: what changed between two versions of a project.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

const STARTER: &str = "examples/direct-starter/starter.ocircuit";

/// The starter with K1 renamed K9, in a scratch file.
fn renamed() -> PathBuf {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join("diff");
    fs::create_dir_all(&directory).expect("scratch");
    let text = fs::read_to_string(root().join(STARTER)).expect("project");
    assert!(text.contains("\"reference\": \"K1\""));
    let path = directory.join("renamed.ocircuit");
    fs::write(
        &path,
        text.replace("\"reference\": \"K1\"", "\"reference\": \"K9\""),
    )
    .expect("write");
    path
}

#[test]
fn a_project_compared_with_itself_has_no_changes() {
    let output = opencircuit(&["diff", STARTER, STARTER]);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(stdout(&output), "no changes\n");
}

#[test]
fn a_project_saved_by_0_0_1_is_the_same_document_after_migration() {
    let output = opencircuit(&[
        "diff",
        "tests/compat/v0.0.1/direct-starter/starter.ocircuit",
        STARTER,
    ]);

    assert_eq!(output.status.code(), Some(0), "{}", stdout(&output));
}

#[test]
fn a_renamed_device_is_reported_and_the_exit_code_says_they_differ() {
    let after = renamed();
    let after = after.to_str().expect("UTF-8 path");

    let text = opencircuit(&["diff", STARTER, after]);
    assert_eq!(text.status.code(), Some(1));
    let report = stdout(&text);
    assert!(report.contains("changed"), "{report}");
    assert!(report.contains("K9"), "{report}");
    assert!(report.contains("reference"), "{report}");

    let json = opencircuit(&["diff", STARTER, after, "--format", "json"]);
    assert_eq!(json.status.code(), Some(1));
    let document: serde_json::Value = serde_json::from_str(&stdout(&json)).expect("JSON");
    assert_eq!(document["command"], "diff");
    assert_eq!(document["ok"], true);
    assert_eq!(document["same"], false);
    assert_eq!(document["changes"]["rows"][0][1], "device");
    assert_eq!(document["changes"]["rows"][0][4], "reference");

    let csv = opencircuit(&["diff", STARTER, after, "--format", "csv"]);
    assert!(stdout(&csv).starts_with("Change,Element,ID,Name,Fields"));
}

#[test]
fn diff_needs_two_files() {
    let output = opencircuit(&["diff", STARTER]);

    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn a_file_that_is_not_a_project_is_a_failure_that_names_it() {
    let output = opencircuit(&[
        "diff",
        STARTER,
        "examples/direct-starter/starter.scenario.json",
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("starter.scenario.json"));
}
