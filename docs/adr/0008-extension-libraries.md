# ADR-0008: Extension libraries (data only)

Status: Accepted (2026-11-15). Spec reference: `docs/spec02.md` §7, §18.2,
§19.2, §20 (M4), §24 item 13. Normative text: `LIBRARY.md`, written by the
pull request that implements it.

## Context

M4 asks for plugins and for a third-party catalog and symbol library. The
catalog is defined in Rust and every behaviour class is implemented by the
simulator; a project file may come from anyone (spec §19.2) and must never
cause code to run. Third parties need to publish their own devices (a vendor
relay, a sensor drawn their way) without forking the project.

## Decision

- **A plugin is a library of data.** A library is a JSON file with a schema
  version, an identifier, a version, a name, a provenance (origin, licence as
  an SPDX expression, optional standard reference) and a list of component
  models. Nothing in it is executed, so a library behaves the same on the
  desktop, the command line and the web application. Plugins with code are
  out of scope.
- **Models reuse the closed behaviour of the core.** A library model names
  one of the existing classes and symbol kinds (`Contactor`, `ContactNo`,
  `Changeover`, …) and declares its parameters, symbols, instance ranges and
  slots. The catalog check that applies to built-in models
  (`ComponentCatalog::new`) applies to library models: a library cannot
  invent a class, a terminal key or a parameter type.
- **Namespaces.** A library identifier matches `[a-z][a-z0-9_]{1,31}` and
  every model identifier of the library is `<library>.<name>`. Identifiers
  used by built-in models, and `opencircuit`, are reserved. Two libraries
  with the same identifier, or a model defined twice, are refused.
- **Own artwork.** A symbol may carry artwork: integer primitives (line,
  polyline, rectangle, circle, dot, text) in scene subunits, in two lists,
  `rest` and an optional `operated` drawn when the contact is closed or the
  actuator held. Every primitive must lie inside the bounds of the symbol
  kind and a lead must reach every terminal, checked when the library is
  loaded. Terminal positions stay those of the kind, so wires behave as for
  built-in symbols. Without artwork, the kind's artwork is used.
- **Loading is explicit and bounded.** Front ends load the libraries the
  user names (`--library` on the command line, a menu in the desktop
  application); a project never loads a library by itself. A project that
  uses a model of a library that is not loaded reports `OC-STR-003`
  (`UnknownModel`), as today. Limits: 1 MiB of text, 256 models, 32 symbols
  per model, 256 primitives per artwork list, 256 bytes per text; unknown
  fields are rejected.
- **Licences are declared, not judged.** The tool shows the licence and
  origin of a library; it does not decide whether a user may use it. Only
  libraries written for this project, under its licence, are committed to
  the repository, and any asset among them is recorded in `ATTRIBUTION.md`.

## Alternatives considered

- **Code plugins (WebAssembly in a sandbox).** Rejected for M4: a runtime
  such as `wasmtime` brings a licence outside the allowlist and a large
  attack surface, and device behaviour would leave the tested core.
- **Embedding libraries in project files.** Rejected: every project would
  carry copies that drift; a library is versioned once.
- **SVG files as symbol artwork.** Rejected: an SVG parser is a large
  dependency and an attack surface, and arbitrary SVG cannot be checked to
  reach the terminals.

## Consequences

- Third parties extend the catalog without code and without a fork; the
  simulator stays the only place where behaviour lives.
- `circuit-project` gains a library reader, `circuit-model` a catalog that
  merges libraries and a `SymbolDefinition` with optional artwork, and the
  renderer draws it.
