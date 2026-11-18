//! The workspace of the window (M9.2, issue #118): the theme, which panels
//! show and on which side, the view switches, the keys of the customizable
//! shortcuts, and named workspaces that bring a set of those back.
//!
//! A fresh window uses [`Layout::default`] and [`Keymap::default`], which are
//! the window as it was before: nothing here changes a window that was not
//! told to. `main` gives the window a [`Preferences`] file in the user's
//! data folder; a test of the window never does.

#![cfg_attr(not(test), deny(clippy::arithmetic_side_effects))]

use std::collections::BTreeMap;

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use serde::{Deserialize, Serialize};

/// Most named workspaces kept.
pub const MAX_WORKSPACES: usize = 16;
/// Longest name of a workspace, in characters.
pub const MAX_NAME_CHARS: usize = 32;

/// The messages of [`Settings::save_workspace`], translated by the caller.
#[cfg(test)]
pub(crate) const MESSAGES: [&str; 2] = [BAD_NAME, TOO_MANY];
const BAD_NAME: &str = "A workspace name is 1 to 32 characters, with no control character.";
const TOO_MANY: &str = "At most 16 workspaces are kept; delete one first.";

/// Which colours the window uses.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeChoice {
    /// Follow the operating system, as a fresh window does.
    #[default]
    System,
    /// Always dark.
    Dark,
    /// Always light.
    Light,
}

impl ThemeChoice {
    /// Every choice, in menu order.
    pub const ALL: [Self; 3] = [Self::System, Self::Dark, Self::Light];

    /// The English name, translated by the caller.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::System => "Follow the system",
            Self::Dark => "Dark",
            Self::Light => "Light",
        }
    }

    /// What egui is told.
    #[must_use]
    pub const fn preference(self) -> egui::ThemePreference {
        match self {
            Self::System => egui::ThemePreference::System,
            Self::Dark => egui::ThemePreference::Dark,
            Self::Light => egui::ThemePreference::Light,
        }
    }
}

/// What a workspace remembers: the theme, the panels and the view switches.
/// The default is a fresh window.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct Layout {
    /// The colours of the window.
    pub theme: ThemeChoice,
    /// The side panels (palette, inspector, operator panel) are collapsed.
    pub side_panels_hidden: bool,
    /// The tool bar under the menus is hidden.
    pub toolbar_hidden: bool,
    /// The palette is on the right and the inspector and the operator panel
    /// on the left.
    pub panels_swapped: bool,
    /// The grid dots are hidden.
    pub grid_off: bool,
    /// The wire tool does not snap to pins.
    pub snap_off: bool,
    /// The cursor is not a cross.
    pub crosshair_off: bool,
    /// The problems list is collapsed.
    pub problems_hidden: bool,
    /// The wire tool snaps to wire ends, middles and crossings.
    pub wire_points: bool,
    /// The element under the pointer is outlined.
    pub hover_highlight: bool,
    /// Rulers along the drawing.
    pub rulers: bool,
}

/// A layout with a name.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Workspace {
    /// What the View menu lists.
    pub name: String,
    /// What it brings back.
    pub layout: Layout,
}

/// A shortcut that can be given other keys. Undo, redo, copy, paste, the
/// arrows, Delete, Escape and Space keep their keys: they are the keys of
/// every program.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    /// Choose the wire tool.
    WireTool,
    /// Rotate the selection.
    Rotate,
    /// Select every element.
    SelectAll,
    /// Duplicate the selection.
    Duplicate,
    /// Open find and replace.
    FindReplace,
    /// Edit the label of the selection on the drawing.
    EditLabel,
    /// Type the point of the next click.
    TypePoint,
    /// Open the command palette.
    CommandPalette,
    /// Go back to the previous view.
    PreviousView,
    /// Fit the whole drawing.
    FitDrawing,
    /// Fit the selection.
    FitSelection,
    /// Trace the net of the selection.
    TraceNet,
    /// Put the keyboard in the command line.
    CommandLine,
    /// Show or hide the side panels.
    SidePanels,
}

