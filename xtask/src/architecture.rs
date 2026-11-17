//! Architecture rules, checked from `cargo metadata` and from the sources.
//!
//! The rules are allowlists and fail closed: a workspace package without a
//! rule, a module of `circuit-sim` without a rule, or a dependency that is not
//! listed is a violation. A check that silently passes when a crate is renamed
//! protects nothing.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

/// What one workspace package may depend on.
struct PackageRule {
    name: &'static str,
    /// Workspace crates it may depend on.
    workspace: &'static [&'static str],
    /// Registry crates it may depend on directly.
    external: &'static [&'static str],
    /// Additional crates allowed as dev-dependencies only.
    dev: &'static [&'static str],
    /// Core crates must stay free of GUI, CLI and OS dependencies, directly
    /// and transitively.
    core: bool,
}

const PACKAGE_RULES: &[PackageRule] = &[
    PackageRule {
        name: "circuit-model",
        workspace: &[],
        external: &["serde", "thiserror"],
        dev: &[],
        core: true,
    },
    PackageRule {
        name: "circuit-sim",
        workspace: &["circuit-model"],
        external: &["thiserror"],
        dev: &["proptest"],
        core: true,
    },
    PackageRule {
        name: "circuit-project",
        workspace: &["circuit-model"],
        external: &["serde", "serde_json", "thiserror"],
        dev: &["proptest"],
        core: true,
    },
    // Edits are pure document transformations: no simulator, no renderer,
    // no I/O.
    PackageRule {
        name: "circuit-commands",
        workspace: &["circuit-model"],
        external: &["thiserror"],
        dev: &["proptest"],
        core: true,
    },
    // Editor logic works on documents and transactions only; the simulator
    // may be used by its tests, never by the editor itself.
    PackageRule {
        name: "circuit-editor",
        workspace: &["circuit-commands", "circuit-model"],
        external: &[],
        dev: &["circuit-sim", "proptest"],
        core: true,
    },
    // The renderer sees the simulation only through `SimSnapshot`, a type of
    // `circuit-model`; depending on `circuit-sim` would let drawing code
    // reach compiled indices and runtime state.
    PackageRule {
        name: "circuit-renderer",
        workspace: &["circuit-model"],
        external: &[],
        dev: &["proptest"],
        core: true,
    },
    // Exporters write the neutral scene; like the renderer they never see
    // the simulator. SVG is written by hand; PDF objects by `pdf-writer`
    // (ADR-0006), a small writer with no font or layout code, and the font
    // that the PDF embeds is cut down to the glyphs it uses by `subsetter`
    // (ADR-0012).
    PackageRule {
        name: "circuit-export",
        workspace: &["circuit-model", "circuit-renderer"],
        external: &["pdf-writer", "subsetter"],
        dev: &["proptest"],
        core: true,
    },
    // Industrial I/O (ADR-0009): the Modbus codec and the I/O map, pure.
    // Sockets and clocks stay in the command line, so the crate is core and
    // must not reach `std::net`.
    PackageRule {
        name: "circuit-io",
        workspace: &["circuit-model"],
        external: &[],
        dev: &["proptest"],
        core: true,
    },
    // Reports (ADR-0014) are tables derived from a document and its catalog,
    // plus the I/O map; they write nothing and never see the simulator.
    PackageRule {
        name: "circuit-report",
        workspace: &["circuit-io", "circuit-model"],
        external: &[],
        dev: &["proptest"],
        core: true,
    },
    // The only package with egui/eframe, which it reaches through the eframe
    // re-export; the GUI never enters the core.
    PackageRule {
        name: "opencircuit-desktop",
        workspace: &[
            "circuit-commands",
            "circuit-editor",
            "circuit-export",
            "circuit-model",
            "circuit-project",
            "circuit-renderer",
            "circuit-sim",
        ],
        // `rfd` gives the native file dialogs (portal over libdbus on Linux).
        // On wasm32, `web-sys`, `js-sys` and `wasm-bindgen-futures` start the
        // application on its canvas, read dropped files and offer downloads.
        external: &["eframe", "js-sys", "rfd", "wasm-bindgen-futures", "web-sys"],
        dev: &["egui_kittest"],
        core: false,
    },
    PackageRule {
        name: "opencircuit-cli",
        workspace: &[
            "circuit-commands",
            "circuit-export",
            "circuit-io",
            "circuit-model",
            "circuit-project",
            "circuit-renderer",
            "circuit-report",
            "circuit-sim",
        ],
        external: &["serde", "serde_json"],
        dev: &["proptest"],
        core: false,
    },
    PackageRule {
        name: "xtask",
        workspace: &[
            "circuit-commands",
            "circuit-export",
            "circuit-model",
            "circuit-project",
            "circuit-renderer",
            "circuit-sim",
        ],
        external: &["serde_json"],
        dev: &["proptest"],
        core: false,
    },
];

