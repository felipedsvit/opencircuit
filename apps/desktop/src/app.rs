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
use circuit_editor::{
    Editor, Hit, Key, Modifiers, Outcome, Pointer, Snap, SnapOptions, SnapTarget, Tool, snap_with,
};
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
use crate::workspace::{Action, ThemeChoice};

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
/// Most views Previous view remembers.
const MAX_VIEWS_BACK: usize = 32;

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
    /// Close the project of the tab shown.
    CloseTab,
    /// Start a new project from the template at this place of
    /// [`crate::templates::TEMPLATES`].
    Template(usize),
    /// Open a saved version as a new, unsaved project.
    #[cfg(not(target_arch = "wasm32"))]
    Restore(crate::versions::Version),
}

/// A project open in a tab that is not the one shown (M9.2): what the
/// window keeps of it while another tab is shown.
#[derive(Debug, Default)]
struct Tab {
    session: Option<Session>,
    mode: Mode,
    editor: Editor,
    inspector: InspectorState,
    camera: Camera,
    view_back: Vec<Camera>,
    view_settled: Option<Camera>,
    path_input: String,
    #[cfg(not(target_arch = "wasm32"))]
    autosave: Option<crate::recovery::Autosave>,
    #[cfg(not(target_arch = "wasm32"))]
    project_lock: Option<crate::project_lock::ProjectLock>,
}

