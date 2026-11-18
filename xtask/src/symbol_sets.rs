//! The symbol sets of M9.4: two extension libraries (`LIBRARY.md`) whose
//! models draw the common parts of a control circuit in an IEC-like and in
//! an ANSI-like style. The artwork is drawn for this project from the
//! geometry of the symbol kinds; it copies no symbol file and claims no
//! conformance with any standard.
//!
//! The libraries are generated here and committed under `libraries/`;
//! `generate-examples --check` keeps them in step with this file, and the
//! tests of `circuit-project` load them into the catalog.

use serde_json::{Value, json};

/// A line from `(x1, y1)` to `(x2, y2)`, in subunits.
fn line(x1: i32, y1: i32, x2: i32, y2: i32) -> Value {
    json!({ "line": { "from": [x1, y1], "to": [x2, y2] } })
}

fn rect(x1: i32, y1: i32, x2: i32, y2: i32) -> Value {
    json!({ "rect": { "from": [x1, y1], "to": [x2, y2] } })
}

fn circle(x: i32, y: i32, radius: i32) -> Value {
    json!({ "circle": { "center": [x, y], "radius": radius } })
}

fn polyline(points: &[(i32, i32)]) -> Value {
    let points: Vec<[i32; 2]> = points.iter().map(|(x, y)| [*x, *y]).collect();
    json!({ "polyline": points })
}

fn text(x: i32, y: i32, words: &str) -> Value {
    json!({ "text": { "at": [x, y], "text": words } })
}

/// The leads of a two-terminal symbol: from each terminal at `y = ±20` to
/// `y = ±reach`.
fn leads(reach: i32) -> Vec<Value> {
    vec![line(0, -20, 0, -reach), line(0, reach, 0, 20)]
}

fn with_leads(reach: i32, mut art: Vec<Value>) -> Vec<Value> {
    let mut all = leads(reach);
    all.append(&mut art);
    all
}

/// The two drawing styles.
#[derive(Clone, Copy)]
enum Style {
    Iec,
    Ansi,
}

impl Style {
    const fn namespace(self) -> &'static str {
        match self {
            Self::Iec => "iec",
            Self::Ansi => "ansi",
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Iec => "IEC-style symbols",
            Self::Ansi => "ANSI-style symbols",
        }
    }

    /// A relay or contactor coil.
    fn coil(self) -> Vec<Value> {
        match self {
            Self::Iec => with_leads(8, vec![rect(-12, -8, 12, 8)]),
            Self::Ansi => with_leads(10, vec![circle(0, 0, 10)]),
        }
    }

    /// A normally open contact at rest and operated.
    fn no(self) -> (Vec<Value>, Vec<Value>) {
        match self {
            Self::Iec => (
                with_leads(8, vec![line(0, 8, -8, -8)]),
                with_leads(8, vec![line(0, 8, 0, -8)]),
            ),
            Self::Ansi => {
                let plates = vec![line(-10, -4, 10, -4), line(-10, 4, 10, 4)];
                let mut operated = plates.clone();
                operated.push(line(0, -4, 0, 4));
                (with_leads(4, plates), with_leads(4, operated))
            }
        }
    }

    /// A normally closed contact at rest and operated.
    fn nc(self) -> (Vec<Value>, Vec<Value>) {
        match self {
            Self::Iec => (
                with_leads(8, vec![line(0, 8, 8, -10), line(0, -8, 10, -8)]),
                with_leads(8, vec![line(0, 8, -8, -6), line(0, -8, 10, -8)]),
            ),
            Self::Ansi => {
                let plates = vec![line(-10, -4, 10, -4), line(-10, 4, 10, 4)];
                let mut rest = plates.clone();
                rest.push(line(-10, 10, 10, -10));
                (with_leads(4, rest), with_leads(4, plates))
            }
        }
    }

    /// The actuator of a pushbutton, beside its contact.
    fn button(self, pressed: bool) -> Vec<Value> {
        match self {
            Self::Iec => {
                let x = if pressed { -4 } else { -8 };
                vec![line(-18, 0, x, 0), line(-18, -5, -18, 5)]
            }
            Self::Ansi => {
                let x = if pressed { -12 } else { -16 };
                vec![line(x, -10, x, 10), line(-20, 0, x, 0)]
            }
        }
    }

    /// A pilot lamp.
    fn lamp(self) -> Vec<Value> {
        match self {
            Self::Iec => with_leads(
                10,
                vec![circle(0, 0, 10), line(-7, -7, 7, 7), line(7, -7, -7, 7)],
            ),
            Self::Ansi => with_leads(
                10,
                vec![
                    circle(0, 0, 10),
                    line(12, -12, 16, -16),
                    line(14, 0, 19, 0),
                    line(12, 12, 16, 16),
                    text(0, 0, "L"),
                ],
            ),
        }
    }

    /// A fuse link.
    fn fuse(self) -> Vec<Value> {
        match self {
            Self::Iec => vec![line(0, -20, 0, 20), rect(-5, -12, 5, 12)],
            Self::Ansi => with_leads(
                12,
                vec![polyline(&[(0, -12), (6, -6), (0, 0), (-6, 6), (0, 12)])],
            ),
        }
    }
}

