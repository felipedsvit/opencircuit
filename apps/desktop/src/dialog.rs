//! The dialogs of the window: one look and one behaviour for every question,
//! offer and error.
//!
//! **Why not the dialogs of the system.** The file pickers are native (the
//! `dialogs` module: the Windows common dialogs, the macOS panels, the
//! desktop portal on Linux), because a file is the system's to choose. A
//! question about the project is not: a native message box blocks the frame,
//! needs `zenity` on Linux, offers neither the three labelled buttons nor the
//! themes of the window, does not exist in the browser build, and cannot be
//! driven by the window tests. So the questions are drawn here, from the
//! tokens of the theme, and follow the conventions of the system they run on:
//! the corner radius ([`crate::theme::window_radius`]) and the order of the
//! buttons (the main one first on Windows, last on macOS and Linux).
//!
//! The pieces are small on purpose: [`Dialog`] for a question that must be
//! answered, [`card`] for the frame of a small popup that takes text, and
//! [`error_banner`] for what went wrong.

use eframe::egui::{
    self, Align, Color32, Context, CornerRadius, Frame, Id, Layout, Margin, Response, RichText,
    Sense, Stroke, Ui, Vec2,
};

use crate::theme::{Tokens, window_radius};

/// What a dialog is about: the colour and the mark of its icon.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    /// A question or an offer.
    Question,
    /// Something that may lose work.
    Warning,
    /// Something that failed.
    Error,
}

/// How much a button matters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Role {
    /// The answer most people want: filled with the accent, takes the focus.
    Primary,
    /// Any other answer, and the way out.
    Secondary,
    /// An answer that loses something: outlined in the danger colour.
    Destructive,
}

/// One button of a dialog.
#[derive(Clone, Copy, Debug)]
pub struct Choice<'a> {
    /// What it says.
    pub label: &'a str,
    /// How much it matters.
    pub role: Role,
    /// Its accessible name, when the text alone does not tell it apart from
    /// another button of the same window.
    pub name: Option<&'a str>,
}

impl<'a> Choice<'a> {
    /// The main answer.
    #[must_use]
    pub const fn primary(label: &'a str) -> Self {
        Self {
            label,
            role: Role::Primary,
            name: None,
        }
    }

    /// An ordinary answer.
    #[must_use]
    pub const fn secondary(label: &'a str) -> Self {
        Self {
            label,
            role: Role::Secondary,
            name: None,
        }
    }

    /// An answer that loses work.
    #[must_use]
    pub const fn destructive(label: &'a str) -> Self {
        Self {
            label,
            role: Role::Destructive,
            name: None,
        }
    }

    /// The same button with an accessible name of its own.
    #[must_use]
    pub const fn named(mut self, name: &'a str) -> Self {
        self.name = Some(name);
        self
    }
}

/// What the person did with a dialog this frame.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Outcome {
    /// The button that was pressed, as an index of the choices.
    pub chosen: Option<usize>,
    /// Escape, or a click outside the dialog.
    pub dismissed: bool,
}

/// Width of a dialog, in points, unless the window is narrower.
const WIDTH: f32 = 400.0;
/// Least width of a button, in points.
const BUTTON_WIDTH: f32 = 92.0;
/// Height of a button, in points.
const BUTTON_HEIGHT: f32 = 30.0;

/// Whether the main button goes first, as on Windows. On macOS and on the
/// desktops of Linux it goes last, at the right edge.
const MAIN_FIRST: bool = cfg!(target_os = "windows");

/// The order the buttons are laid out in, from the right edge in. `choices`
/// are given with the main one first and the way out last.
fn layout_order(count: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..count).collect();
    if MAIN_FIRST {
        order.reverse();
    }
    order
}

/// The frame of a dialog or of a popup that takes text: the surface of the
/// window, the radius of the system, a hairline and the shadow of a window.
pub fn card(style: &egui::Style) -> Frame {
    let tokens = Tokens::for_mode(style.visuals.dark_mode);
    Frame::window(style)
        .fill(tokens.panel)
        .stroke(Stroke::new(1.0, tokens.border))
        .corner_radius(CornerRadius::same(window_radius()))
        .inner_margin(Margin::same(20))
}