/// Most projects a window keeps open in tabs.
pub const MAX_TABS: usize = 16;

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
    /// The views left with a zoom, a pan or a jump, newest last (View >
    /// Previous view, Alt+Left).
    view_back: Vec<Camera>,
    /// The view as it was when the pointer and the wheel last rested.
    view_settled: Option<Camera>,
    /// The wire tool also snaps to the ends, middles and crossings of wires.
    wire_points: bool,
    /// The element under the pointer is outlined.
    hover_highlight: bool,
    /// Rulers along the top and left edges of the drawing.
    rulers: bool,
    /// A point being typed for the next click (G): the text, the grid point
    /// a relative point starts from, where the field is on screen, and
    /// whether the field still has to take the keyboard.
    point_entry: Option<(String, GridPoint, egui::Pos2, bool)>,
    /// The print preview (File > Print preview).
    print_preview: crate::print_preview::PrintPreview,
    /// The grid point the context menu was opened at.
    context_point: Option<GridPoint>,
    /// The projects of the tabs not shown, in tab order without the shown
    /// one, which is at `active_tab`.
    tabs: Vec<Tab>,
    /// Where the shown project is among the tabs.
    active_tab: usize,
    /// The store and interval of the autosave, for the autosave of each new
    /// tab, and the panic copy of the first one, which writes those of the
    /// others too.
    #[cfg(not(target_arch = "wasm32"))]
    recovery_setup: Option<(
        crate::recovery::RecoveryStore,
        f64,
        crate::recovery::PanicCopy,
    )>,
    /// The keys of the customizable shortcuts and the named workspaces.
    settings: crate::workspace::Settings,
    /// The colours chosen in View > Theme.
    theme_choice: crate::workspace::ThemeChoice,
    /// The colours egui was last told to use.
    theme_applied: Option<crate::workspace::ThemeChoice>,
    /// The palette, inspector and operator panel are collapsed.
    side_panels_hidden: bool,
    /// The tool bar is hidden.
    toolbar_hidden: bool,
    /// The palette is on the right, the inspector and operator on the left.
    panels_swapped: bool,
    /// The name typed for a new workspace in View > Workspaces.
    workspace_name: String,
    /// The window that gives shortcuts other keys, while open: the action
    /// waiting for its new keys, and the last message.
    shortcut_editor: Option<(Option<crate::workspace::Action>, String)>,
    /// Where the settings are kept between sessions; off unless `main` turns
    /// it on.
    #[cfg(not(target_arch = "wasm32"))]
    preferences: Option<crate::workspace::Preferences>,
    /// The settings as last written, to write them again only on a change.
    #[cfg(not(target_arch = "wasm32"))]
    settings_stored: Option<crate::workspace::Settings>,
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
        let autosave = crate::recovery::Autosave::new(store.clone(), interval_seconds, 0.0);
        self.recover_offer = autosave
            .store()
            .orphans(autosave.key(), autosave.stale_after());
        self.recovery_setup = Some((store, interval_seconds, autosave.panic_copy()));
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

    /// Keep the workspace (theme, panels, view switches), the keys of the
    /// shortcuts and the named workspaces in `preferences`, and bring back
    /// what it holds.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn enable_preferences(&mut self, preferences: crate::workspace::Preferences) {
        let settings = preferences.load();
        self.apply_layout(&settings.layout);
        self.settings = settings;
        self.settings_stored = Some(self.settings_now());
        self.preferences = Some(preferences);
    }

    /// The settings as the window is now.
    #[cfg(not(target_arch = "wasm32"))]
    fn settings_now(&self) -> crate::workspace::Settings {
        let mut settings = self.settings.clone();
        settings.format = 1;
        settings.layout = self.layout();
        settings
    }

    /// Write the settings when they changed since they were last written.
    #[cfg(not(target_arch = "wasm32"))]
    fn store_settings(&mut self) {
        let Some(preferences) = &self.preferences else {
            return;
        };
        let now = self.settings_now();
        if self.settings_stored.as_ref() == Some(&now) {
            return;
        }
        if let Err(error) = preferences.store(&now) {
            self.error = Some(error);
        }
        // Written or not, it is not tried again every frame.
        self.settings_stored = Some(now);
    }

    /// The theme, panels and view switches of the window.
    #[must_use]
    pub fn layout(&self) -> crate::workspace::Layout {
        crate::workspace::Layout {
            theme: self.theme_choice,
            side_panels_hidden: self.side_panels_hidden,
            toolbar_hidden: self.toolbar_hidden,
            panels_swapped: self.panels_swapped,
            grid_off: self.grid_off,
            snap_off: self.snap_off,
            crosshair_off: self.crosshair_off,
            problems_hidden: self.diagnostics_hidden,
            wire_points: self.wire_points,
            hover_highlight: self.hover_highlight,
            rulers: self.rulers,
        }
    }

    /// Give the window the theme, panels and view switches of `layout`.
    pub fn apply_layout(&mut self, layout: &crate::workspace::Layout) {
        self.theme_choice = layout.theme;
        self.side_panels_hidden = layout.side_panels_hidden;
        self.toolbar_hidden = layout.toolbar_hidden;
        self.panels_swapped = layout.panels_swapped;
        self.grid_off = layout.grid_off;
        self.snap_off = layout.snap_off;
        self.crosshair_off = layout.crosshair_off;
        self.diagnostics_hidden = layout.problems_hidden;
        self.wire_points = layout.wire_points;
        self.hover_highlight = layout.hover_highlight;
        self.rulers = layout.rulers;
    }

    /// The keys of the customizable shortcuts.
    #[must_use]
    pub const fn keymap(&self) -> &crate::workspace::Keymap {
        &self.settings.keys
    }

    /// Give `action` the keys `chord`, as the shortcut window does.
    ///
    /// # Errors
    ///
    /// The keys are reserved or belong to another action.
    pub fn set_shortcut(
        &mut self,
        action: Action,
        chord: crate::workspace::Chord,
    ) -> Result<(), crate::workspace::KeymapError> {
        self.settings.keys.set(action, chord)
    }

    /// Keep the window's layout as the workspace `name` (View > Workspaces).
    pub fn save_workspace(&mut self, name: &str) -> bool {
        let layout = self.layout();
        match self.settings.save_workspace(name, layout) {
            Ok(()) => true,
            Err(message) => {
                self.error = Some(tr(message).to_owned());
                false
            }
        }
    }

    /// Bring back the workspace `name`; whether there is one.
    pub fn open_workspace(&mut self, name: &str) -> bool {
        let Some(layout) = self
            .settings
            .workspaces
            .iter()
            .find(|workspace| workspace.name == name)
            .map(|workspace| workspace.layout.clone())
        else {
            return false;
        };
        self.apply_layout(&layout);
        true
    }

    /// The names of the workspaces, in the order they were saved.
    #[must_use]
    pub fn workspace_names(&self) -> Vec<&str> {
        self.settings
            .workspaces
            .iter()
            .map(|workspace| workspace.name.as_str())
            .collect()
    }

    /// Whether the palette, inspector and operator panel are shown.
    #[must_use]
    pub const fn side_panels_shown(&self) -> bool {
        !self.side_panels_hidden
    }

    /// Show or collapse the side panels.
    pub const fn show_side_panels(&mut self, shown: bool) {
        self.side_panels_hidden = !shown;
    }

    /// Open the window that gives shortcuts other keys.
    pub fn open_shortcut_editor(&mut self) {
        self.shortcut_editor = Some((None, String::new()));
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

    /// Go back to the view before the last zoom, pan or jump, as Alt+Left
    /// does. Returns whether there was one.
    pub fn previous_view(&mut self) -> bool {
        match self.view_back.pop() {
            Some(camera) => {
                self.camera = camera;
                self.view_settled = Some(camera);
                true
            }
            None => false,
        }
    }

    /// How many views Previous view can go back.
    #[must_use]
    pub fn views_back(&self) -> usize {
        self.view_back.len()
    }

    /// Snap the wire tool to the ends, middles and crossings of wires too.
    pub fn set_wire_point_snap(&mut self, on: bool) {
        self.wire_points = on;
    }

    /// Start typing the grid point of the next click, as G does in edit
    /// mode: `x,y`, or `@dx,dy` from the point under the pointer.
    pub fn type_point(&mut self) {
        if self.mode != Mode::Edit || self.session.is_none() {
            return;
        }
        let from = self.cursor.unwrap_or(GridPoint::new(0, 0));
        let at = self
            .grid_to_screen(from)
            .map_or(egui::pos2(200.0, 200.0), |[x, y]| {
                egui::pos2(x + 12.0, y + 12.0)
            });
        self.point_entry = Some((String::new(), from, at, true));
    }

    /// Whether a point is being typed.
    #[must_use]
    pub const fn typing_point(&self) -> bool {
        self.point_entry.is_some()
    }

    /// Press and release the primary button at `point` with the tool in
    /// hand, as a click on the drawing does.
    fn click_at(&mut self, point: GridPoint) {
        let Some(session) = &self.session else {
            return;
        };
        let (doc, catalog) = (session.document(), session.catalog());
        self.editor.pointer_move(doc, catalog, point);
        let down = self
            .editor
            .pointer_down(doc, catalog, point, Modifiers::default());
        self.apply(down);
        if let Some(session) = &self.session {
            let up = self
                .editor
                .pointer_up(session.document(), session.catalog(), point);
            self.apply(up);
        }
    }

    /// The field of a typed point; Enter clicks there, Esc drops it.
    fn point_entry(&mut self, ui: &Ui) {
        if self.mode != Mode::Edit {
            self.point_entry = None;
        }
        let Some((text, from, at, focus)) = &mut self.point_entry else {
            return;
        };
        let mut finished = None;
        egui::Area::new(ui.scope_id().with(egui::IdSalt::new("point entry")))
            .order(egui::Order::Foreground)
            .fixed_pos(*at)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    let label = ui.label(tr("Point"));
                    let field = ui
                        .add(
                            egui::TextEdit::singleline(text)
                                .id_salt("point entry input")
                                .hint_text(tr("x,y or @dx,dy"))
                                .desired_width(110.0),
                        )
                        .labelled_by(label.id);
                    if std::mem::take(focus) {
                        field.request_focus();
                    }
                    if field.lost_focus() {
                        let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
                        finished = Some(enter.then(|| commandline::typed_point(text, *from)));
                    }
                });
            });
        match finished {
            None => {}
            Some(None) => self.point_entry = None,
            Some(Some(None)) => {
                self.error = Some(tr("Type a point as x,y or @dx,dy.").to_owned());
                self.point_entry = None;
            }
            Some(Some(Some(point))) => {
                self.point_entry = None;
                self.click_at(point);
            }
        }
    }

    /// Start recording a macro of the edits of the project.
    pub fn start_macro(&mut self) {
        if let Some(session) = &mut self.session {
            session.start_recording();
        }
    }

    /// How many steps the macro being recorded has, and how many changes it
    /// left out; `None` when none is being recorded.
    #[must_use]
    pub fn macro_steps(&self) -> Option<(usize, usize)> {
        self.session
            .as_ref()
            .and_then(Session::recording)
            .map(|recorder| (recorder.steps(), recorder.left_out()))
    }

    /// Stop recording and return the macro as an edit file.
    pub fn finish_macro(&mut self) -> Option<String> {
        self.session
            .as_mut()
            .and_then(Session::stop_recording)
            .map(|recorder| recorder.to_edit_file())
    }

    /// Stop recording and save the macro where the person chooses; a
    /// browser downloads it.
    fn save_macro_dialog(&mut self) {
        let Some(text) = self.finish_macro() else {
            return;
        };
        #[cfg(target_arch = "wasm32")]
        if let Err(error) = crate::web::download("macro.json", text.as_bytes(), "application/json")
        {
            self.error = Some(error);
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(path) = dialogs::choose_macro_path()
            && let Err(error) = crate::files::write_atomic(&path, text)
        {
            self.error = Some(error.to_string());
        }
    }

    /// Play a macro (an edit file) on the project, as one step one undo
    /// takes back.
    pub fn play_macro_text(&mut self, text: &str) {
        let Some(session) = &mut self.session else {
            return;
        };
        match session.play_macro(text) {
            Ok(_) => self.error = None,
            Err(error) => self.error = Some(trf("The macro was not played: {0}", &[&error])),
        }
        self.inspector.clear();
        if let Some(session) = &self.session {
            self.editor.retain_existing(session.document());
        }
    }

    /// Open the print preview of the project.
    pub fn open_print_preview(&mut self) {
        if let Some(session) = &self.session {
            self.print_preview.open(session);
        }
    }

    /// The pages of the print preview and the page shown, from 0, while it
    /// is open.
    #[must_use]
    pub fn print_preview_page(&self) -> Option<(usize, usize)> {
        self.print_preview
            .pages()
            .map(|pages| (self.print_preview.page(), pages))
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
        if self.allow_close || !self.any_dirty() {
            return true;
        }
        // The question is about the project shown: a tab with changes is
        // shown first.
        if !self.is_dirty()
            && let Some(index) = self.dirty_tab()
        {
            self.switch_tab(index);
        }
        self.confirm = Some(Pending::Close);
        false
    }

    /// Whether the project shown or one in another tab has unsaved changes.
    #[must_use]
    pub fn any_dirty(&self) -> bool {
        self.is_dirty() || self.dirty_tab().is_some()
    }

    /// The place among the tabs of the first project not shown that has
    /// unsaved changes.
    fn dirty_tab(&self) -> Option<usize> {
        let at = self
            .tabs
            .iter()
            .position(|tab| tab.session.as_ref().is_some_and(Session::is_dirty))?;
        Some(if at >= self.active_tab {
            at.saturating_add(1)
        } else {
            at
        })
    }

    /// How many projects are open, in tabs; one or none without tabs.
    #[must_use]
    pub fn tab_count(&self) -> usize {
        self.tabs
            .len()
            .saturating_add(usize::from(self.session.is_some()))
    }

    /// Where the shown project is among the tabs.
    #[must_use]
    pub const fn active_tab(&self) -> usize {
        self.active_tab
    }

    /// The names of the projects in the tabs, in tab order.
    #[must_use]
    pub fn tab_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .tabs
            .iter()
            .map(|tab| {
                tab.session.as_ref().map_or_else(String::new, |session| {
                    session.document().project.name.clone()
                })
            })
            .collect();
        if let Some(session) = &self.session {
            let at = self.active_tab.min(names.len());
            names.insert(at, session.document().project.name.clone());
        }
        names
    }

    /// Swap what the window shows with what `tab` keeps.
    fn swap_tab(&mut self, tab: &mut Tab) {
        std::mem::swap(&mut self.session, &mut tab.session);
        std::mem::swap(&mut self.mode, &mut tab.mode);
        std::mem::swap(&mut self.editor, &mut tab.editor);
        std::mem::swap(&mut self.inspector, &mut tab.inspector);
        std::mem::swap(&mut self.camera, &mut tab.camera);
        std::mem::swap(&mut self.view_back, &mut tab.view_back);
        std::mem::swap(&mut self.view_settled, &mut tab.view_settled);
        std::mem::swap(&mut self.path_input, &mut tab.path_input);
        #[cfg(not(target_arch = "wasm32"))]
        {
            std::mem::swap(&mut self.autosave, &mut tab.autosave);
            std::mem::swap(&mut self.project_lock, &mut tab.project_lock);
        }
        // What was under way in the other tab does not carry over.
        self.held = None;
        self.label_edit = None;
        self.point_entry = None;
        self.context_menu = None;
        self.waiving = None;
        self.focus = None;
        self.snapped = None;
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.history = None;
        }
    }

    /// Show the project of the tab at `index`.
    pub fn switch_tab(&mut self, index: usize) {
        if index == self.active_tab || index >= self.tab_count() || self.session.is_none() {
            return;
        }
        let mut shown = Tab::default();
        self.swap_tab(&mut shown);
        let at = self.active_tab.min(self.tabs.len());
        self.tabs.insert(at, shown);
        if index < self.tabs.len() {
            let mut target = self.tabs.remove(index);
            self.swap_tab(&mut target);
            self.active_tab = index;
        }
    }

    /// Put the shown project in a tab of its own and show an empty one,
    /// with an autosave of its own; `false` when no other tab can be opened.
    fn push_tab(&mut self) -> bool {
        if self.session.is_none() {
            return true;
        }
        if self.tab_count() >= MAX_TABS {
            self.error = Some(trf("At most {0} projects are open at once.", &[&MAX_TABS]));
            return false;
        }
        let mut shown = Tab::default();
        #[cfg(not(target_arch = "wasm32"))]
        if let Some((store, interval, root)) = &self.recovery_setup {
            let autosave = crate::recovery::Autosave::new(store.clone(), *interval, 0.0);
            root.adopt(autosave.panic_copy());
            shown.autosave = Some(autosave);
        }
        self.swap_tab(&mut shown);
        let at = self.active_tab.min(self.tabs.len());
        self.tabs.insert(at, shown);
        self.active_tab = self.tabs.len();
        true
    }

    /// Take back the tab just pushed when nothing was opened in it.
    fn pop_empty_tab(&mut self, before: usize) {
        if self.session.is_none() && !self.tabs.is_empty() && self.active_tab != before {
            let index = before.min(self.tabs.len().saturating_sub(1));
            let mut target = self.tabs.remove(index);
            self.swap_tab(&mut target);
            self.active_tab = index;
        }
    }

    /// A new, empty project in a tab of its own; the shown one stays open.
    pub fn new_tab(&mut self) {
        let before = self.active_tab;
        if self.push_tab() {
            self.perform(Pending::New);
            self.pop_empty_tab(before);
        }
    }

    /// Open the project at `path` in a tab of its own; the shown one stays
    /// open. A project open already in a tab is shown instead.
    pub fn open_in_new_tab(&mut self, path: &Path) {
        if let Some(index) = self.tab_of(path) {
            self.switch_tab(index);
            return;
        }
        let before = self.active_tab;
        if self.push_tab() {
            self.open_path(path);
            self.pop_empty_tab(before);
        }
    }

    /// The tab whose project was opened from or saved to `path`.
    fn tab_of(&self, path: &Path) -> Option<usize> {
        let same = |session: &Session| session.path().is_some_and(|own| own == path);
        if self.session.as_ref().is_some_and(same) {
            return Some(self.active_tab);
        }
        let at = self
            .tabs
            .iter()
            .position(|tab| tab.session.as_ref().is_some_and(same))?;
        Some(if at >= self.active_tab {
            at.saturating_add(1)
        } else {
            at
        })
    }

    /// Close the project shown, asking first about unsaved changes; the
    /// next tab is shown.
    pub fn close_tab(&mut self) {
        if self.session.is_some() {
            self.guard(Pending::CloseTab);
        }
    }

    /// Drop the project shown, whose changes were saved or dropped on
    /// purpose, and show the tab next to it.
    fn drop_shown(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(autosave) = &mut self.autosave {
            autosave.clear();
        }
        if self.tabs.is_empty() {
            let mut empty = Tab::default();
            // The first tab keeps the autosave of the window.
            #[cfg(not(target_arch = "wasm32"))]
            {
                empty.autosave = self.autosave.take();
            }
            self.swap_tab(&mut empty);
            self.active_tab = 0;
            self.error = None;
            return;
        }
        let index = self.active_tab.min(self.tabs.len().saturating_sub(1));
        let mut target = self.tabs.remove(index);
        self.swap_tab(&mut target);
        self.active_tab = index;
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
                // Another tab with unsaved changes is asked about next; the
                // project answered for is closed.
                if let Some(index) = self.dirty_tab() {
                    self.drop_shown();
                    let index = if index > self.active_tab {
                        index.saturating_sub(1)
                    } else {
                        index
                    };
                    self.switch_tab(index);
                    self.confirm = Some(Pending::Close);
                    return;
                }
                // Closed on purpose: no copy is kept for a recovery.
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(autosave) = &mut self.autosave {
                    autosave.clear();
                }
                self.allow_close = true;
                self.close_now = true;
            }
            Pending::CloseTab => self.drop_shown(),
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
        // The projects of the other tabs are kept too.
        for tab in &mut self.tabs {
            if let Some(autosave) = &mut tab.autosave {
                autosave.tick(now, tab.session.as_ref());
            }
        }
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
        if self.print_preview.show(ui, self.session.as_ref())
            == Some(crate::print_preview::Action::ExportPdf)
        {
            self.export_dialog("pdf");
        }
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
        if self.settings.keys.clone().pressed(ui, Action::CommandLine) {
            self.command_line.focus();
        }
        // The shortcut window takes the keys it waits for before any
        // shortcut sees them.
        self.shortcut_window(ui);
        if self.mode == Mode::Edit {
            self.shortcuts(ui);
        }
        self.view_keys(ui);

        if !self.styled {
            theme::apply_style(ui.ctx());
            self.styled = true;
        }
        // A fresh window never tells egui a theme: it follows the system as
        // before.
        if self.theme_applied.unwrap_or_default() != self.theme_choice {
            ui.ctx().set_theme(self.theme_choice.preference());
            self.theme_applied = Some(self.theme_choice);
        }
        let tokens = Tokens::of(ui);
        self.update_trace();
        self.menu_bar(ui);
        #[cfg(not(target_arch = "wasm32"))]
        self.store_settings();
        if !self.toolbar_hidden {
            egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        }
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
        // The side panels go on the side the workspace puts them on.
        type Side = fn(&'static str) -> egui::Panel;
        let (palette_panel, inspector_panel): (Side, Side) = if self.panels_swapped {
            (egui::Panel::right, egui::Panel::left)
        } else {
            (egui::Panel::left, egui::Panel::right)
        };
        match self.mode {
            _ if self.side_panels_hidden => {}
            Mode::Edit if self.session.is_some() => {
                palette_panel("palette")
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
                inspector_panel("inspector")
                    .resizable(true)
                    .default_size(280.0)
                    .show(ui, |ui| self.inspector(ui));
            }
            Mode::Simulate if self.session.as_ref().is_some_and(Session::can_simulate) => {
                inspector_panel("operator")
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
        let keys = self.settings.keys.clone();
        if keys.pressed(ui, Action::WireTool) {
            self.editor.set_tool(Tool::Wire);
        }
        if keys.pressed(ui, Action::FindReplace) {
            self.find_replace.open();
        }
        if keys.pressed(ui, Action::EditLabel) {
            self.edit_label();
        }
        if keys.pressed(ui, Action::TypePoint) {
            self.type_point();
        }
        let mut actions = Vec::new();
        if keys.pressed(ui, Action::SelectAll) {
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
        if keys.pressed(ui, Action::Duplicate) {
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
        if keys.pressed(ui, Action::Rotate) {
            actions.push(Key::Rotate);
        }
        if keys.pressed(ui, Action::Mirror) {
            actions.push(Key::Mirror);
        }
        for (key, action) in [
            (egui::Key::Delete, Key::Delete),
            (egui::Key::Backspace, Key::Delete),
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
        let keys = self.settings.keys.clone();
        // The command palette, in both modes.
        if keys.pressed(ui, Action::CommandPalette) {
            self.command_palette.open();
        }
        if keys.pressed(ui, Action::PreviousView) {
            self.previous_view();
        }
        if keys.pressed(ui, Action::SidePanels) {
            self.side_panels_hidden = !self.side_panels_hidden;
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
        if keys.pressed(ui, Action::FitDrawing) {
            self.fit_pending = true;
        }
        if keys.pressed(ui, Action::FitSelection) && !self.editor.selection().is_empty() {
            self.frame_selection = true;
        }
        if keys.pressed(ui, Action::TraceNet) {
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
                    if ui
                        .add_enabled(has_session, egui::Button::new(tr("Print preview…")))
                        .clicked()
                    {
                        ui.close();
                        self.open_print_preview();
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
                    ui.separator();
                    if ui.button(tr("New tab")).clicked() {
                        ui.close();
                        self.new_tab();
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    if ui.button(tr("Open in a new tab…")).clicked() {
                        ui.close();
                        if let Some(path) = dialogs::pick_project() {
                            self.open_in_new_tab(&path);
                        }
                    }
                    if ui
                        .add_enabled(has_session, egui::Button::new(tr("Close tab")))
                        .clicked()
                    {
                        ui.close();
                        self.close_tab();
                    }
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
                    let wires: Vec<circuit_model::WireId> = self
                        .editor
                        .selection()
                        .iter()
                        .filter_map(|subject| match subject {
                            Subject::Wire(id) => Some(*id),
                            _ => None,
                        })
                        .collect();
                    if ui
                        .add_enabled(
                            editing && !wires.is_empty(),
                            egui::Button::new(tr("Route the selected wires")),
                        )
                        .on_hover_text(tr(
                            "An orthogonal route around the symbols, undone in one step",
                        ))
                        .clicked()
                    {
                        ui.close();
                        if let Some(session) = &self.session {
                            let request = circuit_commands::route::route(
                                session.document(),
                                session.catalog(),
                                &wires,
                            );
                            self.commit(request);
                        }
                    }
                    let recording = self.macro_steps().is_some();
                    if !recording
                        && ui
                            .add_enabled(has_session, egui::Button::new(tr("Record macro")))
                            .on_hover_text(tr("Record the next edits as an edit file to play back"))
                            .clicked()
                    {
                        ui.close();
                        self.start_macro();
                    }
                    if recording
                        && ui
                            .button(tr("Stop recording and save the macro…"))
                            .clicked()
                    {
                        ui.close();
                        self.save_macro_dialog();
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    if ui
                        .add_enabled(editing, egui::Button::new(tr("Play macro…")))
                        .clicked()
                    {
                        ui.close();
                        if let Some(path) = dialogs::pick_macro() {
                            match crate::files::read_text(
                                &path,
                                circuit_commands::script::MAX_EDIT_BYTES,
                            ) {
                                Ok(text) => self.play_macro_text(&text),
                                Err(error) => self.error = Some(error.to_string()),
                            }
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
                    ui.separator();
                    let selected = editing && !self.editor.selection().is_empty();
                    if ui
                        .add_enabled(selected, egui::Button::new(tr("Mirror selection")))
                        .on_hover_text("M")
                        .clicked()
                    {
                        ui.close();
                        self.run_command("mirror");
                    }
                    if ui
                        .add_enabled(selected, egui::Button::new(tr("Group selection")))
                        .clicked()
                    {
                        ui.close();
                        let number = self
                            .session
                            .as_ref()
                            .map_or(0, |session| session.document().groups.len())
                            .saturating_add(1);
                        self.run_command(&format!("group Group {number}"));
                    }
                    if ui
                        .add_enabled(selected, egui::Button::new(tr("Ungroup selection")))
                        .clicked()
                    {
                        ui.close();
                        self.run_command("ungroup");
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    if ui
                        .add_enabled(editing, egui::Button::new(tr("Import catalog data (CSV)…")))
                        .on_hover_text(tr(
                            "Manufacturer, part number, description, location and datasheet of each device, by designator",
                        ))
                        .clicked()
                    {
                        ui.close();
                        if let Some(path) = dialogs::pick_csv() {
                            match crate::files::read_text(&path, circuit_commands::script::MAX_EDIT_BYTES) {
                                Ok(text) => self.import_catalog_text(&text),
                                Err(error) => self.error = Some(error.to_string()),
                            }
                        }
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
                    if ui
                        .add_enabled(
                            !self.view_back.is_empty(),
                            egui::Button::new(tr("Previous view")),
                        )
                        .on_hover_text("Alt+Left")
                        .clicked()
                    {
                        self.previous_view();
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
                    ui.checkbox(
                        &mut self.wire_points,
                        tr("Snap to wire ends, middles and crossings"),
                    );
                    ui.checkbox(
                        &mut self.hover_highlight,
                        tr("Outline the element under the pointer"),
                    );
                    ui.checkbox(&mut self.rulers, tr("Show rulers"));
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
                    ui.separator();
                    self.workspace_menu(ui);
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

    /// The end of the View menu: the theme, the panels, the named
    /// workspaces and the keys of the shortcuts.
    fn workspace_menu(&mut self, ui: &mut Ui) {
        ui.menu_button(tr("Theme"), |ui| {
            for choice in ThemeChoice::ALL {
                if ui
                    .radio_value(&mut self.theme_choice, choice, tr(choice.name()))
                    .clicked()
                {
                    ui.close();
                }
            }
        });
        let mut side = !self.side_panels_hidden;
        if ui
            .checkbox(&mut side, tr("Show the side panels"))
            .on_hover_text(self.settings.keys.chord(Action::SidePanels).text())
            .changed()
        {
            self.side_panels_hidden = !side;
        }
        let mut toolbar = !self.toolbar_hidden;
        if ui.checkbox(&mut toolbar, tr("Show the tool bar")).changed() {
            self.toolbar_hidden = !toolbar;
        }
        ui.checkbox(
            &mut self.panels_swapped,
            tr("Palette on the right, inspector on the left"),
        );
        ui.menu_button(tr("Workspaces"), |ui| {
            let names: Vec<String> = self
                .workspace_names()
                .into_iter()
                .map(str::to_owned)
                .collect();
            if names.is_empty() {
                ui.label(RichText::new(tr("No saved workspace")).weak());
            }
            for name in &names {
                ui.horizontal(|ui| {
                    if ui.button(name).clicked() {
                        ui.close();
                        self.open_workspace(name);
                    }
                    if ui
                        .small_button("×")
                        .on_hover_text(tr("Delete this workspace"))
                        .clicked()
                    {
                        self.settings.delete_workspace(name);
                    }
                });
            }
            ui.separator();
            ui.horizontal(|ui| {
                let field = ui.add(
                    egui::TextEdit::singleline(&mut self.workspace_name)
                        .hint_text(tr("name"))
                        .desired_width(120.0),
                );
                let entered =
                    field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
                if (ui.button(tr("Save the current layout")).clicked() || entered)
                    && self.save_workspace(&self.workspace_name.clone())
                {
                    self.workspace_name.clear();
                    ui.close();
                }
            });
        });
        if ui.button(tr("Customize shortcuts…")).clicked() {
            ui.close();
            self.open_shortcut_editor();
        }
    }

    /// The window that gives shortcuts other keys: a row per action with its
    /// keys, a button that waits for the new keys, and one that puts the
    /// keys of a fresh window back.
    fn shortcut_window(&mut self, ui: &Ui) {
        let Some((mut waiting, mut message)) = self.shortcut_editor.take() else {
            return;
        };
        // The next key pressed, with its modifiers, goes to the waiting
        // action; Escape stops waiting.
        if let Some(action) = waiting {
            let pressed = ui.input(|input| {
                input.events.iter().find_map(|event| match event {
                    egui::Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => Some(crate::workspace::Chord::pressed(*key, *modifiers)),
                    _ => None,
                })
            });
            if let Some(chord) = pressed {
                ui.input_mut(|input| {
                    input
                        .events
                        .retain(|event| !matches!(event, egui::Event::Key { .. }));
                });
                waiting = None;
                if chord.key == egui::Key::Escape && chord.modifiers == crate::workspace::Mods::NONE
                {
                    message.clear();
                } else {
                    message = match self.settings.keys.set(action, chord) {
                        Ok(()) => String::new(),
                        Err(crate::workspace::KeymapError::Reserved(chord)) => {
                            trf("{0} is kept for its usual use.", &[&chord.text()])
                        }
                        Err(crate::workspace::KeymapError::Taken(chord, other)) => trf(
                            "{0} already does: {1}.",
                            &[&chord.text(), &tr(other.description())],
                        ),
                    };
                }
            }
        }
        let mut open = true;
        egui::Window::new(tr("Customize shortcuts"))
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                egui::Grid::new("custom_keys")
                    .num_columns(3)
                    .show(ui, |ui| {
                        for action in Action::ALL {
                            ui.label(tr(action.description()));
                            let keys = if waiting == Some(action) {
                                tr("press the new keys…").to_owned()
                            } else {
                                self.settings.keys.chord(action).text()
                            };
                            if ui.button(RichText::new(keys).monospace()).clicked() {
                                waiting = Some(action);
                                message.clear();
                            }
                            if ui
                                .add_enabled(
                                    self.settings.keys.is_changed(action),
                                    egui::Button::new(tr("Default")),
                                )
                                .clicked()
                                && !self.settings.keys.reset(action)
                            {
                                message = tr(
                                "Another shortcut has these keys now; give it other keys first.",
                            )
                            .to_owned();
                            }
                            ui.end_row();
                        }
                    });
                if !message.is_empty() {
                    ui.colored_label(ui.visuals().warn_fg_color, &message);
                }
                if ui.button(tr("All shortcuts to their defaults")).clicked() {
                    self.settings.keys.reset_all();
                    waiting = None;
                    message.clear();
                }
            });
        if open {
            self.shortcut_editor = Some((waiting, message));
        }
    }

    /// Whether the window that gives shortcuts other keys is open.
    #[must_use]
    pub const fn shortcut_editor_open(&self) -> bool {
        self.shortcut_editor.is_some()
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
                        // A shortcut given other keys shows them.
                        if let Some(custom) = Action::for_keys_row(keys)
                            .filter(|action| self.settings.keys.is_changed(*action))
                        {
                            ui.monospace(self.settings.keys.chord(custom).text());
                            ui.label(tr(action));
                            ui.end_row();
                            continue;
                        }
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
        if !self.tabs.is_empty() {
            self.document_tabs(ui);
            return;
        }
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

    /// One tab per open project, when there are several: the shown one as
    /// the single tab is drawn, the others as buttons that show them, each
    /// with a button that closes it.
    fn document_tabs(&mut self, ui: &mut Ui) {
        let tokens = Tokens::of(ui);
        let names = self.tab_names();
        let dirty: Vec<bool> = {
            let mut dirty: Vec<bool> = self
                .tabs
                .iter()
                .map(|tab| tab.session.as_ref().is_some_and(Session::is_dirty))
                .collect();
            dirty.insert(self.active_tab.min(dirty.len()), self.is_dirty());
            dirty
        };
        let mut show = None;
        let mut close = None;
        ui.horizontal(|ui| {
            for (index, (name, unsaved)) in names.iter().zip(dirty).enumerate() {
                let mut text = name.clone();
                if unsaved {
                    text.push_str(tr(" (unsaved)"));
                }
                if index == self.active_tab {
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
                            ui.label(RichText::new(text).strong());
                        });
                } else if ui
                    .add(egui::Button::new(text).frame(false))
                    .on_hover_text(tr("Show this project"))
                    .clicked()
                {
                    show = Some(index);
                }
                let label = trf("Close {0}", &[name]);
                let response = ui.small_button("×").on_hover_text(label.as_str());
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Button, true, label.as_str())
                });
                if response.clicked() {
                    close = Some(index);
                }
            }
        });
        if let Some(index) = close {
            self.switch_tab(index);
            self.close_tab();
        } else if let Some(index) = show {
            self.switch_tab(index);
        }
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
            if let Some((steps, left_out)) = self.macro_steps() {
                ui.separator();
                let text = if left_out == 0 {
                    trf("Recording macro: {0} steps", &[&steps])
                } else {
                    trf(
                        "Recording macro: {0} steps, {1} left out",
                        &[&steps, &left_out],
                    )
                };
                ui.colored_label(ui.visuals().warn_fg_color, text)
                    .on_hover_text(tr(
                        "A change left out (a hand-drawn wire route, a layer, a supply) cannot be written as an operation of an edit file",
                    ));
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

    /// The wire tools the context menu offers at its point, in edit mode:
    /// trim the wire there, extend a free end there, and copy the selected
    /// wires alongside, two grid units across their first segment.
    fn wire_tool_requests(&self) -> Vec<(&'static str, inspector::Request)> {
        use circuit_commands::wire_tools;
        let (Some(session), Some(at)) = (&self.session, self.context_point) else {
            return Vec::new();
        };
        if self.mode != Mode::Edit {
            return Vec::new();
        }
        let (doc, catalog) = (session.document(), session.catalog());
        let mut offered = Vec::new();
        if let Some(wire) = wire_tools::wire_at(doc, catalog, at) {
            let trimmed = wire_tools::trim(doc, catalog, wire, at);
            if trimmed.is_ok() {
                offered.push((tr("Trim wire here"), trimmed));
            }
        }
        let extended = wire_tools::extend(doc, catalog, at);
        if extended.is_ok() {
            offered.push((tr("Extend wire end"), extended));
        }
        let wires: Vec<circuit_model::WireId> = self
            .editor
            .selection()
            .iter()
            .filter_map(|subject| match subject {
                Subject::Wire(id) => Some(*id),
                _ => None,
            })
            .collect();
        if let Some(first) = wires.first() {
            let layout = LayoutIndex::new(doc, catalog);
            let across = doc
                .wires
                .iter()
                .find(|wire| wire.id == *first)
                .and_then(|wire| layout.wire_polyline(wire))
                .and_then(|path| match path.get(..2) {
                    Some([a, b]) if a.y == b.y => Some((0, 2)),
                    Some(_) => Some((2, 0)),
                    None => None,
                })
                .unwrap_or((0, 2));
            offered.push((
                tr("Parallel copy of the wires"),
                wire_tools::parallel_copy(doc, catalog, &wires, across),
            ));
        }
        offered
    }

    /// Keep the view the drawing rested at, so that Previous view can go
    /// back to it: a zoom with the wheel or a pan with the pointer counts once,
    /// when it ends.
    fn remember_view(&mut self, ui: &Ui) {
        let busy = ui.input(|input| {
            input.pointer.any_down()
                || input.smooth_scroll_delta != egui::Vec2::ZERO
                || (input.zoom_delta() - 1.0).abs() > f32::EPSILON
        });
        if busy {
            return;
        }
        if let Some(settled) = self.view_settled
            && settled != self.camera
        {
            self.view_back.push(settled);
            if self.view_back.len() > MAX_VIEWS_BACK {
                self.view_back.remove(0);
            }
        }
        self.view_settled = Some(self.camera);
    }

    /// Rulers along the top and left edges of the drawing, in grid units,
    /// with the place of the pointer marked.
    fn paint_rulers(&self, ui: &Ui, events: &canvas::CanvasEvents) {
        const BAND: f32 = 16.0;
        let tokens = Tokens::of(ui);
        let [left, top] = events.origin;
        let [width, height] = events.size;
        let painter = ui.painter().with_clip_rect(egui::Rect::from_min_size(
            egui::pos2(left, top),
            egui::vec2(width, height),
        ));
        let fill = ui.visuals().extreme_bg_color;
        painter.rect_filled(
            egui::Rect::from_min_size(egui::pos2(left, top), egui::vec2(width, BAND)),
            0.0,
            fill,
        );
        painter.rect_filled(
            egui::Rect::from_min_size(egui::pos2(left, top), egui::vec2(BAND, height)),
            0.0,
            fill,
        );
        // Ticks at least 40 points apart, at 1, 2 or 5 times a power of ten.
        let per_grid = self.camera.to_screen_length(SUBUNITS_PER_GRID);
        if !(per_grid.is_finite() && per_grid > 0.0) {
            return;
        }
        let step = [1, 2, 5, 10, 20, 50, 100, 200, 500, 1000, 2000, 5000, 10_000]
            .into_iter()
            .find(|step| per_grid * *step as f32 >= 40.0)
            .unwrap_or(10_000);
        let stroke = egui::Stroke::new(1.0, tokens.text_faint);
        let font = egui::FontId::proportional(9.0);
        let along = |vertical: bool| {
            let length = if vertical { height } else { width };
            let first = self
                .camera
                .to_grid([0.0, 0.0])
                .map_or(0, |point| if vertical { point.y } else { point.x });
            let mut n = (i64::from(first) / step - 1) * step;
            for _ in 0..200 {
                let point = if vertical {
                    GridPoint::new(0, i32::try_from(n).unwrap_or(0))
                } else {
                    GridPoint::new(i32::try_from(n).unwrap_or(0), 0)
                };
                let [x, y] = self.camera.to_screen(ScenePoint::from_grid(point));
                let place = if vertical { y } else { x };
                if place > length {
                    break;
                }
                if place >= BAND {
                    let (from, to, label_at, align) = if vertical {
                        (
                            egui::pos2(left + BAND - 5.0, top + place),
                            egui::pos2(left + BAND, top + place),
                            egui::pos2(left + 2.0, top + place + 1.0),
                            egui::Align2::LEFT_TOP,
                        )
                    } else {
                        (
                            egui::pos2(left + place, top + BAND - 5.0),
                            egui::pos2(left + place, top + BAND),
                            egui::pos2(left + place + 2.0, top + 1.0),
                            egui::Align2::LEFT_TOP,
                        )
                    };
                    painter.line_segment([from, to], stroke);
                    painter.text(
                        label_at,
                        align,
                        n.to_string(),
                        font.clone(),
                        tokens.text_faint,
                    );
                }
                n += step;
            }
        };
        along(false);
        along(true);
        if let Some(hover) = events.hover {
            let mark = egui::Stroke::new(1.5, tokens.crosshair);
            painter.line_segment(
                [
                    egui::pos2(left + hover[0], top),
                    egui::pos2(left + hover[0], top + BAND),
                ],
                mark,
            );
            painter.line_segment(
                [
                    egui::pos2(left, top + hover[1]),
                    egui::pos2(left + BAND, top + hover[1]),
                ],
                mark,
            );
        }
    }

    /// The menu of a secondary click on the drawing: what can be done to the
    /// selection, and the view.
    fn context_menu(&mut self, ui: &mut Ui, events: &canvas::CanvasEvents) {
        if let Some(at) = events.context {
            let [left, top] = events.origin;
            self.context_menu = Some(egui::pos2(left + at[0], top + at[1]));
            self.context_point = self.camera.to_grid(at);
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
        let wire_tools = self.wire_tool_requests();
        let mut wire_tool: Option<inspector::Request> = None;
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
                    for (text, request) in wire_tools {
                        if ui.button(text).clicked() {
                            wire_tool = Some(request);
                        }
                    }
                    ui.separator();
                    if ui.button(tr("Zoom to the whole drawing")).clicked() {
                        fit = true;
                    }
                });
            });
        // The click that opened the menu is not one that closes it.
        let outside = events.context.is_none() && area.response.clicked_elsewhere();
        if chosen.is_some() || fit || label || wire_tool.is_some() || outside {
            self.context_menu = None;
        }
        if let Some(request) = wire_tool {
            self.commit(request);
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

    /// Set the catalog data of the devices from CSV text (Modify > Import
    /// catalog data), in one undoable step; the command line says how many
    /// devices changed and which designators the project does not have.
    pub fn import_catalog_text(&mut self, text: &str) {
        let Some(session) = &self.session else {
            return;
        };
        match circuit_commands::catalog_csv::import(session.document(), text) {
            Ok((tx, imported)) => {
                self.commit(Ok(tx));
                self.command_line.say(&trf(
                    "Catalog data of {0} device(s) imported.",
                    &[&imported.changed],
                ));
                if !imported.unknown.is_empty() {
                    self.command_line
                        .say(&trf("No device for: {0}", &[&imported.unknown.join(", ")]));
                }
            }
            Err(error) => self.error = Some(error.to_string()),
        }
    }

    /// Select one element and centre the view on it.
    pub fn select(&mut self, subject: Subject) {
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
        let hovered = if self.hover_highlight {
            events
                .hover
                .and_then(|at| self.camera.to_grid(at))
                .and_then(|point| {
                    circuit_editor::hit_test(session.document(), session.catalog(), point)
                })
                .map(|hit| hit.subject())
        } else {
            None
        };
        self.paint_over_canvas(ui, &events, selecting);
        if let Some(bounds) = hovered.and_then(|subject| scene.subject_bounds(&subject)) {
            let [left, top] = events.origin;
            let corner = |point: ScenePoint| {
                let [x, y] = self.camera.to_screen(point);
                egui::pos2(left + x, top + y)
            };
            let outline =
                egui::Rect::from_two_pos(corner(bounds.min), corner(bounds.max)).expand(4.0);
            ui.painter().rect_stroke(
                outline,
                3.0,
                egui::Stroke::new(1.5, Tokens::of(ui).window_select),
                egui::StrokeKind::Outside,
            );
        }
        if self.rulers {
            self.paint_rulers(ui, &events);
        }
        self.label_editor(ui, &scene, &events);
        self.point_entry(ui);
        self.remember_view(ui);
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
        let wire_points = self.wire_points;
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
                snap_with(doc, catalog, pointer, radius, SnapOptions { wire_points })
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
        SnapTarget::Wire(_) | SnapTarget::Crossing(..) => vec![
            Shape::Line {
                from: at(-r, -r),
                to: at(r, r),
            },
            Shape::Line {
                from: at(-r, r),
                to: at(r, -r),
            },
        ],
        // A diamond on an end or a bend.
        SnapTarget::WireEnd(_) => vec![Shape::Polyline(vec![
            at(0, -r),
            at(r, 0),
            at(0, r),
            at(-r, 0),
            at(0, -r),
        ])],
        // A triangle on a middle.
        SnapTarget::WireMiddle(_) => vec![Shape::Polyline(vec![
            at(0, -r),
            at(r, r),
            at(-r, r),
            at(0, -r),
        ])],
        SnapTarget::Grid => Vec::new(),
    }
}
