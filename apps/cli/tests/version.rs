//! `opencircuit --version` prints the version of the program in the same words
//! as Help > About in the window (`OpenCircuit <version>`), so the two always
//! agree. Both read the version of the workspace package.

use std::process::{Command, Output};

fn opencircuit(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_opencircuit"))
        .args(args)
        .env_remove("CI")
        .output()
        .expect("the program runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn version_prints_the_same_text_as_about() {
    let output = opencircuit(&["--version"]);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        stdout(&output),
        format!("OpenCircuit {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn the_short_flag_prints_the_version_too() {
    let output = opencircuit(&["-V"]);

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        stdout(&output),
        format!("OpenCircuit {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn help_lists_the_version_flag() {
    let output = opencircuit(&["--help"]);

    assert_eq!(output.status.code(), Some(0));
    assert!(stdout(&output).contains("opencircuit --version"));
}