fn symbol(
    key: &str,
    kind: &str,
    max: u8,
    slotted: bool,
    art: (Vec<Value>, Option<Vec<Value>>),
) -> Value {
    let (rest, operated) = art;
    let artwork = match operated {
        Some(operated) => json!({ "rest": rest, "operated": operated }),
        None => json!({ "rest": rest }),
    };
    json!({
        "key": key,
        "kind": kind,
        "min": 0,
        "max": max,
        "slotted": slotted,
        "artwork": artwork,
    })
}

fn load_params() -> Value {
    json!([
        { "key": "rated_voltage_mv", "type": { "int": { "min": 1, "max": 4_294_967_295_u64 } }, "default": 24000 },
        { "key": "connection", "type": { "enum": ["dc", "line_neutral", "line_line"] }, "default": "dc" }
    ])
}

/// The library of one style.
fn library(style: Style) -> Value {
    let ns = style.namespace();
    let label = match style {
        Style::Iec => "IEC style",
        Style::Ansi => "ANSI style",
    };
    let (no_rest, no_operated) = style.no();
    let (nc_rest, nc_operated) = style.nc();
    let pushed = |mut contact: Vec<Value>, pressed: bool| {
        contact.extend(style.button(pressed));
        contact
    };
    let mut coil = symbol("coil", "coil", 1, false, (style.coil(), None));
    if let Some(min) = coil.get_mut("min") {
        *min = json!(1);
    }
    json!({
        "schema_version": 2,
        "id": ns,
        "version": 1,
        "name": style.name(),
        "provenance": {
            "origin": "Drawn for OpenCircuit from the geometry of its symbol kinds; no symbol file was copied, and no conformance with a standard is claimed",
            "license": "MIT OR Apache-2.0",
            "standard_reference": null
        },
        "models": [
            {
                "id": format!("{ns}.relay"),
                "version": 1,
                "class": "relay",
                "operator": null,
                "category": "automation",
                "title": format!("Relay ({label})"),
                "note": null,
                "params": load_params(),
                "symbols": [
                    coil,
                    symbol("no", "contact_no", 4, true, (no_rest.clone(), Some(no_operated.clone()))),
                    symbol("nc", "contact_nc", 4, true, (nc_rest.clone(), Some(nc_operated.clone())))
                ]
            },
            {
                "id": format!("{ns}.pushbutton_no"),
                "version": 1,
                "class": "pushbutton",
                "operator": "push",
                "category": "controls",
                "title": format!("Pushbutton, normally open ({label})"),
                "note": null,
                "params": [],
                "symbols": [
                    symbol("no", "contact_no", 1, false, (pushed(no_rest, false), Some(pushed(no_operated, true))))
                ]
            },
            {
                "id": format!("{ns}.pushbutton_nc"),
                "version": 1,
                "class": "pushbutton",
                "operator": "push",
                "category": "controls",
                "title": format!("Pushbutton, normally closed ({label})"),
                "note": null,
                "params": [],
                "symbols": [
                    symbol("nc", "contact_nc", 1, false, (pushed(nc_rest, false), Some(pushed(nc_operated, true))))
                ]
            },
            {
                "id": format!("{ns}.lamp"),
                "version": 1,
                "class": "lamp",
                "operator": null,
                "category": "controls",
                "title": format!("Pilot lamp ({label})"),
                "note": null,
                "params": load_params(),
                "symbols": [ symbol("lamp", "lamp", 1, false, (style.lamp(), None)) ]
            },
            {
                "id": format!("{ns}.fuse"),
                "version": 1,
                "class": "fuse",
                "operator": null,
                "category": "protection",
                "title": format!("Fuse ({label})"),
                "note": "A fuse never melts in the simulation: it always conducts.",
                "params": [],
                "symbols": [ symbol("link", "fuse_link", 1, false, (style.fuse(), None)) ]
            }
        ]
    })
}

/// The generated libraries, as `(path, text)`.
pub(crate) fn outputs() -> Vec<Result<(String, Vec<u8>), String>> {
    [
        (Style::Iec, "libraries/iec-style.oclib"),
        (Style::Ansi, "libraries/ansi-style.oclib"),
    ]
    .into_iter()
    .map(|(style, path)| {
        serde_json::to_string_pretty(&library(style))
            .map(|text| (path.to_owned(), format!("{text}\n").into_bytes()))
            .map_err(|error| format!("{path}: {error}"))
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_sets_load_into_the_catalog() {
        for output in outputs() {
            let (path, bytes) = output.unwrap();
            let text = String::from_utf8(bytes).unwrap();
            let library = circuit_project::library::from_str(&text)
                .unwrap_or_else(|error| panic!("{path}: {error}"));
            circuit_project::stdlib::catalog()
                .with_library(library)
                .unwrap_or_else(|error| panic!("{path}: {error}"));
        }
    }
}