/// Crates a core crate must never reach, even through another dependency.
const FORBIDDEN_IN_CORE: &[&str] = &["anyhow", "clap", "eframe", "egui", "winit"];

/// Which modules of `circuit-sim` a module may import with `crate::`.
///
/// The solver must not know signals or the runtime, the logic must not know
/// the solver, and nothing may depend on the runtime.
const SIM_MODULE_RULES: &[(&str, &[&str])] = &[
    ("bitset", &[]),
    ("union_find", &[]),
    ("fingerprint", &[]),
    ("logic", &[]),
    ("compiled", &["fingerprint"]),
    ("compile", &["compiled", "fingerprint", "union_find"]),
    ("solve", &["bitset", "compiled", "union_find"]),
    ("trace", &["compiled", "fingerprint"]),
    (
        "runtime",
        &[
            "bitset",
            "compiled",
            "fingerprint",
            "logic",
            "solve",
            "trace",
        ],
    ),
];

/// Standard-library paths the core must not touch: no filesystem, sockets,
/// clock, threads, processes or environment.
const FORBIDDEN_CORE_APIS: &[&str] = &[
    "std::env",
    "std::fs",
    "std::net",
    "std::process",
    "std::thread",
    "std::time::Instant",
    "std::time::SystemTime",
];

/// Names of the core crates, whose sources are scanned.
pub(crate) fn core_crates() -> impl Iterator<Item = &'static str> {
    PACKAGE_RULES
        .iter()
        .filter(|rule| rule.core)
        .map(|rule| rule.name)
}

fn text<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn list<'a>(value: &'a Value, key: &str) -> &'a [Value] {
    value
        .get(key)
        .and_then(Value::as_array)
        .map_or(&[], Vec::as_slice)
}

