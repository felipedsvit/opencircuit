//! The symbol editor (M9.4, issue #118): draw the symbol of a part and write
//! it as an extension library (`LIBRARY.md`).
//!
//! A person chooses what the part is (a template: lamp, buzzer, fuse, relay
//! coil, pushbutton, block), names it, and types its drawing as one
//! primitive per line, in scene subunits around the origin:
//!
//! ```text
//! line x1 y1 x2 y2
//! polyline x1 y1 x2 y2 x3 y3 …
//! rect x1 y1 x2 y2
//! circle x y radius
//! dot x y radius
//! text x y words
//! ```
//!
//! The library is checked by the same reader as any library file, so what
//! the editor saves is what the window and the command line load. Nothing
//! here draws a symbol of a standard: the person draws their own.

use circuit_model::ComponentCatalog;
use serde_json::{Value, json};

/// Most lines of a drawing.
const MAX_LINES: usize = 256;

/// What the part is: the class, the symbol kind and the parameters of its
/// model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Template {
    /// A pilot lamp (two terminals, a load).
    Lamp,
    /// A buzzer (two terminals, a load).
    Buzzer,
    /// A fuse (two terminals, always conducting).
    Fuse,
    /// The coil of a relay, with a normally open and a normally closed
    /// contact of the default drawing.
    RelayCoil,
    /// A pushbutton with a normally open contact.
    PushbuttonNo,
    /// A pushbutton with a normally closed contact.
    PushbuttonNc,
    /// A symbol-only block of two terminals.
    Block2,
    /// A symbol-only block of four terminals.
    Block4,
}

impl Template {
    /// Every template, in menu order.
    pub const ALL: [Self; 8] = [
        Self::Lamp,
        Self::Buzzer,
        Self::Fuse,
        Self::RelayCoil,
        Self::PushbuttonNo,
        Self::PushbuttonNc,
        Self::Block2,
        Self::Block4,
    ];

    /// The English name, translated by the caller.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Lamp => "Lamp",
            Self::Buzzer => "Buzzer",
            Self::Fuse => "Fuse",
            Self::RelayCoil => "Relay coil",
            Self::PushbuttonNo => "Pushbutton, normally open",
            Self::PushbuttonNc => "Pushbutton, normally closed",
            Self::Block2 => "Block, 2 terminals",
            Self::Block4 => "Block, 4 terminals",
        }
    }

    /// The symbol kind the drawing is for, as the library writes it.
    const fn kind(self) -> &'static str {
        match self {
            Self::Lamp => "lamp",
            Self::Buzzer => "buzzer",
            Self::Fuse => "fuse_link",
            Self::RelayCoil => "coil",
            Self::PushbuttonNo => "contact_no",
            Self::PushbuttonNc => "contact_nc",
            Self::Block2 => "block2",
            Self::Block4 => "block4",
        }
    }

    /// The key of the drawn symbol.
    #[must_use]
    pub const fn symbol_key(self) -> &'static str {
        match self {
            Self::Lamp | Self::Buzzer => "load",
            Self::Fuse => "link",
            Self::RelayCoil => "coil",
            Self::PushbuttonNo => "no",
            Self::PushbuttonNc => "nc",
            Self::Block2 | Self::Block4 => "block",
        }
    }

    /// Whether the part has a second drawing while it is operated.
    #[must_use]
    pub const fn has_operated(self) -> bool {
        matches!(self, Self::PushbuttonNo | Self::PushbuttonNc)
    }

    /// The drawing a new part of this template starts from: its leads to
    /// the terminals, which every drawing must reach.
    #[must_use]
    pub const fn starter(self) -> &'static str {
        match self {
            Self::Lamp | Self::Buzzer => "line 0 -20 0 -10\nline 0 10 0 20\ncircle 0 0 10\n",
            Self::Fuse => "line 0 -20 0 20\nrect -5 -12 5 12\n",
            Self::RelayCoil => "line 0 -20 0 -8\nline 0 8 0 20\nrect -12 -8 12 8\n",
            Self::PushbuttonNo => {
                "line 0 -20 0 -8\nline 0 8 0 20\nline 0 8 -8 -8\nline -18 0 -8 0\n"
            }
            Self::PushbuttonNc => {
                "line 0 -20 0 -8\nline 0 8 0 20\nline 0 8 8 -10\nline 0 -8 10 -8\nline -18 0 -4 0\n"
            }
            Self::Block2 => "line 0 -20 0 -10\nline 0 10 0 20\nrect -15 -10 15 10\n",
            Self::Block4 => {
                "line -20 -20 -20 -10\nline 20 -20 20 -10\nline -20 10 -20 20\nline 20 10 20 20\nrect -30 -10 30 10\n"
            }
        }
    }

    /// The model fields the template fixes.
    fn model(self) -> (&'static str, Value, &'static str, Value) {
        let load = json!([
            { "key": "rated_voltage_mv", "type": { "int": { "min": 1, "max": 4_294_967_295_u64 } }, "default": 24000 },
            { "key": "connection", "type": { "enum": ["dc", "line_neutral", "line_line"] }, "default": "dc" }
        ]);
        match self {
            Self::Lamp => ("lamp", Value::Null, "controls", load),
            Self::Buzzer => ("buzzer", Value::Null, "controls", load),
            Self::Fuse => ("fuse", Value::Null, "protection", json!([])),
            Self::RelayCoil => ("relay", Value::Null, "automation", load),
            Self::PushbuttonNo | Self::PushbuttonNc => {
                ("pushbutton", json!("push"), "controls", json!([]))
            }
            Self::Block2 | Self::Block4 => ("symbolic", Value::Null, "automation", json!([])),
        }
    }
}

