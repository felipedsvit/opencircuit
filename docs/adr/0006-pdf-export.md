# ADR-0006: PDF export with `pdf-writer`

Status: Accepted (2026-11-15). The text part (Courier, non-ASCII as `?`) is
amended by [ADR-0012](0012-pdf-font.md). Spec reference: `docs/spec02.md` §18.1, §24
item 7, §20 (M3).

## Context

M3 must export the drawing as PDF or record why it is deferred. The SVG
exporter (`circuit-export`) writes text from the neutral scene; a PDF needs
binary-safe object syntax, a cross-reference table and fonts. The choice must
fit the rules of the project: licences in `deny.toml`, the core MSRV of Rust
1.85, a WebAssembly build of the core, byte-for-byte deterministic output, no
third-party font or symbol data copied into the repository, and a small
dependency tree.

## Spike

Each candidate was added to an empty crate outside the repository and
inspected with `cargo tree` and `cargo metadata` (2026-11-15):

| Crate | Licence | Crates pulled | Rust version | Notes |
| --- | --- | ---: | --- | --- |
| `pdf-writer` 0.15 | MIT OR Apache-2.0 | 5 (`bitflags`, `itoa`, `memchr`, `ryu`) | builds on 1.85 | low-level object writer; no fonts, no layout |
| `printpdf` 0.12 | MIT | 130 | 1.88 | pulls an MPL-2.0 crate, outside the allowlist |
| `krilla` 0.8 | MIT OR Apache-2.0 | 70 | 1.92 | font subsetting and shaping; above the core MSRV |
| `svg2pdf` 0.13 | MIT OR Apache-2.0 | 91 | — | would go through SVG and `usvg`/font database |
| `lopdf` 0.47 | MIT | 94 | 1.88 | document manipulation, more than needed |
| hand-written writer | — | 0 | any | xref offsets, escaping and streams to maintain ourselves |

With `pdf-writer` a one-page document with a line and a Courier text was
written twice and compared: identical bytes (660 bytes, no creation date, no
document ID). It built with Rust 1.85 and for `wasm32-unknown-unknown`, and
every crate of its tree is under a licence of the allowlist (MIT, Apache-2.0,
Unlicense, BSL-1.0).

## Decision

- **Use `pdf-writer`** in `circuit-export`, behind `circuit_export::pdf`, the
  same input as SVG: the neutral `Scene` and a `Theme`. It is the only
  candidate that fits the MSRV and the licence allowlist with a tree of a few
  small crates, and it leaves layout to us, which the scene already does.
- **One page sized to the drawing.** One scene subunit is one PDF point; the
  page is the scene bounds plus the SVG margin. The PDF `y` axis grows
  upwards, so every point is mirrored about the top of the page.
- **Text in Courier**, one of the 14 standard fonts that every viewer has, so
  no font file is embedded and no font data or metrics are copied. Courier is
  monospaced (every glyph is 600/1000 em wide), so a centred label is placed
  exactly. Characters outside WinAnsi are written as `?`; this limitation is
  documented and tested.
- **Circles** are four cubic Bézier curves with the usual constant
  0.5523 × radius, computed from integer coordinates.
- **Determinism.** No creation or modification date and no document ID are
  written; objects are numbered in a fixed order; numbers are formatted by
  `pdf-writer` from values that are exact (integers) or computed the same way
  on every platform (IEEE `f32`). The bytes are compared in tests and the
  example drawings are committed as goldens.
- **Front ends.** `opencircuit export --output file.pdf` and a PDF button in
  the desktop application, as for SVG.

## Alternatives considered

- **Defer PDF.** Rejected: the spike found a choice that costs five small
  crates and no new licence.
- **Hand-written writer.** Rejected for now: it avoids the dependency but
  puts object syntax, string escaping and the cross-reference table on us,
  which `pdf-writer` already tests.
- **Embed a font.** Rejected: it brings a font licence and file into the
  repository; Courier from the standard 14 is enough for designators.
- **Convert the SVG.** Rejected: `svg2pdf` pulls a font database and a large
  tree for no gain over writing from the scene.

## Consequences

- `circuit-export` gains one external dependency, recorded in its
  architecture rule; `cargo deny` checks it.
- PDF and SVG come from the same scene and the same themes.
- Non-Latin designators lose their characters in PDF until a font is chosen;
  SVG keeps them.
