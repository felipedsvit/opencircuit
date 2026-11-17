//! The window: a bar to open, edit and run a project, the canvas, the
//! palette and inspector when editing, the operator controls with the trace
//! when simulating, and the diagnostics.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use circuit_commands::commandline::{
    self, ModeRequest, Request, SimRequest, ToolRequest, ViewRequest,
};
use circuit_commands::designators::{self, Scope};
use circuit_commands::{ops, waivers};
use circuit_editor::{Editor, Hit, Key, Modifiers, Outcome, Pointer, Snap, SnapTarget, Tool, snap};
use circuit_model::library::{Library, MAX_LIBRARY_BYTES};
use circuit_model::{
    CatalogError, Category, ComponentCatalog, Control, DeviceId, Diagnostic, DiagnosticCode,
    GridPoint, InputAction, LayoutIndex, Severity, SheetId, StaticNets, Subject, WaiverIndex,
};
use circuit_renderer::{
    SUBUNITS_PER_GRID, Scene, SceneItem, SceneOptions, ScenePoint, SceneRect, Shape, StyleKey,
    build_scene,
};
use eframe::egui::{self, Align, KeyboardShortcut, Layout, RichText, Ui};

use crate::camera::{Camera, DEFAULT_ZOOM};
use crate::canvas;
use crate::clock::{SPEEDS, SimClock};
use crate::cmdline::CommandLine;
use crate::dialogs;
use crate::documentation;
use crate::i18n::{self, Language, tr, trf, trn};
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
    /// Start a new project from the template at this place of
    /// [`crate::templates::TEMPLATES`].
    Template(usize),
    /// Open a saved version as a new, unsaved project.
    #[cfg(not(target_arch = "wasm32"))]
    Restore(crate::versions::Version),
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

/// What a click in the list of problems asked for.
enum PanelAction {
    /// Select and show the subject of a diagnostic.
    Select(Subject),
    /// Connect what a `OC-STR-018` warning found touching.
    Connect(Subject, Vec<Subject>),
    /// Start typing the reason for waiving a diagnostic.
    StartWaive(String, Subject),
    /// Waive it with the reason typed.
    Waive,
    /// Do not waive it after all.
    KeepIt,
    /// Take a waiver away.
    Restore(String, Subject),
    /// Write the list as CSV.
    Save,
}

/// One diagnostic of the list: the line that selects its subject, as it always
/// was, and small buttons after it for what can be done about it.
fn problem_row(ui: &mut Ui, diagnostic: &Diagnostic, action: &mut Option<PanelAction>) {
    let text = format!(
        "{} {} {}: {}",
        diagnostic.severity(),
        diagnostic.code(),
        diagnostic.subject(),
        diagnostic.message()
    );
    ui.horizontal(|ui| {
        if ui.selectable_label(false, text).clicked() {
            *action = Some(PanelAction::Select(diagnostic.subject().clone()));
        }
        let name = format!("{} {}", diagnostic.code(), diagnostic.subject());
        let button = |ui: &mut Ui, text: &str, hint: &str| {
            let response = ui.small_button(text).on_hover_text(hint);
            let label = format!("{text} {name}");
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, true, label.as_str())
            });
            response
        };
        if diagnostic.code() == DiagnosticCode::LooseConnection
            && button(ui, tr("Connect"), tr("Join them with a wire")).clicked()
        {
            *action = Some(PanelAction::Connect(
                diagnostic.subject().clone(),
                diagnostic.related().to_vec(),
            ));
        }
        let waivable = diagnostic.severity() != Severity::Error
            && diagnostic.code() != DiagnosticCode::InvalidWaiver;
        if waivable && button(ui, tr("Ignore"), tr("Accept it, with a reason")).clicked() {
            *action = Some(PanelAction::StartWaive(
                diagnostic.code().as_str().to_owned(),
                diagnostic.subject().clone(),
            ));
        }
    });
}

/// The desktop application.
#[derive(Debug, Default)]
pub struct DesktopApp {
    session: Option<Session>,
    mode: Mode,
    /// What the window says its texts in (issue #103); English until chosen.
    language: Language,
    /// Tool, selection and gesture; the selection is shared by both modes.
    editor: Editor,
    /// Where the pointer snapped with the wire tool, as of the last frame.
    snapped: Option<Snap>,
    inspector: InspectorState,
    /// The tab and the search of the palette.
    palette: palette::PaletteState,
    /// The command line over the drawing.
    command_line: CommandLine,
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
    /// project opened here compiles against the standard catalog with them.
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
    /// The cursor cross over the drawing is not drawn.
    crosshair_off: bool,
    /// Where the context menu of the drawing is open, in screen points.
    context_menu: Option<egui::Pos2>,
    /// The net of the selection is outlined (View menu, key T).
    trace_net: bool,
    /// What is traced this frame, and a line about it for the status bar.
    traced: (BTreeSet<Subject>, String),
    /// The diagnostic a reason is being typed for, to waive it.
    waiving: Option<(String, Subject)>,
    /// The reason being typed.
    waive_reason: String,
    /// The copy of a project with unsaved changes, kept for a recovery after
    /// an unexpected exit; off unless `main` turns it on.
    #[cfg(not(target_arch = "wasm32"))]
    autosave: Option<crate::recovery::Autosave>,
    /// Copies left by windows that ended without closing, offered once.
    #[cfg(not(target_arch = "wasm32"))]
    recover_offer: Vec<crate::recovery::Entry>,
    /// Where this window locks the projects it opens; off unless `main`
    /// turns it on.
    #[cfg(not(target_arch = "wasm32"))]
    project_locks: Option<crate::project_lock::ProjectLocks>,
    /// The lock of the open project, or the note that another window holds it.
    #[cfg(not(target_arch = "wasm32"))]
    project_lock: Option<crate::project_lock::ProjectLock>,
    /// Seconds between two attempts to take a lock another window holds,
    /// and when the last attempt was, in window seconds.
    #[cfg(not(target_arch = "wasm32"))]
    lock_retry: (f64, f64),
    /// Where every save keeps a version; off unless `main` turns it on.
    #[cfg(not(target_arch = "wasm32"))]
    versions: Option<crate::versions::VersionStore>,
    /// The version history window, while open: each version and how many
    /// elements differ from the project as it was when the window opened.
    #[cfg(not(target_arch = "wasm32"))]
    history: Option<Vec<(crate::versions::Version, usize)>>,
    /// The projects opened or saved last; off unless `main` turns it on.
    #[cfg(not(target_arch = "wasm32"))]
    recent: Option<crate::recent::RecentProjects>,
    /// The command palette (Ctrl+K).
    command_palette: crate::command_palette::PaletteState,
    /// The find and replace window (Ctrl+H).
    find_replace: crate::find_replace::FindReplace,
    /// The label being typed over the drawing (F2): the element, its text,
    /// and whether the field still has to take the keyboard.
    label_edit: Option<(Subject, String, bool)>,
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

