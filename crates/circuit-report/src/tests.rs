use circuit_model::builder::pin;
use circuit_model::{
    ComponentCatalog, Device, DeviceInfo, Diagnostic, DiagnosticCode, GridPoint, Schematic,
    SchematicBuilder, Subject, WireId, WireNumbering,
};

use super::*;

/// S1 feeds the coil of K1; K1 has an auxiliary contact; K2 is a coil with
/// no contact; K1 and K2 are the same part.
fn project() -> Schematic {
    let mut b = SchematicBuilder::new("report");
    let s1 = b.device("S1", "pushbutton_no", &[]).unwrap();
    let k1 = b.device("K1", "contactor.generic", &[]).unwrap();
    let k2 = b.device("K2", "contactor.generic", &[]).unwrap();
    let s1_contact = b.symbol(s1, "contact", None, GridPoint::new(0, 0)).unwrap();
    let k1_coil = b.symbol(k1, "coil", None, GridPoint::new(10, 0)).unwrap();
    b.symbol(k1, "aux_no", Some(1), GridPoint::new(30, 0))
        .unwrap();
    let k2_coil = b.symbol(k2, "coil", None, GridPoint::new(20, 0)).unwrap();
    b.wire(pin(s1_contact, "p2"), pin(k1_coil, "a1")).unwrap();
    b.wire(pin(k1_coil, "a2"), pin(k2_coil, "a1")).unwrap();
    let mut doc = b.finish();
    let part = DeviceInfo {
        manufacturer: Some("Acme".to_owned()),
        part_no: Some("C-9".to_owned()),
        ..DeviceInfo::default()
    };
    for device in &mut doc.devices {
        if device.reference.0.starts_with('K') {
            device.info = Some(part.clone());
        }
    }
    doc.project.wire_numbering = Some(WireNumbering {
        prefix: "W".to_owned(),
        start: 1,
        step: std::num::NonZeroU32::MIN,
        per_sheet: false,
    });
    doc
}

fn catalog() -> ComponentCatalog {
    ComponentCatalog::builtin()
}

#[test]
fn the_names_of_the_reports_round_trip() {
    for kind in Kind::ALL {
        assert_eq!(Kind::parse(kind.name()), Some(kind));
    }
    assert_eq!(Kind::parse("nope"), None);
}

#[test]
fn designators_sort_by_their_number_not_their_text() {
    let mut references = ["K10", "K2", "S1", "K1", "KT1", "K"];
    references.sort_by_key(|reference| natural(reference));

    assert_eq!(references, ["K", "K1", "K2", "K10", "KT1", "S1"]);
}

#[test]
fn the_bill_of_materials_groups_equal_parts_and_lists_their_references() {
    let table = bom(&project(), &catalog());

    assert_eq!(
        table.columns,
        [
            "Item",
            "Qty",
            "References",
            "Model",
            "Manufacturer",
            "Part no",
            "Description"
        ]
    );
    assert_eq!(
        table.rows,
        [
            ["1", "2", "K1 K2", "contactor.generic v1", "Acme", "C-9", ""],
            ["2", "1", "S1", "pushbutton_no v1", "", "", ""],
        ]
    );
}

#[test]
fn a_supply_tap_is_not_a_part() {
    let mut b = SchematicBuilder::new("taps");
    let supply = b.dc_supply(24_000).unwrap();
    let tap = b
        .device(
            "P24",
            "supply.terminal",
            &[
                (
                    "conductor",
                    circuit_model::ParamValue::Text("dc_pos".to_owned()),
                ),
                (
                    "supply",
                    circuit_model::ParamValue::Int(i64::try_from(supply.0).unwrap()),
                ),
            ],
        )
        .unwrap();
    b.symbol(tap, "terminal", None, GridPoint::new(0, 0))
        .unwrap();
    let s1 = b.device("S1", "pushbutton_no", &[]).unwrap();
    b.symbol(s1, "contact", None, GridPoint::new(0, 10))
        .unwrap();

    let table = bom(&b.finish(), &catalog());

    assert_eq!(table.rows.len(), 1);
    assert_eq!(table.rows[0][2], "S1");
}

#[test]
fn devices_with_different_catalog_data_are_different_parts() {
    let mut doc = project();
    if let Some(device) = doc.devices.iter_mut().find(|d| d.reference.0 == "K2") {
        device.info = None;
    }

    let table = bom(&doc, &catalog());

    assert_eq!(table.rows.len(), 3);
}

