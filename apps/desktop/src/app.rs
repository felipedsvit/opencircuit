//! The window: a bar to open, edit and run a project, the canvas, the
//! palette and inspector when editing, the operator controls with the trace
//! when simulating, and the diagnostics.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use circuit_editor::{Editor, Hit, Key, Modifiers, Outcome, Pointer, Snap, SnapTarget, Tool, snap};
use circuit_model::library::{Library, MAX_LIBRARY_BYTES};
use circuit_model::{
    CatalogError, ComponentCatalog, Control, DeviceId, GridPoint, InputAction, LayoutIndex,
    SheetId, Subject,
};
use circuit_renderer::{
    SUBUNITS_PER_GRID, Scene, SceneItem, SceneOptions, ScenePoint, SceneRect, Shape, StyleKey,
    build_scene,
};
use eframe::egui::{self, Align, KeyboardShortcut, Layout, RichText, Ui};

use crate::camera::Camera;
use crate::canvas;
use crate::clock::{SPEEDS, SimClock};
use crate::dialogs;
use crate::icons::{self, Icon};
use crate::inspector::{self, InspectorState};
use crate::palette;
use crate::session::{SaveError, Session};
use crate::theme::{self, Tokens};

/// Radius of the ring marking the pin under the pointer, in subunits.
const PIN_RING: i64 = SUBUNITS_PER_GRID / 2;
/// How far from the pointer the wire tool looks for a pin, a junction or a
/// wire, in screen points: the same reach at every zoom.
const SNAP_POINTS: f32 = 12.0;
/// The largest snapping radius in subunits, so that a zoomed-out view does
/// not jump across several grid units.
const SNAP_MAX: i64 = 2 * SUBUNITS_PER_GRID;
/// Zoom factor of one press of `+`.
const KEY_ZOOM: f32 = 1.25;

/// An action that would drop unsaved changes, waiting for the user to save,
/// discard or cancel.
#[derive(Clone, Debug, Eq, PartialEq)]
enum Pending {
    /// Start a new project.
    New,
    /// Open this file.
    Open(PathBuf),
    /// Choose a file to open.
    OpenDialog,
    /// Close the window.
    Close,
}

/// What the window is for at the moment.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Mode {
    /// Run the circuit and operate it.
    #[default]
    Simulate,
    /// Change the circuit.
    Edit,
}

/// The desktop application.
#[derive(Debug, Default)]
pub struct DesktopApp {
    session: Option<Session>,
    mode: Mode,
    /// Tool, selection and gesture; the selection is shared by both modes.
    editor: Editor,
    /// Where the pointer snapped with the wire tool, as of the last frame.
    snapped: Option<Snap>,
    inspector: InspectorState,
    /// Text of the path field.
    path_input: String,
    /// The last error, shown until the next successful action.
    error: Option<String>,
    camera: Camera,
    /// Top-left corner of the canvas on screen, as of the last frame.
    canvas_origin: Option<[f32; 2]>,
    /// Frame the whole scene at the next frame.
    fit_pending: bool,
    /// Centre the camera on this subject at the next frame.
    focus: Option<Subject>,
    /// Zoom by this factor about the centre of the canvas at the next frame.
    zoom_request: Option<f32>,
    /// Frame the selection at the next frame.
    frame_selection: bool,
    /// The pushbutton held down on the canvas, released with the pointer.
    held: Option<DeviceId>,
    /// The action waiting for an answer about unsaved changes.
    confirm: Option<Pending>,
    /// The user agreed to close the window.
    allow_close: bool,
    /// Extension libraries loaded in this window, in loading order. Every
    /// project opened here compiles against the built-in catalog with them.
    libraries: Vec<Library>,
    /// Files a browser has finished reading, opened at the next frame.
    #[cfg(target_arch = "wasm32")]
    inbox: crate::web::Inbox,
    /// Close the window at the next frame.
    close_now: bool,
    /// The window title last sent.
    title: String,
    /// The colours and sizes of the window have been given to egui.
    styled: bool,
    /// The grid dots are hidden (edit mode only).
    grid_off: bool,
    /// The wire tool does not snap to pins, junctions and wires.
    snap_off: bool,
    /// The list of diagnostics under the drawing is collapsed.
    diagnostics_hidden: bool,
    /// The grid point under the pointer, as of the last frame.
    cursor: Option<GridPoint>,
    /// Frame this sheet at the next frame.
    fit_sheet: Option<SheetId>,
    /// The window that lists the keys is open.
    show_keys: bool,
    /// The window about the program is open.
    show_about: bool,
}

impl DesktopApp {
    /// A window, with the project at `path` open when one is given.
    #[must_use]
    pub fn new(path: Option<&Path>) -> Self {
        let mut app = Self::default();
        if let Some(path) = path {
            app.open_path(path);
        }
        app
    }