/// One line of a drawing as a primitive of `LIBRARY.md`.
fn primitive(line: &str) -> Result<Option<Value>, String> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return Ok(None);
    }
    let (word, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    let numbers = |count: Option<usize>| -> Result<Vec<i64>, String> {
        let values: Vec<i64> = rest
            .split_whitespace()
            .map(|value| {
                value
                    .parse::<i64>()
                    .map_err(|_| format!("`{value}` is not a whole number"))
            })
            .collect::<Result<_, _>>()?;
        match count {
            Some(count) if values.len() != count => Err(format!("`{word}` takes {count} numbers")),
            _ => Ok(values),
        }
    };
    let at = |values: &[i64], index: usize| values.get(index).copied().unwrap_or(0);
    Ok(Some(match word {
        "line" => {
            let v = numbers(Some(4))?;
            json!({ "line": { "from": [at(&v, 0), at(&v, 1)], "to": [at(&v, 2), at(&v, 3)] } })
        }
        "rect" => {
            let v = numbers(Some(4))?;
            json!({ "rect": { "from": [at(&v, 0), at(&v, 1)], "to": [at(&v, 2), at(&v, 3)] } })
        }
        "circle" | "dot" => {
            let v = numbers(Some(3))?;
            json!({ word: { "center": [at(&v, 0), at(&v, 1)], "radius": at(&v, 2) } })
        }
        "polyline" => {
            let v = numbers(None)?;
            if v.len() < 4 || v.len() % 2 != 0 {
                return Err("`polyline` takes pairs of numbers, at least two points".to_owned());
            }
            let points: Vec<[i64; 2]> =
                v.chunks(2).map(|pair| [at(pair, 0), at(pair, 1)]).collect();
            json!({ "polyline": points })
        }
        "text" => {
            let mut parts = rest.trim().splitn(3, char::is_whitespace);
            let (Some(x), Some(y), Some(words)) = (parts.next(), parts.next(), parts.next()) else {
                return Err("`text` takes x, y and the words".to_owned());
            };
            let number = |value: &str| {
                value
                    .parse::<i64>()
                    .map_err(|_| format!("`{value}` is not a whole number"))
            };
            json!({ "text": { "at": [number(x)?, number(y)?], "text": words.trim() } })
        }
        other => {
            return Err(format!(
                "unknown primitive `{other}`; use line, polyline, rect, circle, dot or text"
            ));
        }
    }))
}

/// A drawing as a list of primitives, the lines numbered in messages.
///
/// # Errors
///
/// The first line that is not a primitive, or more lines than allowed.
pub fn primitives(drawing: &str) -> Result<Vec<Value>, String> {
    let lines: Vec<&str> = drawing.lines().collect();
    if lines.len() > MAX_LINES {
        return Err(format!("a drawing has at most {MAX_LINES} lines"));
    }
    let mut out = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        if let Some(value) =
            primitive(line).map_err(|error| format!("line {}: {error}", index.saturating_add(1)))?
        {
            out.push(value);
        }
    }
    Ok(out)
}

/// What the editor holds: one part of one library.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Draft {
    /// The namespace of the library, such as `mine`.
    pub namespace: String,
    /// The name of the library.
    pub library_name: String,
    /// The name of the part inside the namespace, such as `lamp_red`.
    pub part: String,
    /// The title shown in the palette.
    pub title: String,
    /// What the part is.
    pub template: Template,
    /// The drawing at rest.
    pub rest: String,
    /// The drawing while operated, for a pushbutton.
    pub operated: String,
    /// Who drew it, for the provenance of the library.
    pub origin: String,
}

impl Default for Draft {
    fn default() -> Self {
        let template = Template::Lamp;
        Self {
            namespace: "mine".to_owned(),
            library_name: "My symbols".to_owned(),
            part: "lamp".to_owned(),
            title: "My lamp".to_owned(),
            template,
            rest: template.starter().to_owned(),
            operated: String::new(),
            origin: "Drawn in the OpenCircuit symbol editor".to_owned(),
        }
    }
}

impl Draft {
    /// The identifier of the model: `namespace.part`.
    #[must_use]
    pub fn model_id(&self) -> String {
        format!("{}.{}", self.namespace.trim(), self.part.trim())
    }

    /// Start the drawing again from the leads of `template`.
    pub fn use_template(&mut self, template: Template) {
        self.template = template;
        self.rest = template.starter().to_owned();
        self.operated = if template.has_operated() {
            template.starter().to_owned()
        } else {
            String::new()
        };
    }

