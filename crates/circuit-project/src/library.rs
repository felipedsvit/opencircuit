//! Extension library files (ADR-0008, `LIBRARY.md`).
//!
//! A library is JSON, read strictly like a project: unknown fields, repeated
//! fields and floating-point numbers are errors, and sizes are bounded. The
//! text becomes a [`Library`] of `circuit-model`; whether its models are
//! consistent, live in their namespace and draw within their symbols is
//! checked when the library is added to a catalog
//! ([`ComponentCatalog::with_library`](circuit_model::ComponentCatalog::with_library)).
//! Nothing in a library is executed.

use circuit_model::catalog::{
    Category, ComponentModel, DeviceClass, MAX_NOTE_BYTES, Operator, ParamDefinition, ParamType,
    Provenance, SymbolDefinition, SymbolKind, TimerMode,
};
use circuit_model::library::{
    ArtPoint, ArtPrimitive, Artwork, Library, MAX_LIBRARY_BYTES, MAX_LIBRARY_MODELS,
    MAX_LIBRARY_SYMBOLS,
};
use circuit_model::limits::{MAX_PARAMS_PER_DEVICE, MAX_TEXT_BYTES};
use circuit_model::schematic::ParamValue;
use serde::Deserialize;

/// The newest library schema version this build reads.
///
/// Version 2 adds the optional model fields `category`, `title` and `note`;
/// version 1 files keep loading, and their models take the category of their
/// class and the identifier as title.
pub const LIBRARY_SCHEMA_VERSION: u32 = 2;

/// The first library schema version.
const FIRST_LIBRARY_SCHEMA_VERSION: u32 = 1;

