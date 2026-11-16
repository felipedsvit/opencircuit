//! The standard parts library `std`: component models built in code, on top
//! of the built-in ones, for the eight tabs of the palette.
//!
//! It is an ordinary extension library (ADR-0008, `LIBRARY.md`): its models
//! name an existing class and existing symbol kinds and draw their own
//! artwork, and nothing in it is executed. It is not a file: the models are
//! assembled here so that every program that opens a project agrees on them.
//! Parts the discrete model cannot simulate use the symbol-only class
//! (ADR-0013) and say so in their note.
//!
//! The namespace is `std`; [`catalog`] is the built-in catalog with this
//! library added, and what every front end starts from.

mod actuators;
mod automation;
mod contacts;
mod controls;
mod lines;
mod motors;
mod protection;
mod supplies;

use circuit_model::catalog::{
    Category, ComponentModel, DeviceClass, PARAM_CONDUCTOR, PARAM_CONNECTION, PARAM_INITIAL_CLOSED,
    PARAM_RATED_VOLTAGE_MV, PARAM_SUPPLY, ParamDefinition, ParamType, Provenance, SymbolDefinition,
    SymbolKind,
};
use circuit_model::library::{ArtPoint, ArtPrimitive, Artwork, Library};
use circuit_model::schematic::ParamValue;
use circuit_model::{ComponentCatalog, Conductor, Connection};

/// The namespace of the standard library: its models are `std.<name>`.
pub const NAMESPACE: &str = "std";

/// The note every symbol-only part carries.
const NOT_SIMULATED: &str =
    "Not simulated: it is drawn and wired, but it has no behaviour in the simulation.";

/// The standard library.
#[must_use]
pub fn library() -> Library {
    let mut models = Vec::new();
    models.extend(supplies::models());
    models.extend(protection::models());
    models.extend(controls::models());
    models.extend(motors::models());
    models.extend(contacts::models());
    models.extend(automation::models());
    models.extend(actuators::models());
    models.extend(lines::models());
    Library {
        id: NAMESPACE.to_owned(),
        version: 1,
        name: "OpenCircuit standard parts".to_owned(),
        provenance: Provenance {
            origin: "OpenCircuit project, original drawings".to_owned(),
            license: "MIT OR Apache-2.0".to_owned(),
            standard_reference: None,
        },
        models,
    }
}

/// The built-in catalog with the standard library added: what the front
/// ends and the command line start from. A library that failed to load
/// would be a defect of this crate, caught by its tests; the built-in
/// catalog is returned then, so that a project of built-in models still
/// opens.
#[must_use]
pub fn catalog() -> ComponentCatalog {
    let builtin = ComponentCatalog::builtin();
    builtin.with_library(library()).unwrap_or(builtin)
}

const fn point(x: i32, y: i32) -> ArtPoint {
    ArtPoint { x, y }
}

/// A segment, in scene subunits (ten per grid unit).
fn line(x1: i32, y1: i32, x2: i32, y2: i32) -> ArtPrimitive {
    ArtPrimitive::Line {
        from: point(x1, y1),
        to: point(x2, y2),
    }
}

/// A rectangle outline between two opposite corners.
fn rect(x1: i32, y1: i32, x2: i32, y2: i32) -> ArtPrimitive {
    ArtPrimitive::Rect {
        from: point(x1, y1),
        to: point(x2, y2),
    }
}

/// A circle outline.
const fn circle(x: i32, y: i32, radius: i32) -> ArtPrimitive {
    ArtPrimitive::Circle {
        center: point(x, y),
        radius,
    }
}

/// A filled circle.
const fn dot(x: i32, y: i32, radius: i32) -> ArtPrimitive {
    ArtPrimitive::Dot {
        center: point(x, y),
        radius,
    }
}

/// A short label centred on a point.
fn text(x: i32, y: i32, label: &str) -> ArtPrimitive {
    ArtPrimitive::Text {
        at: point(x, y),
        text: label.to_owned(),
    }
}

/// Connected segments through the points in order.
fn polyline(points: &[(i32, i32)]) -> ArtPrimitive {
    ArtPrimitive::Polyline(points.iter().map(|&(x, y)| point(x, y)).collect())
}

/// The two leads of a vertical two-terminal symbol: from the terminals, 20
/// subunits above and below the origin, in to `inner` subunits from it.
fn vertical_leads(inner: i32) -> Vec<ArtPrimitive> {
    vec![
        line(0, -20, 0, inner.saturating_neg()),
        line(0, inner, 0, 20),
    ]
}

/// `parts` after the leads of a vertical two-terminal symbol.
fn with_vertical_leads(inner: i32, parts: Vec<ArtPrimitive>) -> Vec<ArtPrimitive> {
    let mut list = vertical_leads(inner);
    list.extend(parts);
    list
}

/// A block of a symbol-only part: a rectangle with a lead from each column
/// of terminals to the top and the bottom edge, and a label in the middle.
/// `columns` are the horizontal offsets of the terminals in subunits.
fn block_art(columns: &[i32], half_width: i32, half_height: i32, label: &str) -> Vec<ArtPrimitive> {
    let mut list = Vec::new();
    for &x in columns {
        list.push(line(x, -20, x, half_height.saturating_neg()));
        list.push(line(x, half_height, x, 20));
    }
    list.push(rect(
        half_width.saturating_neg(),
        half_height.saturating_neg(),
        half_width,
        half_height,
    ));
    if !label.is_empty() {
        list.push(text(0, 0, label));
    }
    list
}

/// A symbol with its own artwork, drawn the same in every state.
fn symbol(
    key: &str,
    kind: SymbolKind,
    min: u8,
    max: u8,
    slotted: bool,
    rest: Vec<ArtPrimitive>,
) -> SymbolDefinition {
    SymbolDefinition::new(key, kind, min, max, slotted).with_artwork(Artwork {
        rest,
        operated: None,
    })
}