impl Action {
    /// Every action, in the order of the shortcut window.
    pub const ALL: [Self; 14] = [
        Self::WireTool,
        Self::Rotate,
        Self::SelectAll,
        Self::Duplicate,
        Self::FindReplace,
        Self::EditLabel,
        Self::TypePoint,
        Self::CommandPalette,
        Self::PreviousView,
        Self::FitDrawing,
        Self::FitSelection,
        Self::TraceNet,
        Self::CommandLine,
        Self::SidePanels,
    ];

    /// What the action does, in English, translated by the caller. The
    /// texts of the keyboard window are reused where there is one.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::WireTool => "wire tool",
            Self::Rotate => "rotate",
            Self::SelectAll => "select all",
            Self::Duplicate => "duplicate",
            Self::FindReplace => "find and replace",
            Self::EditLabel => "edit the label of the selection",
            Self::TypePoint => "type the point of the next click",
            Self::CommandPalette => "command palette: run a command, find an element",
            Self::PreviousView => "previous view",
            Self::FitDrawing => "fit the drawing",
            Self::FitSelection => "fit the selection",
            Self::TraceNet => "trace the net of the selection",
            Self::CommandLine => "put the keyboard in the command line",
            Self::SidePanels => "show or hide the side panels",
        }
    }

    /// The keys of a fresh window.
    #[must_use]
    pub const fn default_chord(self) -> Chord {
        let (modifiers, key) = match self {
            Self::WireTool => (Mods::NONE, Key::W),
            Self::Rotate => (Mods::NONE, Key::R),
            Self::SelectAll => (Mods::COMMAND, Key::A),
            Self::Duplicate => (Mods::COMMAND, Key::D),
            Self::FindReplace => (Mods::COMMAND, Key::H),
            Self::EditLabel => (Mods::NONE, Key::F2),
            Self::TypePoint => (Mods::NONE, Key::G),
            Self::CommandPalette => (Mods::COMMAND, Key::K),
            Self::PreviousView => (Mods::ALT, Key::ArrowLeft),
            Self::FitDrawing => (Mods::NONE, Key::Num0),
            Self::FitSelection => (Mods::NONE, Key::F),
            Self::TraceNet => (Mods::NONE, Key::T),
            Self::CommandLine => (Mods::COMMAND, Key::Num9),
            Self::SidePanels => (Mods::COMMAND, Key::B),
        };
        Chord { modifiers, key }
    }

    /// The row of the keyboard window that shows this action's keys, when
    /// it has one of its own.
    #[must_use]
    pub fn for_keys_row(keys: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|action| action.default_chord().text() == keys)
    }
}

/// Modifiers of a chord: the command key (Ctrl, or Cmd on a Mac), Shift and
/// Alt.
#[derive(
    Clone, Copy, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
)]
pub struct Mods {
    /// Ctrl, or Cmd on a Mac.
    #[serde(default)]
    pub command: bool,
    /// Shift.
    #[serde(default)]
    pub shift: bool,
    /// Alt.
    #[serde(default)]
    pub alt: bool,
}

impl Mods {
    /// No modifier.
    pub const NONE: Self = Self {
        command: false,
        shift: false,
        alt: false,
    };
    /// The command key.
    pub const COMMAND: Self = Self {
        command: true,
        shift: false,
        alt: false,
    };
    /// Alt.
    pub const ALT: Self = Self {
        command: false,
        shift: false,
        alt: true,
    };

    const fn egui(self) -> Modifiers {
        Modifiers {
            alt: self.alt,
            ctrl: false,
            shift: self.shift,
            mac_cmd: false,
            command: self.command,
        }
    }
}

/// Keys that run an action: a key and its modifiers.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Chord {
    /// The modifiers held.
    pub modifiers: Mods,
    /// The key pressed.
    pub key: Key,
}

