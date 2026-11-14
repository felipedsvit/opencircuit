//! The window: a bar to open and run a project, the canvas, the operator
//! controls with the trace, and the diagnostics.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use circuit_model::{Control, DeviceId, InputAction, LayoutIndex, Subject};
use circuit_renderer::SceneOptions;
use eframe::egui::{self, RichText, Ui};

use crate::camera::Camera;
use crate::canvas;
use crate::clock::{SPEEDS, SimClock};
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
    /// The pushbutton held down on the canvas, released with the pointer.
    held: Option<DeviceId>,
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
        self.held = None;
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

        // One `step_n` and one snapshot per frame, before anything is drawn.
        let frame_us = SimClock::frame_us(ui.input(|input| input.stable_dt));
        if let Some(session) = &mut self.session {
            session.advance(frame_us);
            if session.is_running() {
                ui.ctx().request_repaint();
            }
        }

        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("diagnostics")
            .resizable(true)
            .default_size(140.0)
            .show(ui, |ui| self.diagnostics(ui));
        if self.session.as_ref().is_some_and(Session::can_simulate) {
            egui::Panel::right("operator")
                .resizable(true)
                .default_size(260.0)
                .show(ui, |ui| self.operator(ui));
        }
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
        if let Some(session) = &mut self.session
            && session.can_simulate()
        {
            ui.horizontal(|ui| simulation_controls(ui, session));
        }
        if let Some(error) = &self.error {
            ui.colored_label(ui.visuals().error_fg_color, error);
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
        let Some(session) = &mut self.session else {
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
            self.selection = hit
                .map(|(symbol, _)| Subject::Symbol(symbol))
                .into_iter()
                .collect();
        }
        // Operating a device on the canvas: a pushbutton is held while the
        // pointer is down; a switch or a thermal relay changes on press.
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
        if ui.button("Pause").clicked() {
            session.pause();
        }
    } else if ui
        .add_enabled(!session.is_faulted(), egui::Button::new("Play"))
        .clicked()
    {
        session.play();
    }
    if ui.button("Step").clicked() {
        session.step();
    }
    if ui.button("Reset").clicked() {
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

impl eframe::App for ViewerApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.show(ui);
    }
}