/// A symbol that draws one thing at rest and another while operated.
fn symbol_with_states(
    key: &str,
    kind: SymbolKind,
    min: u8,
    max: u8,
    rest: Vec<ArtPrimitive>,
    operated: Vec<ArtPrimitive>,
) -> SymbolDefinition {
    SymbolDefinition::new(key, kind, min, max, false).with_artwork(Artwork {
        rest,
        operated: Some(operated),
    })
}

/// A symbol drawn with the artwork of its kind.
fn plain(key: &str, kind: SymbolKind, min: u8, max: u8, slotted: bool) -> SymbolDefinition {
    SymbolDefinition::new(key, kind, min, max, slotted)
}

/// A model of the library, named `std.<name>`.
fn model(
    name: &str,
    class: DeviceClass,
    category: Category,
    title: &str,
    params: Vec<ParamDefinition>,
    symbols: Vec<SymbolDefinition>,
) -> ComponentModel {
    ComponentModel::new(
        &format!("{NAMESPACE}.{name}"),
        1,
        class,
        params,
        symbols,
        // The library gives every model its own provenance when it is added
        // to a catalog; this one is replaced.
        Provenance {
            origin: String::new(),
            license: String::new(),
            standard_reference: None,
        },
    )
    .with_category(category)
    .with_title(title)
}

/// A symbol-only part (ADR-0013): one block symbol `body`.
fn symbolic(
    name: &str,
    category: Category,
    title: &str,
    kind: SymbolKind,
    art: Vec<ArtPrimitive>,
) -> ComponentModel {
    model(
        name,
        DeviceClass::Symbolic,
        category,
        title,
        Vec::new(),
        vec![symbol("body", kind, 1, 1, false, art)],
    )
    .with_note(NOT_SIMULATED)
}

/// The rated voltage of a load, in millivolts; mandatory.
fn rated_voltage() -> ParamDefinition {
    ParamDefinition {
        key: PARAM_RATED_VOLTAGE_MV.to_owned(),
        ty: ParamType::Int {
            min: 1,
            max: i64::from(u32::MAX),
        },
        default: None,
    }
}

/// How a two-terminal load connects to its supply; mandatory.
fn connection() -> ParamDefinition {
    ParamDefinition {
        key: PARAM_CONNECTION.to_owned(),
        ty: ParamType::Enum(
            Connection::ALL
                .into_iter()
                .map(|connection| connection.key().to_owned())
                .collect(),
        ),
        default: None,
    }
}

/// The parameters of a two-terminal load.
fn load_params() -> Vec<ParamDefinition> {
    vec![rated_voltage(), connection()]
}

/// Whether a maintained switch starts closed; false by default.
fn initial_closed() -> ParamDefinition {
    ParamDefinition {
        key: PARAM_INITIAL_CLOSED.to_owned(),
        ty: ParamType::Bool,
        default: Some(ParamValue::Bool(false)),
    }
}

/// A supply terminal of one conductor only, which is its default.
fn fixed_conductor(conductor: Conductor) -> ParamDefinition {
    ParamDefinition {
        key: PARAM_CONDUCTOR.to_owned(),
        ty: ParamType::Enum(vec![conductor.key().to_owned()]),
        default: Some(ParamValue::Text(conductor.key().to_owned())),
    }
}

/// The supply a terminal belongs to; mandatory.
fn supply_ref() -> ParamDefinition {
    ParamDefinition {
        key: PARAM_SUPPLY.to_owned(),
        ty: ParamType::SupplyRef,
        default: None,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use circuit_model::catalog::Operator;

    use super::*;

    fn standard() -> ComponentCatalog {
        ComponentCatalog::builtin()
            .with_library(library())
            .unwrap_or_else(|error| panic!("the standard library is invalid: {error}"))
    }

    #[test]
    fn the_standard_library_is_valid_and_is_part_of_the_default_catalog() {
        let default = catalog();

        assert_eq!(default, standard());
        assert!(default.models().len() > ComponentCatalog::builtin().models().len());
    }

    #[test]
    fn every_model_has_its_own_title_and_a_note_when_it_is_symbol_only() {
        let library = library();
        let mut titles = BTreeSet::new();
        for model in &library.models {
            assert!(titles.insert(model.title().to_owned()), "{}", model.id());
            assert_ne!(model.title(), model.id(), "{}", model.id());
            assert_eq!(
                model
                    .note()
                    .is_some_and(|note| note.starts_with("Not simulated")),
                model.class() == DeviceClass::Symbolic,
                "{}",
                model.id()
            );
        }
    }

    #[test]
    fn every_tab_of_the_palette_has_a_standard_part() {
        let library = library();
        for category in Category::ALL {
            assert!(
                library
                    .models
                    .iter()
                    .any(|model| model.category() == category),
                "{category:?}"
            );
        }
    }

    #[test]
    fn the_library_stays_within_the_limits_of_a_library() {
        let library = library();

        assert!(library.models.len() <= circuit_model::library::MAX_LIBRARY_MODELS);
        for model in &library.models {
            assert!(model.symbols().len() <= circuit_model::library::MAX_LIBRARY_SYMBOLS);
        }
    }

    #[test]
    fn operators_follow_the_class_of_each_model() {
        for model in &library().models {
            assert!(model.operator().fits(model.class()), "{}", model.id());
            if model.class() == DeviceClass::Symbolic {
                assert_eq!(model.operator(), Operator::None, "{}", model.id());
            }
        }
    }

    #[test]
    fn the_namespace_is_reserved_for_the_standard_library() {
        let again = standard().with_library(library());

        assert!(again.is_err());
    }
}