impl Chord {
    /// The chord as the keyboard window writes it, such as `Ctrl+H`.
    #[must_use]
    pub fn text(self) -> String {
        let mut text = String::new();
        if self.modifiers.command {
            text.push_str("Ctrl+");
        }
        if self.modifiers.shift {
            text.push_str("Shift+");
        }
        if self.modifiers.alt {
            text.push_str("Alt+");
        }
        text.push_str(self.key.name());
        text
    }

    /// Read a chord written as [`text`](Self::text) writes it; `None` when
    /// it names no key.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut modifiers = Mods::NONE;
        let mut rest = text.trim();
        loop {
            if let Some(after) = rest.strip_prefix("Ctrl+") {
                modifiers.command = true;
                rest = after;
            } else if let Some(after) = rest.strip_prefix("Shift+") {
                modifiers.shift = true;
                rest = after;
            } else if let Some(after) = rest.strip_prefix("Alt+") {
                modifiers.alt = true;
                rest = after;
            } else {
                break;
            }
        }
        if rest.is_empty() {
            return None;
        }
        Some(Self {
            modifiers,
            key: Key::from_name(rest)?,
        })
    }

    /// The chord of a key pressed with `modifiers`.
    #[must_use]
    pub const fn pressed(key: Key, modifiers: Modifiers) -> Self {
        Self {
            modifiers: Mods {
                command: modifiers.command,
                shift: modifiers.shift,
                alt: modifiers.alt,
            },
            key,
        }
    }

    /// Whether the chord is reserved: the keys that cannot be given to an
    /// action (undo, redo, copy, paste, save, the arrows without Alt,
    /// Delete, Backspace, Escape, Space, Enter, Tab and the zoom keys).
    #[must_use]
    pub fn is_reserved(self) -> bool {
        let plain = self.modifiers == Mods::NONE
            || self.modifiers.shift && !self.modifiers.command && !self.modifiers.alt;
        let command_only = self.modifiers == Mods::COMMAND;
        match self.key {
            Key::Escape | Key::Enter | Key::Tab | Key::Space | Key::Delete | Key::Backspace => true,
            Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown => !self.modifiers.alt,
            Key::Plus | Key::Equals | Key::Minus => plain,
            Key::Z | Key::Y | Key::C | Key::V | Key::S => command_only || self.modifiers.command,
            _ => false,
        }
    }

    /// Consume this chord from the input of `ui`; whether it was pressed.
    pub fn consume(self, ui: &egui::Ui) -> bool {
        ui.input_mut(|input| {
            input.consume_shortcut(&KeyboardShortcut::new(self.modifiers.egui(), self.key))
        })
    }
}

impl Serialize for Chord {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.text())
    }
}

impl<'de> Deserialize<'de> for Chord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).ok_or_else(|| serde::de::Error::custom(format_args!("no key `{text}`")))
    }
}

/// Why keys were not given to an action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum KeymapError {
    /// Undo, copy, the arrows and the other reserved keys stay as they are.
    Reserved(Chord),
    /// Another action has these keys.
    Taken(Chord, Action),
}

/// The keys of the customizable shortcuts. Only the actions given other keys
/// are stored; every other action has its default.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Keymap {
    changed: BTreeMap<Action, Chord>,
}

impl Keymap {
    /// The keys of `action`.
    #[must_use]
    pub fn chord(&self, action: Action) -> Chord {
        self.changed
            .get(&action)
            .copied()
            .unwrap_or_else(|| action.default_chord())
    }

    /// Whether `action` has other keys than in a fresh window.
    #[must_use]
    pub fn is_changed(&self, action: Action) -> bool {
        self.changed.contains_key(&action)
    }

    /// The action these keys run, if any.
    #[must_use]
    pub fn action_of(&self, chord: Chord) -> Option<Action> {
        Action::ALL
            .into_iter()
            .find(|action| self.chord(*action) == chord)
    }