/// Why a library could not be read.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum LibraryError {
    /// The text is larger than [`MAX_LIBRARY_BYTES`].
    #[error("the library is {size} bytes; the limit is {limit}")]
    TooLarge {
        /// Size of the text.
        size: usize,
        /// The limit.
        limit: usize,
    },
    /// The text does not match the library schema.
    #[error("the library does not match the library schema: {0}")]
    Schema(String),
    /// The library uses a schema version this build does not read.
    #[error("the library uses schema version {found}; this build reads version {supported}")]
    UnsupportedVersion {
        /// Version found in the file.
        found: u32,
        /// Version this build reads.
        supported: u32,
    },
    /// A size limit was exceeded.
    #[error("the library has too many {what}; the limit is {limit}")]
    LimitExceeded {
        /// What there is too much of.
        what: &'static str,
        /// The limit.
        limit: usize,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LibraryFile {
    schema_version: u32,
    id: String,
    version: u32,
    name: String,
    provenance: ProvenanceFile,
    models: Vec<ModelFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProvenanceFile {
    origin: String,
    license: String,
    standard_reference: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelFile {
    id: String,
    version: u32,
    class: ClassFile,
    operator: Option<OperatorFile>,
    category: Option<String>,
    title: Option<String>,
    note: Option<String>,
    params: Vec<ParamFile>,
    symbols: Vec<SymbolFile>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ClassFile {
    Contactor,
    Relay,
    ThermalOverload,
    TimerOn,
    TimerOff,
    Pushbutton,
    LatchingSwitch,
    Fuse,
    Motor,
    Lamp,
    Buzzer,
    PowerSupply,
    MechanicalInterlock,
    Transformer,
    Symbolic,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
enum OperatorFile {
    None,
    Push,
    Latch,
    Emergency,
    Position,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParamFile {
    key: String,
    #[serde(rename = "type")]
    ty: ParamTypeFile,
    default: Option<ParamValue>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum ParamTypeFile {
    Bool,
    Int { min: i64, max: i64 },
    Enum(Vec<String>),
    SupplyRef,
    DeviceRef,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SymbolFile {
    key: String,
    kind: KindFile,
    min: u8,
    max: u8,
    slotted: bool,
    artwork: Option<ArtworkFile>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum KindFile {
    Coil,
    ContactNo,
    ContactNc,
    Changeover,
    PowerContacts { poles: u8 },
    ThermalElement { poles: u8 },
    FuseLink,
    Motor,
    MotorSixTerminal,
    Lamp,
    Buzzer,
    SupplyTerminal,
    InterlockLink,
    TransformerPrimary,
    TransformerSecondary,
    Block2,
    Block4,
    Block8,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtworkFile {
    rest: Vec<PrimitiveFile>,
    operated: Option<Vec<PrimitiveFile>>,
}

type PointFile = [i32; 2];

#[derive(Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
enum PrimitiveFile {
    Line { from: PointFile, to: PointFile },
    Polyline(Vec<PointFile>),
    Rect { from: PointFile, to: PointFile },
    Circle { center: PointFile, radius: i32 },
    Dot { center: PointFile, radius: i32 },
    Text { at: PointFile, text: String },
}

/// Read a library from its text.
///
/// # Errors
///
/// Returns [`LibraryError`] when the text is too large, does not match the
/// schema (including an unknown or repeated field and a floating-point
/// number), uses another schema version, or exceeds a size limit.
pub fn from_str(text: &str) -> Result<Library, LibraryError> {
    if text.len() > MAX_LIBRARY_BYTES {
        return Err(LibraryError::TooLarge {
            size: text.len(),
            limit: MAX_LIBRARY_BYTES,
        });
    }
    let file: LibraryFile =
        serde_json::from_str(text).map_err(|error| LibraryError::Schema(error.to_string()))?;
    if !(FIRST_LIBRARY_SCHEMA_VERSION..=LIBRARY_SCHEMA_VERSION).contains(&file.schema_version) {
        return Err(LibraryError::UnsupportedVersion {
            found: file.schema_version,
            supported: LIBRARY_SCHEMA_VERSION,
        });
    }
    limit("models", file.models.len(), MAX_LIBRARY_MODELS)?;
    for text in [
        &file.id,
        &file.name,
        &file.provenance.origin,
        &file.provenance.license,
    ]
    .into_iter()
    .chain(file.provenance.standard_reference.as_ref())
    {
        limit("bytes in a text", text.len(), MAX_TEXT_BYTES)?;
    }
    let provenance = Provenance {
        origin: file.provenance.origin,
        license: file.provenance.license,
        standard_reference: file.provenance.standard_reference,
    };
    let has_v2_fields = |model: &ModelFile| {
        model.category.is_some() || model.title.is_some() || model.note.is_some()
    };
    if file.schema_version < 2 {
        if let Some(model) = file.models.iter().find(|model| has_v2_fields(model)) {
            return Err(LibraryError::Schema(format!(
                "model `{}`: `category`, `title` and `note` need schema version 2",
                model.id
            )));
        }
    }
    let models = file
        .models
        .into_iter()
        .map(|model| to_model(model, &provenance))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Library {
        id: file.id,
        version: file.version,
        name: file.name,
        provenance,
        models,
    })
}

fn limit(what: &'static str, count: usize, max: usize) -> Result<(), LibraryError> {
    if count > max {
        Err(LibraryError::LimitExceeded { what, limit: max })
    } else {
        Ok(())
    }
}

fn to_model(model: ModelFile, provenance: &Provenance) -> Result<ComponentModel, LibraryError> {
    limit("bytes in a text", model.id.len(), MAX_TEXT_BYTES)?;
    limit(
        "parameters in a model",
        model.params.len(),
        MAX_PARAMS_PER_DEVICE,
    )?;
    limit(
        "symbols in a model",
        model.symbols.len(),
        MAX_LIBRARY_SYMBOLS,
    )?;
    let class = match model.class {
        ClassFile::Contactor => DeviceClass::Contactor,
        ClassFile::Relay => DeviceClass::Relay,
        ClassFile::ThermalOverload => DeviceClass::ThermalOverload,
        ClassFile::TimerOn => DeviceClass::Timer(TimerMode::OnDelay),
        ClassFile::TimerOff => DeviceClass::Timer(TimerMode::OffDelay),
        ClassFile::Pushbutton => DeviceClass::Pushbutton,
        ClassFile::LatchingSwitch => DeviceClass::LatchingSwitch,
        ClassFile::Fuse => DeviceClass::Fuse,
        ClassFile::Motor => DeviceClass::Motor,
        ClassFile::Lamp => DeviceClass::Lamp,
        ClassFile::Buzzer => DeviceClass::Buzzer,
        ClassFile::PowerSupply => DeviceClass::PowerSupply,
        ClassFile::MechanicalInterlock => DeviceClass::MechanicalInterlock,
        ClassFile::Transformer => DeviceClass::Transformer,
        ClassFile::Symbolic => DeviceClass::Symbolic,
    };
    let params = model
        .params
        .into_iter()
        .map(to_param)
        .collect::<Result<Vec<_>, _>>()?;
    let symbols = model
        .symbols
        .into_iter()
        .map(to_symbol)
        .collect::<Result<Vec<_>, _>>()?;
    let mut built = ComponentModel::new(
        &model.id,
        model.version,
        class,
        params,
        symbols,
        provenance.clone(),
    );
    if let Some(operator) = model.operator {
        built = built.with_operator(match operator {
            OperatorFile::None => Operator::None,
            OperatorFile::Push => Operator::Push,
            OperatorFile::Latch => Operator::Latch,
            OperatorFile::Emergency => Operator::Emergency,
            OperatorFile::Position => Operator::Position,
        });
    }
    if let Some(key) = &model.category {
        let category = Category::from_key(key).ok_or_else(|| {
            LibraryError::Schema(format!("model `{}`: `{key}` is not a category", model.id))
        })?;
        built = built.with_category(category);
    }
    if let Some(title) = &model.title {
        limit("bytes in a text", title.len(), MAX_TEXT_BYTES)?;
        built = built.with_title(title);
    }
    if let Some(note) = &model.note {
        limit("bytes in a note", note.len(), MAX_NOTE_BYTES)?;
        built = built.with_note(note);
    }
    Ok(built)
}

fn to_param(param: ParamFile) -> Result<ParamDefinition, LibraryError> {
    limit("bytes in a text", param.key.len(), MAX_TEXT_BYTES)?;
    let ty = match param.ty {
        ParamTypeFile::Bool => ParamType::Bool,
        ParamTypeFile::Int { min, max } => ParamType::Int { min, max },
        ParamTypeFile::Enum(choices) => {
            limit(
                "choices in a parameter",
                choices.len(),
                MAX_PARAMS_PER_DEVICE,
            )?;
            for choice in &choices {
                limit("bytes in a text", choice.len(), MAX_TEXT_BYTES)?;
            }
            ParamType::Enum(choices)
        }
        ParamTypeFile::SupplyRef => ParamType::SupplyRef,
        ParamTypeFile::DeviceRef => ParamType::DeviceRef,
    };
    if let Some(ParamValue::Text(text)) = &param.default {
        limit("bytes in a text", text.len(), MAX_TEXT_BYTES)?;
    }
    Ok(ParamDefinition {
        key: param.key,
        ty,
        default: param.default,
    })
}

fn to_symbol(symbol: SymbolFile) -> Result<SymbolDefinition, LibraryError> {
    limit("bytes in a text", symbol.key.len(), MAX_TEXT_BYTES)?;
    let kind = match symbol.kind {
        KindFile::Coil => SymbolKind::Coil,
        KindFile::ContactNo => SymbolKind::ContactNo,
        KindFile::ContactNc => SymbolKind::ContactNc,
        KindFile::Changeover => SymbolKind::Changeover,
        KindFile::PowerContacts { poles } => SymbolKind::PowerContacts { poles },
        KindFile::ThermalElement { poles } => SymbolKind::ThermalElement { poles },
        KindFile::FuseLink => SymbolKind::FuseLink,
        KindFile::Motor => SymbolKind::Motor,
        KindFile::MotorSixTerminal => SymbolKind::MotorSixTerminal,
        KindFile::Lamp => SymbolKind::Lamp,
        KindFile::Buzzer => SymbolKind::Buzzer,
        KindFile::SupplyTerminal => SymbolKind::SupplyTerminal,
        KindFile::InterlockLink => SymbolKind::InterlockLink,
        KindFile::TransformerPrimary => SymbolKind::TransformerPrimary,
        KindFile::TransformerSecondary => SymbolKind::TransformerSecondary,
        KindFile::Block2 => SymbolKind::Block2,
        KindFile::Block4 => SymbolKind::Block4,
        KindFile::Block8 => SymbolKind::Block8,
    };
    let definition =
        SymbolDefinition::new(&symbol.key, kind, symbol.min, symbol.max, symbol.slotted);
    Ok(match symbol.artwork {
        None => definition,
        Some(artwork) => definition.with_artwork(Artwork {
            rest: to_primitives(artwork.rest)?,
            operated: artwork.operated.map(to_primitives).transpose()?,
        }),
    })
}

fn to_primitives(list: Vec<PrimitiveFile>) -> Result<Vec<ArtPrimitive>, LibraryError> {
    use circuit_model::library::MAX_ART_PRIMITIVES;
    limit("primitives in an artwork", list.len(), MAX_ART_PRIMITIVES)?;
    let point = |[x, y]: PointFile| ArtPoint { x, y };
    list.into_iter()
        .map(|primitive| {
            Ok(match primitive {
                PrimitiveFile::Line { from, to } => ArtPrimitive::Line {
                    from: point(from),
                    to: point(to),
                },
                PrimitiveFile::Polyline(points) => {
                    limit("points in a polyline", points.len(), MAX_ART_PRIMITIVES)?;
                    ArtPrimitive::Polyline(points.into_iter().map(point).collect())
                }
                PrimitiveFile::Rect { from, to } => ArtPrimitive::Rect {
                    from: point(from),
                    to: point(to),
                },
                PrimitiveFile::Circle { center, radius } => ArtPrimitive::Circle {
                    center: point(center),
                    radius,
                },
                PrimitiveFile::Dot { center, radius } => ArtPrimitive::Dot {
                    center: point(center),
                    radius,
                },
                PrimitiveFile::Text { at, text } => {
                    limit("bytes in a text", text.len(), MAX_TEXT_BYTES)?;
                    ArtPrimitive::Text {
                        at: point(at),
                        text,
                    }
                }
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use circuit_model::{ComponentCatalog, ModelRef};
    use proptest::prelude::*;

    use super::*;

    const LIBRARY_MD: &str = include_str!("../../../LIBRARY.md");

    fn example() -> &'static str {
        LIBRARY_MD
            .split_once("```json\n")
            .and_then(|(_, rest)| rest.split_once("```"))
            .map(|(json, _)| json)
            .unwrap()
    }

    #[test]
    fn the_example_of_library_md_loads_into_the_builtin_catalog() {
        let library = from_str(example()).unwrap();
        let catalog = ComponentCatalog::builtin().with_library(library).unwrap();
        let model = catalog
            .model(&ModelRef {
                id: "acme.relay_r2".to_owned(),
                version: 1,
            })
            .unwrap();

        assert_eq!(model.class(), DeviceClass::Relay);
        assert_eq!(model.provenance().license, "MIT OR Apache-2.0");
        assert!(model.symbols()[1].artwork().is_some());
    }

    #[test]
    fn unknown_fields_floats_repeats_and_other_versions_are_refused() {
        let text = example();
        let unknown = text.replacen("\"version\": 1,", "\"version\": 1, \"extra\": 0,", 1);
        let float = text.replacen("\"min\": 0,", "\"min\": 0.0,", 1);
        let repeated = text.replacen("\"name\":", "\"name\": \"x\", \"name\":", 1);
        let newer = text.replacen("\"schema_version\": 2", "\"schema_version\": 3", 1);
        let older = text.replacen("\"schema_version\": 2", "\"schema_version\": 0", 1);

        for broken in [&unknown, &float, &repeated] {
            assert!(
                matches!(from_str(broken), Err(LibraryError::Schema(_))),
                "{broken}"
            );
        }
        assert_eq!(
            from_str(&newer),
            Err(LibraryError::UnsupportedVersion {
                found: 3,
                supported: 2
            })
        );
        assert_eq!(
            from_str(&older),
            Err(LibraryError::UnsupportedVersion {
                found: 0,
                supported: 2
            })
        );
    }

    const V2_LINE: &str =
        "\"category\": \"automation\", \"title\": \"ACME relay R2\", \"note\": \"Example only\",";

    fn model_named<'a>(
        catalog: &'a ComponentCatalog,
        id: &str,
    ) -> &'a circuit_model::ComponentModel {
        catalog
            .model(&ModelRef {
                id: id.to_owned(),
                version: 1,
            })
            .unwrap()
    }

    #[test]
    fn a_version_2_library_gives_its_models_a_category_a_title_and_a_note() {
        let library = from_str(example()).unwrap();
        let catalog = ComponentCatalog::builtin().with_library(library).unwrap();
        let model = model_named(&catalog, "acme.relay_r2");

        assert_eq!(model.category(), Category::Automation);
        assert_eq!(model.title(), "ACME relay R2");
        assert_eq!(model.note(), Some("Example only"));
    }

    #[test]
    fn a_version_1_library_still_loads_with_the_defaults_of_its_class() {
        let v1 = example()
            .replacen("\"schema_version\": 2", "\"schema_version\": 1", 1)
            .replacen(V2_LINE, "", 1);
        assert!(!v1.contains("category"));

        let library = from_str(&v1).unwrap();
        let catalog = ComponentCatalog::builtin().with_library(library).unwrap();
        let model = model_named(&catalog, "acme.relay_r2");

        assert_eq!(model.category(), Category::default_for(DeviceClass::Relay));
        assert_eq!(model.title(), "acme.relay_r2");
        assert_eq!(model.note(), None);
    }

    #[test]
    fn a_version_1_library_may_not_use_the_version_2_fields() {
        let v1 = example().replacen("\"schema_version\": 2", "\"schema_version\": 1", 1);

        assert!(matches!(from_str(&v1), Err(LibraryError::Schema(_))));
    }

    #[test]
    fn an_unknown_category_and_oversized_texts_are_refused() {
        let unknown = example().replacen("\"automation\"", "\"hydraulics\"", 1);
        let long_title = example().replacen("ACME relay R2", &"t".repeat(300), 1);
        let long_note = example().replacen("Example only", &"n".repeat(600), 1);

        assert!(matches!(from_str(&unknown), Err(LibraryError::Schema(_))));
        for broken in [&long_title, &long_note] {
            assert!(matches!(
                from_str(broken),
                Err(LibraryError::LimitExceeded { .. })
            ));
        }
    }

    #[test]
    fn an_empty_title_is_refused_when_the_library_is_added_to_a_catalog() {
        let empty = example().replacen("ACME relay R2", "", 1);
        let library = from_str(&empty).unwrap();

        assert!(ComponentCatalog::builtin().with_library(library).is_err());
    }

    #[test]
    fn text_beyond_the_size_limit_is_refused_before_parsing() {
        let huge = " ".repeat(MAX_LIBRARY_BYTES + 1);

        assert!(matches!(
            from_str(&huge),
            Err(LibraryError::TooLarge { .. })
        ));
    }

    #[test]
    fn a_long_text_field_is_a_limit_error() {
        let long = example().replacen(
            "\"ACME control relays\"",
            &format!("\"{}\"", "x".repeat(300)),
            1,
        );

        assert!(matches!(
            from_str(&long),
            Err(LibraryError::LimitExceeded { .. })
        ));
    }

    const SYMBOLIC_LIBRARY: &str = r#"{
  "schema_version": 2,
  "id": "blocks",
  "version": 1,
  "name": "Blocks",
  "provenance": { "origin": "test", "license": "MIT OR Apache-2.0", "standard_reference": null },
  "models": [
    {
      "id": "blocks.plc",
      "version": 1,
      "class": "symbolic",
      "operator": null,
      "category": "automation",
      "title": "PLC block",
      "note": "Not simulated",
      "params": [],
      "symbols": [
        { "key": "body", "kind": "block8", "min": 1, "max": 1, "slotted": false, "artwork": null }
      ]
    }
  ]
}"#;

    #[test]
    fn a_symbolic_block_library_loads_and_is_marked_with_its_category_and_note() {
        let library = from_str(SYMBOLIC_LIBRARY).unwrap();
        let catalog = ComponentCatalog::builtin().with_library(library).unwrap();
        let model = model_named(&catalog, "blocks.plc");

        assert_eq!(model.class(), DeviceClass::Symbolic);
        assert_eq!(model.category(), Category::Automation);
        assert_eq!(model.note(), Some("Not simulated"));
        assert_eq!(model.symbols()[0].kind(), SymbolKind::Block8);
    }

    #[test]
    fn a_block_kind_on_a_class_that_runs_is_refused_when_added_to_a_catalog() {
        let wrong = SYMBOLIC_LIBRARY.replacen("\"symbolic\"", "\"lamp\"", 1);
        let library = from_str(&wrong).unwrap();

        assert!(ComponentCatalog::builtin().with_library(library).is_err());
    }

    proptest! {
        #[test]
        fn arbitrary_text_never_panics(text in ".{0,512}") {
            let _ = from_str(&text);
        }

        #[test]
        fn arbitrary_artwork_numbers_never_panic_when_added_to_a_catalog(
            x in any::<i32>(),
            y in any::<i32>(),
            radius in any::<i32>(),
        ) {
            let text = example().replacen(
                "{ \"rect\": { \"from\": [-12, -12], \"to\": [12, 12] } }",
                &format!("{{ \"circle\": {{ \"center\": [{x}, {y}], \"radius\": {radius} }} }}"),
                1,
            );
            if let Ok(library) = from_str(&text) {
                let _ = ComponentCatalog::builtin().with_library(library);
            }
        }
    }
}