/// The same frame, tighter, for a popup with one field.
pub fn popup_card(style: &egui::Style) -> Frame {
    card(style).inner_margin(Margin::same(10))
}

/// Paint the icon of a dialog in a square of `size` points.
fn paint_icon(ui: &mut Ui, kind: Kind, size: f32) {
    let tokens = Tokens::of(ui);
    let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    let colour = match kind {
        Kind::Question => tokens.accent,
        Kind::Warning => tokens.warn,
        Kind::Error => tokens.danger,
    };
    let painter = ui.painter();
    painter.circle_filled(rect.center(), size / 2.0, colour.gamma_multiply(0.18));
    let stroke = Stroke::new((size / 14.0).max(1.5), colour);
    let middle = rect.center();
    let unit = size / 10.0;
    match kind {
        Kind::Question => {
            // A question mark: the hook, and the dot.
            let hook = [
                middle + egui::vec2(-2.2 * unit, -1.6 * unit),
                middle + egui::vec2(-1.6 * unit, -2.8 * unit),
                middle + egui::vec2(0.0, -3.3 * unit),
                middle + egui::vec2(1.6 * unit, -2.8 * unit),
                middle + egui::vec2(2.2 * unit, -1.4 * unit),
                middle + egui::vec2(1.6 * unit, -0.2 * unit),
                middle + egui::vec2(0.0, 0.8 * unit),
                middle + egui::vec2(0.0, 1.6 * unit),
            ];
            painter.line(hook.to_vec(), stroke);
            painter.circle_filled(middle + egui::vec2(0.0, 3.2 * unit), stroke.width, colour);
        }
        Kind::Warning => {
            // An exclamation mark: the bar, and the dot.
            painter.line_segment(
                [
                    middle + egui::vec2(0.0, -3.4 * unit),
                    middle + egui::vec2(0.0, 0.9 * unit),
                ],
                stroke,
            );
            painter.circle_filled(middle + egui::vec2(0.0, 3.0 * unit), stroke.width, colour);
        }
        Kind::Error => {
            // A cross.
            for (a, b) in [((-2.8, -2.8), (2.8, 2.8)), ((-2.8, 2.8), (2.8, -2.8))] {
                painter.line_segment(
                    [
                        middle + egui::vec2(a.0 * unit, a.1 * unit),
                        middle + egui::vec2(b.0 * unit, b.1 * unit),
                    ],
                    stroke,
                );
            }
        }
    }
}

/// One button, drawn for its role, for the rows of a dialog that has a
/// list (the copies to recover).
pub fn choice_button(ui: &mut Ui, choice: Choice<'_>) -> Response {
    let tokens = Tokens::of(ui);
    let response = button(ui, choice, tokens);
    if let Some(name) = choice.name {
        let enabled = ui.is_enabled();
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, name));
    }
    response
}

/// One button, drawn for its role.
fn button(ui: &mut Ui, choice: Choice<'_>, tokens: Tokens) -> Response {
    let (fill, text, stroke) = match choice.role {
        Role::Primary => (tokens.accent_fill, tokens.on_accent, Stroke::NONE),
        Role::Secondary => (tokens.control, tokens.text, Stroke::NONE),
        Role::Destructive => (
            Color32::TRANSPARENT,
            tokens.danger,
            Stroke::new(1.0, tokens.danger.gamma_multiply(0.6)),
        ),
    };
    let text = if choice.role == Role::Primary {
        RichText::new(choice.label).color(text).strong()
    } else {
        RichText::new(choice.label).color(text)
    };
    ui.add(
        egui::Button::new(text)
            .fill(fill)
            .stroke(stroke)
            .corner_radius(CornerRadius::same(6))
            .min_size(Vec2::new(BUTTON_WIDTH, BUTTON_HEIGHT)),
    )
}

