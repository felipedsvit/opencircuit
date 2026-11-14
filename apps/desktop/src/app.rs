//! The window: a bar to open a project, the canvas, and the diagnostics.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use circuit_model::{LayoutIndex, Subject};
use circuit_renderer::SceneOptions;
use eframe::egui::{self, RichText, Ui};

use crate::camera::Camera;
use crate::canvas;
use crate::session::Session;

/// The desktop application.
#[derive(Debug, Default)]
pub struct ViewerApp {
    session: Option<Session>,
    /// Text of the path field.
    path_input: String,
    /// The last error, shown until the next successful action.
    error: Option<String>,
    camera: Camera,
    /// Frame the whole scene at the next frame.
    fit_pending: bool,
    selection: BTreeSet<Subject>,
    /// Centre the camera on this subject at the next frame.
    focus: Option<Subject>,
}

impl ViewerApp {
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

    fn replace(&mut self, session: Session) {
        self.session = Some(session);
        self.error = None;
        self.selection.clear();
        self.focus = None;
        self.fit_pending = true;
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
        &self.selection
    }

    /// Draw the whole window into `ui`.
    pub fn show(&mut self, ui: &mut Ui) {
        let dropped = ui.input(|input| {
            input
                .raw
                .dropped_files
                .first()
                .map(|file| file.path().to_path_buf())
        });
        if let Some(path) = dropped {
            self.open_path(&path);
        }

        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("diagnostics")
            .resizable(true)
            .default_size(140.0)
            .show(ui, |ui| self.diagnostics(ui));
        egui::CentralPanel::default().show(ui, |ui| self.canvas(ui));
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label("Project");
            let field = ui.add(
                egui::TextEdit::singleline(&mut self.path_input)
                    .hint_text("path to a .ocircuit file")
                    .desired_width(360.0),
            );
            let entered =
                field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
            if ui.button("Open").clicked() || entered {
                let path = PathBuf::from(self.path_input.trim());
                self.open_path(&path);
            }
            if self.session.is_some() && ui.button("Fit").clicked() {
                self.fit_pending = true;
            }
            if let Some(session) = &self.session {
                ui.separator();
                ui.label(RichText::new(&session.document().project.name).strong());
            }
        });
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
        }
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
        if diagnostics.is_empty() {
            ui.label("No diagnostics: the project compiles.");
            return;
        }
        let mut chosen = None;
        egui::ScrollArea::vertical().show(ui, |ui| {
            for diagnostic in diagnostics {
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
        });
        if let Some(subject) = chosen {
            self.select(subject);
        }
    }

    /// A line describing the selected symbol, if exactly one is selected.
    fn selected_description(&self, session: &Session) -> Option<String> {
        let mut selected = self.selection.iter();
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
        self.selection = BTreeSet::from([subject.clone()]);
        self.focus = Some(subject);
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
        let options = SceneOptions {
            selected: self.selection.clone(),
        };
        let scene = session.scene(&options);
        let fit = std::mem::take(&mut self.fit_pending);
        let events = canvas::show(ui, &scene, &mut self.camera, fit);

        if let Some(subject) = self.focus.take()
            && let Some(bounds) = scene.subject_bounds(&subject)
        {
            self.camera.center_on(bounds, events.size);
        }
        if let Some(click) = events.clicked {
            let layout = LayoutIndex::new(session.document(), session.catalog());
            let hit = self
                .camera
                .to_grid(click)
                .and_then(|point| layout.symbol_at(point));
            self.selection = hit.map(Subject::Symbol).into_iter().collect();
        }
    }
}

impl eframe::App for ViewerApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}
