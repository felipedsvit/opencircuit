# ADR-0004: Canonical JSON project format and version policy

Status: Accepted (2026-11-14). Spec reference: `docs/spec02.md` §15.
Normative text: [`FORMAT.md`](../../FORMAT.md).

## Context

Project files are the engineer's work product. They live in version control,
are diffed by people, are read by tools in other languages, and must open in
future versions of the program. A silent misreading (a dropped field, a float
rounded differently on another platform) is worse than a refusal.

## Decision

- **JSON, UTF-8, `.ocircuit`**, with mandatory `schema_version` and `next_id`.
- **No floating point.** Voltages in millivolts, frequencies in millihertz,
  times in milliseconds; `1.0` is rejected where an integer is expected.
- **Canonical writing.** Fixed field order, collections sorted by ID,
  parameters sorted by key, two-space indentation, explicit `null`, LF line
  endings, final newline. Saving a canonical file that was just loaded
  reproduces it byte for byte. `.gitattributes` pins LF for the format and the
  goldens.
- **Two-phase reading.** Phase 1 reads only `schema_version`: a newer version
  is refused (`ProjectError::NewerSchema`) with nothing else read; an older one
  is migrated by pure `Value -> Value` functions, each with a before/after
  fixture. Phase 2 deserializes the current schema strictly
  (`deny_unknown_fields`, repeated fields and repeated parameter keys
  rejected) and applies size limits.
- **Unknown fields are not preserved.** Preserving them would need `flatten`
  on every struct and would conflict with strict validation.
- **The loader checks syntax, schema and limits only.** ID ranges, uniqueness,
  references, designators, models, terminals and parameters are diagnostics of
  `Schematic::validate` and the compiler, so a broken project still opens.
- **I/O is the application's job.** `circuit-project` converts text and model
  and never touches the filesystem; applications write atomically (temporary
  file, flush, `fsync`, rename).
- **Model versions are independent of `schema_version`.** A published
  `(model id, version)` never changes.

## Alternatives considered

- **Floating-point values with a tolerance.** Rejected: platform-dependent
  printing; cannot promise byte-identical round-trips.
- **A binary or RON/TOML format.** Rejected: JSON is diffable and readable
  from every language the UI and tooling may use, including WASM.
- **Preserving unknown fields for forward compatibility.** Rejected, see above;
  a newer file is refused instead.
- **Loader that rejects out-of-range IDs and dangling references.** Rejected:
  the user could not open the project to fix it, and the diagnostics would
  not reach the CLI or the editor's panel.
- **Best-effort reading of newer schemas.** Rejected: partial reads risk
  saving a file with silently dropped data.

## Consequences

- Every schema change is a version bump with a migration, a fixture pair and a
  changelog line in `FORMAT.md`.
- Files diff cleanly, and canonical form is testable: examples are generated
  and checked by `xtask generate-examples --check`.
- Hostile input is bounded: 16 MiB of text, 100 000 entities per collection,
  1 024 route points per wire, 64 parameters per device, 256 bytes per text
  value.

## Evidence

`crates/circuit-project/src/lib.rs`, `limits.rs`, `migrate.rs`, `tests.rs`
(canonical form, round-trip properties, rejections; the `FORMAT.md` example
and catalog table are compared with the code), `examples/`.
