use std::collections::BTreeMap;

use circuit_model::builder::pin;
use circuit_model::{
    ComponentCatalog, Device, DeviceId, Endpoint, GridPoint, Junction, JunctionId, ModelRef,
    ParamValue, PhaseSequence, PinRef, ProjectMeta, QuarterTurn, ReferenceDesignator,
    SchematicBuilder, SupplyId, SupplyKind, SupplyProfile, SymbolId, SymbolInstance, SymbolKey,
    TerminalKey, Wire, WireId,
};
use proptest::prelude::*;

use super::*;

/// The canonical text of [`sample`]. It fixes field order, indentation,
/// explicit nulls, line endings and the final newline.
const SAMPLE: &str = r#"{
  "schema_version": 2,
  "next_id": 6,
  "project": {
    "name": "demo",
    "grid_unit": 10,
    "author": null,
    "company": null,
    "revision": null,
    "date": null
  },
  "sheets": [],
  "supplies": [
    {
      "id": 1,
      "kind": "dc",
      "nominal_voltage_mv": 24000,
      "phase_voltage_mv": null,
      "frequency_millihz": null,
      "phase_sequence": null
    }
  ],
  "devices": [
    {
      "id": 2,
      "reference": "P24",
      "model": {
        "id": "supply.terminal",
        "version": 1
      },
      "params": {
        "conductor": "dc_pos",
        "supply": 1
      }
    }
  ],
  "symbols": [
    {
      "id": 3,
      "device_id": 2,
      "symbol_key": "terminal",
      "slot": null,
      "position": {
        "x": 0,
        "y": -4
      },
      "rotation": "deg0"
    }
  ],
  "junctions": [
    {
      "id": 4,
      "position": {
        "x": 2,
        "y": 0
      }
    }
  ],
  "wires": [
    {
      "id": 5,
      "from": {
        "terminal": {
          "symbol_id": 3,
          "terminal": "out"
        }
      },
      "to": {
        "junction": 4
      },
      "route": [
        {
          "x": 1,
          "y": 0
        }
      ]
    }
  ]
}
"#;

/// [`SAMPLE`] as version 1 wrote it, before sheets and the title block: it
/// loads through the migration to the same document.
const SAMPLE_V1: &str = r#"{
  "schema_version": 1,
  "next_id": 6,
  "project": {
    "name": "demo",
    "grid_unit": 10
  },
  "supplies": [
    {
      "id": 1,
      "kind": "dc",
      "nominal_voltage_mv": 24000,
      "phase_voltage_mv": null,
      "frequency_millihz": null,
      "phase_sequence": null
    }
  ],
  "devices": [
    {
      "id": 2,
      "reference": "P24",
      "model": {
        "id": "supply.terminal",
        "version": 1
      },
      "params": {
        "conductor": "dc_pos",
        "supply": 1
      }
    }
  ],
  "symbols": [
    {
      "id": 3,
      "device_id": 2,
      "symbol_key": "terminal",
      "slot": null,
      "position": {
        "x": 0,
        "y": -4
      },
      "rotation": "deg0"
    }
  ],
  "junctions": [
    {
      "id": 4,
      "position": {
        "x": 2,
        "y": 0
      }
    }
  ],
  "wires": [
    {
      "id": 5,
      "from": {
        "terminal": {
          "symbol_id": 3,
          "terminal": "out"
        }
      },
      "to": {
        "junction": 4
      },
      "route": [
        {
          "x": 1,
          "y": 0
        }
      ]
    }
  ]
}
"#;

fn sample() -> Schematic {
    let mut builder = SchematicBuilder::new("demo");
    let supply = builder.dc_supply(24_000).unwrap();
    let device = builder
        .device(
            "P24",
            "supply.terminal",
            &[
                ("conductor", ParamValue::Text("dc_pos".to_owned())),
                ("supply", ParamValue::Int(i64::try_from(supply.0).unwrap())),
            ],
        )
        .unwrap();
    let tap = builder
        .symbol(device, "terminal", None, GridPoint::new(0, -4))
        .unwrap();
    let junction = builder.junction(GridPoint::new(2, 0)).unwrap();
    builder.wire(pin(tap, "out"), junction).unwrap();
    let mut doc = builder.finish();
    doc.wires[0].route.push(GridPoint::new(1, 0));
    doc
}