    /// Give `action` the keys `chord`. Reserved keys and keys of another
    /// action are refused, so that no two actions share keys.
    ///
    /// # Errors
    ///
    /// [`KeymapError`] says why the keys were refused.
    pub fn set(&mut self, action: Action, chord: Chord) -> Result<(), KeymapError> {
        if chord.is_reserved() {
            return Err(KeymapError::Reserved(chord));
        }
        if let Some(other) = self.action_of(chord).filter(|other| *other != action) {
            return Err(KeymapError::Taken(chord, other));
        }
        if chord == action.default_chord() {
            self.changed.remove(&action);
        } else {
            self.changed.insert(action, chord);
        }
        Ok(())
    }

    /// Give `action` its keys of a fresh window back, unless another action
    /// has them now; whether it did.
    pub fn reset(&mut self, action: Action) -> bool {
        let chord = action.default_chord();
        if self.action_of(chord).is_some_and(|other| other != action) {
            return false;
        }
        self.changed.remove(&action);
        true
    }

    /// Every action back to its keys of a fresh window.
    pub fn reset_all(&mut self) {
        self.changed.clear();
    }

    /// Whether the action was pressed this frame; the keys are consumed.
    pub fn pressed(&self, ui: &egui::Ui, action: Action) -> bool {
        self.chord(action).consume(ui)
    }

    /// A keymap read from a file: actions that would share keys, or that
    /// have reserved keys, are left at their defaults.
    #[must_use]
    fn sanitized(self) -> Self {
        let mut clean = Self::default();
        for (action, chord) in self.changed {
            // Refused keys are left out, as in a fresh window.
            let _ = clean.set(action, chord);
        }
        clean
    }
}

/// What is kept between sessions: the layout in use, the keys and the named
/// workspaces.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct Settings {
    /// Format of the file; 1.
    pub format: u32,
    /// The layout of the last window.
    pub layout: Layout,
    /// Keys given to the customizable shortcuts.
    pub keys: Keymap,
    /// Named workspaces, in the order they were saved.
    pub workspaces: Vec<Workspace>,
}

impl Settings {
    /// Read settings from `text`; damaged or foreign text gives the
    /// defaults, and bad entries are dropped.
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        let Some(mut settings) = serde_json::from_str::<Self>(text)
            .ok()
            .filter(|settings| settings.format == 1)
        else {
            return Self::default();
        };
        settings.keys = settings.keys.sanitized();
        settings
            .workspaces
            .retain(|workspace| valid_name(&workspace.name));
        settings.workspaces.truncate(MAX_WORKSPACES);
        settings
    }

    /// The settings as their file holds them.
    ///
    /// # Errors
    ///
    /// When serde cannot write them, which it always can.
    pub fn to_text(&self) -> Result<String, serde_json::Error> {
        let mut settings = self.clone();
        settings.format = 1;
        serde_json::to_string_pretty(&settings)
    }

    /// Keep `layout` under `name`, replacing a workspace of that name.
    ///
    /// # Errors
    ///
    /// A message for the status line: a bad name, or too many workspaces.
    pub fn save_workspace(&mut self, name: &str, layout: Layout) -> Result<(), &'static str> {
        let name = name.trim();
        if !valid_name(name) {
            return Err(BAD_NAME);
        }
        if let Some(existing) = self
            .workspaces
            .iter_mut()
            .find(|workspace| workspace.name == name)
        {
            existing.layout = layout;
            return Ok(());
        }
        if self.workspaces.len() >= MAX_WORKSPACES {
            return Err(TOO_MANY);
        }
        self.workspaces.push(Workspace {
            name: name.to_owned(),
            layout,
        });
        Ok(())
    }

    /// Forget the workspace called `name`; whether there was one.
    pub fn delete_workspace(&mut self, name: &str) -> bool {
        let before = self.workspaces.len();
        self.workspaces.retain(|workspace| workspace.name != name);
        self.workspaces.len() != before
    }
}

/// Whether `name` can name a workspace.
fn valid_name(name: &str) -> bool {
    let count = name.chars().count();
    (1..=MAX_NAME_CHARS).contains(&count)
        && !name.chars().any(char::is_control)
        && name.trim() == name
}

