# ADR-0015: Text notes and layers

Status: Accepted (2026-11-17). Schema version 4 ([FORMAT.md](../../FORMAT.md)).
Tracking issues: #101 (text), #102 (layers), #107 (M8).

## Context

A drawing carries words that are not part of any device: a title over a block of
circuits, a remark beside a contact, a note for the person who wires the panel.
Until now the only text a drawing could have was a designator, a number, or the
title block. The editor had no way to say anything about the circuit in the
circuit.

## Decision (text)

- **A text note is its own entity.** `Schematic.texts` is a list of
  `TextNote { id, position, text, anchor }`, sorted by `id`, omitted from the
  file when empty. The ID comes from the same counter as every other entity and
  is never reused (ADR-0001), so a note can be named in a selection, a waiver or
  an edit file as `Subject::Text`.
- **It is documentation only.** A note has no terminals and no electrical
  meaning, joins nothing, and is not in the compiled circuit: a project with
  notes and the same project without them have the same fingerprint, simulation
  and traces. No report lists a note (the bill of materials, wire numbers,
  terminal plan, cross-references and problems are functions of devices, wires
  and the catalog). A note is drawn in the scene and so appears in SVG and PDF
  exports.
- **One line, bounded.** The text is 1 to 120 characters, at most 256 bytes (the
  bound of every text in a project file), with no control character. The
  commands refuse anything else. A file that has one anyway loads, is the warning
  `OC-STR-021` (`InvalidText`), and is drawn cleaned (control characters become
  spaces) and cut at 120 characters; a text over 256 bytes is refused when the
  file is read, like any other.
- **Anchor.** `left` (the text starts at the point) or `center`. The area a note
  covers, for hit-testing and the box selection, is derived from the character
  count (`text_bounds`: 0.48 grid units a character, two units high) and is never
  stored.
- **Edits are inverses of each other**: `InsertText` and `RemoveText`, `SetText`
  and `MoveText`. Undo of an add removes the note; of a delete puts it back with
  the same ID. Deleting, moving, selecting (click, box, select all) and dragging
  work as for symbols.
- **Window, additions only.** The Modify menu gets "Add text" after the
  existing entries, the command line gets `text` (`text` chooses the tool,
  `text x,y words` adds a note), and a selected note shows a "Text" section in the
  inspector to edit its words. Nothing existing moves or changes. The Text tool
  adds a note with the words `Text` at the click, selects it, and the words are
  then typed in the inspector, so adding needs no modal dialog.
- **Command line files.** `add_text` (`at`, `text`, `anchor`, `as`) and
  `set_text` (`text` by name or ID, `to`); notes are elements for `move` and
  `delete` as `{ "text": name-or-id }`.
- **Migration.** Version 3 to 4 only raises the version, because the field is
  optional; a version 3 file that already has `texts` is refused rather than
  reinterpreted.

## Not decided here

Rotated text, several lines, fonts and sizes, and copy and paste of notes are not
part of this change: they can be added later without changing what is stored
(the note would gain optional fields, and the migration would again only raise
the version).
