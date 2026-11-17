# ADR-0018: Schema version 5 — documentation data and document views

Status: Accepted (2026-11-17). Schema version 5 ([FORMAT.md](../../FORMAT.md)).
Tracking issue: #118 (M9.3).

## Context

M9 adds to a project what an electrical drawing set carries besides the
circuit: the client in the title block, what a wire is made of, revision
clouds, comments and drawn shapes, groups, mirrored symbols, QR codes, the
manufacturer data and datasheet of a device, a single-line diagram and a
panel layout that show the devices of the schematic, and PLC modules with
their channels. All of it is documentation. The project format changes once
for the milestone, so every field is decided here before any is written.

## Decision

### One schema change, no electrical meaning

- **Schema version 5.** Every field below is optional and omitted from the
  file when empty or at its default, so a version 4 document is a valid
  version 5 document: the migration from 4 to 5 only raises the version. A
  version 4 document that already has one of these fields is refused rather
  than reinterpreted, as in ADR-0014 and ADR-0015.
- **Documentation never changes the circuit.** The compiler and the
  simulator read none of these fields: a project with them and the same
  project without them compile to the same circuit, fingerprint and traces.
  Mirroring a symbol moves its terminals on the drawing, which only moves
  where wires end; connectivity is by terminal key, never by position.
- **New identities.** `ShapeId`, `CommentId`, `GroupId`, `ViewId` and `PlcId`
  come from the document counter (ADR-0001), are never reused, and are
  subjects of selections, diagnostics, waivers and edit files
  (`Subject::Shape`, `Comment`, `Group`, `View`, `Plc`). A reference to an
  element that does not exist is the error `OC-STR-002`, as for any other.
- **Texts are bounded** like every text of a project file: at most 256 bytes,
  no control character. Shorter bounds are given per field.

### Project and wires

- `project.client`: the client, a text of at most 64 characters, shown in the
  title block of every sheet under the company.
- `Wire.spec`: `{ colour, cross_section, kind, material, insulation }`, each
  an optional text of at most 32 characters (`"BU"`, `"1.5 mm²"`, `"H07V-K"`,
  `"Cu"`, `"PVC"`). The stored colour is what the wire is made of, not how it
  is drawn: the window and the exports keep drawing conductors in the
  colours of their energized state. The `wires` report gets one column per
  field, in a project where a wire has data (the report of any other project
  is as before); a net whose wires differ shows each value once.

### Annotations

- **Shapes** are `Schematic.shapes`, sorted by ID: `Shape { id, kind, layer }`
  where `kind` is one of
  - `polyline { points, closed }`, 2 to 64 grid points;
  - `rectangle { from, to }`, two opposite corners;
  - `circle { center, radius }`, radius 1 to 1 000 grid units;
  - `arc { center, radius, start, sweep }`, angles in whole degrees, `start`
    0 to 359 and `sweep` 1 to 360, counter-clockwise as seen on the sheet
    from the positive x axis (to the right);
  - `cloud { from, to, revision }`, a revision cloud around a rectangle with
    an optional revision mark (at most 8 characters) drawn in a triangle;
  - `qr { position, size, text }`, a QR code whose top-left corner is at
    `position` and whose side is `size` grid units (2 to 100), encoding
    `text` (1 to 256 bytes) with error correction level M. The modules are
    computed when the scene is built (`qrcodegen`, MIT) and never stored.
  A shape is drawn in the scene, and so in SVG, PDF and DXF exports, and can
  be selected, moved, deleted and put on a layer like a text note. Coordinates
  stay on the grid: there is no free drawing, scaling or fillet.
- **Comments** are `Schematic.comments`: `Comment { id, subject, text,
  author }`, a text of at most 200 characters about one element (`subject`),
  with an optional author of at most 32 characters. A comment is not drawn:
  the window lists the comments of the selected element, and
  `opencircuit report --kind comments` lists them all. Deleting an element
  deletes its comments in the same step.

### Groups and mirrored symbols

- **Groups** are `Schematic.groups`: `Group { id, name, members }`, a name of
  1 to 32 characters and 1 to 1 024 members among symbols, wires, junctions,
  texts and shapes. Clicking a member selects the whole group; Group and
  Ungroup are one undoable step each, and ungrouping keeps the elements. An
  element in two groups is a warning (`OC-STR-023`). Deleting an element
  takes it out of its group; a group left with no member is deleted with it.
- **Mirrored symbols:** `SymbolInstance.mirrored`, omitted when false. The
  symbol is mirrored about the vertical line through its origin, then
  rotated. Terminal positions, the drawing, the hit area and the cross-
  reference zone follow; terminal keys and labels do not change.