/// The settings file in the user's data folder.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug)]
pub struct Preferences {
    file: std::path::PathBuf,
}

#[cfg(not(target_arch = "wasm32"))]
impl Preferences {
    /// Largest file read.
    const MAX_BYTES: usize = 256 * 1024;

    /// The file in the user's data folder.
    #[must_use]
    pub fn in_user_data() -> Option<Self> {
        Some(Self::at(
            crate::recovery::user_data_dir()?.join("workspace.json"),
        ))
    }

    /// The settings in `file`, created at the first change.
    #[must_use]
    pub const fn at(file: std::path::PathBuf) -> Self {
        Self { file }
    }

    /// The settings; the defaults when the file is missing or damaged.
    #[must_use]
    pub fn load(&self) -> Settings {
        crate::files::read_text(&self.file, Self::MAX_BYTES)
            .map(|text| Settings::from_text(&text))
            .unwrap_or_default()
    }

    /// Write `settings`.
    ///
    /// # Errors
    ///
    /// The message of the failed write.
    pub fn store(&self, settings: &Settings) -> Result<(), String> {
        if let Some(dir) = self.file.parent() {
            crate::recovery::private_dir(dir).map_err(|error| error.to_string())?;
        }
        let text = settings.to_text().map_err(|error| error.to_string())?;
        crate::files::write_atomic(&self.file, text).map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_chords_are_the_keys_of_the_keyboard_window() {
        for (keys, _) in crate::palette::KEYS {
            if let Some(action) = Action::for_keys_row(keys) {
                assert_eq!(action.default_chord().text(), *keys);
            }
        }
        // Every action of the window that had keys before is found by its row.
        for keys in [
            "W", "R", "Ctrl+A", "Ctrl+D", "Ctrl+H", "F2", "G", "Ctrl+K", "Alt+Left", "0", "F", "T",
        ] {
            assert!(Action::for_keys_row(keys).is_some(), "{keys}");
        }
    }

    #[test]
    fn no_two_default_chords_are_the_same_and_none_is_reserved() {
        let mut seen = std::collections::BTreeSet::new();
        for action in Action::ALL {
            let chord = action.default_chord();
            assert!(seen.insert(chord), "{action:?}");
            assert!(!chord.is_reserved(), "{action:?}");
        }
    }

    #[test]
    fn chords_are_written_and_read_back() {
        for action in Action::ALL {
            let chord = action.default_chord();
            assert_eq!(Chord::parse(&chord.text()), Some(chord));
        }
        let chord = Chord::parse("Ctrl+Shift+Alt+F5");
        assert_eq!(chord.map(Chord::text).as_deref(), Some("Ctrl+Shift+Alt+F5"));
        assert_eq!(Chord::parse("Ctrl+Nothing"), None);
        assert_eq!(Chord::parse(""), None);
    }

    #[test]
    fn keys_of_another_action_or_reserved_keys_are_refused() {
        let mut keys = Keymap::default();
        let r = Action::Rotate.default_chord();
        assert_eq!(
            keys.set(Action::WireTool, r),
            Err(KeymapError::Taken(r, Action::Rotate))
        );
        let undo = Chord::pressed(Key::Z, Modifiers::COMMAND);
        assert_eq!(
            keys.set(Action::WireTool, undo),
            Err(KeymapError::Reserved(undo))
        );
        let e = Chord::pressed(Key::E, Modifiers::NONE);
        assert_eq!(keys.set(Action::WireTool, e), Ok(()));
        assert_eq!(keys.chord(Action::WireTool), e);
        assert!(keys.is_changed(Action::WireTool));
        // W is free now and can go to another action; the reset of the wire
        // tool is then refused.
        let w = Action::WireTool.default_chord();
        assert_eq!(keys.set(Action::Rotate, w), Ok(()));
        assert!(!keys.reset(Action::WireTool));
        keys.reset_all();
        assert_eq!(keys, Keymap::default());
    }

    #[test]
    fn setting_the_default_keys_back_stores_nothing() {
        let mut keys = Keymap::default();
        let e = Chord::pressed(Key::E, Modifiers::NONE);
        assert_eq!(keys.set(Action::WireTool, e), Ok(()));
        assert_eq!(
            keys.set(Action::WireTool, Action::WireTool.default_chord()),
            Ok(())
        );
        assert_eq!(keys, Keymap::default());
    }

    #[test]
    fn a_damaged_file_gives_the_defaults_and_bad_entries_are_dropped() {
        assert_eq!(Settings::from_text("not json"), Settings::default());
        assert_eq!(Settings::from_text("{\"format\": 2}"), Settings::default());
        // Two actions on one key: the second is dropped; a reserved key too.
        let text = r#"{
            "format": 1,
            "layout": { "theme": "dark", "rulers": true },
            "keys": { "wire_tool": "E", "rotate": "E", "duplicate": "Ctrl+Z" },
            "workspaces": [
                { "name": "Review", "layout": { "side_panels_hidden": true } },
                { "name": "", "layout": {} }
            ]
        }"#;
        let settings = Settings::from_text(text);
        assert_eq!(settings.layout.theme, ThemeChoice::Dark);
        assert!(settings.layout.rulers);
        assert_eq!(settings.keys.chord(Action::WireTool).text(), "E");
        assert_eq!(settings.keys.chord(Action::Rotate).text(), "R");
        assert_eq!(settings.keys.chord(Action::Duplicate).text(), "Ctrl+D");
        assert_eq!(settings.workspaces.len(), 1);
    }

