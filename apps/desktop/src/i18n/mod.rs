//! The language of the window (issue #103, ADR-0017).
//!
//! English is the source and the default: the texts of the program are the
//! keys of the catalog, so a text with no translation shows as it is written.
//! Brazilian Portuguese is the first translation. The language is a state of
//! the window, chosen from the environment when the program starts and from the
//! Help menu while it runs; it is not stored, because the application has no
//! settings storage.
//!
//! Only what the window says is translated: menus, hints, the status bar, the
//! palette, the inspector, the problems list and the dialogs. Diagnostic codes
//! and messages, the command line and every file format stay English, as the
//! project rules say.
//!
//! The language is read from a thread-local that the window sets at the start
//! of every frame, so that two windows on two threads (the tests of the
//! window) never see each other's language.

mod pt_br;

use std::cell::Cell;
use std::fmt::Display;

/// A language the window can speak.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum Language {
    /// English, the source of every text.
    #[default]
    English,
    /// Brazilian Portuguese.
    PortugueseBrazil,
}

impl Language {
    /// Every language, in the order of the menu.
    pub const ALL: [Self; 2] = [Self::English, Self::PortugueseBrazil];

    /// The name of the language in that language, as the menu shows it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::PortugueseBrazil => "Português (Brasil)",
        }
    }

    /// The language a locale name asks for: `pt_BR.UTF-8`, `pt-BR`, `pt`,
    /// `en_US.UTF-8`, `C`. `None` for a name that is not recognised, so that
    /// the next variable of the environment gets its say.
    #[must_use]
    pub fn from_locale(name: &str) -> Option<Self> {
        let name = name.trim();
        let language = name
            .split(['_', '-', '.', '@'])
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        match language.as_str() {
            "pt" => Some(Self::PortugueseBrazil),
            "en" | "c" | "posix" => Some(Self::English),
            _ => None,
        }
    }

    /// The language of the environment, as `get` reads it: the first of
    /// `LC_ALL`, `LC_MESSAGES` and `LANG` that is set and not empty decides, as
    /// in POSIX. A language this window does not have, and no variable at all,
    /// give English.
    #[must_use]
    pub fn from_environment(get: impl Fn(&str) -> Option<String>) -> Self {
        ["LC_ALL", "LC_MESSAGES", "LANG"]
            .into_iter()
            .filter_map(get)
            .find(|value| !value.trim().is_empty())
            .and_then(|value| Self::from_locale(&value))
            .unwrap_or_default()
    }

    /// The text with this language's translation, or as written.
    #[must_use]
    pub fn translate(self, text: &'static str) -> &'static str {
        match self {
            Self::English => text,
            Self::PortugueseBrazil => pt_br::lookup(text).unwrap_or(text),
        }
    }

    /// Whether a count takes the singular form: one in English, none or one in
    /// Brazilian Portuguese.
    #[must_use]
    pub const fn is_singular(self, count: usize) -> bool {
        match self {
            Self::English => count == 1,
            Self::PortugueseBrazil => count <= 1,
        }
    }
}

thread_local! {
    static CURRENT: Cell<Language> = const { Cell::new(Language::English) };
}

/// Make `language` the language of this thread; the window calls it at the
/// start of every frame.
pub fn set_language(language: Language) {
    CURRENT.with(|current| current.set(language));
}

/// The language of this thread.
#[must_use]
pub fn language() -> Language {
    CURRENT.with(Cell::get)
}