### Device data

- `DeviceInfo.datasheet`: a link to the datasheet of the device, an
  `http://`, `https://` or `file:` URL or a relative path, at most 256 bytes.
  The window opens it with the system (a link of another kind is refused,
  never run).
- **Import from CSV** (`circuit_commands::catalog_csv`): a header row names
  the columns `reference` (required), `manufacturer`, `part_no`,
  `description`, `location` and `datasheet`, in any order and case; quoting
  follows RFC 4180. Each row sets the catalog data of the device with that
  designator: a filled cell replaces the field, an empty cell leaves it. A
  row for a designator the project does not have is reported and skipped.
  The import is one undoable step, refused as a whole when the CSV cannot be
  read (at most 10 000 rows). The core reads text only; the command line and
  the window read the file.

### Sheet kinds and device views

- `Sheet.kind`: `schematic` (the default, omitted), `single_line` or
  `panel_layout`. The title block names the kind of a sheet that is not a
  schematic.
- **Device views** are `Schematic.views`: `DeviceView { id, device_id,
  position, width, height, layer }`, a place where a device of the schematic
  is shown again: its footprint on a panel layout, its symbol on a
  single-line diagram. The view is the same device (the K1 of the panel is
  the K1 of the schematic); it has no terminals and joins nothing. It is
  drawn as a rectangle of `width` by `height` grid units (1 to 200) with the
  designator; inside a `single_line` sheet it is drawn instead as a
  single-line glyph chosen by the category of the device's model. Under the
  designator, a view shows the cross-reference to the sheet and zone of the
  device's first symbol on the schematic, derived as in ADR-0011. Lines
  between single-line glyphs are polylines.
- **The kind of a sheet is checked, not enforced.** A view that is not inside
  a `single_line` or `panel_layout` sheet, or a symbol inside one, is the
  warning `OC-STR-024`. Neither changes the circuit.
- The `devices` report gets the columns `datasheet` and `views` (the sheets
  of the device's views).

### PLC modules

- **PLC modules** are `Schematic.plcs`: `PlcModule { id, name, description,
  position, channels }`. The name follows the rule of a designator (`A1`); the
  description is at most 64 characters; `position`, when set, places a box
  listing the channels on the drawing. A channel is `{ address, direction,
  device, label }`: an address of 1 to 16 characters (`I0.0`, `%QX1.2`), a
  direction `input` or `output`, an optional device it reads or drives, and
  an optional label of at most 32 characters. At most 256 channels per module.
- **Documentation only.** A PLC program still runs outside OpenCircuit,
  against the Modbus server (ADR-0009); a channel neither reads nor drives
  anything in the simulation.
- Two channels of one module with the same address, or a device given to two
  channels of the same direction, is the warning `OC-STR-025`.
- **The `io` report** has one row per channel: module, address, direction,
  device, label, and the Modbus table and address of that device in the map
  of ADR-0009 (`coil` for an operable input such as a pushbutton,
  `discrete_input` for an observable output such as a coil), empty when the
  device is not in the map.

### Diagnostics

| Code | Name | Severity | What |
| --- | --- | --- | --- |
| `OC-STR-023` | `InvalidAnnotation` | Warning | A shape, comment, group, view, PLC module, wire specification, client or datasheet that breaks a bound of this ADR, or an element in two groups |
| `OC-STR-024` | `ViewOffItsSheet` | Warning | A device view outside a single-line or panel-layout sheet, or a symbol inside one |
| `OC-STR-025` | `PlcChannelConflict` | Warning | Two channels of a module with one address, or one device on two channels of the same direction |

A file whose texts break a bound still loads, as in ADR-0015; a text over 256
bytes is refused when the file is read.

### Window, command line and edit files: additions only

Each new field is reachable without a new panel: closed sections of the
inspector, entries at the end of the existing menus, commands of the command
line of the window, and edit-file operations
(`set_project` with `client`, `set_wire_spec`, `add_shape`, `add_comment`,
`group`, `ungroup`, `mirror`, `import_catalog`, `set_sheet` with `kind`,
`add_view`, `add_plc`, `set_plc`). Existing flags, fields and exit codes do
not change.

## Consequences

- A version 5 file does not open in a build that reads version 4; a version 4
  file opens in this build and is saved as version 5.
- Reports, exports and the window grow; the simulator, the compiled circuit,
  the Modbus map and every golden trace stay the same.

## Not decided here

Drawing tools with the pointer for every shape kind (the command line and the
edit files add them; the pointer moves and deletes them), comments drawn on
the sheet, nested groups, a panel layout with real dimensions in millimetres
and rails, and running a PLC program.
