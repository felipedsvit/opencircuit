//! `opencircuit export <project>... --output-dir <dir> --to svg|pdf`: several
//! projects in one command (M9.2, #118).

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

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("export-many")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    dir
}

const STARTER: &str = "examples/direct-starter/starter.ocircuit";
const SHEETS: &str = "examples/sheets/starter-sheets.ocircuit";

#[test]
fn several_projects_go_into_one_folder_named_after_their_files() {
    let dir = scratch("pdf");
    let output = opencircuit(&[
        "export",
        STARTER,
        SHEETS,
        "--output-dir",
        dir.to_str().expect("utf-8"),
        "--to",
        "pdf",
        "--format",
        "json",
    ]);

    assert_eq!(
        output.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let document: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("one JSON document");
    assert_eq!(document["command"], "export");
    assert_eq!(document["ok"], true);
    assert_eq!(document["kind"], "pdf");
    let files = document["files"].as_array().expect("files");
    assert_eq!(files.len(), 2);
    assert_eq!(files[1]["pages"], 2, "one page per sheet");
    for name in ["starter.pdf", "starter-sheets.pdf"] {
        let bytes = fs::read(dir.join(name)).expect(name);
        assert!(bytes.starts_with(b"%PDF"), "{name}");
    }
}

#[test]
fn one_project_that_fails_is_reported_and_the_others_are_written() {
    let dir = scratch("partial");
    let output = opencircuit(&[
        "export",
        "examples/direct-starter/starter.scenario.json",
        STARTER,
        "--output-dir",
        dir.to_str().expect("utf-8"),
        "--to",
        "svg",
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("starter.scenario.json"));
    assert!(dir.join("starter.svg").is_file());
}

#[test]
fn two_projects_with_one_name_are_refused_before_anything_is_written() {
    let dir = scratch("clash");
    let output = opencircuit(&[
        "export",
        STARTER,
        "tests/compat/v0.0.1/direct-starter/starter.ocircuit",
        "--output-dir",
        dir.to_str().expect("utf-8"),
        "--to",
        "pdf",
    ]);

    assert_eq!(output.status.code(), Some(1));
    assert!(!dir.exists());
}

#[test]
fn the_options_of_a_folder_and_of_a_file_do_not_mix() {
    for args in [
        &["export", STARTER, "--output-dir", "x"][..],
        &["export", STARTER, "--output-dir", "x", "--to", "png"][..],
        &[
            "export",
            STARTER,
            "--output",
            "a.pdf",
            "--output-dir",
            "x",
            "--to",
            "pdf",
        ][..],
        &["export", STARTER, "--output", "a.pdf", "--to", "pdf"][..],
        &["export", STARTER, SHEETS, "--output", "a.pdf"][..],
    ] {
        assert_eq!(opencircuit(args).status.code(), Some(2), "{args:?}");
    }
}
