# Contributing

## Before you start

Read [`ARCHITECTURE.md`](ARCHITECTURE.md) and the relevant sections of
`ideia.md`. The project is pre-implementation; contracts are still being
stabilized, so open an issue before large changes.

## Development setup

Use the toolchain pinned in `rust-toolchain.toml`. Run all commands from the
repository root:

```sh
cargo test --workspace
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p xtask -- check-architecture
```

## Rules

- Code identifiers, test names and diagnostic messages are English.
- Tests live next to the code they cover; golden scenarios live under `tests/`
  or `examples/`.
- Every new crate must be added explicitly to the workspace members.
- Intra-workspace dependencies use `{ workspace = true }`.
- Do not add `egui` or `eframe` to a `circuit-*` crate.
- Do not add filesystem, socket or clock access to the core.
- Do not commit generated `target/` directories or nested `Cargo.lock` files.
- Every behavior change needs a regression test.
- Every schema change needs a migration.
- Every component declares its terminals and provenance.

## Commits and pull requests

Keep commits focused. A change should leave the workspace compiling and the tests
passing. Describe the behavior change, the tests added and any architecture
decision in the pull request.

## Licensing

Contributions are accepted under the project dual license: MIT OR Apache-2.0.
Do not introduce GPL or otherwise incompatible code without an explicit
licensing decision. New dependencies must be compatible with the project
licenses and are checked by `cargo-deny`.

## Graphic assets

Create project-owned symbols following documented electrotechnical conventions.
Do not copy IEC 60617 or third-party symbol files without verifying the license.
Record origin, revision and normative reference in the symbol metadata.
