//! `opencircuit`: headless validation, simulation and golden tests.

#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

use std::io::Write as _;
use std::process::ExitCode;

use opencircuit_cli::{EXIT_USAGE, Env, run};

fn main() -> ExitCode {
    // `std::env::args` panics on an argument that is not UTF-8 (a file name
    // on Unix may be any bytes): read them as they are and refuse that one.
    let mut args: Vec<String> = Vec::new();
    for arg in std::env::args_os().skip(1) {
        match arg.into_string() {
            Ok(arg) => args.push(arg),
            Err(arg) => {
                // Nothing more can be done when standard error is closed.
                let _ = writeln!(
                    std::io::stderr(),
                    "error: the argument `{}` is not valid UTF-8; rename the file or pass another path",
                    arg.to_string_lossy()
                );
                return ExitCode::from(EXIT_USAGE);
            }
        }
    }
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
