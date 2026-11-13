//! Repository automation tasks.
//!
//! Run through Cargo so the workspace environment is configured:
//!
//! ```text
//! cargo run -p xtask -- check-architecture
//! ```

use std::collections::BTreeSet;
use std::process::{Command, ExitCode};

use serde_json::Value;

const SIMULATION_CORE: &[&str] = &["circuit-electrical", "circuit-logic", "circuit-simulation"];
const RUNTIME_FORBIDDEN: &[&str] = &[
    "circuit-commands",
    "circuit-editor",
    "circuit-project",
    "circuit-renderer",
];
const UI_ADAPTERS: &[&str] = &["circuit-editor", "circuit-renderer"];
const SIMULATION_FORBIDDEN: &[&str] = &["circuit-simulation"];
const GUI_DEPENDENCIES: &[&str] = &["eframe", "egui"];

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match (args.next().as_deref(), args.next()) {
        (Some("check-architecture"), None) => check_architecture(),
        _ => {
            eprintln!("usage: cargo run -p xtask -- check-architecture");
            ExitCode::from(2)
        }
    }
}

fn check_architecture() -> ExitCode {
    let metadata = match cargo_metadata() {
        Ok(metadata) => metadata,
        Err(error) => {
            eprintln!("architecture check: {error}");
            return ExitCode::FAILURE;
        }
    };

    let Some(packages) = metadata["packages"].as_array() else {
        eprintln!("architecture check: cargo metadata did not return a package list");
        return ExitCode::FAILURE;
    };

    let mut violations = BTreeSet::new();

    for package in packages {
        let Some(name) = package["name"].as_str() else {
            continue;
        };
        let dependencies = dependency_names(package);

        for dependency in &dependencies {
            let dependency = *dependency;

            if name == "circuit-domain" && dependency.starts_with("circuit-") {
                violations.insert(format!(
                    "circuit-domain must not depend on workspace crate `{dependency}`"
                ));
            }

            if SIMULATION_CORE.contains(&name) && RUNTIME_FORBIDDEN.contains(&dependency) {
                violations.insert(format!(
                    "`{name}` must not depend on `{dependency}`; simulation and logic stay headless"
                ));
            }

            if UI_ADAPTERS.contains(&name) && SIMULATION_FORBIDDEN.contains(&dependency) {
                violations.insert(format!(
                    "`{name}` must not depend on `{dependency}`; consume neutral snapshot contracts instead"
                ));
            }

            if name.starts_with("circuit-") && GUI_DEPENDENCIES.contains(&dependency) {
                violations.insert(format!(
                    "`{name}` must not depend on `{dependency}`; GUI code belongs in apps/desktop"
                ));
            }
        }
    }

    if violations.is_empty() {
        println!(
            "architecture check passed for {} workspace package(s)",
            packages.len()
        );
        ExitCode::SUCCESS
    } else {
        eprintln!("architecture check failed:");
        for violation in violations {
            eprintln!("  - {violation}");
        }
        ExitCode::FAILURE
    }
}

fn cargo_metadata() -> Result<Value, String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let output = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .output()
        .map_err(|error| format!("could not run `cargo metadata`: {error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("`cargo metadata` failed: {}", stderr.trim()));
    }

    serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("could not parse `cargo metadata` output: {error}"))
}

fn dependency_names(package: &Value) -> BTreeSet<&str> {
    package["dependencies"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|dependency| dependency["name"].as_str())
        .collect()
}
