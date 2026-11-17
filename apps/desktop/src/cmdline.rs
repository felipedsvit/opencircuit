//! The command line over the drawing: a field to type commands in, the last
//! lines of what was typed and answered, and what may be typed next. What a
//! command means is `circuit_commands::commandline`; this only shows it.

use std::collections::VecDeque;

use eframe::egui::{self, Key, Modifiers, RichText, Ui};

use crate::theme::Tokens;

/// Most lines remembered for the arrows.
const MAX_HISTORY: usize = 200;
/// Most lines kept in the log.
const MAX_LOG: usize = 200;
/// Lines of the log shown over the drawing.
const SHOWN: usize = 4;
/// Width of the field in points.
const WIDTH: f32 = 460.0;

/// What the command line remembers between frames.
#[derive(Debug, Default)]
pub struct CommandLine {
    /// What is being typed.
    input: String,
    /// Lines typed, oldest first.
    history: Vec<String>,
    /// What was typed and what came back, oldest first.
    log: VecDeque<String>,
    /// The place in the history while walking it with the arrows.
    recall: Option<usize>,
    /// Give the keyboard to the field at the next frame.
    focus: bool,
}

impl CommandLine {
    /// Give the keyboard to the field at the next frame.
    pub fn focus(&mut self) {
        self.focus = true;
    }

    /// Add what the user should read; a text of several lines is added line
    /// by line.
    pub fn say(&mut self, text: &str) {
        for line in text.lines() {
            if self.log.len() >= MAX_LOG {
                self.log.pop_front();
            }
            self.log.push_back(line.to_owned());
        }
    }

    /// The lines of the log, oldest first.
    #[must_use]
    pub fn lines(&self) -> Vec<&str> {
        self.log.iter().map(String::as_str).collect()
    }

    /// Remember a line for the arrows, unless it repeats the last one.
    fn remember(&mut self, line: &str) {
        if self.history.last().map(String::as_str) != Some(line) {
            if self.history.len() >= MAX_HISTORY {
                self.history.remove(0);
            }
            self.history.push(line.to_owned());
        }
        self.recall = None;
    }

    /// Step through the history: `up` goes to an older line.
    fn walk(&mut self, up: bool) {
        let last = self.history.len().checked_sub(1);
        let next = match (self.recall, up) {
            (None, true) => last,
            (None, false) => return,
            (Some(at), true) => Some(at.saturating_sub(1)),
            (Some(at), false) => at.checked_add(1).filter(|next| Some(*next) <= last),
        };
        self.recall = next;
        self.input = next
            .and_then(|at| self.history.get(at))
            .cloned()
            .unwrap_or_default();
    }

    /// Replace the last word of the input by `word`.
    fn complete(&mut self, word: &str) {
        let keep = self
            .input
            .rfind(char::is_whitespace)
            .map_or(0, |at| at.saturating_add(1));
        self.input.truncate(keep);
        self.input.push_str(word);
        self.input.push(' ');
    }

