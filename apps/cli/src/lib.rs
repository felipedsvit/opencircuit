//! Headless command line for OpenCircuit.
//!
//! The tool only orchestrates: it reads files, calls the same model,
//! project and simulation APIs every other front end uses, and writes
//! results. It holds no electrical logic of its own.
//!
//! Exit codes: `0` success; `1` validation failure (an error diagnostic, or
//! a warning with `--deny-warnings`) or test failure (trace differs from the
//! golden) or a project that cannot be loaded for `export`; `2` usage or I/O
//! error.

#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]
#![cfg_attr(
    not(test),
    deny(clippy::indexing_slicing, clippy::arithmetic_side_effects)
)]

mod args;
mod commands;
/// The edit files of `edit --apply`, read by `circuit_commands::script`.
pub use circuit_commands::script as edit;
mod fsio;
mod mcp;
pub mod scenario;
mod serve;

use std::fmt;
use std::io::{self, Write};
use std::path::PathBuf;

pub use commands::{Overrides, ScenarioFailure, ScenarioRun, run_scenario};

/// Everything went well.
pub const EXIT_OK: u8 = 0;
/// The project has an error, or a golden test failed.
pub const EXIT_FAILED: u8 = 1;
/// The command line is wrong, or a file could not be read or written.
pub const EXIT_USAGE: u8 = 2;

const USAGE: &str = "\
OpenCircuit headless tool

usage:
  opencircuit validate <project.ocircuit> [--format text|json] [--deny-warnings]
                       [--library <file.oclib|dir>]
  opencircuit simulate <project.ocircuit> --scenario <scenario.json>
                       [--tick-us N] [--fault-policy halt|report]
                       [--trace <file>] [--snapshot-json <file>]
                       [--format text|json] [--library <file.oclib|dir>]
  opencircuit test <path>... [--update-golden] [--format text|json]
  opencircuit new <project.ocircuit> [--name <text>] [--force]
                  [--format text|json]
  opencircuit edit <project.ocircuit> --apply <edit.json> [--output <file>]
                   [--dry-run] [--format text|json]
                   [--library <file.oclib|dir>]
  opencircuit export <project.ocircuit> --output <file.svg|file.pdf>
                     [--theme print|live] [--sheet <number>|all]
                     [--format text|json] [--library <file.oclib|dir>]
  opencircuit export <project.ocircuit>... --output-dir <dir> --to svg|pdf
                     [--theme print|live] [--sheet <number>|all]
                     [--format text|json] [--library <file.oclib|dir>]
  opencircuit report <project.ocircuit> --kind bom|wires|terminals|strips|xref|io|problems|devices|all
                     [--format text|json|csv] [--output <file.csv|json|txt|pdf>]
                     [--library <file.oclib|dir>]
  opencircuit diff <before.ocircuit> <after.ocircuit> [--format text|json|csv]
  opencircuit mcp [--root <directory>]
  opencircuit serve <project.ocircuit> [--listen <address:port>]
                    [--print-map [--format text|json]]
                    [--library <file.oclib|dir>] [--duration-ms N]

A directory given to --library stands for its *.oclib files. Scenarios name
their libraries in `libraries` (scenario schema version 2).

diff lists what changed between two versions of a project: elements added,
removed or changed, matched by their stable IDs, with the fields that differ.
It exits with 0 when they are the same and 1 when they differ.

report derives the tables of a project (bill of materials, wires, terminals,
cross-reference exceptions, I/O map, problems). Without --output the tables
go to standard output in --format; with --output the extension of the file
picks the format and --format (text or json) says how the confirmation is
written. A CSV holds one table; a PDF sets every table on A4 landscape
pages.

mcp is a Model Context Protocol server on standard input and output (JSON-RPC,
one message per line). Its tools run the commands new, validate, simulate,
edit, report and export on files inside the root directory (the current one
unless --root says otherwise); no path may leave it.

serve answers Modbus TCP on 127.0.0.1:5020 unless --listen names another
address. Modbus has no authentication. OpenCircuit is a simulator, not a
controller: never connect it to a real plant.

With --format json a command writes one JSON document on stdout, with
format 1, the command and ok; a failure with exit code 1 is a document with
ok false and an error, and an exit code 2 still writes on stderr.

exit codes: 0 success, 1 validation or test failure, 2 usage or I/O error
";

/// The parts of the process environment the tool reads.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Env {
    /// Whether the `CI` variable is set. Goldens are never rewritten in CI.
    pub ci: bool,
}

/// A problem that ends the command with [`EXIT_USAGE`].
#[derive(Debug)]
pub(crate) enum CliError {
    /// The command line or a scenario is wrong.
    Usage(String),
    /// A file could not be read or written.
    File { path: PathBuf, source: io::Error },
    /// Standard output or standard error could not be written.
    Output(io::Error),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(message) => f.write_str(message),
            Self::File { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Output(source) => write!(f, "could not write output: {source}"),
        }
    }
}

impl From<io::Error> for CliError {
    fn from(source: io::Error) -> Self {
        Self::Output(source)
    }
}

/// Run the tool with the given arguments (without the program name) and
/// return the process exit code.
pub fn run(args: &[String], env: &Env, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let outcome = match args::parse(args) {
        Ok(args::Command::Help) => out
            .write_all(USAGE.as_bytes())
            .map(|()| EXIT_OK)
            .map_err(CliError::from),
        Ok(command) => commands::execute(&command, env, out, err),
        Err(message) => Err(CliError::Usage(format!("{message}\n\n{USAGE}"))),
    };
    match outcome {
        Ok(code) => code,
        Err(error) => {
            // If standard error is gone there is nowhere left to report to.
            let _ = writeln!(err, "error: {error}");
            EXIT_USAGE
        }
    }
}