    /// Keep a copy of a project with unsaved changes in `store`, written at
    /// most once per `interval_seconds`, and offer the copies that windows
    /// which ended without closing left there.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn enable_recovery(
        &mut self,
        store: crate::recovery::RecoveryStore,
        interval_seconds: f64,
    ) {
        let autosave = crate::recovery::Autosave::new(store, interval_seconds, 0.0);
        self.recover_offer = autosave
            .store()
            .orphans(autosave.key(), autosave.stale_after());
        self.autosave = Some(autosave);
    }

    /// The project a panic hook writes before the process ends (`main`
    /// installs the hook), when recovery is on.
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn panic_copy(&self) -> Option<crate::recovery::PanicCopy> {
        self.autosave
            .as_ref()
            .map(crate::recovery::Autosave::panic_copy)
    }

    /// Lock each project this window opens in `locks`, and say when another
    /// window has it open; a lock another window holds is tried again every
    /// `retry_seconds`.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn enable_project_locks(
        &mut self,
        locks: crate::project_lock::ProjectLocks,
        retry_seconds: f64,
    ) {
        self.project_locks = Some(locks);
        self.lock_retry = (retry_seconds.max(0.0), f64::NEG_INFINITY);
    }

    /// Keep a version of the project in `store` at every save.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn enable_version_history(&mut self, store: crate::versions::VersionStore) {
        self.versions = Some(store);
    }

    /// Keep the list of the projects opened or saved last in `recent`, and
    /// show it in File > Open recent.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn enable_recent_projects(&mut self, recent: crate::recent::RecentProjects) {
        self.recent = Some(recent);
    }

    /// The projects in File > Open recent, newest first; empty when the list
    /// is off.
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn recent_projects(&self) -> Vec<PathBuf> {
        self.recent
            .as_ref()
            .map(crate::recent::RecentProjects::list)
            .unwrap_or_default()
    }

    /// Put the file of the project first in the list of recent projects.
    #[cfg(not(target_arch = "wasm32"))]
    fn record_recent(&mut self) {
        let (Some(recent), Some(path)) =
            (&self.recent, self.session.as_ref().and_then(Session::path))
        else {
            return;
        };
        // Best effort: a list that cannot be written only loses an entry.
        let _ = recent.record(path);
    }

    /// Open the find and replace window, as Ctrl+H does in edit mode.
    pub fn open_find_replace(&mut self) {
        self.find_replace.open();
    }

    /// Whether the find and replace window is open.
    #[must_use]
    pub const fn find_replace_open(&self) -> bool {
        self.find_replace.is_open()
    }

    /// Start typing the label of the selected element over the drawing, as
    /// F2 does: the designator of a symbol's device, the words of a text
    /// note or the number of a wire. Nothing happens unless one such element
    /// is selected in edit mode.
    pub fn edit_label(&mut self) {
        let Some(session) = &self.session else {
            return;
        };
        if self.mode != Mode::Edit {
            return;
        }
        let mut selected = self.editor.selection().iter();
        let (Some(subject), None) = (selected.next(), selected.next()) else {
            return;
        };
        let doc = session.document();
        let text = match subject {
            Subject::Symbol(id) => doc
                .symbols
                .iter()
                .find(|symbol| symbol.id == *id)
                .and_then(|symbol| doc.devices.iter().find(|d| d.id == symbol.device_id))
                .map(|device| device.reference.0.clone()),
            Subject::Text(id) => doc
                .texts
                .iter()
                .find(|note| note.id == *id)
                .map(|note| note.text.clone()),
            Subject::Wire(id) => {
                let nets = StaticNets::build(doc, session.catalog());
                let numbers = circuit_model::WireNumbers::build(doc, session.catalog(), &nets);
                Some(numbers.of_wire(*id).unwrap_or_default().to_owned())
            }
            _ => None,
        };
        if let Some(text) = text {
            self.label_edit = Some((subject.clone(), text, true));
        }
    }

    /// Whether a label is being typed over the drawing.
    #[must_use]
    pub const fn label_editing(&self) -> bool {
        self.label_edit.is_some()
    }

    /// The field of the label over its element; Enter commits it, anything
    /// else that takes the focus away drops it.
    fn label_editor(&mut self, ui: &Ui, scene: &Scene, events: &canvas::CanvasEvents) {
        if self.mode != Mode::Edit {
            self.label_edit = None;
        }
        let Some((subject, text, focus)) = &mut self.label_edit else {
            return;
        };
        let Some(bounds) = scene.subject_bounds(subject) else {
            self.label_edit = None;
            return;
        };
        let [x, y] = self.camera.to_screen(bounds.min);
        let at = egui::pos2(events.origin[0] + x, events.origin[1] + y - 28.0);
        let mut finished = None;
        egui::Area::new(ui.scope_id().with(egui::IdSalt::new("label editor")))
            .order(egui::Order::Foreground)
            .fixed_pos(at)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    let label = ui.label(tr("Label"));
                    let field = ui
                        .add(
                            egui::TextEdit::singleline(text)
                                .id_salt("label editor input")
                                .desired_width(160.0),
                        )
                        .labelled_by(label.id);
                    if std::mem::take(focus) {
                        field.request_focus();
                    }
                    if field.lost_focus() {
                        finished = Some(ui.input(|input| input.key_pressed(egui::Key::Enter)));
                    }
                });
            });
        let Some(commit) = finished else {
            return;
        };
        let Some((subject, text)) = self.label_edit.take().map(|(s, t, _)| (s, t)) else {
            return;
        };
        let Some(session) = &self.session else {
            return;
        };
        if !commit {
            return;
        }
        let doc = session.document();
        let text = text.trim();
        let request = match subject {
            Subject::Symbol(id) => doc
                .symbols
                .iter()
                .find(|symbol| symbol.id == id)
                .map(|symbol| ops::set_reference(doc, symbol.device_id, text)),
            Subject::Text(id) => Some(ops::set_text(doc, id, text)),
            Subject::Wire(id) => {
                let nets = StaticNets::build(doc, session.catalog());
                let numbers = circuit_model::WireNumbers::build(doc, session.catalog(), &nets);
                // The number as it is generated needs no override.
                (numbers.of_wire(id) != Some(text))
                    .then(|| circuit_commands::documentation::set_wire_number(doc, id, Some(text)))
            }
            _ => None,
        };
        if let Some(request) = request {
            self.commit(request);
        }
    }

    /// Open the command palette, as Ctrl+K does.
    pub fn open_command_palette(&mut self) {
        self.command_palette.open();
    }

    /// Whether the command palette is open.
    #[must_use]
    pub const fn command_palette_open(&self) -> bool {
        self.command_palette.is_open()
    }

    /// The palette over the drawing, and what was chosen in it.
    fn command_palette(&mut self, ui: &Ui) {
        let choice = self.command_palette.show(
            ui,
            self.session.as_ref().map(Session::document),
            self.session.as_ref().map(Session::catalog),
        );
        match choice {
            None => {}
            Some(crate::command_palette::Choice::Element(subject)) => self.go_to(subject),
            Some(crate::command_palette::Choice::Command(command)) => {
                use crate::command_palette::Command;
                match command {
                    Command::New => self.new_project(),
                    Command::Open => self.guard(Pending::OpenDialog),
                    Command::Save => {
                        self.save();
                    }
                    Command::SaveAs => {
                        self.save_as_dialog();
                    }
                    Command::SaveCopy => self.save_copy_dialog(),
                    Command::Undo => self.undo(),
                    Command::Redo => self.redo(),
                    Command::Fit => self.fit_pending = true,
                    Command::Edit => self.set_mode(Mode::Edit),
                    Command::Simulate => self.set_mode(Mode::Simulate),
                    Command::ExportSvg => self.export_dialog("svg"),
                    Command::ExportPdf => self.export_dialog("pdf"),
                    Command::Library => self.library_dialog(),
                    Command::Keys => self.show_keys = true,
                }
            }
        }
    }

    /// Select an element and bring it into view: a device by its first
    /// symbol, a sheet by its frame.
    fn go_to(&mut self, subject: Subject) {
        match subject {
            Subject::Sheet(id) => self.fit_sheet = Some(id),
            Subject::Device(id) => {
                let first = self.session.as_ref().and_then(|session| {
                    session
                        .document()
                        .symbols
                        .iter()
                        .filter(|symbol| symbol.device_id == id)
                        .map(|symbol| symbol.id)
                        .min()
                });
                self.editor.set_selection(BTreeSet::from([subject]));
                self.focus = first.map(Subject::Symbol);
            }
            other => self.select(other),
        }
    }

    /// Start a new project from a template, asking first when the open
    /// project has unsaved changes.
    pub fn new_from_template(&mut self, index: usize) {
        self.guard(Pending::Template(index));
    }

    /// Write the project to `path` without making it its file: the project
    /// keeps its file and its unsaved changes. Returns whether it was
    /// written.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_copy_to(&mut self, path: &Path) -> bool {
        let Some(session) = &self.session else {
            return false;
        };
        match session
            .text()
            .map_err(|error| error.to_string())
            .and_then(|text| {
                crate::files::write_atomic(path, text).map_err(|error| error.to_string())
            }) {
            Ok(()) => {
                self.error = None;
                true
            }
            Err(error) => {
                self.error = Some(error);
                false
            }
        }
    }

    /// Choose where to write a copy of the project; a browser downloads it.
    fn save_copy_dialog(&mut self) {
        let Some(session) = &self.session else {
            return;
        };
        let name = format!("{} (copy)", session.document().project.name);
        #[cfg(target_arch = "wasm32")]
        {
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
            if let Err(error) = downloaded {
                self.error = Some(error);
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(path) = dialogs::choose_save_path(&name) {
            self.save_copy_to(&path);
        }
    }

    /// Open the version history window of the project, newest first.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn show_version_history(&mut self) {
        let (Some(store), Some(session)) = (&self.versions, &self.session) else {
            return;
        };
        let Some(path) = session.path() else {
            return;
        };
        let current = session.document();
        let listed = store
            .list(path)
            .into_iter()
            .map(|version| {
                let changes = version
                    .read()
                    .ok()
                    .and_then(|text| circuit_project::from_str(&text).ok())
                    .map_or(0, |old| circuit_report::diff(&old, current).rows.len());
                (version, changes)
            })
            .collect();
        self.history = Some(listed);
    }

    /// How many versions the history window lists, if it is open.
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn versions_listed(&self) -> Option<usize> {
        self.history.as_ref().map(Vec::len)
    }

    /// Keep the project as it is now as a version of its file.
    #[cfg(not(target_arch = "wasm32"))]
    fn record_version(&mut self) {
        let (Some(store), Some(session)) = (&self.versions, &self.session) else {
            return;
        };
        let (Some(path), Ok(text)) = (session.path(), session.text()) else {
            return;
        };
        // The project is saved either way; a version that cannot be kept is
        // said, not treated as a failed save.
        if let Err(error) = store.record(path, &text) {
            self.error = Some(trf("The version history was not updated: {0}", &[&error]));
        }
    }

    /// Whether another window has the open project open too.
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn open_elsewhere(&self) -> bool {
        self.project_lock
            .as_ref()
            .is_some_and(crate::project_lock::ProjectLock::open_elsewhere)
    }

    /// How many copies are offered for recovery.
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn recovery_offered(&self) -> usize {
        self.recover_offer.len()
    }

    /// Whether the copy kept for recovery is the project as it is now.
    #[cfg(not(target_arch = "wasm32"))]
    #[must_use]
    pub fn recovery_current(&self) -> bool {
        self.autosave
            .as_ref()
            .is_some_and(|autosave| autosave.is_current(self.session.as_ref()))
    }

    /// Open a project file. On failure the current project stays open and
    /// the error is shown.
    pub fn open_path(&mut self, path: &Path) {
        self.path_input = path.display().to_string();
        match Session::open(path) {
            Ok(session) => {
                self.replace(session);
                #[cfg(not(target_arch = "wasm32"))]
                self.record_recent();
            }
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

    /// Whether the cursor over the drawing is a cross while editing.
    #[must_use]
    pub const fn crosshair_shown(&self) -> bool {
        !self.crosshair_off
    }

    /// Whether the context menu of the drawing is open.
    #[must_use]
    pub const fn context_menu_open(&self) -> bool {
        self.context_menu.is_some()
    }

    /// What the command line has said, oldest first.
    #[must_use]
    pub fn command_log(&self) -> Vec<&str> {
        self.command_line.lines()
    }

    /// The tool of the editor.
    #[must_use]
    pub const fn tool(&self) -> &Tool {
        self.editor.tool()
    }

    /// The tab of the palette that is open.
    #[must_use]
    pub const fn palette_tab(&self) -> Category {
        self.palette.tab()
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
                #[cfg(not(target_arch = "wasm32"))]
                self.record_version();
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
                #[cfg(not(target_arch = "wasm32"))]
                {
                    self.record_version();
                    self.record_recent();
                }
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

    /// Write the list of problems as CSV: to a file the user chooses, or as a
    /// download in a browser.
    fn save_problems_dialog(&mut self) {
        let Some(session) = self.session.as_ref() else {
            return;
        };
        let csv = session.problems_csv();
        let name = session.document().project.name.clone();
        #[cfg(target_arch = "wasm32")]
        if let Err(error) =
            crate::web::download(&format!("{name}-problems.csv"), csv.as_bytes(), "text/csv")
        {
            self.error = Some(error);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(path) = dialogs::choose_problems_path(&name)
            && let Err(error) = crate::files::write_atomic(&path, csv)
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
            Pending::Template(index) => {
                let Some(template) = crate::templates::TEMPLATES.get(index) else {
                    return;
                };
                match Session::load(template.text, None) {
                    Ok(session) => {
                        self.replace(session);
                        self.path_input.clear();
                    }
                    Err(error) => self.error = Some(error.to_string()),
                }
            }
            Pending::OpenDialog => {
                if let Some(path) = dialogs::pick_project() {
                    self.open_path(&path);
                }
            }
            #[cfg(not(target_arch = "wasm32"))]
            Pending::Restore(version) => {
                let loaded = version
                    .read()
                    .map_err(|error| error.to_string())
                    .and_then(|text| Session::load(&text, None).map_err(|error| error.to_string()));
                match loaded {
                    Ok(mut session) => {
                        session.mark_unsaved();
                        self.replace(session);
                        self.path_input.clear();
                        self.history = None;
                    }
                    Err(error) => self.error = Some(error),
                }
            }
            Pending::Close => {
                // Closed on purpose: no copy is kept for a recovery.
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(autosave) = &mut self.autosave {
                    autosave.clear();
                }
                self.allow_close = true;
                self.close_now = true;
            }
        }
    }

    /// Write, renew or remove the copy kept for a recovery, and wake the
    /// window when the next one is due.
    #[cfg(not(target_arch = "wasm32"))]
    fn autosave_tick(&mut self, ui: &Ui) {
        let Some(autosave) = &mut self.autosave else {
            return;
        };
        // Closing with the changes dropped: the copy was removed and is not
        // written again while the window goes away.
        if self.allow_close {
            return;
        }
        let now = ui.input(|input| input.time);
        autosave.tick(now, self.session.as_ref());
        if self.session.as_ref().is_some_and(Session::is_dirty) {
            let wait = autosave.due_in(now).max(0.1);
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(wait));
        }
    }

    /// File > Open recent: the projects opened or saved last, and a way to
    /// empty the list.
    #[cfg(not(target_arch = "wasm32"))]
    fn recent_menu(&mut self, ui: &mut Ui) {
        let projects = self.recent_projects();
        ui.add_enabled_ui(!projects.is_empty(), |ui| {
            ui.menu_button(tr("Open recent"), |ui| {
                for path in &projects {
                    let name = path.file_name().map_or_else(
                        || path.display().to_string(),
                        |name| name.to_string_lossy().into_owned(),
                    );
                    if ui
                        .button(name)
                        .on_hover_text(path.display().to_string())
                        .clicked()
                    {
                        ui.close();
                        if !path.is_file() {
                            if let Some(recent) = &self.recent {
                                let _ = recent.forget(path);
                            }
                            self.error = Some(trf(
                                "{0} is no longer there; it was taken off the list.",
                                &[&path.display()],
                            ));
                        } else {
                            self.request_open(path.clone());
                        }
                    }
                }
                ui.separator();
                if ui.button(tr("Clear the list")).clicked() {
                    ui.close();
                    if let Some(recent) = &self.recent {
                        let _ = recent.clear();
                    }
                }
            });
        });
    }

    /// The version history of the project, while it is open.
    #[cfg(not(target_arch = "wasm32"))]
    fn history_window(&mut self, ui: &Ui) {
        let Some(listed) = &self.history else {
            return;
        };
        let mut open = true;
        let mut restore = None;
        egui::Window::new(tr("Version history"))
            .open(&mut open)
            .collapsible(false)
            .show(ui.ctx(), |ui| {
                if listed.is_empty() {
                    ui.label(tr("No version yet: one is kept at every save."));
                }
                egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .show(ui, |ui| {
                        for (index, (version, changes)) in listed.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(version.label());
                                ui.label(trf("{0} differences", &[changes]));
                                let response = ui.button(tr("Open as new project"));
                                let label = format!("Open version {}", index.saturating_add(1));
                                response.widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::Role::Button,
                                        true,
                                        label.as_str(),
                                    )
                                });
                                if response.clicked() {
                                    restore = Some(version.clone());
                                }
                            });
                        }
                    });
            });
        if let Some(version) = restore {
            self.guard(Pending::Restore(version));
            if self.confirm.is_none() {
                self.history = None;
            }
        } else if !open {
            self.history = None;
        }
    }

    /// Hold the lock of the open project: take it when the project or its
    /// file changes, release the old one, and try again now and then while
    /// another window holds it.
    #[cfg(not(target_arch = "wasm32"))]
    fn relock(&mut self, ui: &Ui) {
        let Some(locks) = &self.project_locks else {
            return;
        };
        let path = self.session.as_ref().and_then(Session::path);
        let held = self
            .project_lock
            .as_ref()
            .map(crate::project_lock::ProjectLock::project);
        let now = ui.input(|input| input.time);
        let (every, last) = self.lock_retry;
        let retry = self.open_elsewhere() && now - last >= every;
        if held == path && !retry {
            return;
        }
        let path = path.map(Path::to_path_buf);
        // Release first: taking the same project again must not find this
        // window's own lock.
        self.project_lock = None;
        self.lock_retry.1 = now;
        self.project_lock = path.map(|path| locks.take(&path));
        if self.open_elsewhere() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(every.max(0.5)));
        }
    }

    /// The offer to recover the copies of projects left by windows that
    /// ended without closing. One copy is recovered at a time; the others
    /// stay for the next start.
    #[cfg(not(target_arch = "wasm32"))]
    fn recovery_dialog(&mut self, ui: &Ui) {
        if self.recover_offer.is_empty() || self.confirm.is_some() {
            return;
        }
        let mut chosen: Option<(usize, bool)> = None;
        let mut later = false;
        egui::Modal::new(egui::Id::unique("recover unsaved work")).show(ui.ctx(), |ui| {
            ui.heading(tr("Recover unsaved work"));
            ui.label(tr(
                "OpenCircuit ended without saving these projects. A copy of each was kept.",
            ));
            for (index, entry) in self.recover_offer.iter().enumerate() {
                ui.separator();
                let place = entry.original.as_ref().map_or_else(
                    || tr("never saved").to_owned(),
                    |path| path.display().to_string(),
                );
                ui.label(format!("{} — {place}", entry.name));
                ui.horizontal(|ui| {
                    let button = |ui: &mut Ui, text: &str| {
                        let response = ui.button(text);
                        let label = format!("{text} {}", entry.name);
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::Button, true, label.as_str())
                        });
                        response
                    };
                    if button(ui, tr("Recover")).clicked() {
                        chosen = Some((index, true));
                    }
                    if button(ui, tr("Discard")).clicked() {
                        chosen = Some((index, false));
                    }
                });
            }
            ui.separator();
            if ui.button(tr("Later")).clicked() {
                later = true;
            }
        });
        if later {
            self.recover_offer.clear();
            return;
        }
        let Some((index, recover)) = chosen else {
            return;
        };
        if index >= self.recover_offer.len() {
            return;
        }
        let entry = self.recover_offer.remove(index);
        if !recover {
            entry.discard();
            return;
        }
        let loaded = entry
            .read()
            .map_err(|error| error.to_string())
            .and_then(|text| {
                Session::load(&text, entry.original.clone()).map_err(|error| error.to_string())
            });
        match loaded {
            Ok(mut session) => {
                session.mark_unsaved();
                self.replace(session);
                entry.discard();
                // The others stay on disk and are offered at the next start:
                // the window holds one project at a time.
                self.recover_offer.clear();
            }
            Err(error) => self.error = Some(error),
        }
    }

    /// The question about unsaved changes, while one is pending.
    fn confirm_dialog(&mut self, ui: &Ui) {
        let Some(pending) = self.confirm.clone() else {
            return;
        };
        let mut answer = None;
        let modal = egui::Modal::new(egui::Id::unique("unsaved changes")).show(ui.ctx(), |ui| {
            ui.heading(tr("Unsaved changes"));
            ui.label(tr("The project has changes that are not saved."));
            ui.horizontal(|ui| {
                if ui.button(tr("Save and continue")).clicked() {
                    answer = Some("save");
                }
                if ui.button(tr("Discard changes")).clicked() {
                    answer = Some("discard");
                }
                if ui.button(tr("Cancel")).clicked() {
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
        // The project that goes was saved, or its changes were dropped on
        // purpose: its copy is not kept.
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(autosave) = &mut self.autosave {
            autosave.clear();
        }
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

    /// The built-in catalog and the standard library, with the libraries of
    /// this window.
    fn catalog(&self) -> Result<ComponentCatalog, CatalogError> {
        self.libraries
            .iter()
            .cloned()
            .try_fold(circuit_project::stdlib::catalog(), |catalog, library| {
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

    /// The language of the window.
    #[must_use]
    pub const fn language(&self) -> Language {
        self.language
    }

    /// Change the language of the window. It is not stored: the program
    /// chooses one from the environment when it starts, and the Help menu
    /// changes it while it runs.
    pub const fn set_language(&mut self, language: Language) {
        self.language = language;
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
        i18n::set_language(self.language);
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
        self.command_palette(ui);
        if let Some(session) = &self.session
            && let Some(request) = self
                .find_replace
                .show(ui, session.document(), session.catalog())
        {
            self.commit(request);
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.recovery_dialog(ui);
            self.history_window(ui);
            self.autosave_tick(ui);
            self.relock(ui);
        }

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
        if ui.input_mut(|input| {
            input.consume_shortcut(&KeyboardShortcut::new(
                egui::Modifiers::COMMAND,
                egui::Key::Num9,
            ))
        }) {
            self.command_line.focus();
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
        self.update_trace();
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
                    .default_size(260.0)
                    .show(ui, |ui| {
                        if let Some(session) = &self.session {
                            palette::show(
                                ui,
                                session.catalog(),
                                &mut self.editor,
                                &mut self.palette,
                            );
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
        if pressed(ui, egui::Modifiers::COMMAND, egui::Key::H) {
            self.find_replace.open();
        }
        if pressed(ui, egui::Modifiers::NONE, egui::Key::F2) {
            self.edit_label();
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
        // The command palette, in both modes.
        if ui.input_mut(|input| {
            input.consume_shortcut(&KeyboardShortcut::new(
                egui::Modifiers::COMMAND,
                egui::Key::K,
            ))
        }) {
            self.command_palette.open();
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
        if pressed(ui, egui::Key::T) {
            self.trace_net = !self.trace_net;
        }
    }

    /// Work out the net to outline this frame: that of the selection while
    /// tracing is on, nothing otherwise.
    fn update_trace(&mut self) {
        self.traced = match (&self.session, self.trace_net) {
            (Some(session), true) if !self.editor.selection().is_empty() => {
                let nets = StaticNets::build(session.document(), session.catalog());
                let traced = nets.traced(self.editor.selection());
                let count =
                    |wanted: fn(&Subject) -> bool| traced.iter().filter(|s| wanted(s)).count();
                let text = if traced.is_empty() {
                    String::new()
                } else {
                    let terminals = trn(
                        "{0} terminal",
                        "{0} terminals",
                        count(|s| matches!(s, Subject::Pin(_))),
                    );
                    let wires = trn(
                        "{0} wire",
                        "{0} wires",
                        count(|s| matches!(s, Subject::Wire(_))),
                    );
                    trf("Net of the selection: {0}, {1}", &[&terminals, &wires])
                };
                (traced, text)
            }
            _ => (BTreeSet::new(), String::new()),
        };
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
                ui.menu_button(tr("File"), |ui| {
                    if ui.button(tr("New project")).clicked() {
                        ui.close();
                        self.new_project();
                    }
                    if ui.button(tr("Open project…")).clicked() {
                        ui.close();
                        self.guard(Pending::OpenDialog);
                    }
                    ui.menu_button(tr("New from template"), |ui| {
                        for (index, template) in crate::templates::TEMPLATES.iter().enumerate() {
                            if ui.button(tr(template.name)).clicked() {
                                ui.close();
                                self.new_from_template(index);
                            }
                        }
                    });
                    #[cfg(not(target_arch = "wasm32"))]
                    if self.recent.is_some() {
                        self.recent_menu(ui);
                    }
                    ui.separator();
                    if ui
                        .add_enabled(has_session, egui::Button::new(tr("Save project")))
                        .clicked()
                    {
                        ui.close();
                        self.save();
                    }
                    if ui
                        .add_enabled(has_session, egui::Button::new(tr("Save project as…")))
                        .clicked()
                    {
                        ui.close();
                        self.save_as_dialog();
                    }
                    if ui
                        .add_enabled(has_session, egui::Button::new(tr("Save a copy…")))
                        .on_hover_text(tr(
                            "Write the project to another file and keep working on this one",
                        ))
                        .clicked()
                    {
                        ui.close();
                        self.save_copy_dialog();
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    if self.versions.is_some() {
                        let saved = self.session.as_ref().and_then(Session::path).is_some();
                        if ui
                            .add_enabled(saved, egui::Button::new(tr("Version history…")))
                            .on_hover_text(tr(
                                "Every save of this project; open one as a new project",
                            ))
                            .clicked()
                        {
                            ui.close();
                            self.show_version_history();
                        }
                    }
                    ui.separator();
                    if ui.button(tr("Load library…")).clicked() {
                        ui.close();
                        self.library_dialog();
                    }
                    ui.add_enabled_ui(has_session, |ui| {
                        ui.menu_button(tr("Export drawing"), |ui| {
                            for (label, extension) in
                                [(tr("As SVG…"), "svg"), (tr("As PDF…"), "pdf")]
                            {
                                if ui.button(label).clicked() {
                                    ui.close();
                                    self.export_dialog(extension);
                                }
                            }
                        });
                    });
                });
                ui.menu_button(tr("Modify"), |ui| {
                    let (can_undo, can_redo) = self.session.as_ref().map_or((false, false), |s| {
                        (s.history().can_undo(), s.history().can_redo())
                    });
                    if ui
                        .add_enabled(
                            editing && can_undo,
                            egui::Button::new(tr("Undo last change")),
                        )
                        .clicked()
                    {
                        ui.close();
                        self.undo();
                    }
                    if ui
                        .add_enabled(editing && can_redo, egui::Button::new(tr("Redo change")))
                        .clicked()
                    {
                        ui.close();
                        self.redo();
                    }
                    ui.separator();
                    for (label, key) in [
                        (tr("Select everything"), Key::SelectAll),
                        (tr("Copy selection"), Key::Copy),
                        (tr("Paste"), Key::Paste),
                        (tr("Duplicate selection"), Key::Duplicate),
                        (tr("Rotate selection"), Key::Rotate),
                        (tr("Delete selection"), Key::Delete),
                    ] {
                        if ui.add_enabled(editing, egui::Button::new(label)).clicked() {
                            ui.close();
                            self.press(key);
                        }
                    }
                    ui.separator();
                    if ui
                        .add_enabled(editing, egui::Button::new(tr("Add text")))
                        .clicked()
                    {
                        ui.close();
                        self.editor.set_tool(Tool::Text);
                    }
                    if ui
                        .add_enabled(editing, egui::Button::new(tr("Renumber designators")))
                        .clicked()
                    {
                        ui.close();
                        if let Some(session) = &self.session {
                            let request = designators::renumber(session.document(), Scope::All);
                            self.commit(request);
                        }
                    }
                    if ui
                        .add_enabled(editing, egui::Button::new(tr("Find and replace…")))
                        .on_hover_text("Ctrl+H")
                        .clicked()
                    {
                        ui.close();
                        self.find_replace.open();
                    }
                    if ui
                        .add_enabled(editing, egui::Button::new(tr("Edit label")))
                        .on_hover_text(tr(
                            "F2: the designator, text or wire number of the selection",
                        ))
                        .clicked()
                    {
                        ui.close();
                        self.edit_label();
                    }
                });
                ui.menu_button(tr("View"), |ui| {
                    if ui
                        .add_enabled(
                            has_session,
                            egui::Button::new(tr("Zoom to the whole drawing")),
                        )
                        .clicked()
                    {
                        ui.close();
                        self.fit_pending = true;
                    }
                    if ui
                        .add_enabled(has_session, egui::Button::new(tr("Zoom in")))
                        .clicked()
                    {
                        self.zoom_request = Some(KEY_ZOOM);
                    }
                    if ui
                        .add_enabled(has_session, egui::Button::new(tr("Zoom out")))
                        .clicked()
                    {
                        self.zoom_request = Some(1.0 / KEY_ZOOM);
                    }
                    ui.separator();
                    let mut grid = !self.grid_off;
                    if ui.checkbox(&mut grid, tr("Show the grid")).changed() {
                        self.grid_off = !grid;
                    }
                    let mut snap = !self.snap_off;
                    if ui.checkbox(&mut snap, tr("Snap wires to pins")).changed() {
                        self.snap_off = !snap;
                    }
                    let mut cross = !self.crosshair_off;
                    if ui.checkbox(&mut cross, tr("Cross-shaped cursor")).changed() {
                        self.crosshair_off = !cross;
                    }
                    let mut problems = !self.diagnostics_hidden;
                    if ui
                        .checkbox(&mut problems, tr("Show the problems list"))
                        .changed()
                    {
                        self.diagnostics_hidden = !problems;
                    }
                    ui.checkbox(&mut self.trace_net, tr("Trace the net of the selection"));
                });
                ui.menu_button(tr("Run"), |ui| {
                    let mut mode = self.mode;
                    ui.add_enabled_ui(has_session, |ui| {
                        ui.radio_value(&mut mode, Mode::Simulate, tr("Simulation mode"));
                        ui.radio_value(&mut mode, Mode::Edit, tr("Drawing mode"));
                    });
                    self.set_mode(mode);
                    ui.separator();
                    let mode = self.mode;
                    if let Some(session) = &mut self.session {
                        let simulating = mode == Mode::Simulate && session.can_simulate();
                        let running = session.is_running();
                        let label = if running {
                            tr("Pause simulation")
                        } else {
                            tr("Start simulation")
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
                            .add_enabled(simulating, egui::Button::new(tr("Advance one tick")))
                            .clicked()
                        {
                            session.step();
                        }
                        if ui
                            .add_enabled(simulating, egui::Button::new(tr("Restart simulation")))
                            .clicked()
                        {
                            ui.close();
                            session.reset();
                        }
                    }
                });
                ui.menu_button(tr("Help"), |ui| {
                    if ui.button(tr("Keyboard shortcuts")).clicked() {
                        ui.close();
                        self.show_keys = true;
                    }
                    if ui
                        .button(tr("Command palette"))
                        .on_hover_text("Ctrl+K")
                        .clicked()
                    {
                        ui.close();
                        self.command_palette.open();
                    }
                    if ui.button(tr("About OpenCircuit")).clicked() {
                        ui.close();
                        self.show_about = true;
                    }
                    ui.separator();
                    ui.menu_button(tr("Language"), |ui| {
                        for language in Language::ALL {
                            let chosen = ui
                                .radio_value(&mut self.language, language, language.name())
                                .clicked();
                            if chosen {
                                i18n::set_language(self.language);
                                ui.close();
                            }
                        }
                    });
                });
            });
        });
    }

    /// The windows of the Help menu.
    fn help_windows(&mut self, ui: &Ui) {
        let ctx = ui.ctx().clone();
        egui::Window::new(tr("Keyboard shortcuts"))
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut self.show_keys)
            .collapsible(false)
            .resizable(false)
            .show(&ctx, |ui| {
                egui::Grid::new("keys").num_columns(2).show(ui, |ui| {
                    for (keys, action) in palette::KEYS {
                        ui.monospace(tr(keys));
                        ui.label(tr(action));
                        ui.end_row();
                    }
                });
            });
        egui::Window::new(tr("About OpenCircuit"))
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut self.show_about)
            .collapsible(false)
            .resizable(false)
            .show(&ctx, |ui| {
                ui.strong(format!("OpenCircuit {}", env!("CARGO_PKG_VERSION")));
                ui.label(tr(
                    "An open-source editor and simulator for industrial control circuits.",
                ));
                ui.label(tr("Licensed under MIT or Apache-2.0."));
            });
    }

    /// The quick-access bar: files, undo and redo, the path of the project
    /// and the mode.
    fn toolbar(&mut self, ui: &mut Ui) {
        // Wrapped, so that a narrow window shows every control on a second
        // row instead of cutting the last ones off.
        ui.horizontal_wrapped(|ui| {
            let has_session = self.session.is_some();
            if icons::button(ui, Icon::New, tr("New"), false).clicked() {
                self.new_project();
            }
            if icons::button(ui, Icon::Open, tr("Open…"), false).clicked() {
                self.guard(Pending::OpenDialog);
            }
            ui.add_enabled_ui(has_session, |ui| {
                if icons::button(ui, Icon::Save, tr("Save"), false).clicked() {
                    self.save();
                }
                if icons::button(ui, Icon::SaveAs, tr("Save as…"), false).clicked() {
                    self.save_as_dialog();
                }
            });
            if icons::button_hint(
                ui,
                Icon::Library,
                tr("Library…"),
                tr("Load an extension library (.oclib): more components for this window"),
                false,
            )
            .clicked()
            {
                self.library_dialog();
            }
            ui.add_enabled_ui(has_session, |ui| {
                ui.menu_button(tr("Export"), |ui| {
                    ui.label(tr("While simulating: the current tick in colour."));
                    ui.label(tr("While editing: the drawing at rest, black on white."));
                    for (label, extension) in [(tr("SVG…"), "svg"), (tr("PDF…"), "pdf")] {
                        if ui.button(label).clicked() {
                            ui.close();
                            self.export_dialog(extension);
                        }
                    }
                });
            });
            ui.separator();
            let name = ui.label(tr("Path"));
            let field = ui
                .add(
                    egui::TextEdit::singleline(&mut self.path_input)
                        .hint_text(tr("path to a .ocircuit file"))
                        .desired_width(260.0),
                )
                .labelled_by(name.id);
            let entered =
                field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if ui.button(tr("Open")).clicked() || entered {
                let path = PathBuf::from(self.path_input.trim());
                self.request_open(path);
            }
            if ui
                .add_enabled(has_session, egui::Button::new(tr("Save here")))
                .clicked()
            {
                let path = PathBuf::from(self.path_input.trim());
                self.save_to(path);
            }
            if has_session {
                if icons::button(ui, Icon::Fit, tr("Fit"), false).clicked() {
                    self.fit_pending = true;
                }
                ui.separator();
                let mut mode = self.mode;
                ui.selectable_value(&mut mode, Mode::Simulate, tr("Simulate"));
                ui.selectable_value(&mut mode, Mode::Edit, tr("Edit"));
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
            name.push_str(tr(" (unsaved)"));
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
                        .on_hover_text(tr("Frame this sheet"))
                        .clicked()
                    {
                        self.fit_sheet = Some(id);
                    }
                }
            }
            if !self.traced.1.is_empty() {
                ui.separator();
                ui.label(RichText::new(self.traced.1.as_str()).color(tokens.text_dim));
            }
            #[cfg(not(target_arch = "wasm32"))]
            if self.open_elsewhere() {
                ui.separator();
                ui.colored_label(ui.visuals().warn_fg_color, tr("Open in another window"))
                    .on_hover_text(tr(
                        "Saving here and in the other window overwrites each other's changes.",
                    ));
            }
            #[cfg(not(target_arch = "wasm32"))]
            if let Some(autosave) = &self.autosave {
                if let Some(failure) = autosave.failure() {
                    ui.separator();
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        trf("Recovery copy failed: {0}", &[&failure]),
                    );
                } else if self.session.as_ref().is_some_and(Session::is_dirty)
                    && autosave.is_current(self.session.as_ref())
                {
                    ui.separator();
                    ui.label(RichText::new(tr("Recovery copy saved")).color(tokens.text_dim));
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let problems = self.session.as_ref().map_or(0, |session| {
                    let waivers = WaiverIndex::new(session.document());
                    let open = session
                        .diagnostics()
                        .iter()
                        .filter(|diagnostic| waivers.of(diagnostic).is_none())
                        .count();
                    open.saturating_add(session.snapshot().map_or(0, |s| s.faults.len()))
                });
                if icons::button_hint(
                    ui,
                    Icon::Warning,
                    tr("Problems"),
                    tr("Show or hide the list of problems"),
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
                    tr("Snap"),
                    tr("Snap wires to pins, junctions and wires"),
                    !self.snap_off,
                )
                .clicked()
                {
                    self.snap_off = !self.snap_off;
                }
                if icons::button_hint(
                    ui,
                    Icon::Grid,
                    tr("Grid"),
                    tr("Show the grid (edit mode)"),
                    !self.grid_off,
                )
                .clicked()
                {
                    self.grid_off = !self.grid_off;
                }
            });
        });
    }

    /// What is drawn over the drawing itself: the box of a selection (solid
    /// when it takes what it contains, dashed when it takes what it
    /// touches), the cross that replaces the cursor and a line about the
    /// view.
    fn paint_over_canvas(
        &self,
        ui: &Ui,
        events: &canvas::CanvasEvents,
        selecting: Option<(circuit_model::GridRect, bool)>,
    ) {
        let tokens = Tokens::of(ui);
        let [left, top] = events.origin;
        let area = egui::Rect::from_min_size(
            egui::pos2(left, top),
            egui::vec2(events.size[0], events.size[1]),
        );
        let painter = ui.painter().with_clip_rect(area);
        let screen = |point: GridPoint| {
            let [x, y] = self.camera.to_screen(ScenePoint::from_grid(point));
            egui::pos2(left + x, top + y)
        };
        if let Some((rect, crossing)) = selecting {
            let corners = [
                screen(rect.min),
                screen(GridPoint::new(rect.max.x, rect.min.y)),
                screen(rect.max),
                screen(GridPoint::new(rect.min.x, rect.max.y)),
            ];
            let color = if crossing {
                tokens.crossing_select
            } else {
                tokens.window_select
            };
            let outline = egui::Rect::from_two_pos(corners[0], corners[2]);
            painter.rect_filled(outline, 0.0, color.gamma_multiply(0.12));
            let stroke = egui::Stroke::new(1.2, color);
            let path = [corners[0], corners[1], corners[2], corners[3], corners[0]];
            if crossing {
                painter.extend(egui::Shape::dashed_line(&path, stroke, 6.0, 4.0));
            } else {
                painter.add(egui::Shape::line(path.to_vec(), stroke));
            }
        }
        let on_drawing = !ui.ctx().is_pointer_over_egui();
        if self.mode == Mode::Edit
            && !self.crosshair_off
            && on_drawing
            && let Some(hover) = events.hover
        {
            ui.ctx().set_cursor_icon(egui::CursorIcon::None);
            let at = egui::pos2(left + hover[0], top + hover[1]);
            let stroke = egui::Stroke::new(1.0, tokens.crosshair);
            for (from, to) in [
                (egui::vec2(-16.0, 0.0), egui::vec2(-3.0, 0.0)),
                (egui::vec2(3.0, 0.0), egui::vec2(16.0, 0.0)),
                (egui::vec2(0.0, -16.0), egui::vec2(0.0, -3.0)),
                (egui::vec2(0.0, 3.0), egui::vec2(0.0, 16.0)),
            ] {
                painter.line_segment([at + from, at + to], stroke);
            }
        }
        let mode = match self.mode {
            Mode::Edit => tr("Drawing"),
            Mode::Simulate => tr("Simulating"),
        };
        let zoom = (self.camera.zoom / DEFAULT_ZOOM * 100.0).round();
        painter.text(
            egui::pos2(left + 10.0, top + 8.0),
            egui::Align2::LEFT_TOP,
            format!("{mode} · {zoom}%"),
            egui::FontId::proportional(11.0),
            tokens.text_faint,
        );
    }

    /// The menu of a secondary click on the drawing: what can be done to the
    /// selection, and the view.
    fn context_menu(&mut self, ui: &mut Ui, events: &canvas::CanvasEvents) {
        if let Some(at) = events.context {
            let [left, top] = events.origin;
            self.context_menu = Some(egui::pos2(left + at[0], top + at[1]));
            // A click on something that is not selected selects it, so the
            // menu acts on what was clicked.
            if self.mode == Mode::Edit
                && let Some(session) = &self.session
                && let Some(point) = self.camera.to_grid(at)
                && let Some(hit) =
                    circuit_editor::hit_test(session.document(), session.catalog(), point)
                && !self.editor.selection().contains(&hit.subject())
            {
                self.editor.set_selection(BTreeSet::from([hit.subject()]));
            }
        }
        let Some(at) = self.context_menu else {
            return;
        };
        let editing = self.mode == Mode::Edit && !self.editor.selection().is_empty();
        let mut chosen: Option<Key> = None;
        let mut fit = false;
        let mut label = false;
        let area = egui::Area::new(ui.scope_id().with(egui::IdSalt::new("context menu")))
            .order(egui::Order::Foreground)
            .fixed_pos(at)
            .show(ui.ctx(), |ui| {
                egui::Frame::menu(ui.style()).show(ui, |ui| {
                    ui.set_min_width(150.0);
                    for (label, key) in [
                        (tr("Rotate selection"), Key::Rotate),
                        (tr("Duplicate selection"), Key::Duplicate),
                        (tr("Copy selection"), Key::Copy),
                        (tr("Delete selection"), Key::Delete),
                    ] {
                        if ui.add_enabled(editing, egui::Button::new(label)).clicked() {
                            chosen = Some(key);
                        }
                    }
                    if ui
                        .add_enabled(self.mode == Mode::Edit, egui::Button::new(tr("Paste")))
                        .clicked()
                    {
                        chosen = Some(Key::Paste);
                    }
                    if ui
                        .add_enabled(editing, egui::Button::new(tr("Edit label")))
                        .clicked()
                    {
                        label = true;
                    }
                    ui.separator();
                    if ui.button(tr("Zoom to the whole drawing")).clicked() {
                        fit = true;
                    }
                });
            });
        // The click that opened the menu is not one that closes it.
        let outside = events.context.is_none() && area.response.clicked_elsewhere();
        if chosen.is_some() || fit || label || outside {
            self.context_menu = None;
        }
        if label {
            self.edit_label();
        }
        if fit {
            self.fit_pending = true;
        }
        if let Some(key) = chosen {
            self.press(key);
        }
    }

    /// The command line over the bottom of the drawing.
    fn command_line_overlay(&mut self, ui: &mut Ui, origin: [f32; 2], size: [f32; 2]) {
        let width = 480.0_f32.min(size[0] - 20.0);
        if width < 200.0 {
            return;
        }
        // Its bottom edge sits a little above the bottom of the drawing and
        // it grows upwards with the lines it shows.
        let at = egui::pos2(origin[0] + size[0] / 2.0, origin[1] + size[1] - 14.0);
        let sent = {
            let catalog = self.session.as_ref().map(Session::catalog);
            self.command_line.show(ui, at, |line| {
                catalog.map_or_else(Vec::new, |catalog| commandline::suggest(line, catalog))
            })
        };
        if let Some(line) = sent {
            self.run_command(&line);
        }
    }

    /// Read a typed line and carry out what it asks.
    fn run_command(&mut self, line: &str) {
        self.command_line.say(&format!("> {line}"));
        let Some(session) = &self.session else {
            self.command_line.say("Open a project first.");
            return;
        };
        let context = commandline::Context {
            doc: session.document(),
            catalog: session.catalog(),
            selection: self.editor.selection(),
        };
        match commandline::run(line, &context) {
            Ok(requests) => {
                for request in requests {
                    self.carry_out(request);
                }
            }
            Err(error) => self.command_line.say(&format!("error: {error}")),
        }
    }

    /// Carry out one request of the command line.
    fn carry_out(&mut self, request: Request) {
        if request.needs_editing() && self.mode != Mode::Edit {
            self.command_line
                .say("That works while drawing: type `mode edit` first.");
            return;
        }
        match request {
            Request::Edit(transaction) => {
                let label = transaction.label.clone();
                self.commit(Ok(transaction));
                match &self.error {
                    Some(error) => self.command_line.say(&format!("error: {error}")),
                    None => self.command_line.say(&label),
                }
            }
            Request::Tool(tool) => self.editor.set_tool(match tool {
                ToolRequest::Select => Tool::Select,
                ToolRequest::Wire => Tool::Wire,
                ToolRequest::Text => Tool::Text,
                ToolRequest::Place {
                    model,
                    symbol,
                    rotation,
                } => Tool::Place {
                    model,
                    symbol,
                    rotation,
                },
            }),
            Request::View(view) => match view {
                ViewRequest::Extents => self.fit_pending = true,
                ViewRequest::Selection => self.frame_selection = true,
                ViewRequest::ZoomIn => self.zoom_request = Some(KEY_ZOOM),
                ViewRequest::ZoomOut => self.zoom_request = Some(1.0 / KEY_ZOOM),
            },
            Request::Mode(mode) => self.set_mode(match mode {
                ModeRequest::Edit => Mode::Edit,
                ModeRequest::Simulate => Mode::Simulate,
            }),
            Request::Simulation(action) => {
                let simulating = self.mode == Mode::Simulate;
                match &mut self.session {
                    Some(session) if simulating && session.can_simulate() => match action {
                        SimRequest::Play => session.play(),
                        SimRequest::Pause => session.pause(),
                        SimRequest::Step => session.step(),
                        SimRequest::Reset => session.reset(),
                    },
                    _ => self
                        .command_line
                        .say("The simulation runs in simulation mode: type `mode simulate`."),
                }
            }
            Request::Undo => self.undo(),
            Request::Redo => self.redo(),
            Request::Save => {
                if self.save() {
                    self.command_line.say("Saved.");
                }
            }
            Request::Message(text) => self.command_line.say(&text),
        }
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
                            if icons::button(ui, Icon::ZoomIn, tr("Zoom in"), false).clicked() {
                                self.zoom_request = Some(KEY_ZOOM);
                            }
                            if icons::button(ui, Icon::ZoomOut, tr("Zoom out"), false).clicked() {
                                self.zoom_request = Some(1.0 / KEY_ZOOM);
                            }
                            if icons::button(ui, Icon::Fit, tr("Zoom extents"), false).clicked() {
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
            let hint = undo_label.map_or_else(
                || tr("Undo").to_owned(),
                |label| trf("Undo: {0}", &[&label]),
            );
            if icons::button_hint(ui, Icon::Undo, tr("Undo"), &hint, false).clicked() {
                self.undo();
            }
        });
        ui.add_enabled_ui(can_redo, |ui| {
            let hint = redo_label.map_or_else(
                || tr("Redo").to_owned(),
                |label| trf("Redo: {0}", &[&label]),
            );
            if icons::button_hint(ui, Icon::Redo, tr("Redo"), &hint, false).clicked() {
                self.redo();
            }
        });
        ui.separator();
        let hint = match self.editor.tool() {
            Tool::Select => tr("Click to select, drag to move, R to rotate, Delete to remove."),
            Tool::Place { .. } => tr("Click to place, R to rotate, Esc to stop placing."),
            Tool::Wire => tr("Click a pin to start, empty space to bend, a pin or wire to finish."),
            Tool::Text => tr("Click to add a note, then type its words in the inspector."),
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
        match self.inspector.take_goto() {
            Some(documentation::Goto::Subject(subject)) => self.select(subject),
            Some(documentation::Goto::Sheet(sheet)) => self.fit_sheet = Some(sheet),
            None => {}
        }
    }

    fn operator(&mut self, ui: &mut Ui) {
        let Some(session) = &mut self.session else {
            return;
        };
        ui.strong(tr("Operator"));
        let operable = session.operable();
        if operable.is_empty() {
            ui.label(tr("No pushbutton, switch or thermal relay."));
        }
        for device in operable {
            let engaged = session.engaged(device.device, device.control);
            let mut label = control_label(&device.reference, device.control, engaged);
            if session.is_pending(device.device) {
                label.push_str(tr(" (next tick)"));
            }
            if ui.button(label).clicked() {
                session.operate(device.device, device.control);
            }
        }
        ui.separator();
        ui.strong(tr("Events"));
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
            ui.label(tr("No project open."));
            return;
        };
        let selected = self.selected_description(session);
        let mut action: Option<PanelAction> = None;
        ui.horizontal(|ui| {
            ui.strong(tr("Diagnostics"));
            if let Some(selected) = selected {
                ui.separator();
                ui.label(selected);
            }
            ui.separator();
            if ui
                .small_button(tr("Save list…"))
                .on_hover_text(tr("Write the problems as a CSV file"))
                .clicked()
            {
                action = Some(PanelAction::Save);
            }
        });
        let diagnostics = session.diagnostics();
        let faults = session.snapshot().map_or(&[][..], |s| s.faults.as_slice());
        if diagnostics.is_empty() && faults.is_empty() {
            ui.label(tr("No diagnostics: the project compiles."));
            if let Some(PanelAction::Save) = action {
                self.save_problems_dialog();
            }
            return;
        }
        let waivers = WaiverIndex::new(session.document());
        let mut waived: Vec<&Diagnostic> = Vec::new();
        egui::ScrollArea::vertical().show(ui, |ui| {
            for (title, list) in [
                (tr("Runtime faults"), faults),
                (tr("Compilation"), diagnostics),
            ] {
                if list.is_empty() {
                    continue;
                }
                let mut heading = Some(title);
                for diagnostic in list {
                    if waivers.of(diagnostic).is_some() {
                        waived.push(diagnostic);
                        continue;
                    }
                    if let Some(title) = heading.take() {
                        ui.label(RichText::new(title).italics());
                    }
                    problem_row(ui, diagnostic, &mut action);
                }
            }
            if let Some((code, subject)) = &self.waiving {
                ui.horizontal(|ui| {
                    ui.label(trf("Reason for waiving {0} on {1}", &[&code, &subject]));
                    let field = ui.add(
                        egui::TextEdit::singleline(&mut self.waive_reason)
                            .id_salt("waive-reason")
                            .desired_width(220.0),
                    );
                    field.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::TextInput, true, tr("Reason"))
                    });
                    if ui.button(tr("Waive it")).clicked() {
                        action = Some(PanelAction::Waive);
                    }
                    if ui.button(tr("Keep it")).clicked() {
                        action = Some(PanelAction::KeepIt);
                    }
                });
            }
            if !waived.is_empty() {
                egui::CollapsingHeader::new(trf("Waived ({0})", &[&waived.len()]))
                    .default_open(false)
                    .show(ui, |ui| {
                        for diagnostic in &waived {
                            let reason = waivers
                                .of(diagnostic)
                                .map_or("", |waiver| waiver.reason.as_str());
                            ui.horizontal(|ui| {
                                ui.label(trf(
                                    "{0} {1}: {2} (reason: {3})",
                                    &[
                                        &diagnostic.code(),
                                        &diagnostic.subject(),
                                        &diagnostic.message(),
                                        &reason,
                                    ],
                                ));
                                let label = trf(
                                    "Restore {0} {1}",
                                    &[&diagnostic.code(), &diagnostic.subject()],
                                );
                                let response = ui.small_button(tr("Restore"));
                                response.widget_info(|| {
                                    egui::WidgetInfo::labeled(
                                        egui::Role::Button,
                                        true,
                                        label.as_str(),
                                    )
                                });
                                if response.clicked() {
                                    action = Some(PanelAction::Restore(
                                        diagnostic.code().as_str().to_owned(),
                                        diagnostic.subject().clone(),
                                    ));
                                }
                            });
                        }
                    });
            }
        });
        match action {
            Some(PanelAction::Select(subject)) => self.select(subject),
            Some(PanelAction::Connect(subject, related)) => {
                if let Some(session) = &self.session {
                    let request = ops::connect_touching(
                        session.document(),
                        session.catalog(),
                        &subject,
                        &related,
                    );
                    self.commit(request);
                }
            }
            Some(PanelAction::StartWaive(code, subject)) => {
                self.waiving = Some((code, subject));
                self.waive_reason.clear();
            }
            Some(PanelAction::Waive) => {
                if let (Some(session), Some((code, subject))) = (&self.session, self.waiving.take())
                {
                    let request =
                        waivers::waive(session.document(), &code, subject, &self.waive_reason);
                    self.commit(request);
                    self.waive_reason.clear();
                }
            }
            Some(PanelAction::KeepIt) => {
                self.waiving = None;
                self.waive_reason.clear();
            }
            Some(PanelAction::Restore(code, subject)) => {
                if let Some(session) = &self.session {
                    let request = waivers::unwaive(session.document(), &code, subject);
                    self.commit(request);
                }
            }
            Some(PanelAction::Save) => self.save_problems_dialog(),
            None => {}
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
        Some(trf(
            "{0} — {1} ({2} v{3}), symbol `{4}`",
            &[
                &device.reference.0,
                &symbol.id,
                &device.model.id,
                &device.model.version,
                &symbol.symbol_key.0,
            ],
        ))
    }

    fn select(&mut self, subject: Subject) {
        self.editor.set_selection(BTreeSet::from([subject.clone()]));
        self.focus = Some(subject);
    }

    /// The scene to draw: in edit mode the preview of the gesture in
    /// progress, with its guide, its selection box and the pin under the
    /// pointer.
    fn scene(&self, session: &Session) -> (Scene, Option<(circuit_model::GridRect, bool)>) {
        let options = SceneOptions {
            selected: self.editor.selection().clone(),
            traced: self.traced.0.clone(),
        };
        if self.mode == Mode::Simulate {
            return (session.scene(&options), None);
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
        let selecting = preview.selection_box.map(|area| (area, preview.crossing));
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
        (scene, selecting)
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
        let (scene, selecting) = self.scene(session);
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
        self.paint_over_canvas(ui, &events, selecting);
        self.label_editor(ui, &scene, &events);
        self.navigation(ui, events.origin, events.size);
        self.command_line_overlay(ui, events.origin, events.size);
        self.context_menu(ui, &events);
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
        (Control::Hold, false) => tr("press"),
        (Control::Hold, true) => tr("release"),
        (Control::Toggle, false) => tr("close"),
        (Control::Toggle, true) => tr("open"),
        (Control::Trip, false) => tr("trip"),
        (Control::Trip, true) => tr("reset"),
    };
    format!("{reference}: {verb}")
}

/// Play, pause, step, reset, fault reset, speed and time.
fn simulation_controls(ui: &mut Ui, session: &mut Session) {
    if session.is_running() {
        if icons::button(ui, Icon::Pause, tr("Pause"), false).clicked() {
            session.pause();
        }
    } else {
        ui.add_enabled_ui(!session.is_faulted(), |ui| {
            if icons::button(ui, Icon::Play, tr("Play"), false).clicked() {
                session.play();
            }
        });
    }
    if icons::button(ui, Icon::Step, tr("Step"), false).clicked() {
        session.step();
    }
    if icons::button(ui, Icon::Reset, tr("Reset"), false).clicked() {
        session.reset();
    }
    if ui
        .add_enabled(session.is_faulted(), egui::Button::new(tr("Reset fault")))
        .clicked()
    {
        session.reset_fault();
    }
    let mut speed = session.clock().speed_index();
    egui::ComboBox::from_label(tr("Speed"))
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
    ui.label(trf(
        "tick {0} · {1} ms",
        &[&session.next_tick(), &elapsed_ms],
    ));
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