/// `SAMPLE` with one replacement applied, for malformed-input tests.
fn sample_with(from: &str, to: &str) -> String {
    assert!(SAMPLE.contains(from), "sample has no `{from}`");
    SAMPLE.replacen(from, to, 1)
}

fn schema_message(result: Result<Schematic, ProjectError>) -> String {
    match result {
        Err(ProjectError::Schema {
            version: 2,
            message,
        }) => message,
        other => panic!("expected a schema error, got {other:?}"),
    }
}

#[test]
fn a_version_1_file_migrates_to_the_same_document_and_saves_as_version_2() {
    let loaded = from_str(SAMPLE_V1).unwrap();

    assert_eq!(loaded, sample());
    assert_eq!(to_string(&loaded).unwrap(), SAMPLE);
}

#[test]
fn a_version_1_file_that_already_has_version_2_fields_is_refused() {
    let with_sheets = SAMPLE_V1.replacen("\"supplies\"", "\"sheets\": [],\n  \"supplies\"", 1);
    assert!(matches!(
        from_str(&with_sheets),
        Err(ProjectError::Migration { from: 1, .. })
    ));
    let with_author = SAMPLE_V1.replacen(
        "\"grid_unit\": 10",
        "\"grid_unit\": 10, \"author\": \"x\"",
        1,
    );
    assert!(matches!(
        from_str(&with_author),
        Err(ProjectError::Migration { from: 1, .. })
    ));
}

#[test]
fn document_is_written_in_canonical_form() {
    assert_eq!(to_string(&sample()).unwrap(), SAMPLE);
}

/// The normative format document, read at compile time so that a drifting
/// example or catalog table fails a test.
const FORMAT_MD: &str = include_str!("../../../FORMAT.md");

#[test]
fn format_document_example_is_the_canonical_sample() {
    let block = FORMAT_MD
        .split_once("```json\n")
        .and_then(|(_, rest)| rest.split_once("```"))
        .map(|(block, _)| block);

    assert_eq!(block, Some(SAMPLE));
}

#[test]
fn format_document_catalog_table_matches_the_builtin_catalog() {
    let section = FORMAT_MD
        .split_once("## Catalog")
        .and_then(|(_, rest)| rest.split_once("\n## "))
        .map(|(section, _)| section)
        .expect("FORMAT.md has a catalog section");

    for model in ComponentCatalog::builtin().models() {
        let id = format!("`{}`", model.id());
        let version = format!("| {} |", model.version());
        let row = section
            .lines()
            .find(|line| line.starts_with('|') && line.contains(&id) && line.contains(&version))
            .unwrap_or_else(|| {
                panic!(
                    "FORMAT.md does not list {} version {}",
                    model.id(),
                    model.version()
                )
            });
        let symbols = model.symbols().iter().map(|symbol| symbol.key().0.as_str());
        let params = model.params().iter().map(|param| param.key.as_str());
        for key in symbols.chain(params) {
            assert!(
                row.contains(&format!("`{key}`")),
                "FORMAT.md row of {} does not mention `{key}`",
                model.id()
            );
        }
    }
}

#[test]
fn canonical_text_loads_into_the_same_document() {
    assert_eq!(from_str(SAMPLE), Ok(sample()));
}

#[test]
fn saving_a_loaded_canonical_file_reproduces_it_byte_for_byte() {
    let loaded = from_str(SAMPLE).unwrap();

    assert_eq!(to_string(&loaded).unwrap(), SAMPLE);
}

#[test]
fn canonical_text_uses_line_feeds_only() {
    assert!(!to_string(&sample()).unwrap().contains('\r'));
}