#[test]
fn the_wires_report_lists_each_net_with_its_number_and_ends() {
    let table = wires(&project(), &catalog());

    assert_eq!(table.columns, ["Number", "Sheet", "From", "To", "Also"]);
    // S1 and K1 share a row of terminals: the net of S1:14 and K1:A1 starts
    // at the upper terminal of K1's coil, and the other net starts at A2.
    assert_eq!(
        table.rows,
        [
            ["W1", "", "K1:A1", "S1:14", ""],
            ["W2", "", "K1:A2", "K2:A1", ""],
        ]
    );
}

#[test]
fn a_net_of_three_terminals_lists_the_third_under_also() {
    let mut doc = project();
    let k2_coil = doc
        .symbols
        .iter()
        .find(|s| s.symbol_key.0 == "coil" && s.position.x == 20)
        .map(|s| s.id)
        .unwrap();
    let k1_aux = doc
        .symbols
        .iter()
        .find(|s| s.symbol_key.0 == "aux_no")
        .map(|s| s.id)
        .unwrap();
    doc.wires.push(circuit_model::Wire {
        id: WireId(doc.next_id),
        from: circuit_model::Endpoint::Terminal(pin_ref(k2_coil, "a1")),
        to: circuit_model::Endpoint::Terminal(pin_ref(k1_aux, "p1")),
        route: Vec::new(),
        number: None,
        layer: None,
        spec: None,
    });
    doc.next_id += 1;

    let table = wires(&doc, &catalog());

    assert_eq!(table.rows[1], ["W2", "", "K1:A2", "K1:13", "K2:A1"]);
}

fn pin_ref(symbol: circuit_model::SymbolId, key: &str) -> PinRef {
    PinRef {
        symbol_id: symbol,
        terminal: circuit_model::TerminalKey(key.to_owned()),
    }
}

#[test]
fn the_terminal_plan_has_a_row_for_every_terminal_with_its_wire() {
    let table = terminals(&project(), &catalog());

    let row = |reference: &str, key: &str| {
        table
            .rows
            .iter()
            .find(|row| {
                row.first().map(String::as_str) == Some(reference)
                    && row.get(3).map(String::as_str) == Some(key)
            })
            .cloned()
    };
    assert_eq!(
        row("K1", "a1"),
        Some(
            ["K1", "coil", "A1", "a1", "W1", "S1:14"]
                .map(str::to_owned)
                .to_vec()
        )
    );
    // The auxiliary contact is not wired: no wire, nothing connected.
    assert_eq!(
        row("K1", "p1"),
        Some(
            ["K1", "aux_no", "13", "p1", "", ""]
                .map(str::to_owned)
                .to_vec()
        )
    );
    // Two terminals for each of S1, K1's coil, K1's contact and K2.
    assert_eq!(table.rows.len(), 8);
}

#[test]
fn the_cross_reference_report_lists_a_coil_without_contacts() {
    let table = xref(&project(), &catalog());

    assert_eq!(table.rows, [["K2", "coil", "Coil without contacts"]]);
}

#[test]
fn contacts_without_their_coil_and_symbols_off_the_sheets_are_exceptions() {
    let mut doc = project();
    let coil = doc
        .symbols
        .iter()
        .position(|s| s.symbol_key.0 == "coil" && s.position.x == 10)
        .unwrap();
    // Take the coil of K1 away from its wires and from the drawing.
    doc.wires.retain(|wire| {
        !matches!(&wire.from, circuit_model::Endpoint::Terminal(p) if p.symbol_id == doc.symbols[coil].id)
            && !matches!(&wire.to, circuit_model::Endpoint::Terminal(p) if p.symbol_id == doc.symbols[coil].id)
    });
    doc.symbols.remove(coil);

    let table = xref(&doc, &catalog());

    assert!(
        table
            .rows
            .iter()
            .any(|row| row == &["K1", "aux_no", "Contacts without a coil"])
    );
}

#[test]
fn with_sheets_every_symbol_outside_them_is_an_exception() {
    let mut doc = project();
    doc.sheets.push(circuit_model::Sheet {
        id: circuit_model::SheetId(doc.next_id),
        number: 1,
        title: "One".to_owned(),
        origin: GridPoint::new(-5, -5),
        size: circuit_model::SheetSize::Custom {
            width: 30,
            height: 20,
        },
        orientation: circuit_model::Orientation::Landscape,
        zones: circuit_model::Zones {
            columns: 4,
            rows: 2,
        },
        kind: Default::default(),
    });
    doc.next_id += 1;

    let table = xref(&doc, &catalog());

    // The auxiliary contact of K1 is at x = 30, beyond the sheet.
    assert!(
        table
            .rows
            .iter()
            .any(|row| row == &["K1", "aux_no", "Outside the sheets"])
    );
}