    /// Open a project file. On failure the current project stays open and
    /// the error is shown.
    pub fn open_path(&mut self, path: &Path) {
        self.path_input = path.display().to_string();
        match Session::open(path) {
            Ok(session) => self.replace(session),
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    /// Open a project from its text; `origin` names it in messages.
    pub fn open_text(&mut self, text: &str, origin: &str) {
        match Session::load(text, Some(PathBuf::from(origin))) {
            Ok(session) => self.replace(session),
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    /// Start a new, empty project in edit mode, asking first when the open
    /// project has unsaved changes.
    pub fn new_project(&mut self) {
        self.guard(Pending::New);
    }

    /// Open a file, asking first when the open project has unsaved changes.
    pub fn request_open(&mut self, path: PathBuf) {
        self.guard(Pending::Open(path));
    }

    /// Whether the window may close now. With unsaved changes it may not:
    /// the user is asked to save, discard or cancel.
    pub fn request_close(&mut self) -> bool {
        if self.allow_close || !self.is_dirty() {
            return true;
        }
        self.confirm = Some(Pending::Close);
        false
    }

    /// Whether the open project has unsaved changes.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.session.as_ref().is_some_and(Session::is_dirty)
    }

    /// Whether a question about unsaved changes is on screen.
    #[must_use]
    pub const fn is_asking(&self) -> bool {
        self.confirm.is_some()
    }

    /// Whether the grid dots are shown while editing.
    #[must_use]
    pub const fn grid_shown(&self) -> bool {
        !self.grid_off
    }

    /// Whether the wire tool snaps to pins, junctions and wires.
    #[must_use]
    pub const fn snapping(&self) -> bool {
        !self.snap_off
    }

    /// Whether the list of problems is shown under the drawing.
    #[must_use]
    pub const fn problems_shown(&self) -> bool {
        !self.diagnostics_hidden
    }

    /// The grid point under the pointer, as of the last frame.
    #[must_use]
    pub const fn cursor(&self) -> Option<GridPoint> {
        self.cursor
    }

    /// Save the project to its file; a project never saved asks where.
    /// Returns whether it was saved.
    pub fn save(&mut self) -> bool {
        let Some(session) = &mut self.session else {
            return false;
        };
        match session.save() {
            Ok(()) => {
                self.error = None;
                true
            }
            Err(SaveError::NoPath) => self.save_as_dialog(),
            Err(error) => {
                self.error = Some(error.to_string());
                false
            }
        }
    }

    /// Save the project to `path` and make it its file. Returns whether it
    /// was saved.
    pub fn save_to(&mut self, path: PathBuf) -> bool {
        let Some(session) = &mut self.session else {
            return false;
        };
        self.path_input = path.display().to_string();
        match session.save_as(path) {
            Ok(()) => {
                self.error = None;
                true
            }
            Err(error) => {
                self.error = Some(error.to_string());
                false
            }
        }
    }

    /// Export the drawing. While simulating the current tick is shown in
    /// colour; while editing the document is drawn at rest for printing.
    fn export_dialog(&mut self, extension: &str) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let with_state = self.mode == Mode::Simulate;
        #[cfg(target_arch = "wasm32")]
        {
            let name = format!("{}.{extension}", session.document().project.name);
            let downloaded = if extension == "pdf" {
                crate::web::download(&name, &session.pdf(with_state), "application/pdf")
            } else {
                crate::web::download(&name, session.svg(with_state).as_bytes(), "image/svg+xml")
            };
            if let Err(error) = downloaded {
                self.error = Some(error);
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(path) = dialogs::choose_export_path(&session.document().project.name, extension)
            && let Err(error) = session.export(&path, with_state)
        {
            self.error = Some(error.to_string());
        }
    }

    /// Ask for library files and load them, stopping at the first error.
    fn library_dialog(&mut self) {
        for path in dialogs::pick_libraries() {
            let origin = path.display().to_string();
            let loaded = crate::files::read_text(&path, MAX_LIBRARY_BYTES)
                .map_err(|error| error.to_string())
                .and_then(|text| self.add_library_text(&text, &origin));
            if let Err(error) = loaded {
                self.error = Some(error);
                return;
            }
        }
    }

    fn save_as_dialog(&mut self) -> bool {
        let name = self.session.as_ref().map_or_else(
            || "project".to_owned(),
            |s| s.document().project.name.clone(),
        );
        #[cfg(target_arch = "wasm32")]
        {
            // A browser cannot write to disk: the project is downloaded.
            let Some(session) = self.session.as_mut() else {
                return false;
            };
            let downloaded = session
                .text()
                .map_err(|error| error.to_string())
                .and_then(|text| {
                    crate::web::download(
                        &format!("{name}.{}", dialogs::PROJECT_EXTENSION),
                        text.as_bytes(),
                        "application/json",
                    )
                });
            match downloaded {
                Ok(()) => {
                    session.mark_saved();
                    true
                }
                Err(error) => {
                    self.error = Some(error);
                    false
                }
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        match dialogs::choose_save_path(&name) {
            Some(path) => self.save_to(path),
            None => false,
        }
    }

    /// Run an action now, or ask first when it would drop unsaved changes.
    fn guard(&mut self, action: Pending) {
        if self.is_dirty() {
            self.confirm = Some(action);
        } else {
            self.perform(action);
        }
    }

    fn perform(&mut self, action: Pending) {
        match action {
            Pending::New => {
                self.replace(Session::untitled());
                self.path_input.clear();
                self.set_mode(Mode::Edit);
            }
            Pending::Open(path) => self.open_path(&path),
            Pending::OpenDialog => {
                if let Some(path) = dialogs::pick_project() {
                    self.open_path(&path);
                }
            }
            Pending::Close => {
                self.allow_close = true;
                self.close_now = true;
            }
        }
    }

    /// The question about unsaved changes, while one is pending.
    fn confirm_dialog(&mut self, ui: &Ui) {
        let Some(pending) = self.confirm.clone() else {
            return;
        };
        let mut answer = None;
        let modal = egui::Modal::new(egui::Id::unique("unsaved changes")).show(ui.ctx(), |ui| {
            ui.heading("Unsaved changes");
            ui.label("The project has changes that are not saved.");
            ui.horizontal(|ui| {
                if ui.button("Save and continue").clicked() {
                    answer = Some("save");
                }
                if ui.button("Discard changes").clicked() {
                    answer = Some("discard");
                }
                if ui.button("Cancel").clicked() {
                    answer = Some("cancel");
                }
            });
        });
        match answer {
            Some("save") => {
                if self.save() {
                    self.confirm = None;
                    self.perform(pending);
                }
            }
            Some("discard") => {
                self.confirm = None;
                self.perform(pending);
            }
            Some(_) => self.confirm = None,
            None if modal.should_close() => self.confirm = None,
            None => {}
        }
    }

    fn replace(&mut self, mut session: Session) {
        if !self.libraries.is_empty() {
            match self.catalog() {
                Ok(catalog) => session.set_catalog(catalog),
                Err(error) => self.error = Some(error.to_string()),
            }
        }
        self.session = Some(session);
        self.error = None;
        self.editor = Editor::new();
        self.inspector.clear();
        self.focus = None;
        self.held = None;
        self.fit_pending = true;
    }

    /// Open a dropped file: a library (`.oclib`) is loaded into the window,
    /// anything else is opened as a project. A browser hands over files
    /// asynchronously; the web entry point reads them and calls
    /// [`DesktopApp::open_text`] or [`DesktopApp::add_library_text`].
    fn open_dropped(&mut self, file: &egui::DroppedFileHandle) {
        #[cfg(target_arch = "wasm32")]
        crate::web::read_dropped(file.clone(), self.inbox.clone());
        #[cfg(not(target_arch = "wasm32"))]
        self.open_dropped_path(file.path());
    }

    /// Open a file dropped from disk.
    #[cfg(not(target_arch = "wasm32"))]
    fn open_dropped_path(&mut self, path: &Path) {
        let path = path.to_path_buf();
        let is_library = path
            .extension()
            .is_some_and(|extension| extension == dialogs::LIBRARY_EXTENSION);
        if is_library {
            let origin = path.display().to_string();
            let loaded = crate::files::read_text(&path, MAX_LIBRARY_BYTES)
                .map_err(|error| error.to_string())
                .and_then(|text| self.add_library_text(&text, &origin));
            if let Err(error) = loaded {
                self.error = Some(error);
            }
        } else {
            self.open_path(&path);
        }
    }

    /// Open the files the browser has finished reading.
    #[cfg(target_arch = "wasm32")]
    fn open_inbox(&mut self) {
        let files: Vec<(String, Vec<u8>)> = self.inbox.borrow_mut().drain(..).collect();
        for (name, bytes) in files {
            let Ok(text) = String::from_utf8(bytes) else {
                self.error = Some(format!("{name} is not UTF-8 text"));
                continue;
            };
            let is_library = Path::new(&name)
                .extension()
                .is_some_and(|extension| extension == dialogs::LIBRARY_EXTENSION);
            if is_library {
                if let Err(error) = self.add_library_text(&text, &name) {
                    self.error = Some(error);
                }
            } else {
                self.open_text(&text, &name);
            }
        }
    }

    /// The built-in catalog with the libraries of this window.
    fn catalog(&self) -> Result<ComponentCatalog, CatalogError> {
        self.libraries
            .iter()
            .cloned()
            .try_fold(ComponentCatalog::builtin(), |catalog, library| {
                catalog.with_library(library)
            })
    }

    /// Load an extension library from its text (ADR-0008) and recompile the
    /// open project with it. `origin` names the file in messages.
    ///
    /// # Errors
    ///
    /// Returns a message when the text is not a library or the library
    /// cannot be added (taken namespace, inconsistent model, bad artwork);
    /// nothing changes then.
    pub fn add_library_text(&mut self, text: &str, origin: &str) -> Result<(), String> {
        let library = circuit_project::library::from_str(text)
            .map_err(|error| format!("{origin}: {error}"))?;
        let catalog = self
            .catalog()
            .and_then(|catalog| catalog.with_library(library.clone()))
            .map_err(|error| format!("{origin}: {error}"))?;
        self.libraries.push(library);
        if let Some(session) = self.session.as_mut() {
            session.set_catalog(catalog);
        }
        Ok(())
    }

    /// The libraries loaded in this window.
    #[must_use]
    pub fn libraries(&self) -> &[Library] {
        &self.libraries
    }

    /// The open project.
    #[must_use]
    pub const fn session(&self) -> Option<&Session> {
        self.session.as_ref()
    }

    /// The error shown to the user, if any.
    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// The selected elements.
    #[must_use]
    pub const fn selection(&self) -> &BTreeSet<Subject> {
        self.editor.selection()
    }

    /// The current mode.
    #[must_use]
    pub const fn mode(&self) -> Mode {
        self.mode
    }

    /// Switch between simulating and editing. Editing stops and clears the
    /// simulation; leaving it cancels the gesture in progress.
    pub fn set_mode(&mut self, mode: Mode) {
        if mode == self.mode {
            return;
        }
        self.mode = mode;
        self.held = None;
        match mode {
            Mode::Edit => {
                if let Some(session) = &mut self.session {
                    session.reset();
                }
            }
            Mode::Simulate => self.editor.set_tool(Tool::Select),
        }
    }

    /// The screen position of a grid point, as of the last frame drawn; for
    /// automation and tests, which drive the canvas with pointer events.
    #[must_use]
    pub fn grid_to_screen(&self, point: GridPoint) -> Option<[f32; 2]> {
        let origin = self.canvas_origin?;
        let [x, y] = self.camera.to_screen(ScenePoint::from_grid(point));
        Some([origin[0] + x, origin[1] + y])
    }

    /// Draw the whole window into `ui`.
    pub fn show(&mut self, ui: &mut Ui) {
        if ui.input(|input| input.viewport().close_requested()) && !self.request_close() {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        if std::mem::take(&mut self.close_now) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        let title = self.session.as_ref().map_or_else(
            || "OpenCircuit".to_owned(),
            |session| {
                let marker = if session.is_dirty() { " *" } else { "" };
                format!("{}{marker} — OpenCircuit", session.document().project.name)
            },
        );
        if title != self.title {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
        let save_as = ui.input_mut(|input| {
            input.consume_shortcut(&KeyboardShortcut::new(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::S,
            ))
        });
        let save = ui.input_mut(|input| {
            input.consume_shortcut(&KeyboardShortcut::new(
                egui::Modifiers::COMMAND,
                egui::Key::S,
            ))
        });
        if save_as {
            self.save_as_dialog();
        } else if save {
            self.save();
        }
        self.confirm_dialog(ui);

        let dropped = ui.input(|input| input.raw.dropped_files.clone());
        for file in dropped {
            self.open_dropped(&file);
        }
        #[cfg(target_arch = "wasm32")]
        self.open_inbox();

        // One `step_n` and one snapshot per frame, before anything is drawn.
        let frame_us = SimClock::frame_us(ui.input(|input| input.stable_dt));
        if let Some(session) = &mut self.session {
            session.advance(frame_us);
            if session.is_running() {
                ui.ctx().request_repaint();
            }
        }
        if self.mode == Mode::Edit {
            self.shortcuts(ui);
        }
        self.view_keys(ui);

        if !self.styled {
            theme::apply_style(ui.ctx());
            self.styled = true;
        }
        let tokens = Tokens::of(ui);
        self.menu_bar(ui);
        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        if self.session.is_some() {
            egui::Panel::top("document")
                .frame(
                    egui::Frame::new()
                        .fill(tokens.chrome_dark)
                        .inner_margin(egui::Margin::symmetric(8, 3)),
                )
                .show(ui, |ui| self.document_tab(ui));
        }
        egui::Panel::bottom("status")
            .frame(
                egui::Frame::new()
                    .fill(tokens.chrome_dark)
                    .inner_margin(egui::Margin::symmetric(8, 2)),
            )
            .show(ui, |ui| self.status_bar(ui));
        if !self.diagnostics_hidden || self.session.is_none() {
            egui::Panel::bottom("diagnostics")
                .resizable(true)
                .default_size(140.0)
                .show(ui, |ui| self.diagnostics(ui));
        }
        self.help_windows(ui);
        match self.mode {
            Mode::Edit if self.session.is_some() => {
                egui::Panel::left("palette")
                    .resizable(true)
                    .default_size(230.0)
                    .show(ui, |ui| {
                        if let Some(session) = &self.session {
                            palette::show(ui, session.catalog(), &mut self.editor);
                        }
                    });
                egui::Panel::right("inspector")
                    .resizable(true)
                    .default_size(280.0)
                    .show(ui, |ui| self.inspector(ui));
            }
            Mode::Simulate if self.session.as_ref().is_some_and(Session::can_simulate) => {
                egui::Panel::right("operator")
                    .resizable(true)
                    .default_size(260.0)
                    .show(ui, |ui| self.operator(ui));
            }
            Mode::Edit | Mode::Simulate => {}
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(tokens.canvas))
            .show(ui, |ui| self.canvas(ui));
    }

    /// Undo, redo and the editor keys, unless a text field has the keyboard.
    fn shortcuts(&mut self, ui: &mut Ui) {
        if ui.ctx().egui_wants_keyboard_input() {
            return;
        }
        let pressed = |ui: &mut Ui, modifiers: egui::Modifiers, key: egui::Key| {
            ui.input_mut(|input| input.consume_shortcut(&KeyboardShortcut::new(modifiers, key)))
        };
        // The most specific shortcut first: Ctrl+Shift+Z would also match
        // Ctrl+Z.
        let shift_command = egui::Modifiers::COMMAND | egui::Modifiers::SHIFT;
        if pressed(ui, shift_command, egui::Key::Z)
            || pressed(ui, egui::Modifiers::COMMAND, egui::Key::Y)
        {
            self.redo();
        } else if pressed(ui, egui::Modifiers::COMMAND, egui::Key::Z) {
            self.undo();
        }
        if pressed(ui, egui::Modifiers::NONE, egui::Key::W) {
            self.editor.set_tool(Tool::Wire);
        }
        let mut actions = Vec::new();
        if pressed(ui, egui::Modifiers::COMMAND, egui::Key::A) {
            actions.push(Key::SelectAll);
        }
        // The window system may turn Ctrl+C and Ctrl+V into clipboard events
        // instead of key presses; either one counts, once. The copy stays in
        // the editor: the system clipboard is not used.
        let clipboard_event = |ui: &mut Ui, wanted: fn(&egui::Event) -> bool| {
            ui.input_mut(|input| {
                let found = input.events.iter().any(wanted);
                input.events.retain(|event| !wanted(event));
                found
            })
        };
        let copy = clipboard_event(ui, |event| matches!(event, egui::Event::Copy));
        if pressed(ui, egui::Modifiers::COMMAND, egui::Key::C) || copy {
            actions.push(Key::Copy);
        }
        let paste = clipboard_event(ui, |event| matches!(event, egui::Event::Paste(_)));
        if pressed(ui, egui::Modifiers::COMMAND, egui::Key::V) || paste {
            actions.push(Key::Paste);
        }
        if pressed(ui, egui::Modifiers::COMMAND, egui::Key::D) {
            actions.push(Key::Duplicate);
        }
        // Shift moves five grid units; the plain arrow one.
        for (modifiers, step) in [(egui::Modifiers::SHIFT, 5), (egui::Modifiers::NONE, 1)] {
            for (key, dx, dy) in [
                (egui::Key::ArrowLeft, -1, 0),
                (egui::Key::ArrowRight, 1, 0),
                (egui::Key::ArrowUp, 0, -1),
                (egui::Key::ArrowDown, 0, 1),
            ] {
                if pressed(ui, modifiers, key) {
                    actions.push(Key::Nudge {
                        dx: dx * step,
                        dy: dy * step,
                    });
                }
            }
        }
        for (key, action) in [
            (egui::Key::Delete, Key::Delete),
            (egui::Key::Backspace, Key::Delete),
            (egui::Key::R, Key::Rotate),
            (egui::Key::Escape, Key::Escape),
            (egui::Key::Space, Key::FlipBend),
        ] {
            if pressed(ui, egui::Modifiers::NONE, key) {
                actions.push(action);
            }
        }
        for action in actions {
            if let Some(session) = &self.session {
                let outcome = self
                    .editor
                    .key(session.document(), session.catalog(), action);
                self.apply(outcome);
            }
        }
    }

    /// Zoom and framing keys, in both modes, unless a text field has the
    /// keyboard: `+` and `-` zoom about the centre, `0` frames the whole
    /// drawing and `F` the selection.
    fn view_keys(&mut self, ui: &mut Ui) {
        if ui.ctx().egui_wants_keyboard_input() {
            return;
        }
        let pressed = |ui: &mut Ui, key: egui::Key| {
            ui.input_mut(|input| {
                input.consume_shortcut(&KeyboardShortcut::new(egui::Modifiers::NONE, key))
            })
        };
        if pressed(ui, egui::Key::Plus) || pressed(ui, egui::Key::Equals) {
            self.zoom_request = Some(KEY_ZOOM);
        }
        if pressed(ui, egui::Key::Minus) {
            self.zoom_request = Some(1.0 / KEY_ZOOM);
        }
        if pressed(ui, egui::Key::Num0) {
            self.fit_pending = true;
        }
        if pressed(ui, egui::Key::F) && !self.editor.selection().is_empty() {
            self.frame_selection = true;
        }
    }

    /// Apply what the editor asked for and report a refusal.
    fn apply(&mut self, outcome: Outcome) {
        if let Some(error) = outcome.error {
            self.error = Some(error.to_string());
        }
        if let Some(tx) = outcome.commit {
            self.commit(Ok(tx));
        }
        if let Some(session) = &self.session {
            self.editor.retain_existing(session.document());
        }
    }

    /// Apply a requested change to the document.
    fn commit(&mut self, request: inspector::Request) {
        let Some(session) = &mut self.session else {
            return;
        };
        match request.and_then(|tx| session.edit(tx)) {
            Ok(()) => {
                self.error = None;
                self.inspector.clear();
            }
            Err(error) => self.error = Some(error.to_string()),
        }
        self.editor.retain_existing(session.document());
    }

    fn undo(&mut self) {
        if let Some(session) = &mut self.session {
            match session.undo() {
                Ok(_) => self.error = None,
                Err(error) => self.error = Some(error.to_string()),
            }
            self.inspector.clear();
            self.editor.retain_existing(session.document());
        }
    }

    fn redo(&mut self) {
        if let Some(session) = &mut self.session {
            match session.redo() {
                Ok(_) => self.error = None,
                Err(error) => self.error = Some(error.to_string()),
            }
            self.inspector.clear();
            self.editor.retain_existing(session.document());
        }
    }

    /// Apply an editor key as if it had been pressed, in edit mode.
    fn press(&mut self, key: Key) {
        if self.mode != Mode::Edit {
            return;
        }
        if let Some(session) = &self.session {
            let outcome = self.editor.key(session.document(), session.catalog(), key);
            self.apply(outcome);
        }
    }

    /// The menu bar: every action of the window, by name.
    fn menu_bar(&mut self, ui: &mut Ui) {
        egui::Panel::top("menubar").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                let has_session = self.session.is_some();
                let editing = has_session && self.mode == Mode::Edit;
                ui.menu_button("File", |ui| {
                    if ui.button("New project").clicked() {
                        ui.close();
                        self.new_project();
                    }
                    if ui.button("Open project…").clicked() {
                        ui.close();
                        self.guard(Pending::OpenDialog);
                    }
                    ui.separator();
                    if ui
                        .add_enabled(has_session, egui::Button::new("Save project"))
                        .clicked()
                    {
                        ui.close();
                        self.save();
                    }
                    if ui
                        .add_enabled(has_session, egui::Button::new("Save project as…"))
                        .clicked()
                    {
                        ui.close();
                        self.save_as_dialog();
                    }
                    ui.separator();
                    if ui.button("Load library…").clicked() {
                        ui.close();
                        self.library_dialog();
                    }
                    ui.add_enabled_ui(has_session, |ui| {
                        ui.menu_button("Export drawing", |ui| {
                            for (label, extension) in [("As SVG…", "svg"), ("As PDF…", "pdf")] {
                                if ui.button(label).clicked() {
                                    ui.close();
                                    self.export_dialog(extension);
                                }
                            }
                        });
                    });
                });
                ui.menu_button("Modify", |ui| {
                    let (can_undo, can_redo) = self.session.as_ref().map_or((false, false), |s| {
                        (s.history().can_undo(), s.history().can_redo())
                    });
                    if ui
                        .add_enabled(editing && can_undo, egui::Button::new("Undo last change"))
                        .clicked()
                    {
                        ui.close();
                        self.undo();
                    }
                    if ui
                        .add_enabled(editing && can_redo, egui::Button::new("Redo change"))
                        .clicked()
                    {
                        ui.close();
                        self.redo();
                    }
                    ui.separator();
                    for (label, key) in [
                        ("Select everything", Key::SelectAll),
                        ("Copy selection", Key::Copy),
                        ("Paste", Key::Paste),
                        ("Duplicate selection", Key::Duplicate),
                        ("Rotate selection", Key::Rotate),
                        ("Delete selection", Key::Delete),
                    ] {
                        if ui.add_enabled(editing, egui::Button::new(label)).clicked() {
                            ui.close();
                            self.press(key);
                        }
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui
                        .add_enabled(has_session, egui::Button::new("Zoom to the whole drawing"))
                        .clicked()
                    {
                        ui.close();
                        self.fit_pending = true;
                    }
                    if ui
                        .add_enabled(has_session, egui::Button::new("Zoom in"))
                        .clicked()
                    {
                        self.zoom_request = Some(KEY_ZOOM);
                    }
                    if ui
                        .add_enabled(has_session, egui::Button::new("Zoom out"))
                        .clicked()
                    {
                        self.zoom_request = Some(1.0 / KEY_ZOOM);
                    }
                    ui.separator();
                    let mut grid = !self.grid_off;
                    if ui.checkbox(&mut grid, "Show the grid").changed() {
                        self.grid_off = !grid;
                    }
                    let mut snap = !self.snap_off;
                    if ui.checkbox(&mut snap, "Snap wires to pins").changed() {
                        self.snap_off = !snap;
                    }
                    let mut problems = !self.diagnostics_hidden;
                    if ui
                        .checkbox(&mut problems, "Show the problems list")
                        .changed()
                    {
                        self.diagnostics_hidden = !problems;
                    }
                });
                ui.menu_button("Run", |ui| {
                    let mut mode = self.mode;
                    ui.add_enabled_ui(has_session, |ui| {
                        ui.radio_value(&mut mode, Mode::Simulate, "Simulation mode");
                        ui.radio_value(&mut mode, Mode::Edit, "Drawing mode");
                    });
                    self.set_mode(mode);
                    ui.separator();
                    let mode = self.mode;
                    if let Some(session) = &mut self.session {
                        let simulating = mode == Mode::Simulate && session.can_simulate();
                        let running = session.is_running();
                        let label = if running {
                            "Pause simulation"
                        } else {
                            "Start simulation"
                        };
                        if ui
                            .add_enabled(simulating, egui::Button::new(label))
                            .clicked()
                        {
                            ui.close();
                            if running {
                                session.pause();
                            } else {
                                session.play();
                            }
                        }
                        if ui
                            .add_enabled(simulating, egui::Button::new("Advance one tick"))
                            .clicked()
                        {
                            session.step();
                        }
                        if ui
                            .add_enabled(simulating, egui::Button::new("Restart simulation"))
                            .clicked()
                        {
                            ui.close();
                            session.reset();
                        }
                    }
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("Keyboard shortcuts").clicked() {
                        ui.close();
                        self.show_keys = true;
                    }
                    if ui.button("About OpenCircuit").clicked() {
                        ui.close();
                        self.show_about = true;
                    }
                });
            });
        });
    }

    /// The windows of the Help menu.
    fn help_windows(&mut self, ui: &Ui) {
        let ctx = ui.ctx().clone();
        egui::Window::new("Keyboard shortcuts")
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut self.show_keys)
            .collapsible(false)
            .resizable(false)
            .show(&ctx, |ui| {
                egui::Grid::new("keys").num_columns(2).show(ui, |ui| {
                    for (keys, action) in palette::KEYS {
                        ui.monospace(*keys);
                        ui.label(*action);
                        ui.end_row();
                    }
                });
            });
        egui::Window::new("About OpenCircuit")
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut self.show_about)
            .collapsible(false)
            .resizable(false)
            .show(&ctx, |ui| {
                ui.strong(format!("OpenCircuit {}", env!("CARGO_PKG_VERSION")));
                ui.label("An open-source editor and simulator for industrial control circuits.");
                ui.label("Licensed under MIT or Apache-2.0.");
            });
    }

    /// The quick-access bar: files, undo and redo, the path of the project
    /// and the mode.
    fn toolbar(&mut self, ui: &mut Ui) {
        // Wrapped, so that a narrow window shows every control on a second
        // row instead of cutting the last ones off.
        ui.horizontal_wrapped(|ui| {
            let has_session = self.session.is_some();
            if icons::button(ui, Icon::New, "New", false).clicked() {
                self.new_project();
            }
            if icons::button(ui, Icon::Open, "Open…", false).clicked() {
                self.guard(Pending::OpenDialog);
            }
            ui.add_enabled_ui(has_session, |ui| {
                if icons::button(ui, Icon::Save, "Save", false).clicked() {
                    self.save();
                }
                if icons::button(ui, Icon::SaveAs, "Save as…", false).clicked() {
                    self.save_as_dialog();
                }
            });
            if icons::button_hint(
                ui,
                Icon::Library,
                "Library…",
                "Load an extension library (.oclib): more components for this window",
                false,
            )
            .clicked()
            {
                self.library_dialog();
            }
            ui.add_enabled_ui(has_session, |ui| {
                ui.menu_button("Export", |ui| {
                    ui.label("While simulating: the current tick in colour.");
                    ui.label("While editing: the drawing at rest, black on white.");
                    for (label, extension) in [("SVG…", "svg"), ("PDF…", "pdf")] {
                        if ui.button(label).clicked() {
                            ui.close();
                            self.export_dialog(extension);
                        }
                    }
                });
            });
            ui.separator();
            let name = ui.label("Path");
            let field = ui
                .add(
                    egui::TextEdit::singleline(&mut self.path_input)
                        .hint_text("path to a .ocircuit file")
                        .desired_width(260.0),
                )
                .labelled_by(name.id);
            let entered =
                field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if ui.button("Open").clicked() || entered {
                let path = PathBuf::from(self.path_input.trim());
                self.request_open(path);
            }
            if ui
                .add_enabled(has_session, egui::Button::new("Save here"))
                .clicked()
            {
                let path = PathBuf::from(self.path_input.trim());
                self.save_to(path);
            }
            if has_session {
                if icons::button(ui, Icon::Fit, "Fit", false).clicked() {
                    self.fit_pending = true;
                }
                ui.separator();
                let mut mode = self.mode;
                ui.selectable_value(&mut mode, Mode::Simulate, "Simulate");
                ui.selectable_value(&mut mode, Mode::Edit, "Edit");
                self.set_mode(mode);
            }
        });
        match self.mode {
            Mode::Simulate => {
                if let Some(session) = &mut self.session
                    && session.can_simulate()
                {
                    ui.horizontal(|ui| simulation_controls(ui, session));
                }
            }
            Mode::Edit => {
                if self.session.is_some() {
                    ui.horizontal(|ui| self.edit_controls(ui));
                }
            }
        }
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
    }

    /// The tab of the open project: its name, and whether it has unsaved
    /// changes.
    fn document_tab(&mut self, ui: &mut Ui) {
        let Some(session) = &self.session else {
            return;
        };
        let tokens = Tokens::of(ui);
        let mut name = session.document().project.name.clone();
        if session.is_dirty() {
            name.push_str(" (unsaved)");
        }
        egui::Frame::new()
            .fill(tokens.tab_active)
            .corner_radius(egui::CornerRadius {
                nw: 5,
                ne: 5,
                sw: 0,
                se: 0,
            })
            .inner_margin(egui::Margin::symmetric(12, 3))
            .show(ui, |ui| {
                ui.label(RichText::new(name).strong());
            });
    }

    /// The status bar: the position of the pointer, one tab per sheet, and
    /// the switches of the view.
    fn status_bar(&mut self, ui: &mut Ui) {
        let tokens = Tokens::of(ui);
        ui.horizontal(|ui| {
            let position = self.cursor.map_or_else(
                || "      ,      ".to_owned(),
                |point| format!("{:>5}, {:<5}", point.x, point.y),
            );
            ui.label(RichText::new(position).monospace().color(tokens.text_dim));
            if let Some(session) = &self.session {
                ui.separator();
                let mut sheets: Vec<(SheetId, u32, String)> = session
                    .document()
                    .sheets
                    .iter()
                    .map(|sheet| (sheet.id, sheet.number, sheet.title.clone()))
                    .collect();
                sheets.sort_by_key(|(_, number, _)| *number);
                for (id, number, title) in sheets {
                    let label = format!("{number}: {title}");
                    if ui
                        .selectable_label(false, label)
                        .on_hover_text("Frame this sheet")
                        .clicked()
                    {
                        self.fit_sheet = Some(id);
                    }
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let problems = self.session.as_ref().map_or(0, |session| {
                    session
                        .diagnostics()
                        .len()
                        .saturating_add(session.snapshot().map_or(0, |s| s.faults.len()))
                });
                if icons::button_hint(
                    ui,
                    Icon::Warning,
                    "Problems",
                    "Show or hide the list of problems",
                    !self.diagnostics_hidden,
                )
                .clicked()
                {
                    self.diagnostics_hidden = !self.diagnostics_hidden;
                }
                ui.label(RichText::new(problems.to_string()).color(tokens.text_dim));
                ui.separator();
                if icons::button_hint(
                    ui,
                    Icon::Snap,
                    "Snap",
                    "Snap wires to pins, junctions and wires",
                    !self.snap_off,
                )
                .clicked()
                {
                    self.snap_off = !self.snap_off;
                }
                if icons::button_hint(
                    ui,
                    Icon::Grid,
                    "Grid",
                    "Show the grid (edit mode)",
                    !self.grid_off,
                )
                .clicked()
                {
                    self.grid_off = !self.grid_off;
                }
            });
        });
    }

    /// Zoom buttons over the top right corner of the drawing.
    fn navigation(&mut self, ui: &Ui, origin: [f32; 2], size: [f32; 2]) {
        let tokens = Tokens::of(ui);
        let at = egui::pos2(origin[0] + size[0] - 38.0, origin[1] + 10.0);
        egui::Area::new(ui.scope_id().with(egui::IdSalt::new("navigation")))
            .order(egui::Order::Foreground)
            .fixed_pos(at)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(tokens.chrome.gamma_multiply(0.92))
                    .corner_radius(6)
                    .inner_margin(egui::Margin::same(2))
                    .show(ui, |ui| {
                        ui.vertical(|ui| {
                            if icons::button(ui, Icon::ZoomIn, "Zoom in", false).clicked() {
                                self.zoom_request = Some(KEY_ZOOM);
                            }
                            if icons::button(ui, Icon::ZoomOut, "Zoom out", false).clicked() {
                                self.zoom_request = Some(1.0 / KEY_ZOOM);
                            }
                            if icons::button(ui, Icon::Fit, "Zoom extents", false).clicked() {
                                self.fit_pending = true;
                            }
                        });
                    });
            });
    }

    fn edit_controls(&mut self, ui: &mut Ui) {
        let (can_undo, can_redo, undo_label, redo_label) = match &self.session {
            Some(session) => {
                let history = session.history();
                (
                    history.can_undo(),
                    history.can_redo(),
                    history.undo_label().map(str::to_owned),
                    history.redo_label().map(str::to_owned),
                )
            }
            None => (false, false, None, None),
        };
        ui.add_enabled_ui(can_undo, |ui| {
            let hint =
                undo_label.map_or_else(|| "Undo".to_owned(), |label| format!("Undo: {label}"));
            if icons::button_hint(ui, Icon::Undo, "Undo", &hint, false).clicked() {
                self.undo();
            }
        });
        ui.add_enabled_ui(can_redo, |ui| {
            let hint =
                redo_label.map_or_else(|| "Redo".to_owned(), |label| format!("Redo: {label}"));
            if icons::button_hint(ui, Icon::Redo, "Redo", &hint, false).clicked() {
                self.redo();
            }
        });
        ui.separator();
        let hint = match self.editor.tool() {
            Tool::Select => "Click to select, drag to move, R to rotate, Delete to remove.",
            Tool::Place { .. } => "Click to place, R to rotate, Esc to stop placing.",
            Tool::Wire => "Click a pin to start, empty space to bend, a pin or wire to finish.",
        };
        ui.label(hint);
    }

    fn inspector(&mut self, ui: &mut Ui) {
        let Some(session) = &self.session else {
            return;
        };
        let request = egui::ScrollArea::vertical()
            .show(ui, |ui| {
                inspector::show(ui, session, self.editor.selection(), &mut self.inspector)
            })
            .inner;
        if let Some(request) = request {
            self.commit(request);
        }
    }

    fn operator(&mut self, ui: &mut Ui) {
        let Some(session) = &mut self.session else {
            return;
        };
        ui.strong("Operator");
        let operable = session.operable();
        if operable.is_empty() {
            ui.label("No pushbutton, switch or thermal relay.");
        }
        for device in operable {
            let engaged = session.engaged(device.device, device.control);
            let mut label = control_label(&device.reference, device.control, engaged);
            if session.is_pending(device.device) {
                label.push_str(" (next tick)");
            }
            if ui.button(label).clicked() {
                session.operate(device.device, device.control);
            }
        }
        ui.separator();
        ui.strong("Events");
        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in session.events() {
                    ui.monospace(line);
                }
            });
    }

    fn diagnostics(&mut self, ui: &mut Ui) {
        let Some(session) = &self.session else {
            ui.label("No project open.");
            return;
        };
        let selected = self.selected_description(session);
        ui.horizontal(|ui| {
            ui.strong("Diagnostics");
            if let Some(selected) = selected {
                ui.separator();
                ui.label(selected);
            }
        });
        let diagnostics = session.diagnostics();
        let faults = session.snapshot().map_or(&[][..], |s| s.faults.as_slice());
        if diagnostics.is_empty() && faults.is_empty() {
            ui.label("No diagnostics: the project compiles.");
            return;
        }
        let mut chosen = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (title, list) in [("Runtime faults", faults), ("Compilation", diagnostics)] {
                if list.is_empty() {
                    continue;
                }
                ui.label(RichText::new(title).italics());
                for diagnostic in list {
                    let text = format!(
                        "{} {} {}: {}",
                        diagnostic.severity(),
                        diagnostic.code(),
                        diagnostic.subject(),
                        diagnostic.message()
                    );
                    if ui.selectable_label(false, text).clicked() {
                        chosen = Some(diagnostic.subject().clone());
                    }
                }
            }
        });
        if let Some(subject) = chosen {
            self.select(subject);
        }
    }

    /// A line describing the selected symbol, if exactly one is selected.
    fn selected_description(&self, session: &Session) -> Option<String> {
        let mut selected = self.editor.selection().iter();
        let (Some(Subject::Symbol(id)), None) = (selected.next(), selected.next()) else {
            return None;
        };
        let layout = LayoutIndex::new(session.document(), session.catalog());
        let symbol = layout.symbol(*id)?;
        let device = layout.device(symbol.device_id)?;
        Some(format!(
            "{} — {} ({} v{}), symbol `{}`",
            device.reference.0,
            symbol.id,
            device.model.id,
            device.model.version,
            symbol.symbol_key.0
        ))
    }

    fn select(&mut self, subject: Subject) {
        self.editor.set_selection(BTreeSet::from([subject.clone()]));
        self.focus = Some(subject);
    }

    /// The scene to draw: in edit mode the preview of the gesture in
    /// progress, with its guide, its selection box and the pin under the
    /// pointer.
    fn scene(&self, session: &Session) -> Scene {
        let options = SceneOptions {
            selected: self.editor.selection().clone(),
        };
        if self.mode == Mode::Simulate {
            return session.scene(&options);
        }
        let preview = self.editor.preview(session.document(), session.catalog());
        let shown = preview.document.as_ref().unwrap_or(session.document());
        let mut scene = build_scene(shown, session.catalog(), None, &options);
        let overlay = |scene: &mut Scene, shape: Shape| {
            if let Some(area) = shape.bounds() {
                scene.bounds = Some(scene.bounds.map_or(area, |b| b.union(&area)));
            }
            scene.items.push(SceneItem {
                shape,
                style: StyleKey::Selected,
                subject: None,
            });
        };
        if preview.guide.len() > 1 {
            overlay(
                &mut scene,
                Shape::Polyline(
                    preview
                        .guide
                        .iter()
                        .map(|p| ScenePoint::from_grid(*p))
                        .collect(),
                ),
            );
        }
        if let Some(area) = preview.selection_box {
            overlay(
                &mut scene,
                Shape::Rect(SceneRect::from_corners(
                    ScenePoint::from_grid(area.min),
                    ScenePoint::from_grid(area.max),
                )),
            );
        }
        let wiring = matches!(self.editor.tool(), Tool::Wire);
        if let Some(found) = self.snapped.as_ref().filter(|_| wiring) {
            for shape in snap_marker(found) {
                overlay(&mut scene, shape);
            }
        } else if let Some(Hit::Pin(pin)) = self.editor.hover() {
            let layout = LayoutIndex::new(session.document(), session.catalog());
            if let Some(at) = layout.pin_position(pin) {
                overlay(
                    &mut scene,
                    Shape::Circle {
                        center: ScenePoint::from_grid(at),
                        radius: PIN_RING,
                    },
                );
            }
        }
        scene
    }

    fn canvas(&mut self, ui: &mut Ui) {
        let Some(session) = &self.session else {
            ui.centered_and_justified(|ui| {
                ui.label(
                    "Open a project: pass its path on the command line, drop the file here, \
                     or type the path above.",
                );
            });
            return;
        };
        let scene = self.scene(session);
        let fit = std::mem::take(&mut self.fit_pending);
        let grid = self.mode == Mode::Edit && !self.grid_off;
        let events = canvas::show(ui, &scene, &mut self.camera, fit, grid);
        self.canvas_origin = Some(events.origin);
        self.cursor = events.hover.and_then(|at| self.camera.to_grid(at));

        if let Some(factor) = self.zoom_request.take() {
            let [width, height] = events.size;
            self.camera.zoom_at([width / 2.0, height / 2.0], factor);
        }
        if std::mem::take(&mut self.frame_selection) {
            let selected = SceneRect::around(
                self.editor
                    .selection()
                    .iter()
                    .filter_map(|subject| scene.subject_bounds(subject))
                    .flat_map(|rect| [rect.min, rect.max]),
            );
            if let Some(bounds) = selected {
                self.camera.fit(bounds, events.size, canvas::FIT_MARGIN);
            }
        }

        if let Some(subject) = self.focus.take()
            && let Some(bounds) = scene.subject_bounds(&subject)
        {
            self.camera.center_on(bounds, events.size);
        }
        if let Some(id) = self.fit_sheet.take()
            && let Some(frame) = session
                .document()
                .sheets
                .iter()
                .find(|sheet| sheet.id == id)
                .and_then(circuit_model::Sheet::frame)
        {
            let corners = SceneRect::from_corners(
                ScenePoint::from_grid(frame.min),
                ScenePoint::from_grid(frame.max),
            );
            self.camera.fit(corners, events.size, canvas::FIT_MARGIN);
        }
        self.navigation(ui, events.origin, events.size);
        match self.mode {
            Mode::Edit => self.edit_on_canvas(ui, events),
            Mode::Simulate => self.operate_on_canvas(events),
        }
    }

    /// Pass pointer events to the editor, in grid points.
    fn edit_on_canvas(&mut self, ui: &Ui, events: canvas::CanvasEvents) {
        let Some(session) = &self.session else {
            return;
        };
        let (doc, catalog) = (session.document(), session.catalog());
        let modifiers = Modifiers {
            shift: ui.input(|input| input.modifiers.shift),
        };
        let mut outcomes = Vec::new();
        // The wire tool snaps to what a wire can end on; the other tools work
        // on the nearest grid point, so a drag never jumps to a pin.
        let wiring = matches!(self.editor.tool(), Tool::Wire) && !self.snap_off;
        let radius = self
            .camera
            .to_scene_length(SNAP_POINTS)
            .map_or(0, |radius| radius.clamp(0, SNAP_MAX));
        let camera = &self.camera;
        let locate = |screen: [f32; 2]| -> Option<Snap> {
            if wiring {
                let at = camera.to_scene(screen)?;
                let pointer = Pointer {
                    x: at.x,
                    y: at.y,
                    per_grid: SUBUNITS_PER_GRID,
                };
                snap(doc, catalog, pointer, radius)
            } else {
                camera.to_grid(screen).map(|point| Snap {
                    point,
                    target: SnapTarget::Grid,
                })
            }
        };
        let hovered = events.hover.and_then(locate);
        if let Some(found) = &hovered {
            self.editor.pointer_move(doc, catalog, found.point);
        }
        self.snapped = hovered.filter(|_| wiring);
        if let Some(at) = events.pressed.and_then(locate).map(|found| found.point) {
            outcomes.push(self.editor.pointer_down(doc, catalog, at, modifiers));
        }
        if let Some(at) = events.released_at.and_then(locate).map(|found| found.point) {
            // A press and its release in one frame: the release applies to the
            // document as it is after the press.
            let pressed_commit = outcomes.iter().any(|o: &Outcome| o.commit.is_some());
            if pressed_commit {
                for outcome in std::mem::take(&mut outcomes) {
                    self.apply(outcome);
                }
                if let Some(session) = &self.session {
                    let up = self
                        .editor
                        .pointer_up(session.document(), session.catalog(), at);
                    outcomes.push(up);
                }
            } else {
                outcomes.push(self.editor.pointer_up(doc, catalog, at));
            }
        }
        for outcome in outcomes {
            self.apply(outcome);
        }
    }

    /// Select what is clicked and operate pushbuttons, switches and thermal
    /// relays.
    fn operate_on_canvas(&mut self, events: canvas::CanvasEvents) {
        let Some(session) = &mut self.session else {
            return;
        };
        let symbol_at = |session: &Session, camera: &Camera, at: [f32; 2]| {
            let layout = LayoutIndex::new(session.document(), session.catalog());
            camera
                .to_grid(at)
                .and_then(|point| layout.symbol_at(point))
                .and_then(|id| layout.symbol(id))
                .map(|symbol| (symbol.id, symbol.device_id))
        };
        if let Some(click) = events.clicked {
            let hit = symbol_at(session, &self.camera, click);
            self.editor.set_selection(
                hit.map(|(symbol, _)| Subject::Symbol(symbol))
                    .into_iter()
                    .collect(),
            );
        }
        // A pushbutton is held while the pointer is down; a switch or a
        // thermal relay changes on press.
        if let Some(pressed) = events.pressed
            && let Some((_, device)) = symbol_at(session, &self.camera, pressed)
            && let Some(control) = session.control_of(device)
        {
            if control == Control::Hold {
                session.queue(InputAction::Press(device));
                self.held = Some(device);
            } else {
                session.operate(device, control);
            }
        }
        if events.released
            && let Some(device) = self.held.take()
        {
            session.queue(InputAction::Release(device));
        }
    }
}

