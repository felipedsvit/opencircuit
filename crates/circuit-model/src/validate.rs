//! Structural validation that needs no component catalog.
//!
//! Every check accumulates; none stops at the first problem. Subjects are
//! chosen canonically (smallest first) so the result is identical for any
//! ordering of the document collections.

use std::collections::{BTreeMap, BTreeSet};

use crate::diagnostics::{Diagnostic, DiagnosticCode, Subject, sort_diagnostics};
use crate::ids::{DeviceId, JunctionId, MAX_ENTITY_ID, SymbolId};
use crate::schematic::{CURRENT_SCHEMA_VERSION, Endpoint, Schematic};

pub(crate) fn validate(doc: &Schematic) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    check_schema_version(doc, &mut out);
    check_id_ranges(doc, &mut out);
    check_unique_ids(doc, &mut out);
    check_references(doc, &mut out);
    check_junction_degrees(doc, &mut out);
    check_designators(doc, &mut out);
    check_supply_profiles(doc, &mut out);

    sort_diagnostics(&mut out);
    out
}

/// The checks of [`validate`] that report the five integrity codes
/// (`UnsupportedSchemaVersion`, `IdOutOfRange`, `DuplicateId`,
/// `DanglingReference`, `SelfLoopWire`) and nothing else. The result is the
/// same as filtering [`validate`] by those codes, at a fraction of the cost.
pub(crate) fn integrity(doc: &Schematic) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    check_schema_version(doc, &mut out);
    check_id_ranges(doc, &mut out);
    check_unique_ids(doc, &mut out);
    check_references(doc, &mut out);

    sort_diagnostics(&mut out);
    out
}

/// Every entity of the document as `(raw id, subject)`.
fn entities(doc: &Schematic) -> impl Iterator<Item = (u64, Subject)> + '_ {
    let supplies = doc.supplies.iter().map(|s| (s.id.0, Subject::Supply(s.id)));
    let devices = doc.devices.iter().map(|d| (d.id.0, Subject::Device(d.id)));
    let symbols = doc.symbols.iter().map(|s| (s.id.0, Subject::Symbol(s.id)));
    let junctions = doc
        .junctions
        .iter()
        .map(|j| (j.id.0, Subject::Junction(j.id)));
    let wires = doc.wires.iter().map(|w| (w.id.0, Subject::Wire(w.id)));
    supplies
        .chain(devices)
        .chain(symbols)
        .chain(junctions)
        .chain(wires)
}

fn check_schema_version(doc: &Schematic, out: &mut Vec<Diagnostic>) {
    if doc.schema_version != CURRENT_SCHEMA_VERSION {
        out.push(Diagnostic::new(
            DiagnosticCode::UnsupportedSchemaVersion,
            Subject::Document,
            format!(
                "document has schema version {} but this build works on version {CURRENT_SCHEMA_VERSION}",
                doc.schema_version
            ),
        ));
    }
}

fn check_id_ranges(doc: &Schematic, out: &mut Vec<Diagnostic>) {
    // `next_id` may be one past the largest valid ID, never more.
    if doc.next_id == 0 || doc.next_id.saturating_sub(1) > MAX_ENTITY_ID {
        out.push(Diagnostic::new(
            DiagnosticCode::IdOutOfRange,
            Subject::Document,
            format!(
                "next_id {} is outside 1..={}",
                doc.next_id,
                MAX_ENTITY_ID.saturating_add(1)
            ),
        ));
    }

    for (id, subject) in entities(doc) {
        let problem = if id == 0 {
            Some("id 0 is not a valid identifier".to_owned())
        } else if id > MAX_ENTITY_ID {
            Some(format!("id {id} exceeds the maximum {MAX_ENTITY_ID}"))
        } else if id >= doc.next_id {
            Some(format!("id {id} is not below next_id {}", doc.next_id))
        } else {
            None
        };
        if let Some(message) = problem {
            out.push(Diagnostic::new(
                DiagnosticCode::IdOutOfRange,
                subject,
                message,
            ));
        }
    }
}