#[test]
fn the_io_report_is_the_modbus_map() {
    let table = io(&project(), &catalog());

    assert_eq!(
        table.columns,
        ["Table", "Address", "Words", "Name", "Meaning"]
    );
    assert_eq!(
        table.rows.first().cloned(),
        Some(
            ["coil", "0", "1", "S1", "1 = pressed"]
                .map(str::to_owned)
                .to_vec()
        )
    );
    assert!(
        table
            .rows
            .iter()
            .any(|row| row[3] == "tick" && row[2] == "4")
    );
}

#[test]
fn the_problems_report_has_a_row_for_each_diagnostic_and_marks_the_waived_ones() {
    let finding = |code, wire| {
        Diagnostic::new(
            code,
            Subject::Wire(WireId(wire)),
            "a junction with one wire",
        )
        .with_related(vec![Subject::Wire(WireId(wire + 1))])
    };
    let diagnostics = [
        finding(DiagnosticCode::JunctionDangling, 3),
        finding(DiagnosticCode::LooseConnection, 7),
    ];
    let mut doc = project();
    doc.waivers.push(circuit_model::Waiver {
        code: "OC-STR-018".to_owned(),
        subject: Subject::Wire(WireId(7)),
        reason: "test point".to_owned(),
    });

    let table = problems(&doc, &diagnostics);

    assert_eq!(
        table.rows,
        [
            [
                "warning",
                "OC-STR-008",
                "wire 3",
                "a junction with one wire",
                "wire 4",
                "",
                ""
            ],
            [
                "warning",
                "OC-STR-018",
                "wire 7",
                "a junction with one wire",
                "wire 8",
                "yes",
                "test point"
            ],
        ]
    );
}

#[test]
fn no_report_depends_on_the_order_of_the_collections() {
    let doc = project();
    let mut shuffled = doc.clone();
    shuffled.devices.reverse();
    shuffled.symbols.reverse();
    shuffled.wires.reverse();
    let catalog = catalog();

    assert_eq!(bom(&doc, &catalog), bom(&shuffled, &catalog));
    assert_eq!(wires(&doc, &catalog), wires(&shuffled, &catalog));
    assert_eq!(terminals(&doc, &catalog), terminals(&shuffled, &catalog));
    assert_eq!(xref(&doc, &catalog), xref(&shuffled, &catalog));
    assert_eq!(io(&doc, &catalog), io(&shuffled, &catalog));
}

#[test]
fn a_description_that_looks_like_a_formula_is_neutralised_in_csv() {
    let mut doc = project();
    let device: &mut Device = &mut doc.devices[0];
    device.info = Some(DeviceInfo {
        description: Some("=HYPERLINK(\"x\")".to_owned()),
        ..DeviceInfo::default()
    });

    let csv = bom(&doc, &catalog()).to_csv();

    assert!(csv.contains("\"'=HYPERLINK(\"\"x\"\")\""), "{csv}");
}

#[test]
fn devices_are_one_row_each_in_natural_order_without_supply_taps() {
    let mut b = SchematicBuilder::new("devices");
    let supply = b.dc_supply(24_000).unwrap();
    let _ = supply;
    let k10 = b.device("K10", "contactor.generic", &[]).unwrap();
    let k2 = b.device("K2", "contactor.generic", &[]).unwrap();
    b.symbol(k10, "coil", None, GridPoint::new(0, 0)).unwrap();
    b.symbol(k10, "aux_no", Some(1), GridPoint::new(10, 0))
        .unwrap();
    b.symbol(k2, "coil", None, GridPoint::new(20, 0)).unwrap();
    let mut doc = b.finish();
    if let Some(device) = doc.devices.iter_mut().find(|d| d.reference.0 == "K2") {
        device.info = Some(DeviceInfo {
            manufacturer: Some("Acme".to_owned()),
            ..DeviceInfo::default()
        });
    }

    let table = devices(&doc, &ComponentCatalog::builtin());

    assert_eq!(table.columns[0], "Reference");
    let rows: Vec<(&str, &str, &str)> = table
        .rows
        .iter()
        .map(|row| (row[0].as_str(), row[2].as_str(), row[4].as_str()))
        .collect();
    assert_eq!(rows, [("K2", "1", "Acme"), ("K10", "2", "")]);
}

#[test]
fn devices_lists_the_sheets_their_symbols_are_on() {
    let doc = project();
    let table = devices(&doc, &ComponentCatalog::builtin());

    assert!(table.rows.iter().all(|row| row[3].is_empty()), "no sheets");
    assert_eq!(table.rows.len(), 3);
    assert!(Kind::parse("devices") == Some(Kind::Devices));
    assert_eq!(Kind::ALL.last(), Some(&Kind::Devices));
}
