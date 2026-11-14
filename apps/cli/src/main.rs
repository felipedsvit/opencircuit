//! `opencircuit`: headless validation, simulation and golden tests.

#![forbid(unsafe_code)]

use std::io::Write as _;
use std::process::ExitCode;

use opencircuit_cli::{Env, run};

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let env = Env {
        ci: std::env::var_os("CI").is_some(),
    };
    let mut stdout = std::io::stdout().lock();
    let mut stderr = std::io::stderr().lock();
    let code = run(&args, &env, &mut stdout, &mut stderr);
    // A closed pipe at exit is not something the tool can report anywhere.
    let _ = stdout.flush();
    ExitCode::from(code)
}