#[test]
fn collections_are_written_sorted_by_id_whatever_their_order_in_memory() {
    let mut builder = SchematicBuilder::new("demo");
    for _ in 0..3 {
        builder.junction(GridPoint::default()).unwrap();
    }
    let sorted = builder.finish();
    let mut shuffled = sorted.clone();
    shuffled.junctions.rotate_left(1);

    assert_eq!(to_string(&shuffled), to_string(&sorted));
}

#[test]
fn file_that_is_not_canonical_still_loads_and_is_canonical_when_saved() {
    let compact: String = SAMPLE.split_whitespace().collect();

    let loaded = from_str(&compact).unwrap();

    assert_eq!(loaded, sample());
    assert_eq!(to_string(&loaded).unwrap(), SAMPLE);
}

#[test]
fn absent_optional_field_reads_as_null() {
    let without_slot = sample_with("      \"slot\": null,\n", "");

    assert_eq!(from_str(&without_slot), Ok(sample()));
}

#[test]
fn unknown_top_level_field_is_rejected() {
    let text = sample_with("\"next_id\": 6,", "\"next_id\": 6, \"author\": \"me\",");

    assert!(schema_message(from_str(&text)).contains("unknown field `author`"));
}

#[test]
fn unknown_nested_field_is_rejected() {
    let text = sample_with("\"x\": 2,", "\"x\": 2, \"z\": 0,");

    assert!(schema_message(from_str(&text)).contains("unknown field `z`"));
}

#[test]
fn repeated_field_is_rejected() {
    let text = sample_with("\"next_id\": 6,", "\"next_id\": 6, \"next_id\": 7,");

    assert!(schema_message(from_str(&text)).contains("duplicate field `next_id`"));
}

#[test]
fn repeated_parameter_is_rejected() {
    let text = sample_with("\"supply\": 1", "\"supply\": 1, \"supply\": 2");

    assert!(schema_message(from_str(&text)).contains("duplicate parameter `supply`"));
}

#[test]
fn unknown_enumeration_value_is_rejected() {
    let text = sample_with("\"deg0\"", "\"deg45\"");

    assert!(schema_message(from_str(&text)).contains("unknown variant `deg45`"));
}

#[test]
fn floating_point_number_is_rejected() {
    for (from, to) in [
        (
            "\"nominal_voltage_mv\": 24000",
            "\"nominal_voltage_mv\": 24000.0",
        ),
        ("\"x\": 1", "\"x\": 1.5"),
        ("\"supply\": 1", "\"supply\": 1.0"),
        ("\"id\": 4", "\"id\": 4e0"),
    ] {
        let result = from_str(&sample_with(from, to));

        assert!(
            matches!(result, Err(ProjectError::Schema { .. })),
            "{to} should be rejected, got {result:?}"
        );
    }
}

#[test]
fn negative_or_oversized_id_is_rejected() {
    for id in ["-1", "18446744073709551616"] {
        let result = from_str(&sample_with("\"id\": 4", &format!("\"id\": {id}")));

        assert!(
            matches!(result, Err(ProjectError::Schema { .. })),
            "{id} should be rejected, got {result:?}"
        );
    }
}

#[test]
fn newer_schema_is_refused_without_reading_the_rest() {
    let text = r#"{ "schema_version": 3, "something": { "new": true } }"#;

    assert_eq!(
        from_str(text),
        Err(ProjectError::NewerSchema {
            found: 3,
            supported: 2
        })
    );
}