/// IDs come from one counter, so they must be unique across every collection,
/// not only inside each one.
fn check_unique_ids(doc: &Schematic, out: &mut Vec<Diagnostic>) {
    let mut by_id: BTreeMap<u64, BTreeSet<Subject>> = BTreeMap::new();
    let mut repeated_within_kind: BTreeSet<(u64, Subject)> = BTreeSet::new();
    for (id, subject) in entities(doc) {
        if !by_id.entry(id).or_default().insert(subject.clone()) {
            repeated_within_kind.insert((id, subject));
        }
    }

    for (id, subject) in repeated_within_kind {
        out.push(Diagnostic::new(
            DiagnosticCode::DuplicateId,
            subject,
            format!("id {id} is used more than once by the same kind of entity"),
        ));
    }

    for (id, subjects) in by_id {
        let mut subjects = subjects.into_iter();
        let (Some(first), Some(second)) = (subjects.next(), subjects.next()) else {
            continue;
        };
        let mut related = vec![second];
        related.extend(subjects);
        out.push(
            Diagnostic::new(
                DiagnosticCode::DuplicateId,
                first,
                format!("id {id} is shared by different kinds of entity"),
            )
            .with_related(related),
        );
    }
}

fn check_references(doc: &Schematic, out: &mut Vec<Diagnostic>) {
    let devices: BTreeSet<DeviceId> = doc.devices.iter().map(|d| d.id).collect();
    let symbols: BTreeSet<SymbolId> = doc.symbols.iter().map(|s| s.id).collect();
    let junctions: BTreeSet<JunctionId> = doc.junctions.iter().map(|j| j.id).collect();

    for symbol in &doc.symbols {
        if !devices.contains(&symbol.device_id) {
            out.push(
                Diagnostic::new(
                    DiagnosticCode::DanglingReference,
                    Subject::Symbol(symbol.id),
                    format!(
                        "symbol {} refers to device {} which does not exist",
                        symbol.id, symbol.device_id
                    ),
                )
                .with_related(vec![Subject::Device(symbol.device_id)]),
            );
        }
    }

    for wire in &doc.wires {
        if wire.from == wire.to {
            out.push(Diagnostic::new(
                DiagnosticCode::SelfLoopWire,
                Subject::Wire(wire.id),
                format!("wire {} starts and ends at the same point", wire.id),
            ));
        }

        for endpoint in [&wire.from, &wire.to] {
            match endpoint {
                Endpoint::Terminal(pin) if !symbols.contains(&pin.symbol_id) => out.push(
                    Diagnostic::new(
                        DiagnosticCode::DanglingReference,
                        Subject::Wire(wire.id),
                        format!(
                            "wire {} ends at symbol {} which does not exist",
                            wire.id, pin.symbol_id
                        ),
                    )
                    .with_related(vec![Subject::Symbol(pin.symbol_id)]),
                ),
                Endpoint::Junction(junction) if !junctions.contains(junction) => out.push(
                    Diagnostic::new(
                        DiagnosticCode::DanglingReference,
                        Subject::Wire(wire.id),
                        format!(
                            "wire {} ends at junction {junction} which does not exist",
                            wire.id
                        ),
                    )
                    .with_related(vec![Subject::Junction(*junction)]),
                ),
                Endpoint::Terminal(_) | Endpoint::Junction(_) => {}
            }
        }
    }
}

fn check_junction_degrees(doc: &Schematic, out: &mut Vec<Diagnostic>) {
    let mut wires_at: BTreeMap<JunctionId, u32> = doc.junctions.iter().map(|j| (j.id, 0)).collect();
    for wire in &doc.wires {
        let mut touched = BTreeSet::new();
        for endpoint in [&wire.from, &wire.to] {
            if let Endpoint::Junction(junction) = endpoint {
                touched.insert(*junction);
            }
        }
        for junction in touched {
            if let Some(count) = wires_at.get_mut(&junction) {
                *count = count.saturating_add(1);
            }
        }
    }

    for (junction, count) in wires_at {
        match count {
            0 => out.push(Diagnostic::new(
                DiagnosticCode::JunctionUnconnected,
                Subject::Junction(junction),
                format!("junction {junction} has no wire"),
            )),
            1 => out.push(Diagnostic::new(
                DiagnosticCode::JunctionDangling,
                Subject::Junction(junction),
                format!("junction {junction} has a single wire"),
            )),
            _ => {}
        }
    }
}