/// The text of the button of an operator control.
fn control_label(reference: &str, control: Control, engaged: bool) -> String {
    let verb = match (control, engaged) {
        (Control::Hold, false) => "press",
        (Control::Hold, true) => "release",
        (Control::Toggle, false) => "close",
        (Control::Toggle, true) => "open",
        (Control::Trip, false) => "trip",
        (Control::Trip, true) => "reset",
    };
    format!("{reference}: {verb}")
}

/// Play, pause, step, reset, fault reset, speed and time.
fn simulation_controls(ui: &mut Ui, session: &mut Session) {
    if session.is_running() {
        if icons::button(ui, Icon::Pause, "Pause", false).clicked() {
            session.pause();
        }
    } else {
        ui.add_enabled_ui(!session.is_faulted(), |ui| {
            if icons::button(ui, Icon::Play, "Play", false).clicked() {
                session.play();
            }
        });
    }
    if icons::button(ui, Icon::Step, "Step", false).clicked() {
        session.step();
    }
    if icons::button(ui, Icon::Reset, "Reset", false).clicked() {
        session.reset();
    }
    if ui
        .add_enabled(session.is_faulted(), egui::Button::new("Reset fault"))
        .clicked()
    {
        session.reset_fault();
    }
    let mut speed = session.clock().speed_index();
    egui::ComboBox::from_label("Speed")
        .selected_text(session.clock().speed().label())
        .show_ui(ui, |ui| {
            for (index, choice) in SPEEDS.iter().enumerate() {
                ui.selectable_value(&mut speed, index, choice.label());
            }
        });
    session.clock_mut().set_speed_index(speed);
    let elapsed_ms = session
        .next_tick()
        .saturating_mul(u64::from(session.tick_us()))
        / 1_000;
    ui.label(format!("tick {} · {elapsed_ms} ms", session.next_tick()));
    if let Some(message) = session.message() {
        ui.colored_label(ui.visuals().error_fg_color, message);
    }
}

impl eframe::App for DesktopApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}

/// The marker of a snap: a ring on a pin, a square on a junction and a cross
/// on a wire. Nothing marks a plain grid point.
fn snap_marker(found: &Snap) -> Vec<Shape> {
    let center = ScenePoint::from_grid(found.point);
    let r = PIN_RING;
    let at = |dx: i64, dy: i64| {
        ScenePoint::new(center.x.saturating_add(dx), center.y.saturating_add(dy))
    };
    match found.target {
        SnapTarget::Pin(_) => vec![Shape::Circle { center, radius: r }],
        SnapTarget::Junction(_) => vec![Shape::Rect(SceneRect::from_corners(at(-r, -r), at(r, r)))],
        SnapTarget::Wire(_) => vec![
            Shape::Line {
                from: at(-r, -r),
                to: at(r, r),
            },
            Shape::Line {
                from: at(-r, r),
                to: at(r, -r),
            },
        ],
        SnapTarget::Grid => Vec::new(),
    }
}