/// Check direct and transitive dependencies of every workspace package
/// against the allowlists. `metadata` is the output of `cargo metadata` with
/// the resolved dependency graph.
pub(crate) fn dependency_violations(metadata: &Value) -> BTreeSet<String> {
    let mut violations = BTreeSet::new();

    let packages: BTreeMap<&str, &Value> = list(metadata, "packages")
        .iter()
        .filter_map(|package| text(package, "id").map(|id| (id, package)))
        .collect();
    let name_of = |id: &str| packages.get(id).and_then(|package| text(package, "name"));

    let members: Vec<&str> = list(metadata, "workspace_members")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    if members.is_empty() {
        violations.insert("cargo metadata lists no workspace member".to_owned());
    }

    // Normal and build edges of the resolved graph, by package id.
    let mut edges: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let nodes = metadata
        .get("resolve")
        .map_or(&[][..], |r| list(r, "nodes"));
    for node in nodes {
        let Some(id) = text(node, "id") else {
            continue;
        };
        let reachable = edges.entry(id).or_default();
        for dependency in list(node, "deps") {
            let is_dev_only = {
                let kinds = list(dependency, "dep_kinds");
                !kinds.is_empty() && kinds.iter().all(|kind| text(kind, "kind") == Some("dev"))
            };
            if let (false, Some(target)) = (is_dev_only, text(dependency, "pkg")) {
                reachable.push(target);
            }
        }
    }

    for member in members {
        let Some(package) = packages.get(member) else {
            violations.insert(format!("workspace member `{member}` has no package entry"));
            continue;
        };
        let Some(name) = text(package, "name") else {
            continue;
        };
        let Some(rule) = PACKAGE_RULES.iter().find(|rule| rule.name == name) else {
            violations.insert(format!(
                "workspace package `{name}` has no architecture rule; add one to xtask before adding the crate"
            ));
            continue;
        };

        for dependency in list(package, "dependencies") {
            let Some(dependency_name) = text(dependency, "name") else {
                continue;
            };
            let is_dev = text(dependency, "kind") == Some("dev");
            let allowed = rule.workspace.contains(&dependency_name)
                || rule.external.contains(&dependency_name)
                || (is_dev && rule.dev.contains(&dependency_name));
            if !allowed {
                let kind = if is_dev {
                    "dev-dependency"
                } else {
                    "dependency"
                };
                violations.insert(format!(
                    "`{name}` must not have {kind} `{dependency_name}`; it is not in the allowlist"
                ));
            }
        }

        if rule.core {
            let mut seen: BTreeSet<&str> = BTreeSet::new();
            let mut pending = vec![member];
            while let Some(current) = pending.pop() {
                if !seen.insert(current) {
                    continue;
                }
                pending.extend(edges.get(current).into_iter().flatten().copied());
            }
            for reached in seen.into_iter().filter_map(name_of) {
                if FORBIDDEN_IN_CORE.contains(&reached) {
                    violations.insert(format!(
                        "core crate `{name}` reaches `{reached}`, directly or transitively"
                    ));
                }
            }
        }
    }

    violations
}

/// Lines of code with their 1-based numbers. Comment lines are skipped so
/// that documentation may name what the code must not use.
fn code_lines(source: &str) -> impl Iterator<Item = (usize, &str)> {
    (1_usize..)
        .zip(source.lines())
        .filter(|(_, line)| !line.trim_start().starts_with("//"))
}

/// Identifiers that follow `crate::` in the code of a source text.
fn crate_imports(source: &str) -> BTreeSet<&str> {
    let mut found = BTreeSet::new();
    for (_, line) in code_lines(source) {
        let mut rest = line;
        while let Some(position) = rest.find("crate::") {
            let after = rest
                .get(position.saturating_add("crate::".len())..)
                .unwrap_or("");
            let end = after
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                .unwrap_or(after.len());
            if let Some(module) = after.get(..end).filter(|module| !module.is_empty()) {
                found.insert(module);
            }
            rest = after;
        }
    }
    found
}

/// Check the `crate::` imports of every module of `circuit-sim`. `modules` is
/// `(module name, source text)` for each production module; test files are
/// not passed in.
pub(crate) fn module_violations(modules: &[(String, String)]) -> BTreeSet<String> {
    let mut violations = BTreeSet::new();
    for (module, source) in modules {
        let Some((_, allowed)) = SIM_MODULE_RULES.iter().find(|(name, _)| name == module) else {
            violations.insert(format!(
                "circuit-sim module `{module}` has no import rule; add one to xtask"
            ));
            continue;
        };
        for import in crate_imports(source) {
            if !allowed.contains(&import) {
                violations.insert(format!(
                    "circuit-sim module `{module}` must not import `crate::{import}`"
                ));
            }
        }
    }
    violations
}