#[test]
fn schema_version_zero_has_no_migration() {
    assert_eq!(
        from_str(r#"{ "schema_version": 0 }"#),
        Err(ProjectError::NoMigration { found: 0 })
    );
}

#[test]
fn missing_or_non_integer_schema_version_is_refused() {
    for text in [
        "{}",
        r#"{ "schema_version": "1" }"#,
        r#"{ "schema_version": 1.0 }"#,
        "[]",
        "null",
    ] {
        assert_eq!(
            from_str(text),
            Err(ProjectError::MissingSchemaVersion),
            "{text}"
        );
    }
}

#[test]
fn text_that_is_not_json_is_a_syntax_error() {
    for text in ["", "{", "schema_version: 1", "\u{0}"] {
        assert!(
            matches!(from_str(text), Err(ProjectError::Syntax(_))),
            "{text:?}"
        );
    }
}

#[test]
fn deeply_nested_json_is_rejected_without_overflowing_the_stack() {
    let text = "[".repeat(200_000);

    assert!(matches!(from_str(&text), Err(ProjectError::Syntax(_))));
}

#[test]
fn oversized_text_is_refused_before_parsing() {
    let text = " ".repeat(MAX_PROJECT_BYTES + 1);

    assert_eq!(
        from_str(&text),
        Err(ProjectError::TooLarge {
            size: MAX_PROJECT_BYTES + 1,
            limit: MAX_PROJECT_BYTES
        })
    );
}

#[test]
fn wire_with_too_many_route_points_is_refused() {
    let mut doc = sample();
    doc.wires[0].route = vec![GridPoint::default(); MAX_ROUTE_POINTS + 1];
    let text = serde_json::to_string(&doc).unwrap();
    let expected = Err(ProjectError::LimitExceeded {
        what: "wire route points",
        limit: MAX_ROUTE_POINTS,
    });

    assert_eq!(from_str(&text), expected);
    assert_eq!(to_string(&doc), expected.map(|_: Schematic| String::new()));
}

#[test]
fn overlong_text_is_refused() {
    let mut doc = sample();
    doc.devices[0].reference = ReferenceDesignator("K".repeat(MAX_TEXT_BYTES + 1));
    let text = serde_json::to_string(&doc).unwrap();

    assert_eq!(
        from_str(&text),
        Err(ProjectError::LimitExceeded {
            what: "reference designator",
            limit: MAX_TEXT_BYTES
        })
    );
}

#[test]
fn device_with_too_many_parameters_is_refused() {
    let mut doc = sample();
    for index in 0..=MAX_PARAMS_PER_DEVICE {
        doc.devices[0]
            .params
            .insert(format!("p{index}"), ParamValue::Bool(true));
    }
    let text = serde_json::to_string(&doc).unwrap();

    assert_eq!(
        from_str(&text),
        Err(ProjectError::LimitExceeded {
            what: "device parameters",
            limit: MAX_PARAMS_PER_DEVICE
        })
    );
}

#[test]
fn structurally_wrong_document_still_loads_so_it_can_be_diagnosed() {
    let dangling = sample_with("\"junction\": 4", "\"junction\": 99");

    let loaded = from_str(&dangling).unwrap();

    assert!(!loaded.validate().is_empty());
}

fn text() -> impl Strategy<Value = String> {
    ".{0,12}"
}

fn point() -> impl Strategy<Value = GridPoint> {
    (any::<i32>(), any::<i32>()).prop_map(|(x, y)| GridPoint { x, y })
}

fn param() -> impl Strategy<Value = ParamValue> {
    prop_oneof![
        any::<bool>().prop_map(ParamValue::Bool),
        any::<i64>().prop_map(ParamValue::Int),
        text().prop_map(ParamValue::Text),
    ]
}

fn endpoint() -> impl Strategy<Value = Endpoint> {
    prop_oneof![
        (any::<u64>(), text()).prop_map(|(symbol, terminal)| Endpoint::Terminal(PinRef {
            symbol_id: SymbolId(symbol),
            terminal: TerminalKey(terminal),
        })),
        any::<u64>().prop_map(|junction| Endpoint::Junction(JunctionId(junction))),
    ]
}

fn supply() -> impl Strategy<Value = SupplyProfile> {
    (
        any::<u64>(),
        prop_oneof![
            Just(SupplyKind::Dc),
            Just(SupplyKind::SinglePhase),
            Just(SupplyKind::ThreePhase)
        ],
        any::<u32>(),
        any::<Option<u32>>(),
        any::<Option<u32>>(),
        prop_oneof![
            Just(None),
            Just(Some(PhaseSequence::L1L2L3)),
            Just(Some(PhaseSequence::L1L3L2))
        ],
    )
        .prop_map(
            |(id, kind, nominal, phase, frequency, sequence)| SupplyProfile {
                id: SupplyId(id),
                kind,
                nominal_voltage_mv: nominal,
                phase_voltage_mv: phase,
                frequency_millihz: frequency,
                phase_sequence: sequence,
            },
        )
}

fn device() -> impl Strategy<Value = Device> {
    (
        any::<u64>(),
        text(),
        text(),
        any::<u32>(),
        prop::collection::btree_map(text(), param(), 0..4),
    )
        .prop_map(|(id, reference, model, version, params)| Device {
            id: DeviceId(id),
            reference: ReferenceDesignator(reference),
            model: ModelRef { id: model, version },
            params: params.into_iter().collect::<BTreeMap<_, _>>(),
        })
}

fn symbol() -> impl Strategy<Value = SymbolInstance> {
    (
        any::<u64>(),
        any::<u64>(),
        text(),
        any::<Option<u8>>(),
        point(),
        prop_oneof![
            Just(QuarterTurn::Deg0),
            Just(QuarterTurn::Deg90),
            Just(QuarterTurn::Deg180),
            Just(QuarterTurn::Deg270)
        ],
    )
        .prop_map(
            |(id, device, key, slot, position, rotation)| SymbolInstance {
                id: SymbolId(id),
                device_id: DeviceId(device),
                symbol_key: SymbolKey(key),
                slot,
                position,
                rotation,
            },
        )
}

fn wire() -> impl Strategy<Value = Wire> {
    (
        any::<u64>(),
        endpoint(),
        endpoint(),
        prop::collection::vec(point(), 0..3),
    )
        .prop_map(|(id, from, to, route)| Wire {
            id: WireId(id),
            from,
            to,
            route,
        })
}

fn schematic() -> impl Strategy<Value = Schematic> {
    (
        (any::<u64>(), text(), any::<u32>()),
        prop::collection::vec(supply(), 0..3),
        prop::collection::vec(device(), 0..4),
        prop::collection::vec(symbol(), 0..4),
        prop::collection::vec(
            (any::<u64>(), point()).prop_map(|(id, position)| Junction {
                id: JunctionId(id),
                position,
            }),
            0..3,
        ),
        prop::collection::vec(wire(), 0..4),
        prop::collection::vec(sheet(), 0..3),
        prop::option::of(".{0,8}"),
    )
        .prop_map(
            |(
                (next_id, name, grid_unit),
                supplies,
                devices,
                symbols,
                junctions,
                wires,
                sheets,
                author,
            )| {
                Schematic {
                    schema_version: CURRENT_SCHEMA_VERSION,
                    next_id,
                    project: ProjectMeta {
                        name,
                        grid_unit,
                        author: author.clone(),
                        company: author.clone(),
                        revision: author.clone(),
                        date: author,
                    },
                    sheets,
                    supplies,
                    devices,
                    symbols,
                    junctions,
                    wires,
                }
            },
        )
}

fn sheet() -> impl Strategy<Value = circuit_model::Sheet> {
    (
        any::<u64>(),
        any::<u32>(),
        ".{0,8}",
        point(),
        prop_oneof![
            Just(circuit_model::SheetSize::A4),
            Just(circuit_model::SheetSize::A3),
            Just(circuit_model::SheetSize::A2),
            (any::<u32>(), any::<u32>())
                .prop_map(|(width, height)| circuit_model::SheetSize::Custom { width, height }),
        ],
        any::<bool>(),
        any::<u8>(),
        any::<u8>(),
    )
        .prop_map(
            |(id, number, title, origin, size, portrait, columns, rows)| circuit_model::Sheet {
                id: circuit_model::SheetId(id),
                number,
                title,
                origin,
                size,
                orientation: if portrait {
                    circuit_model::Orientation::Portrait
                } else {
                    circuit_model::Orientation::Landscape
                },
                zones: circuit_model::Zones { columns, rows },
            },
        )
}

proptest! {
    #[test]
    fn any_document_survives_save_and_load(doc in schematic()) {
        let text = to_string(&doc).unwrap();
        let mut canonical = doc;
        canonical.sort_canonical();

        prop_assert_eq!(from_str(&text), Ok(canonical));
    }

    #[test]
    fn saving_what_was_loaded_reproduces_the_bytes(doc in schematic()) {
        let text = to_string(&doc).unwrap();
        let loaded = from_str(&text).unwrap();

        prop_assert_eq!(to_string(&loaded).unwrap(), text);
    }

    #[test]
    fn arbitrary_text_never_panics_the_loader(text in ".{0,200}") {
        let _ = from_str(&text);
    }
}

/// Keys of the schema, so that random JSON reaches past the first checks.
const SCHEMA_KEYS: &[&str] = &[
    "schema_version",
    "next_id",
    "project",
    "name",
    "grid_unit",
    "supplies",
    "devices",
    "symbols",
    "junctions",
    "wires",
    "id",
    "kind",
    "nominal_voltage_mv",
    "phase_voltage_mv",
    "frequency_millihz",
    "phase_sequence",
    "reference",
    "model",
    "version",
    "params",
    "device_id",
    "symbol_key",
    "slot",
    "position",
    "x",
    "y",
    "rotation",
    "from",
    "to",
    "route",
    "terminal",
    "junction",
    "symbol_id",
];

/// Random JSON whose objects mostly use the keys of the schema.
fn hostile_json() -> impl Strategy<Value = serde_json::Value> {
    use serde_json::Value;
    let leaf = prop_oneof![
        Just(Value::Null),
        any::<bool>().prop_map(Value::Bool),
        any::<i64>().prop_map(Value::from),
        any::<u64>().prop_map(Value::from),
        (-1.0e6_f64..1.0e6).prop_map(Value::from),
        prop::sample::select(vec![
            "dc",
            "three_phase",
            "deg90",
            "l1l2l3",
            "K1",
            "",
            "pushbutton_no"
        ])
        .prop_map(|s| Value::from(s.to_owned())),
    ];
    leaf.prop_recursive(5, 96, 8, |inner| {
        prop_oneof![
            prop::collection::vec(inner.clone(), 0..6).prop_map(serde_json::Value::Array),
            prop::collection::vec((prop::sample::select(SCHEMA_KEYS.to_vec()), inner), 0..8)
                .prop_map(|pairs| {
                    serde_json::Value::Object(
                        pairs
                            .into_iter()
                            .map(|(key, value)| (key.to_owned(), value))
                            .collect(),
                    )
                }),
        ]
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn arbitrary_json_never_panics_the_loader(value in hostile_json()) {
        let _ = from_str(&value.to_string());
    }

    #[test]
    fn arbitrary_json_with_the_current_schema_version_never_panics(
        value in hostile_json(),
        next_id in any::<u64>(),
    ) {
        let mut object = match value {
            serde_json::Value::Object(map) => map,
            other => {
                let mut map = serde_json::Map::new();
                map.insert("devices".to_owned(), other);
                map
            }
        };
        object.insert("schema_version".to_owned(), serde_json::Value::from(1));
        object.insert("next_id".to_owned(), serde_json::Value::from(next_id));
        let _ = from_str(&serde_json::Value::Object(object).to_string());
    }

    #[test]
    fn corrupted_canonical_file_never_panics_the_loader(
        flips in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 1..8),
        cut in any::<prop::sample::Index>(),
    ) {
        let mut bytes = canonical_sample().into_bytes();
        for (at, byte) in flips {
            let position = at.index(bytes.len());
            if let Some(slot) = bytes.get_mut(position) {
                *slot = byte;
            }
        }
        bytes.truncate(cut.index(bytes.len().saturating_add(1)));
        let _ = from_str(&String::from_utf8_lossy(&bytes));
    }
}

/// The canonical text of a reference document.
fn canonical_sample() -> String {
    to_string(&from_str(SAMPLE).unwrap()).unwrap()
}

#[test]
fn deeply_nested_json_is_refused_without_overflowing_the_stack() {
    let depth = 100_000;
    let text = format!("{}{}", "[".repeat(depth), "]".repeat(depth));

    assert!(from_str(&text).is_err());
}
