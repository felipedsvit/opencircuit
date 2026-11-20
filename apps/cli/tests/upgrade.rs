//! `opencircuit --upgrade` takes no other argument and is listed in the usage
//! text. The check itself needs the network, so it is not run here.

use std::process::{Command, Output};

fn opencircuit(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_opencircuit"))
        .args(args)
        .env_remove("CI")
        .output()
        .expect("the program runs")
}

#[test]
fn upgrade_takes_no_other_argument() {
    let output = opencircuit(&["--upgrade", "extra"]);

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("`--upgrade` takes no other argument")
    );
}

#[test]
fn help_lists_the_upgrade_flag() {
    let output = opencircuit(&["--help"]);

    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("opencircuit --upgrade"));
}