/// Check core sources for standard-library APIs the core must not use.
/// `files` is `(display path, source text)`.
pub(crate) fn forbidden_api_violations(files: &[(String, String)]) -> BTreeSet<String> {
    let mut violations = BTreeSet::new();
    for (path, source) in files {
        for (line, content) in code_lines(source) {
            for api in FORBIDDEN_CORE_APIS {
                if content.contains(api) {
                    violations.insert(format!("{path}:{line}: the core must not use `{api}`"));
                }
            }
        }
    }
    violations
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// A package name with its dependencies as `(name, kind)`.
    type Package<'a> = (&'a str, &'a [(&'a str, Option<&'a str>)]);

    /// Synthetic `cargo metadata`: every package is a workspace member unless
    /// its name is listed in `registry`.
    fn metadata(packages: &[Package<'_>], registry: &[&str]) -> Value {
        let package_values: Vec<Value> = packages
            .iter()
            .map(|(name, dependencies)| {
                json!({
                    "id": name,
                    "name": name,
                    "dependencies": dependencies
                        .iter()
                        .map(|(dependency, kind)| json!({ "name": dependency, "kind": kind }))
                        .collect::<Vec<_>>(),
                })
            })
            .collect();
        let nodes: Vec<Value> = packages
            .iter()
            .map(|(name, dependencies)| {
                json!({
                    "id": name,
                    "deps": dependencies
                        .iter()
                        .map(|(dependency, kind)| json!({
                            "pkg": dependency,
                            "dep_kinds": [{ "kind": kind }],
                        }))
                        .collect::<Vec<_>>(),
                })
            })
            .collect();
        let members: Vec<&str> = packages
            .iter()
            .map(|(name, _)| *name)
            .filter(|name| !registry.contains(name))
            .collect();
        json!({
            "packages": package_values,
            "workspace_members": members,
            "resolve": { "nodes": nodes },
        })
    }

    fn violations(packages: &[Package<'_>], registry: &[&str]) -> Vec<String> {
        dependency_violations(&metadata(packages, registry))
            .into_iter()
            .collect()
    }

    const REGISTRY: &[&str] = &[
        "serde",
        "serde_json",
        "thiserror",
        "proptest",
        "eframe",
        "egui",
        "egui_kittest",
        "helper",
    ];

    #[test]
    fn target_layout_has_no_violation() {
        let found = violations(
            &[
                ("circuit-model", &[("serde", None), ("thiserror", None)]),
                (
                    "circuit-sim",
                    &[
                        ("circuit-model", None),
                        ("thiserror", None),
                        ("proptest", Some("dev")),
                    ],
                ),
                (
                    "circuit-project",
                    &[
                        ("circuit-model", None),
                        ("serde_json", None),
                        ("thiserror", None),
                    ],
                ),
                (
                    "opencircuit-cli",
                    &[
                        ("circuit-model", None),
                        ("circuit-sim", None),
                        ("circuit-project", None),
                        ("serde_json", None),
                    ],
                ),
                (
                    "circuit-renderer",
                    &[("circuit-model", None), ("proptest", Some("dev"))],
                ),
                (
                    "circuit-commands",
                    &[
                        ("circuit-model", None),
                        ("thiserror", None),
                        ("proptest", Some("dev")),
                    ],
                ),
                (
                    "circuit-editor",
                    &[
                        ("circuit-commands", None),
                        ("circuit-model", None),
                        ("circuit-sim", Some("dev")),
                        ("proptest", Some("dev")),
                    ],
                ),
                (
                    "opencircuit-desktop",
                    &[
                        ("circuit-model", None),
                        ("circuit-project", None),
                        ("circuit-renderer", None),
                        ("circuit-sim", None),
                        ("eframe", None),
                        ("egui_kittest", Some("dev")),
                    ],
                ),
                ("xtask", &[("serde_json", None)]),
                ("serde", &[]),
                ("serde_json", &[]),
                ("thiserror", &[]),
                ("proptest", &[]),
                ("eframe", &[("egui", None)]),
                ("egui", &[]),
                ("egui_kittest", &[("egui", None)]),
            ],
            REGISTRY,
        );

        assert_eq!(found, Vec::<String>::new());
    }

    #[test]
    fn unknown_workspace_crate_fails_closed() {
        let found = violations(&[("circuit-plugin", &[])], REGISTRY);

        assert_eq!(
            found,
            [
                "workspace package `circuit-plugin` has no architecture rule; add one to xtask before adding the crate"
            ]
        );
    }

    #[test]
    fn renderer_depending_on_circuit_sim_is_rejected() {
        let found = violations(
            &[
                ("circuit-model", &[]),
                (
                    "circuit-renderer",
                    &[("circuit-model", None), ("circuit-sim", None)],
                ),
                ("circuit-sim", &[("circuit-model", None)]),
            ],
            REGISTRY,
        );

        assert_eq!(
            found,
            [
                "`circuit-renderer` must not have dependency `circuit-sim`; it is not in the allowlist"
            ]
        );
    }

    #[test]
    fn commands_depending_on_circuit_sim_is_rejected() {
        let found = violations(
            &[
                ("circuit-model", &[]),
                (
                    "circuit-commands",
                    &[("circuit-model", None), ("circuit-sim", None)],
                ),
                ("circuit-sim", &[("circuit-model", None)]),
            ],
            REGISTRY,
        );

        assert_eq!(
            found,
            [
                "`circuit-commands` must not have dependency `circuit-sim`; it is not in the allowlist"
            ]
        );
    }

    #[test]
    fn editor_depending_on_circuit_sim_outside_its_tests_is_rejected() {
        let found = violations(
            &[
                ("circuit-model", &[]),
                (
                    "circuit-editor",
                    &[("circuit-model", None), ("circuit-sim", None)],
                ),
                ("circuit-sim", &[("circuit-model", None)]),
            ],
            REGISTRY,
        );

        assert_eq!(
            found,
            ["`circuit-editor` must not have dependency `circuit-sim`; it is not in the allowlist"]
        );
    }

    #[test]
    fn renderer_reaching_egui_is_rejected() {
        let found = violations(
            &[
                ("circuit-model", &[]),
                (
                    "circuit-renderer",
                    &[("circuit-model", None), ("helper", None)],
                ),
                ("helper", &[("egui", None)]),
                ("egui", &[]),
            ],
            REGISTRY,
        );

        assert!(
            found.contains(
                &"core crate `circuit-renderer` reaches `egui`, directly or transitively"
                    .to_owned()
            ),
            "{found:?}"
        );
    }

    #[test]
    fn circuit_model_with_a_workspace_dependency_is_rejected() {
        let found = violations(
            &[
                ("circuit-model", &[("circuit-sim", None)]),
                ("circuit-sim", &[("circuit-model", None)]),
            ],
            REGISTRY,
        );

        assert_eq!(
            found,
            ["`circuit-model` must not have dependency `circuit-sim`; it is not in the allowlist"]
        );
    }

    #[test]
    fn circuit_sim_depending_on_circuit_project_is_rejected() {
        let found = violations(
            &[
                ("circuit-model", &[]),
                (
                    "circuit-sim",
                    &[("circuit-model", None), ("circuit-project", None)],
                ),
                ("circuit-project", &[("circuit-model", None)]),
            ],
            REGISTRY,
        );

        assert_eq!(
            found,
            [
                "`circuit-sim` must not have dependency `circuit-project`; it is not in the allowlist"
            ]
        );
    }

    #[test]
    fn dev_only_crate_is_rejected_as_a_normal_dependency() {
        let found = violations(
            &[
                ("circuit-model", &[]),
                (
                    "circuit-sim",
                    &[("circuit-model", None), ("proptest", None)],
                ),
                ("proptest", &[]),
            ],
            REGISTRY,
        );

        assert_eq!(
            found,
            ["`circuit-sim` must not have dependency `proptest`; it is not in the allowlist"]
        );
    }

    #[test]
    fn transitive_gui_dependency_of_a_core_crate_is_rejected() {
        let found = violations(
            &[
                ("circuit-model", &[("serde", None)]),
                ("serde", &[("helper", None)]),
                ("helper", &[("egui", None)]),
                ("egui", &[]),
            ],
            REGISTRY,
        );

        assert_eq!(
            found,
            ["core crate `circuit-model` reaches `egui`, directly or transitively"]
        );
    }

    #[test]
    fn gui_dependency_reached_only_through_a_dev_dependency_is_ignored() {
        let found = violations(
            &[
                ("circuit-model", &[]),
                (
                    "circuit-sim",
                    &[("circuit-model", None), ("proptest", Some("dev"))],
                ),
                ("proptest", &[("egui", None)]),
                ("egui", &[]),
            ],
            REGISTRY,
        );

        assert_eq!(found, Vec::<String>::new());
    }

    #[test]
    fn metadata_without_members_is_a_violation_not_a_pass() {
        let found: Vec<String> = dependency_violations(&json!({})).into_iter().collect();

        assert_eq!(found, ["cargo metadata lists no workspace member"]);
    }

    fn module(name: &str, source: &str) -> (String, String) {
        (name.to_owned(), source.to_owned())
    }

    #[test]
    fn allowed_module_imports_pass() {
        let found = module_violations(&[
            module(
                "solve",
                "use crate::compiled::NetIx;\nuse crate::union_find::UnionFind;",
            ),
            module(
                "runtime",
                "use crate::solve::Solver;\nuse crate::logic::step_timer;",
            ),
            module("logic", "use circuit_model::TimerMode;"),
        ]);

        assert_eq!(found, BTreeSet::new());
    }

    #[test]
    fn solver_importing_the_runtime_is_rejected() {
        let found: Vec<String> =
            module_violations(&[module("solve", "use crate::runtime::Simulator;")])
                .into_iter()
                .collect();

        assert_eq!(
            found,
            ["circuit-sim module `solve` must not import `crate::runtime`"]
        );
    }

    #[test]
    fn logic_importing_the_solver_is_rejected() {
        let found: Vec<String> =
            module_violations(&[module("logic", "fn f(state: crate::solve::LoadState) {}")])
                .into_iter()
                .collect();

        assert_eq!(
            found,
            ["circuit-sim module `logic` must not import `crate::solve`"]
        );
    }

    #[test]
    fn module_without_a_rule_fails_closed() {
        let found: Vec<String> = module_violations(&[module("netlist", "")])
            .into_iter()
            .collect();

        assert_eq!(
            found,
            ["circuit-sim module `netlist` has no import rule; add one to xtask"]
        );
    }

    #[test]
    fn comments_may_name_what_the_code_must_not_use() {
        let source =
            "//! Never calls `std::fs`.\n    // see crate::runtime for the caller\nfn f() {}";

        assert_eq!(
            forbidden_api_violations(&[("lib.rs".to_owned(), source.to_owned())]),
            BTreeSet::new()
        );
        assert_eq!(
            module_violations(&[module("logic", source)]),
            BTreeSet::new()
        );
    }

    #[test]
    fn clock_and_filesystem_access_in_the_core_is_rejected_with_its_location() {
        let found: Vec<String> = forbidden_api_violations(&[(
            "crates/circuit-sim/src/runtime.rs".to_owned(),
            "use std::collections::BTreeMap;\nlet now = std::time::Instant::now();\nstd::fs::read(path);"
                .to_owned(),
        )])
        .into_iter()
        .collect();

        assert_eq!(
            found,
            [
                "crates/circuit-sim/src/runtime.rs:2: the core must not use `std::time::Instant`",
                "crates/circuit-sim/src/runtime.rs:3: the core must not use `std::fs`",
            ]
        );
    }
}
