# ADR-0019: Dialogs, and the width of the library

Status: Accepted (2026-11-18). Part of M9.5.

## Context

The window asked its questions (unsaved changes, recover a copy) with the
default `egui::Modal`: a heading, a line and three equal grey buttons, which
looked like an old desktop. Errors were a red line of text with no way to clear
it. The library in the left column drew its cells at one size, so the column
could neither be narrower than eight icons nor use extra width.

## Decision

- **File pickers are native, questions are not.** Open, save and export use
  `rfd` (the Windows common dialogs, the macOS panels, the desktop portal on
  Linux): a file is the system's to choose. A native *message box* was
  evaluated and not used: it blocks the frame, needs `zenity` on Linux (the
  `xdg-portal` backend of `rfd` has no message dialog), offers no three buttons
  with our own words, ignores the theme, does not exist in the browser build
  and cannot be driven by the window tests (`apps/desktop/tests/` clicks the
  buttons by their names).
- **One component for the questions** (`apps/desktop/src/dialog.rs`): `Dialog`
  (icon, title, message, choices with a role: primary, secondary, destructive),
  `card` for popups that take text, and `error_banner` for errors, drawn from
  the tokens of the theme. It follows the conventions of the system it runs
  on: the corner radius (`theme::window_radius`: 8 on Windows, 10 on macOS, 12
  on Linux) and the order of the buttons (the main one first on Windows, last on
  macOS and Linux). The main button has the keyboard when the dialog opens;
  Escape and a click outside dismiss it. The texts and the names of the buttons
  are unchanged, so the window tests pass unchanged.
- **Colours for the buttons** are tokens (`accent_fill` and `on_accent`, and
  `danger`), checked at 4.5:1 by `theme::tests`; the accent itself is too light
  for white text.
- **The library is a grid with a width.** A cell is never drawn larger than its
  full size (70 × 62 points). A row has four cells, whatever the width: the
  cells share the width and shrink in proportion with the column, down to 36
  points, and the eight category icons do the same (26 down to 18 points), so
  nothing breaks into another line (`palette::grid`, `palette::icon_size`). The
  column goes from what four small cells and eight small icons need (about 174
  points) to four full cells and a few points of air (about 310;
  `palette::MIN_WIDTH`, `MAX_WIDTH`). A row would lose a cell only for a width
  that the column cannot have. The default width of the column, 260 points, is
  unchanged.

## Consequences

- At the default width the library shows four smaller cells in a row where it
  showed three at full size; widening the column to its limit gives four at full
  size.
- A new question uses `Dialog`; a new popup with a field uses `dialog::card`
  or `popup_card`. A message box of the system is not added without a new ADR.