/// A text of the window in the current language; as written when it has no
/// translation.
#[must_use]
pub fn tr(text: &'static str) -> &'static str {
    language().translate(text)
}

/// A text with `{0}`, `{1}`… in it, translated and filled with `args`.
#[must_use]
pub fn trf(template: &'static str, args: &[&dyn Display]) -> String {
    fill(tr(template), args)
}

/// `count` of a thing in the current language: `one` is the template of the
/// singular form and `other` that of the plural, both with `{0}` for the number.
#[must_use]
pub fn trn(one: &'static str, other: &'static str, count: usize) -> String {
    let template = if language().is_singular(count) {
        one
    } else {
        other
    };
    trf(template, &[&count])
}

/// Replace `{0}`, `{1}`… in `template` with the arguments. A placeholder
/// with no argument stays as written.
#[must_use]
pub fn fill(template: &str, args: &[&dyn Display]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let (before, from_brace) = rest.split_at(open);
        out.push_str(before);
        let argument = from_brace
            .get(1..)
            .and_then(|after| after.split_once('}'))
            .and_then(|(digits, tail)| {
                let index = digits.parse::<usize>().ok()?;
                Some((args.get(index)?, tail))
            });
        match argument {
            Some((value, tail)) => {
                out.push_str(&value.to_string());
                rest = tail;
            }
            None => {
                out.push('{');
                rest = from_brace.get(1..).unwrap_or_default();
            }
        }
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_language_is_english_and_a_thread_starts_in_it() {
        assert_eq!(Language::default(), Language::English);
        assert_eq!(language(), Language::English);
        assert_eq!(tr("Zoom in"), "Zoom in");
    }

    #[test]
    fn locale_names_pick_a_language_and_unknown_ones_pick_none() {
        for (name, language) in [
            ("pt_BR.UTF-8", Some(Language::PortugueseBrazil)),
            ("pt-BR", Some(Language::PortugueseBrazil)),
            ("pt", Some(Language::PortugueseBrazil)),
            ("PT_br", Some(Language::PortugueseBrazil)),
            ("en_US.UTF-8", Some(Language::English)),
            ("C", Some(Language::English)),
            ("POSIX", Some(Language::English)),
            ("de_DE.UTF-8", None),
            ("", None),
        ] {
            assert_eq!(Language::from_locale(name), language, "{name:?}");
        }
    }

    #[test]
    fn the_environment_is_read_as_posix_does_and_defaults_to_english() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| {
                pairs
                    .iter()
                    .find(|(key, _)| *key == name)
                    .map(|(_, value)| (*value).to_owned())
            }
        };

        // LC_ALL beats LC_MESSAGES beats LANG.
        let all = env(&[("LC_ALL", "en_US.UTF-8"), ("LANG", "pt_BR.UTF-8")]);
        assert_eq!(Language::from_environment(all), Language::English);
        let messages = env(&[("LC_MESSAGES", "pt_BR.UTF-8"), ("LANG", "en_US.UTF-8")]);
        assert_eq!(
            Language::from_environment(messages),
            Language::PortugueseBrazil
        );
        let lang = env(&[("LANG", "pt_BR.UTF-8")]);
        assert_eq!(Language::from_environment(lang), Language::PortugueseBrazil);
        // An empty variable is as good as none; nothing at all is English.
        let empty = env(&[("LC_ALL", ""), ("LANG", "pt_BR")]);
        assert_eq!(
            Language::from_environment(empty),
            Language::PortugueseBrazil
        );
        assert_eq!(Language::from_environment(env(&[])), Language::English);
        // A language the window lacks is English, and does not fall through.
        let german = env(&[("LC_ALL", "de_DE"), ("LANG", "pt_BR")]);
        assert_eq!(Language::from_environment(german), Language::English);
    }

    #[test]
    fn a_text_with_no_translation_shows_as_written_in_every_language() {
        let text = "This text has no translation";

        for language in Language::ALL {
            assert_eq!(language.translate(text), text);
        }
    }

    #[test]
    fn the_language_is_per_thread_and_set_language_changes_tr() {
        set_language(Language::PortugueseBrazil);
        assert_eq!(language(), Language::PortugueseBrazil);
        assert_ne!(tr("Zoom in"), "Zoom in");
        let other = std::thread::spawn(language).join().unwrap_or_default();
        assert_eq!(other, Language::English);

        set_language(Language::English);
        assert_eq!(tr("Zoom in"), "Zoom in");
    }

    #[test]
    fn placeholders_are_filled_in_order_by_number_and_left_when_there_is_no_argument() {
        assert_eq!(fill("{0} of {1}", &[&2, &"three"]), "2 of three");
        assert_eq!(fill("{1} before {0}", &[&"a", &"b"]), "b before a");
        assert_eq!(fill("no placeholder", &[&1]), "no placeholder");
        assert_eq!(fill("{2} missing {x} {", &[&1]), "{2} missing {x} {");
        assert_eq!(fill("{0}{0}", &[&"ab"]), "abab");
        assert_eq!(fill("ação {0} é útil", &[&"é"]), "ação é é útil");
    }

    #[test]
    fn a_count_takes_the_singular_of_its_language() {
        assert_eq!(trn("{0} wire", "{0} wires", 1), "1 wire");
        assert_eq!(trn("{0} wire", "{0} wires", 0), "0 wires");
        assert_eq!(trn("{0} wire", "{0} wires", 5), "5 wires");
        assert!(Language::PortugueseBrazil.is_singular(0));
        assert!(Language::PortugueseBrazil.is_singular(1));
        assert!(!Language::PortugueseBrazil.is_singular(2));
    }

    /// Every source file that writes texts of the window.
    const SOURCES: [&str; 8] = [
        include_str!("../app.rs"),
        include_str!("../print_preview.rs"),
        include_str!("../command_palette.rs"),
        include_str!("../find_replace.rs"),
        include_str!("../palette.rs"),
        include_str!("../inspector.rs"),
        include_str!("../documentation.rs"),
        include_str!("../dialogs.rs"),
    ];

    /// The `{0}`, `{1}`… of a text, in order.
    fn placeholders(text: &str) -> Vec<String> {
        let mut found = Vec::new();
        let mut rest = text;
        while let Some((_, after)) = rest.split_once('{') {
            let Some((inside, tail)) = after.split_once('}') else {
                break;
            };
            found.push(inside.to_owned());
            rest = tail;
        }
        found.sort();
        found
    }

    /// The string literal that starts at the first quote of `text`, after
    /// optional white space, and what follows it.
    fn literal(text: &str) -> Option<(String, &str)> {
        let body = text.trim_start().strip_prefix('"')?;
        let mut out = String::new();
        let mut chars = body.char_indices();
        while let Some((at, c)) = chars.next() {
            match c {
                '"' => return Some((out, body.get(at + 1..)?)),
                '\\' => out.push(chars.next()?.1),
                other => out.push(other),
            }
        }
        None
    }

    /// Every text the sources hand to `tr`, `trf` and `trn`.
    fn keys_in_the_sources() -> Vec<String> {
        let mut keys = Vec::new();
        for source in SOURCES {
            for call in ["tr(", "trf(", "trn("] {
                let mut rest = source;
                while let Some(at) = rest.find(call) {
                    let before = rest[..at].chars().next_back();
                    rest = &rest[at + call.len()..];
                    // `str(` or `fn tr(`: not a call of ours.
                    if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                        continue;
                    }
                    if let Some((first, after)) = literal(rest) {
                        keys.push(first);
                        if call == "trn("
                            && let Some((second, _)) =
                                after.trim_start().strip_prefix(',').and_then(literal)
                        {
                            keys.push(second);
                        }
                    }
                }
            }
        }
        keys.sort();
        keys.dedup();
        keys
    }

    /// Technical texts that read the same in both languages.
    const THE_SAME: [&str; 23] = [
        "{0} ({1})",
        "{0} terminal",
        "{0} wire",
        "0",
        "+ / -",
        "Alt+Left",
        "Ctrl+A",
        "Ctrl+C / Ctrl+V",
        "Ctrl+D",
        "Ctrl+H",
        "Ctrl+K",
        "Ctrl+Z / Ctrl+Y",
        "Del",
        "Esc",
        "F",
        "F2",
        "G",
        "PDF…",
        "R",
        "SVG…",
        "T",
        "W",
        "zoom",
    ];

    #[test]
    fn the_catalog_is_sorted_with_no_repeated_key() {
        let keys: Vec<&str> = pt_br::CATALOG.iter().map(|(key, _)| *key).collect();

        for pair in keys.windows(2) {
            assert!(
                pair[0] < pair[1],
                "{:?} is not before {:?}",
                pair[0],
                pair[1]
            );
        }
    }

    #[test]
    fn no_translation_is_empty_and_each_keeps_the_placeholders_of_its_key() {
        for (key, translation) in pt_br::CATALOG {
            assert!(!translation.trim().is_empty(), "{key:?} has no translation");
            assert_eq!(
                placeholders(key),
                placeholders(translation),
                "{key:?} and its translation differ in their placeholders"
            );
            assert_eq!(
                key.starts_with(' '),
                translation.starts_with(' '),
                "{key:?}: a leading space is part of the text"
            );
            if key == translation {
                assert!(
                    THE_SAME.contains(key),
                    "{key:?} is the same in both languages"
                );
            }
        }
    }

    #[test]
    fn every_text_the_sources_translate_has_a_translation_and_every_translation_is_used() {
        let used = keys_in_the_sources();
        assert!(used.len() > 150, "{} texts found", used.len());

        for key in &used {
            assert!(
                pt_br::lookup(key).is_some(),
                "{key:?} is written with tr but has no translation"
            );
        }
        let in_tables: Vec<&str> = crate::palette::KEYS
            .iter()
            .flat_map(|(keys, action)| [*keys, *action])
            .chain(
                circuit_model::Category::ALL
                    .into_iter()
                    .map(circuit_model::Category::label),
            )
            .chain(
                circuit_commands::SupplyPreset::ALL
                    .into_iter()
                    .map(circuit_commands::SupplyPreset::label),
            )
            .chain(
                crate::templates::TEMPLATES
                    .iter()
                    .map(|template| template.name),
            )
            .chain(
                crate::command_palette::COMMANDS
                    .iter()
                    .map(|(_, label)| *label),
            )
            .chain(
                crate::command_palette::KINDS
                    .iter()
                    .map(|(_, label)| *label),
            )
            .chain(
                crate::workspace::Action::ALL
                    .into_iter()
                    .map(crate::workspace::Action::description),
            )
            .chain(
                crate::workspace::ThemeChoice::ALL
                    .into_iter()
                    .map(crate::workspace::ThemeChoice::name),
            )
            .chain(crate::workspace::MESSAGES)
            .collect();
        for key in &in_tables {
            assert!(
                pt_br::lookup(key).is_some(),
                "{key:?} is in a table but has no translation"
            );
        }
        for (key, _) in pt_br::CATALOG {
            assert!(
                used.iter().any(|used| used == key) || in_tables.contains(key),
                "{key:?} is translated but no source uses it"
            );
        }
    }

    #[test]
    fn a_text_the_catalog_lacks_falls_back_to_english_while_the_rest_is_translated() {
        set_language(Language::PortugueseBrazil);

        assert_eq!(tr("Paste"), "Colar");
        assert_eq!(tr("A text nobody translated"), "A text nobody translated");
        assert_eq!(trf("Undo: {0}", &[&"Place K1"]), "Desfazer: Place K1");
        assert_eq!(trn("{0} wire", "{0} wires", 0), "0 fio");
        assert_eq!(trn("{0} wire", "{0} wires", 3), "3 fios");

        set_language(Language::English);
    }

    #[test]
    fn the_scan_finds_calls_over_several_lines_and_skips_other_functions() {
        let (key, rest) = literal("  \"two\\\"words\", x").unwrap();
        assert_eq!(key, "two\"words");
        assert_eq!(rest, ", x");
        assert!(literal("not a literal").is_none());
        assert_eq!(placeholders("{1} before {0}"), ["0", "1"]);
    }
}
