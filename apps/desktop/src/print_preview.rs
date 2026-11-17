//! Print preview (M9.2, issue #118): the pages a PDF of the drawing has, at
//! rest and in the colours of paper, one at a time, before exporting.

use circuit_renderer::Scene;
use eframe::egui::{self, Ui};

use crate::camera::Camera;
use crate::canvas;
use crate::i18n::{tr, trf};
use crate::session::Session;

/// Size of the page area of the window, in points.
const PAGE: egui::Vec2 = egui::vec2(560.0, 400.0);

/// The preview between frames.
#[derive(Debug, Default)]
pub struct PrintPreview {
    /// The pages and the revision of the project they were drawn from, while
    /// the window is open.
    pages: Option<(Vec<Scene>, u64)>,
    /// The page shown, from 0.
    page: usize,
}

/// What the person asked for in the preview.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Action {
    /// Export the drawing as PDF.
    ExportPdf,
}

impl PrintPreview {
    /// Open the preview of the project on its first page.
    pub fn open(&mut self, session: &Session) {
        self.pages = Some((session.print_pages(), session.history().revision()));
        self.page = 0;
    }

    /// The number of pages, while the preview is open.
    #[must_use]
    pub fn pages(&self) -> Option<usize> {
        self.pages.as_ref().map(|(pages, _)| pages.len())
    }

    /// The page shown, from 0.
    #[must_use]
    pub const fn page(&self) -> usize {
        self.page
    }

    /// Draw the window while it is open.
    pub fn show(&mut self, ui: &Ui, session: Option<&Session>) -> Option<Action> {
        let session = match (session, &self.pages) {
            (Some(session), Some(_)) => session,
            _ => {
                self.pages = None;
                return None;
            }
        };
        // Drawn again after a change of the project.
        if self
            .pages
            .as_ref()
            .is_some_and(|(_, revision)| *revision != session.history().revision())
        {
            self.pages = Some((session.print_pages(), session.history().revision()));
        }
        let count = self.pages().unwrap_or(0);
        self.page = self.page.min(count.saturating_sub(1));
        let mut open = true;
        let mut action = None;
        egui::Window::new(tr("Print preview"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(self.page > 0, egui::Button::new(tr("Previous page")))
                        .clicked()
                    {
                        self.page = self.page.saturating_sub(1);
                    }
                    ui.label(trf(
                        "Page {0} of {1}",
                        &[&self.page.saturating_add(1), &count],
                    ));
                    if ui
                        .add_enabled(
                            self.page.saturating_add(1) < count,
                            egui::Button::new(tr("Next page")),
                        )
                        .clicked()
                    {
                        self.page = self.page.saturating_add(1);
                    }
                    ui.separator();
                    if ui.button(tr("Export as PDF…")).clicked() {
                        action = Some(Action::ExportPdf);
                    }
                });
                if let Some(scene) = self
                    .pages
                    .as_ref()
                    .and_then(|(pages, _)| pages.get(self.page))
                {
                    ui.allocate_ui(PAGE, |ui| {
                        ui.set_min_size(PAGE);
                        // Paper: light colours on white, whatever the theme.
                        ui.visuals_mut().dark_mode = false;
                        let paper = egui::Rect::from_min_size(ui.cursor().min, PAGE);
                        ui.painter().rect_filled(paper, 0.0, egui::Color32::WHITE);
                        let mut camera = Camera::default();
                        canvas::show(ui, scene, &mut camera, true, false);
                    });
                }
            });
        if !open {
            self.pages = None;
        }
        action
    }
}
