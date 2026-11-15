# ADR-0011: Sheets as frames over one canvas

Status: Accepted (2026-11-15). Builds on [ADR-0004](0004-canonical-json-format.md)
(canonical JSON, version policy) and [ADR-0006](0006-pdf-export.md) (PDF
export). Tracking issue: #56 (M5).

## Context

A schematic is drawn on one unbounded integer grid, and the PDF export is one
page cropped to the drawing (ADR-0006). Electrical documentation needs more
than that: drawings printed on standard paper, a frame with zones that a
cross-reference can name ("sheet 2, column 4"), a title block with the
project, revision and author, and a project that spans several sheets.

Two models were considered:

1. **Frames over the canvas.** A sheet is a rectangle placed on the existing
   canvas, with a paper size, an orientation and zones. Wires keep connecting
   whatever they reach; which sheet a symbol is on follows from where it is.
2. **Separate canvases joined by connectors.** Each sheet is its own canvas,
   and a circuit continues on another sheet through off-page connectors that
   join nets by name. This is how many printed sets are organised, but the
   compiler would have to join nets by label, every connectivity rule and
   golden would be revisited, and the editor would need navigation between
   canvases.

## Decision

- **A sheet is a frame over the single canvas.** The compiler and the
  simulator do not change: connectivity is still copper (wires and
  junctions), as in ADR-0003.
- **Persisted** (schema version 2, with a migration from version 1 that adds
  no sheet): `sheets`, each with an `id` from the document counter, a
  `number` (unique, from 1), a `title`, an `origin` (`GridPoint`, top-left
  corner), a `size` (`a4`, `a3`, `a2` or a custom width and height in grid
  units), an `orientation` (`landscape`, `portrait`) and its `zones`
  (columns and rows). `project` gains `author`, `company`, `revision` and
  `date`, all plain text: the core has no clock.
- **Derived, never stored:** the sheet of a symbol is the sheet whose frame
  contains the symbol's origin, and a zone is named by its column number and
  row letter. A symbol outside every frame belongs to no sheet. Frames may
  not overlap (a validation error), and sheet numbers are unique.
- **Scale is unchanged:** one grid unit is 10 PostScript points (about
  3.53 mm), the scale of ADR-0006. A paper size is the largest whole number
  of grid units that fits it: A4 landscape is 84 by 59 grid units.
- **A document with no sheet is drawn and exported as today**: one page
  sized to the drawing. With sheets, the export makes one page per sheet, at
  the paper size, in sheet-number order, each with its frame, zones and
  title block.
- **A wire that crosses a frame edge** stays connected and is reported as a
  warning, because the printed sheets would cut it. Continuation markers are
  a later decision.

## Consequences

- `Schematic` and `FORMAT.md` move to schema version 2 with the first real
  migration; every committed `.ocircuit` is migrated by the example
  generator, and version-1 files keep loading.
- Cross-references (M5-9) are a pure function of the document, the catalog
  and the sheets, drawn as generated text and never stored.
- The renderer draws frames and title blocks as ordinary scene shapes, so the
  SVG and PDF exporters keep a single drawing path.
- The separate-canvas model stays possible: it would supersede this ADR and
  add net labels to the compiler, with its own goldens.
