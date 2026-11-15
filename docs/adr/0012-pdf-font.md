# ADR-0012: An embedded font for the PDF export

Status: Accepted (2026-11-15). Amends the text part of
[ADR-0006](0006-pdf-export.md) ("Courier text"); the rest of that ADR stands.
Tracking issue: #56 (M5).

## Context

ADR-0006 drew the text of a PDF with the standard Courier font, so that no
font was embedded, and wrote every character other than printable ASCII as
`?`. Two things showed that this was too little:

- the star-delta motor shows `Δ` while it runs in delta, and the PDF drew
  `?` in its place;
- the title block of a sheet (ADR-0011) holds a title, an author and a
  company, which in Portuguese carry accents.

## Decision

- **Embed Liberation Mono Regular 2.1.5**, a font under the SIL Open Font
  License 1.1, metric-compatible with Courier New (0.6 em per glyph, so the
  text widths and the centring of ADR-0006 do not change) and covering
  Latin, Greek and Cyrillic. The file is the unmodified file of the upstream
  release, kept in `assets/fonts/liberation-mono/` with its licence text and
  recorded in `ATTRIBUTION.md` with its SHA-256. It is compiled into the
  binary, so the core still does no file access.
- **Embed only a subset.** `subsetter` (MIT OR Apache-2.0, `deny(unsafe_code)`,
  one dependency of its own) keeps the glyphs of the characters a scene
  draws, which turns 320 KB into a few kilobytes per document. The subset
  is a composite font (`Type0`, `Identity-H`, TrueType `CIDFontType2`) whose
  codes are the glyph IDs of the subset, with a `ToUnicode` map so that the
  text can be searched and copied. Glyphs are numbered in character order, and
  the subset is named by a hash of its data, so the same scene gives the same
  bytes on every platform.
- **A character the font lacks is drawn as the font's empty-box glyph**, not
  as `?`, and has no entry in `ToUnicode`.
- **A small reader instead of a font crate.** The export needs a character to
  glyph lookup and a few metrics from one fixed, bundled file. `ttf-parser`
  was the obvious crate and was tried first, but its author has declared it
  unmaintained (RUSTSEC-2026-0192), which `cargo deny` refuses. The maintained
  replacement (`read-fonts`) brings about ten crates, procedural macros among
  them, into a core crate. `sfnt.rs` reads `cmap` (formats 4 and 12), `head`,
  `hhea`, `OS/2`, `hmtx` and `maxp`, under 200 lines with every access
  checked. It was compared with `ttf-parser` on every code point up to
  U+10FFFF and on every metric: 2 305 characters have a glyph and all agree.
- **SVG is unchanged.** It still names `sans-serif`; the viewer chooses the
  font.

## Consequences

- The font is the first third-party file in the repository, so `AGENTS.md`
  says that `assets/` may hold the files listed in `ATTRIBUTION.md`. Its
  licence needs the copyright notice and the licence text to accompany the
  file, which they do; the subset inside a PDF belongs to the document and is
  never offered as a font file.
- The Reserved Font Name "Liberation" forbids renaming or modifying the file
  in the repository; changing the font means a new release, a new checksum and
  a new licence text.
- The committed PDFs of `examples/` grow from about 15 KB to about 27 KB for
  the largest, because each carries a subset.
- The web build carries the 320 KB font in its binary, next to the fonts of
  egui.
- A font under another licence, or a different weight, would be a new ADR.