/// A question that must be answered before anything else: `title`, the
/// `message` under it, whatever the body draws, then the buttons `choices`,
/// given with the main one first and the way out last.
#[derive(Clone, Copy, Debug)]
pub struct Dialog<'a> {
    /// Identifies the dialog between frames.
    pub id: Id,
    /// Its icon.
    pub kind: Kind,
    /// The question, in a few words.
    pub title: &'a str,
    /// What it is about; may be empty.
    pub message: &'a str,
    /// The answers.
    pub choices: &'a [Choice<'a>],
}

impl Dialog<'_> {
    /// Show the dialog over the window and report what the person did with it
    /// this frame. `body` draws more under the message. Escape and a click
    /// outside dismiss it.
    ///
    /// The main button has the keyboard when the dialog opens, so Enter
    /// answers it.
    pub fn show(&self, ctx: &Context, body: impl FnOnce(&mut Ui)) -> Outcome {
        let tokens = Tokens::for_mode(ctx.global_style().visuals.dark_mode);
        let mut outcome = Outcome::default();
        // A dialog that was not drawn in the pass before has just opened.
        // (The pass number is read first: the context is locked inside
        // `data_mut`.)
        let now = ctx.cumulative_pass_nr();
        let opened = ctx.data_mut(|data| {
            let last = data.get_temp::<u64>(self.id).unwrap_or(0);
            data.insert_temp(self.id, now);
            now.saturating_sub(last) > 1
        });
        let width = WIDTH.min((ctx.content_rect().width() - 48.0).max(240.0));
        let backdrop = if tokens.canvas.r() < 128 { 150 } else { 90 };
        let response = egui::Modal::new(self.id)
            .frame(card(&ctx.global_style()))
            .backdrop_color(Color32::from_black_alpha(backdrop))
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.horizontal_top(|ui| {
                    paint_icon(ui, self.kind, 40.0);
                    ui.add_space(6.0);
                    ui.vertical(|ui| {
                        ui.add(egui::Label::new(
                            RichText::new(self.title).size(17.0).strong(),
                        ));
                        ui.add_space(2.0);
                        if !self.message.is_empty() {
                            ui.label(RichText::new(self.message).color(tokens.text_dim));
                        }
                        body(ui);
                    });
                });
                ui.add_space(18.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    for index in layout_order(self.choices.len()) {
                        let Some(choice) = self.choices.get(index).copied() else {
                            continue;
                        };
                        let response = button(ui, choice, tokens);
                        if let Some(name) = choice.name {
                            let enabled = ui.is_enabled();
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(egui::Role::Button, enabled, name)
                            });
                        }
                        if opened && choice.role == Role::Primary {
                            response.request_focus();
                        }
                        if response.clicked() {
                            outcome.chosen = Some(index);
                        }
                    }
                });
            });
        outcome.dismissed = outcome.chosen.is_none() && response.should_close();
        outcome
    }
}

/// What went wrong, in a banner: the danger mark, the message, and a button
/// that clears it. `true` when the person pressed that button.
pub fn error_banner(ui: &mut Ui, message: &str, dismiss: &str) -> bool {
    let tokens = Tokens::of(ui);
    let mut dismissed = false;
    Frame::new()
        .fill(tokens.danger.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0, tokens.danger.gamma_multiply(0.55)))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                paint_icon(ui, Kind::Error, 16.0);
                ui.label(RichText::new(message).color(tokens.text));
                let close = ui.small_button("×");
                let enabled = ui.is_enabled();
                close.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Button, enabled, dismiss)
                });
                dismissed = close.on_hover_text(dismiss).clicked();
            });
        });
    dismissed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_main_button_is_first_on_windows_and_last_elsewhere() {
        let order = layout_order(3);

        if MAIN_FIRST {
            assert_eq!(order, vec![2, 1, 0]);
        } else {
            assert_eq!(order, vec![0, 1, 2]);
        }
        assert_eq!(layout_order(0), Vec::<usize>::new());
    }

    #[test]
    fn a_choice_remembers_its_role() {
        assert_eq!(Choice::primary("Save").role, Role::Primary);
        assert_eq!(Choice::secondary("Cancel").role, Role::Secondary);
        assert_eq!(Choice::destructive("Discard").role, Role::Destructive);
    }
}
