//! Repository automation tasks.
//!
//! Run through Cargo so the workspace environment is configured:
//!
//! ```text
//! cargo run -p xtask -- check-architecture
//! cargo run -p xtask -- generate-examples [--check]
//! cargo run -p xtask --release -- bench
//! ```

#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]
#![cfg_attr(not(test), deny(clippy::indexing_slicing))]

mod architecture;
mod bench;
mod examples;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use serde_json::Value;

const USAGE: &str = "\
usage: cargo run -p xtask -- check-architecture
       cargo run -p xtask -- generate-examples [--check]
       cargo run -p xtask --release -- bench";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match (args.next().as_deref(), args.next()) {
        (Some("check-architecture"), None) => report("architecture check", check_architecture()),
        (Some("generate-examples"), None) => report("example generation", generate_examples(false)),
        (Some("generate-examples"), Some(flag)) if flag == "--check" => {
            report("example check", generate_examples(true))
        }
        (Some("bench"), None) => report("benchmark", bench::run()),
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// Print the outcome of a task. `Ok` carries the summary; `Err` the problems.
fn report(task: &str, outcome: Result<String, Vec<String>>) -> ExitCode {
    match outcome {
        Ok(summary) => {
            println!("{task} passed: {summary}");
            ExitCode::SUCCESS
        }
        Err(problems) => {
            eprintln!("{task} failed:");
            for problem in problems {
                eprintln!("  - {problem}");
            }
            ExitCode::FAILURE
        }
    }
}

fn check_architecture() -> Result<String, Vec<String>> {
    let metadata = cargo_metadata().map_err(|error| vec![error])?;
    let root = workspace_root(&metadata).map_err(|error| vec![error])?;

    let mut violations = architecture::dependency_violations(&metadata);

    let sim_modules = sim_modules(&root).map_err(|error| vec![error])?;
    violations.extend(architecture::module_violations(&sim_modules));

    let mut core_sources = Vec::new();
    for name in architecture::core_crates() {
        let directory = root.join("crates").join(name).join("src");
        collect_sources(&root, &directory, &mut core_sources).map_err(|error| vec![error])?;
    }
    violations.extend(architecture::forbidden_api_violations(&core_sources));

    if violations.is_empty() {
        Ok(format!(
            "{} circuit-sim modules and {} core source files checked",
            sim_modules.len(),
            core_sources.len()
        ))
    } else {
        Err(violations.into_iter().collect())
    }
}

/// Write every generated project file, or with `check_only` verify that the
/// files on disk are up to date without touching them.
fn generate_examples(check_only: bool) -> Result<String, Vec<String>> {
    let metadata = cargo_metadata().map_err(|error| vec![error])?;
    let root = workspace_root(&metadata).map_err(|error| vec![error])?;

    let mut problems = Vec::new();
    let mut written = 0_usize;
    let outputs = examples::outputs();
    let total = outputs.len();
    for output in outputs {
        let (relative, text) = match output {
            Ok(output) => output,
            Err(error) => {
                problems.push(error);
                continue;
            }
        };
        let path = root.join(&relative);
        let current = std::fs::read(&path).ok();
        if current.as_deref() == Some(text.as_slice()) {
            continue;
        }
        if check_only {
            problems.push(format!(
                "{relative} is stale; run `cargo run -p xtask -- generate-examples`"
            ));
            continue;
        }
        let result = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&path, &text));
        match result {
            Ok(()) => written = written.saturating_add(1),
            Err(error) => problems.push(format!("{relative}: {error}")),
        }
    }

    if problems.is_empty() {
        Ok(format!("{total} files, {written} written"))
    } else {
        Err(problems)
    }
}

fn cargo_metadata() -> Result<Value, String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let output = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--locked"])
        .output()
        .map_err(|error| format!("could not run `cargo metadata`: {error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("`cargo metadata` failed: {}", stderr.trim()));
    }

    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("could not parse `cargo metadata` output: {error}"))
}

fn workspace_root(metadata: &Value) -> Result<PathBuf, String> {
    metadata
        .get("workspace_root")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| "cargo metadata did not report the workspace root".to_owned())
}

/// Production modules of `circuit-sim` as `(module name, source)`. Test
/// support files are not subject to the import rules.
fn sim_modules(root: &Path) -> Result<Vec<(String, String)>, String> {
    let directory = root.join("crates").join("circuit-sim").join("src");
    let mut sources = Vec::new();
    collect_sources(root, &directory, &mut sources)?;
    let mut modules = Vec::new();
    for (path, source) in sources {
        let Some(module) = Path::new(&path).file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        if module == "lib" || module == "testkit" || module.ends_with("_tests") {
            continue;
        }
        modules.push((module.to_owned(), source));
    }
    Ok(modules)
}

/// Read every `.rs` file under `directory`, recursively, as
/// `(path relative to the workspace root, source)` in a stable order.
fn collect_sources(
    root: &Path,
    directory: &Path,
    out: &mut Vec<(String, String)>,
) -> Result<(), String> {
    collect_sources_bounded(root, directory, out, MAX_SOURCE_DEPTH)
}

/// Deepest directory nesting followed below a crate's `src`. A symbolic-link
/// cycle must end in an error, not in unbounded recursion.
const MAX_SOURCE_DEPTH: u32 = 16;

fn collect_sources_bounded(
    root: &Path,
    directory: &Path,
    out: &mut Vec<(String, String)>,
    depth_left: u32,
) -> Result<(), String> {
    let Some(depth_left) = depth_left.checked_sub(1) else {
        return Err(format!(
            "{} is nested deeper than {MAX_SOURCE_DEPTH} directories",
            directory.display()
        ));
    };
    let entries = std::fs::read_dir(directory)
        .map_err(|error| format!("could not read {}: {error}", directory.display()))?;
    let mut paths = BTreeSet::new();
    for entry in entries {
        let entry =
            entry.map_err(|error| format!("could not read {}: {error}", directory.display()))?;
        paths.insert(entry.path());
    }
    for path in paths {
        if path.is_dir() {
            collect_sources_bounded(root, &path, out, depth_left)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let source = std::fs::read_to_string(&path)
                .map_err(|error| format!("could not read {}: {error}", path.display()))?;
            let display = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((display, source));
        }
    }
    Ok(())
}