    /// The library as the file holds it.
    ///
    /// # Errors
    ///
    /// The first line of a drawing that is not a primitive.
    pub fn to_json(&self) -> Result<String, String> {
        let template = self.template;
        let (class, operator, category, params) = template.model();
        let rest = primitives(&self.rest).map_err(|error| format!("at rest: {error}"))?;
        let mut artwork = json!({ "rest": rest });
        if template.has_operated() && !self.operated.trim().is_empty() {
            let operated =
                primitives(&self.operated).map_err(|error| format!("operated: {error}"))?;
            if let Some(object) = artwork.as_object_mut() {
                object.insert("operated".to_owned(), Value::Array(operated));
            }
        }
        let drawn = json!({
            "key": template.symbol_key(),
            "kind": template.kind(),
            "min": 1,
            "max": 1,
            "slotted": false,
            "artwork": artwork,
        });
        let mut symbols = vec![drawn];
        if template == Template::RelayCoil {
            // The contacts of a relay keep the default drawing.
            for (key, kind) in [("no", "contact_no"), ("nc", "contact_nc")] {
                symbols.push(json!({
                    "key": key, "kind": kind, "min": 0, "max": 4, "slotted": true, "artwork": null
                }));
            }
        }
        let library = json!({
            "schema_version": 2,
            "id": self.namespace.trim(),
            "version": 1,
            "name": self.library_name.trim(),
            "provenance": {
                "origin": self.origin.trim(),
                "license": "MIT OR Apache-2.0",
                "standard_reference": null
            },
            "models": [{
                "id": self.model_id(),
                "version": 1,
                "class": class,
                "operator": operator,
                "category": category,
                "title": self.title.trim(),
                "note": null,
                "params": params,
                "symbols": symbols,
            }]
        });
        serde_json::to_string_pretty(&library)
            .map(|text| format!("{text}\n"))
            .map_err(|error| error.to_string())
    }

    /// The library checked by the reader of library files and added to
    /// `catalog`, as the window would load it.
    ///
    /// # Errors
    ///
    /// Why the library is refused: a drawing that is not a primitive, a
    /// primitive outside the bounds of the symbol or a terminal no line
    /// reaches, a namespace that is taken, and every other rule of
    /// `LIBRARY.md`.
    pub fn check(&self, catalog: &ComponentCatalog) -> Result<(String, ComponentCatalog), String> {
        let text = self.to_json()?;
        let library =
            circuit_project::library::from_str(&text).map_err(|error| error.to_string())?;
        let catalog = catalog
            .clone()
            .with_library(library)
            .map_err(|error| error.to_string())?;
        Ok((text, catalog))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> ComponentCatalog {
        circuit_project::stdlib::catalog()
    }

    #[test]
    fn every_template_starts_from_a_drawing_the_reader_accepts() {
        for template in Template::ALL {
            let mut draft = Draft::default();
            draft.use_template(template);
            draft.part = format!("part{}", template.symbol_key());
            let checked = draft.check(&catalog());
            assert!(checked.is_ok(), "{template:?}: {checked:?}");
            let (_, catalog) = checked.unwrap();
            assert!(catalog.latest().any(|model| model.id() == draft.model_id()));
        }
    }

    #[test]
    fn primitives_are_read_one_per_line_and_errors_name_the_line() {
        let parsed = primitives(
            "line 0 -20 0 20\n\n# a comment\ncircle 0 0 5\ntext 0 0 M 1\npolyline 0 0 5 5 10 0\n",
        )
        .unwrap();
        assert_eq!(parsed.len(), 4);
        assert_eq!(
            parsed[2],
            json!({ "text": { "at": [0, 0], "text": "M 1" } })
        );
        assert_eq!(
            primitives("line 0 0 1 1\nstar 1 2").unwrap_err(),
            "line 2: unknown primitive `star`; use line, polyline, rect, circle, dot or text"
        );
        assert!(primitives("line 0 0 1").is_err());
        assert!(primitives("polyline 0 0 1").is_err());
        assert!(primitives("circle a b c").is_err());
    }

    #[test]
    fn a_drawing_that_misses_a_terminal_or_leaves_the_bounds_is_refused() {
        let mut draft = Draft {
            rest: "circle 0 0 10\n".to_owned(),
            ..Draft::default()
        };
        assert!(draft.check(&catalog()).is_err());
        draft.rest = "line 0 -20 0 20\ncircle 0 0 50\n".to_owned();
        assert!(draft.check(&catalog()).is_err());
        draft.rest = Template::Lamp.starter().to_owned();
        draft.namespace = "std".to_owned();
        assert!(draft.check(&catalog()).is_err());
    }

    #[test]
    fn a_saved_library_is_read_back_as_it_was_written() {
        let draft = Draft::default();
        let (text, _) = draft.check(&catalog()).unwrap();
        let library = circuit_project::library::from_str(&text).unwrap();
        assert_eq!(library.id, "mine");
        assert_eq!(library.models.len(), 1);
    }
}