    /// Draw the command line with the middle of its bottom edge at `at` and
    /// return the line the user sent, if any. `suggest` gives what may be typed next for the input.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        at: egui::Pos2,
        suggest: impl Fn(&str) -> Vec<String>,
    ) -> Option<String> {
        let tokens = Tokens::of(ui);
        let mut sent = None;
        egui::Area::new(ui.scope_id().with(egui::IdSalt::new("command line")))
            .order(egui::Order::Foreground)
            .pivot(egui::Align2::CENTER_BOTTOM)
            .fixed_pos(at)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(tokens.command)
                    .stroke(egui::Stroke::new(1.0, tokens.border))
                    .corner_radius(6)
                    .inner_margin(egui::Margin::symmetric(8, 5))
                    .show(ui, |ui| {
                        ui.set_width(WIDTH);
                        sent = self.contents(ui, &tokens, &suggest);
                    });
            });
        sent
    }

    fn contents(
        &mut self,
        ui: &mut Ui,
        tokens: &Tokens,
        suggest: &impl Fn(&str) -> Vec<String>,
    ) -> Option<String> {
        let skip = self.log.len().saturating_sub(SHOWN);
        for line in self.log.iter().skip(skip) {
            ui.label(RichText::new(line).small().color(tokens.text_dim));
        }
        let suggestions = suggest(&self.input);
        let mut chosen = None;
        if !suggestions.is_empty() {
            ui.horizontal_wrapped(|ui| {
                for word in &suggestions {
                    if ui.small_button(word).clicked() {
                        chosen = Some(word.clone());
                    }
                }
            });
        }
        let mut sent = None;
        ui.horizontal(|ui| {
            let prompt = ui.label(RichText::new("Command").small().color(tokens.text_dim));
            let field = ui
                .add(
                    egui::TextEdit::singleline(&mut self.input)
                        // Its own identifier: the suggestions above it come
                        // and go, and an automatic one would change with
                        // them and drop the keyboard focus.
                        .id_salt("command line input")
                        // Tab completes; it does not move the focus away.
                        .lock_focus(true)
                        .hint_text("type a command, `help` lists them")
                        .desired_width(f32::INFINITY),
                )
                .labelled_by(prompt.id);
            if std::mem::take(&mut self.focus) {
                field.request_focus();
            }
            if field.has_focus() {
                let (up, down, tab, escape) = ui.input_mut(|input| {
                    (
                        input.consume_key(Modifiers::NONE, Key::ArrowUp),
                        input.consume_key(Modifiers::NONE, Key::ArrowDown),
                        input.consume_key(Modifiers::NONE, Key::Tab),
                        input.consume_key(Modifiers::NONE, Key::Escape),
                    )
                });
                if up {
                    self.walk(true);
                }
                if down {
                    self.walk(false);
                }
                if tab && let Some(first) = suggestions.first() {
                    chosen = Some(first.clone());
                }
                if escape {
                    self.input.clear();
                    self.recall = None;
                    field.surrender_focus();
                }
            }
            if field.lost_focus() && ui.input(|input| input.key_pressed(Key::Enter)) {
                let line = self.input.trim().to_owned();
                // An empty line repeats the last one, as in a drawing
                // program.
                let line = if line.is_empty() {
                    self.history.last().cloned().unwrap_or_default()
                } else {
                    line
                };
                self.input.clear();
                if !line.is_empty() {
                    self.remember(&line);
                    sent = Some(line);
                }
            }
        });
        if let Some(word) = chosen {
            self.complete(&word);
            self.focus = true;
        }
        sent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_keeps_the_last_lines_and_splits_text_into_lines() {
        let mut line = CommandLine::default();

        line.say("one\ntwo");
        for n in 0..(MAX_LOG + 10) {
            line.say(&format!("line {n}"));
        }

        assert_eq!(line.lines().len(), MAX_LOG);
        assert_eq!(line.lines().last(), Some(&"line 209"));
        assert!(!line.lines().contains(&"one"));
    }

    #[test]
    fn the_arrows_walk_the_history_and_a_repeat_is_remembered_once() {
        let mut line = CommandLine::default();
        for text in ["wire", "zoom e", "zoom e", "undo"] {
            line.remember(text);
        }
        assert_eq!(line.history, ["wire", "zoom e", "undo"]);

        line.walk(true);
        assert_eq!(line.input, "undo");
        line.walk(true);
        line.walk(true);
        line.walk(true);
        assert_eq!(line.input, "wire");
        line.walk(false);
        assert_eq!(line.input, "zoom e");
        line.walk(false);
        line.walk(false);
        assert_eq!(line.input, "");
        line.walk(false);
        assert_eq!(line.recall, None);
    }

    #[test]
    fn the_history_is_bounded() {
        let mut line = CommandLine::default();

        for n in 0..(MAX_HISTORY + 5) {
            line.remember(&format!("zoom {n}"));
        }

        assert_eq!(line.history.len(), MAX_HISTORY);
        assert_eq!(line.history.first().map(String::as_str), Some("zoom 5"));
    }

    #[test]
    fn completing_replaces_the_last_word_only() {
        let mut line = CommandLine {
            input: "place contac".to_owned(),
            ..CommandLine::default()
        };

        line.complete("contactor.generic");

        assert_eq!(line.input, "place contactor.generic ");
        line.input = "zo".to_owned();
        line.complete("zoom");
        assert_eq!(line.input, "zoom ");
    }
}