    #[test]
    fn settings_survive_a_round_trip() {
        let mut settings = Settings::default();
        settings.layout.panels_swapped = true;
        let _ = settings
            .keys
            .set(Action::TraceNet, Chord::pressed(Key::N, Modifiers::COMMAND));
        assert_eq!(
            settings.save_workspace(
                "Wide",
                Layout {
                    side_panels_hidden: true,
                    ..Layout::default()
                }
            ),
            Ok(())
        );
        let text = settings.to_text().unwrap();
        let mut expected = settings.clone();
        expected.format = 1;
        assert_eq!(Settings::from_text(&text), expected);
    }

    #[test]
    fn workspaces_are_named_replaced_bounded_and_deleted() {
        let mut settings = Settings::default();
        assert!(settings.save_workspace(" ", Layout::default()).is_err());
        assert!(
            settings
                .save_workspace("bad\nname", Layout::default())
                .is_err()
        );
        assert_eq!(settings.save_workspace(" Draw ", Layout::default()), Ok(()));
        let swapped = Layout {
            panels_swapped: true,
            ..Layout::default()
        };
        assert_eq!(settings.save_workspace("Draw", swapped.clone()), Ok(()));
        assert_eq!(settings.workspaces.len(), 1);
        assert_eq!(settings.workspaces[0].layout, swapped);
        for index in 1..MAX_WORKSPACES {
            assert_eq!(
                settings.save_workspace(&format!("W{index}"), Layout::default()),
                Ok(())
            );
        }
        assert!(
            settings
                .save_workspace("One more", Layout::default())
                .is_err()
        );
        assert!(settings.delete_workspace("Draw"));
        assert!(!settings.delete_workspace("Draw"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn preferences_are_kept_in_a_file() {
        let dir = std::env::temp_dir().join(format!(
            "opencircuit-workspace-{}-{}",
            std::process::id(),
            line!()
        ));
        let preferences = Preferences::at(dir.join("workspace.json"));
        assert_eq!(preferences.load(), Settings::default());
        let mut settings = Settings::default();
        settings.layout.theme = ThemeChoice::Light;
        preferences.store(&settings).unwrap();
        assert_eq!(preferences.load().layout.theme, ThemeChoice::Light);
        let _ = std::fs::remove_dir_all(dir);
    }
}