fn check_designators(doc: &Schematic, out: &mut Vec<Diagnostic>) {
    let mut by_reference: BTreeMap<&str, BTreeSet<DeviceId>> = BTreeMap::new();
    for device in &doc.devices {
        if !device.reference.is_valid() {
            out.push(Diagnostic::new(
                DiagnosticCode::InvalidReference,
                Subject::Device(device.id),
                "reference designator must be an uppercase letter followed by up to 15 uppercase letters, digits or underscores",
            ));
        }
        by_reference
            .entry(device.reference.0.as_str())
            .or_default()
            .insert(device.id);
    }

    for devices in by_reference.into_values() {
        let mut devices = devices.into_iter();
        let (Some(first), Some(second)) = (devices.next(), devices.next()) else {
            continue;
        };
        let mut related = vec![Subject::Device(second)];
        related.extend(devices.map(Subject::Device));
        out.push(
            Diagnostic::new(
                DiagnosticCode::DuplicateReference,
                Subject::Device(first),
                "reference designator is used by more than one device",
            )
            .with_related(related),
        );
    }
}

fn check_supply_profiles(doc: &Schematic, out: &mut Vec<Diagnostic>) {
    for supply in &doc.supplies {
        if let Some(reason) = supply.inconsistency() {
            out.push(Diagnostic::new(
                DiagnosticCode::InvalidSupplyProfile,
                Subject::Supply(supply.id),
                reason,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::geometry::{GridPoint, QuarterTurn};
    use crate::ids::{SupplyId, WireId};
    use crate::schematic::{
        Device, Junction, ModelRef, PinRef, ReferenceDesignator, SymbolInstance, SymbolKey,
        TerminalKey, Wire,
    };
    use crate::supply::{SupplyKind, SupplyProfile};

    fn device(id: u64, reference: &str) -> Device {
        Device {
            id: DeviceId(id),
            reference: ReferenceDesignator(reference.to_owned()),
            model: ModelRef {
                id: "contactor.generic".to_owned(),
                version: 1,
            },
            params: BTreeMap::new(),
        }
    }

    fn symbol(id: u64, device_id: u64) -> SymbolInstance {
        SymbolInstance {
            id: SymbolId(id),
            device_id: DeviceId(device_id),
            symbol_key: SymbolKey("coil".to_owned()),
            slot: None,
            position: GridPoint::default(),
            rotation: QuarterTurn::Deg0,
        }
    }

    fn pin(symbol_id: u64, terminal: &str) -> Endpoint {
        Endpoint::Terminal(PinRef {
            symbol_id: SymbolId(symbol_id),
            terminal: TerminalKey(terminal.to_owned()),
        })
    }

    fn wire(id: u64, from: Endpoint, to: Endpoint) -> Wire {
        Wire {
            id: WireId(id),
            from,
            to,
            route: Vec::new(),
        }
    }

    /// K1 coil (symbol 3) wired from both terminals to junction 4.
    fn valid() -> Schematic {
        let mut doc = Schematic::new("valid");
        doc.supplies.push(SupplyProfile {
            id: SupplyId(1),
            kind: SupplyKind::Dc,
            nominal_voltage_mv: 24_000,
            phase_voltage_mv: None,
            frequency_millihz: None,
            phase_sequence: None,
        });
        doc.devices.push(device(2, "K1"));
        doc.symbols.push(symbol(3, 2));
        doc.junctions.push(Junction {
            id: JunctionId(4),
            position: GridPoint::new(2, 0),
        });
        doc.wires
            .push(wire(5, pin(3, "a1"), Endpoint::Junction(JunctionId(4))));
        doc.wires
            .push(wire(6, pin(3, "a2"), Endpoint::Junction(JunctionId(4))));
        doc.next_id = 7;
        doc
    }

    fn codes(doc: &Schematic) -> Vec<(DiagnosticCode, Subject)> {
        doc.validate()
            .into_iter()
            .map(|d| (d.code(), d.subject().clone()))
            .collect()
    }

    #[test]
    fn accepts_a_consistent_document() {
        assert_eq!(valid().validate(), []);
    }

    #[test]
    fn rejects_a_schema_version_other_than_current() {
        let mut doc = valid();
        doc.schema_version = 999;

        assert_eq!(
            codes(&doc),
            [(DiagnosticCode::UnsupportedSchemaVersion, Subject::Document)]
        );
    }

    #[test]
    fn rejects_id_zero() {
        let mut doc = valid();
        doc.supplies[0].id = SupplyId(0);

        assert_eq!(
            codes(&doc),
            [(DiagnosticCode::IdOutOfRange, Subject::Supply(SupplyId(0)))]
        );
    }

    #[test]
    fn rejects_id_above_max_entity_id() {
        let mut doc = valid();
        doc.supplies[0].id = SupplyId(u64::MAX);
        doc.next_id = MAX_ENTITY_ID + 1;

        assert_eq!(
            codes(&doc),
            [(
                DiagnosticCode::IdOutOfRange,
                Subject::Supply(SupplyId(u64::MAX))
            )]
        );
    }

    #[test]
    fn rejects_id_not_below_next_id() {
        let mut doc = valid();
        doc.next_id = 6;

        assert_eq!(
            codes(&doc),
            [(DiagnosticCode::IdOutOfRange, Subject::Wire(WireId(6)))]
        );
    }

    #[test]
    fn rejects_next_id_of_zero_or_beyond_the_id_space() {
        let mut doc = Schematic::new("empty");
        doc.next_id = 0;
        assert_eq!(
            codes(&doc),
            [(DiagnosticCode::IdOutOfRange, Subject::Document)]
        );

        doc.next_id = MAX_ENTITY_ID + 2;
        assert_eq!(
            codes(&doc),
            [(DiagnosticCode::IdOutOfRange, Subject::Document)]
        );

        doc.next_id = MAX_ENTITY_ID + 1;
        assert_eq!(codes(&doc), []);
    }

    #[test]
    fn rejects_id_repeated_inside_one_collection() {
        let mut doc = valid();
        doc.devices.push(device(2, "K2"));

        assert_eq!(
            codes(&doc),
            [(DiagnosticCode::DuplicateId, Subject::Device(DeviceId(2)))]
        );
    }

    #[test]
    fn rejects_id_shared_across_collections() {
        let mut doc = valid();
        doc.junctions[0].id = JunctionId(2);
        for wire in &mut doc.wires {
            wire.to = Endpoint::Junction(JunctionId(2));
        }

        let diagnostics = doc.validate();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code(), DiagnosticCode::DuplicateId);
        assert_eq!(diagnostics[0].subject(), &Subject::Device(DeviceId(2)));
        assert_eq!(diagnostics[0].related(), [Subject::Junction(JunctionId(2))]);
    }

    #[test]
    fn rejects_symbol_of_unknown_device() {
        let mut doc = valid();
        doc.symbols[0].device_id = DeviceId(99);

        let diagnostics = doc.validate();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code(), DiagnosticCode::DanglingReference);
        assert_eq!(diagnostics[0].subject(), &Subject::Symbol(SymbolId(3)));
        assert_eq!(diagnostics[0].related(), [Subject::Device(DeviceId(99))]);
    }

    #[test]
    fn rejects_wire_to_unknown_symbol() {
        let mut doc = valid();
        doc.wires[0].from = pin(99, "a1");

        assert_eq!(
            codes(&doc),
            [(DiagnosticCode::DanglingReference, Subject::Wire(WireId(5)))]
        );
    }

    #[test]
    fn rejects_wire_to_unknown_junction() {
        let mut doc = valid();
        doc.wires[0].to = Endpoint::Junction(JunctionId(99));

        assert_eq!(
            codes(&doc),
            [
                (DiagnosticCode::DanglingReference, Subject::Wire(WireId(5))),
                (
                    DiagnosticCode::JunctionDangling,
                    Subject::Junction(JunctionId(4))
                ),
            ]
        );
    }

    #[test]
    fn rejects_wire_that_starts_and_ends_at_the_same_point() {
        let mut doc = valid();
        doc.wires[0].from = Endpoint::Junction(JunctionId(4));

        assert_eq!(
            codes(&doc),
            [(DiagnosticCode::SelfLoopWire, Subject::Wire(WireId(5)))]
        );
    }

    #[test]
    fn rejects_junction_without_wires() {
        let mut doc = valid();
        doc.wires.clear();
        doc.next_id = 5;

        assert_eq!(
            codes(&doc),
            [(
                DiagnosticCode::JunctionUnconnected,
                Subject::Junction(JunctionId(4))
            )]
        );
    }

    #[test]
    fn warns_about_junction_with_a_single_wire() {
        let mut doc = valid();
        doc.wires.pop();

        let diagnostics = doc.validate();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code(), DiagnosticCode::JunctionDangling);
        assert!(!diagnostics[0].is_error());
    }

    #[test]
    fn rejects_duplicate_reference_designator() {
        let mut doc = valid();
        doc.devices.push(device(7, "K1"));
        doc.devices.push(device(8, "K1"));
        doc.next_id = 9;

        let diagnostics = doc.validate();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code(), DiagnosticCode::DuplicateReference);
        assert_eq!(diagnostics[0].subject(), &Subject::Device(DeviceId(2)));
        assert_eq!(
            diagnostics[0].related(),
            [Subject::Device(DeviceId(7)), Subject::Device(DeviceId(8))]
        );
    }

    #[test]
    fn rejects_malformed_reference_designator() {
        let mut doc = valid();
        doc.devices[0].reference = ReferenceDesignator("k1".to_owned());

        assert_eq!(
            codes(&doc),
            [(
                DiagnosticCode::InvalidReference,
                Subject::Device(DeviceId(2))
            )]
        );
    }

    #[test]
    fn rejects_inconsistent_supply_profile() {
        let mut doc = valid();
        doc.supplies[0].frequency_millihz = Some(60_000);

        assert_eq!(
            codes(&doc),
            [(
                DiagnosticCode::InvalidSupplyProfile,
                Subject::Supply(SupplyId(1))
            )]
        );
    }

    /// The integrity diagnostics are what `validate` reports for the same
    /// five codes, so a new check that emits one of them cannot be left out
    /// of `integrity` unnoticed.
    #[test]
    fn integrity_is_the_integrity_part_of_validate() {
        const INTEGRITY: [DiagnosticCode; 5] = [
            DiagnosticCode::UnsupportedSchemaVersion,
            DiagnosticCode::IdOutOfRange,
            DiagnosticCode::DuplicateId,
            DiagnosticCode::DanglingReference,
            DiagnosticCode::SelfLoopWire,
        ];
        let mut broken: Vec<Schematic> = vec![valid()];
        let mut doc = valid();
        doc.schema_version += 1;
        broken.push(doc);
        let mut doc = valid();
        doc.symbols[0].id = SymbolId(0);
        doc.next_id = 3;
        broken.push(doc);
        let mut doc = valid();
        doc.devices.push(device(2, "K2"));
        broken.push(doc);
        let mut doc = valid();
        doc.symbols[0].device_id = DeviceId(99);
        broken.push(doc);
        let mut doc = valid();
        doc.wires[0].from = pin(99, "a1");
        doc.wires[1].to = doc.wires[1].from.clone();
        broken.push(doc);
        // Findings of the checks `integrity` leaves out.
        let mut doc = valid();
        doc.devices.push(device(7, "K1"));
        doc.junctions.push(Junction {
            id: JunctionId(8),
            position: GridPoint::default(),
        });
        doc.next_id = 9;
        broken.push(doc);

        for doc in broken {
            let expected: Vec<Diagnostic> = doc
                .validate()
                .into_iter()
                .filter(|diagnostic| INTEGRITY.contains(&diagnostic.code()))
                .collect();
            assert_eq!(doc.integrity_errors(), expected);
        }
    }

    #[test]
    fn result_does_not_depend_on_collection_order() {
        let mut doc = valid();
        doc.devices.push(device(2, "K1"));
        doc.devices.push(device(7, "K1"));
        doc.symbols.push(symbol(8, 77));
        doc.junctions.push(Junction {
            id: JunctionId(9),
            position: GridPoint::default(),
        });
        doc.wires.push(wire(
            10,
            Endpoint::Junction(JunctionId(98)),
            Endpoint::Junction(JunctionId(99)),
        ));
        doc.next_id = 11;
        let forward = doc.validate();

        doc.devices.reverse();
        doc.symbols.reverse();
        doc.junctions.reverse();
        doc.wires.reverse();
        doc.supplies.reverse();

        assert!(
            forward.len() >= 5,
            "fixture should produce several findings"
        );
        assert_eq!(doc.validate(), forward);
    }
}
