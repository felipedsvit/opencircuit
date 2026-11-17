//! The number of every wire (ADR-0014).
//!
//! Numbers are derived, never stored: a net takes the override of its wire
//! with the smallest ID when one has [`Wire::number`](crate::schematic::Wire::number), and otherwise the next
//! number of the project rule, [`WireNumbering`](crate::documentation::WireNumbering). All the wires of a net, and
//! so all the segments of a wire drawn through junctions, carry one number.
//!
//! Nets are ordered by their first point: the sheet number (a point outside
//! every sheet comes last), then the zone (row, then column), then `y`, then
//! `x`, then the index of the net. An override keeps the place of its net in
//! the sequence, so overriding one wire never shifts the others. A project
//! with no rule has only its overrides.

use std::collections::BTreeMap;

use crate::catalog::ComponentCatalog;
use crate::ids::WireId;
use crate::layout::LayoutIndex;
use crate::nets::{NetId, StaticNets};
use crate::schematic::{Endpoint, Schematic};
use crate::sheet::sheet_at;

/// The numbers of the wires of a document.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct WireNumbers {
    by_net: BTreeMap<NetId, String>,
    by_wire: BTreeMap<WireId, String>,
}

/// Where a net starts, for ordering: sheet number, zone row, zone column,
/// `y`, `x`. A point outside every sheet sorts after all the others.
type Start = (u32, u8, u8, i32, i32);

impl WireNumbers {
    /// The numbers of a document, from its nets and the rule in its project
    /// information.
    #[must_use]
    pub fn build(doc: &Schematic, catalog: &ComponentCatalog, nets: &StaticNets) -> Self {
        let layout = LayoutIndex::new(doc, catalog);
        let wire_override: BTreeMap<WireId, &str> = doc
            .wires
            .iter()
            .filter_map(|wire| match wire.number.as_deref() {
                Some(number) if !number.is_empty() => Some((wire.id, number)),
                _ => None,
            })
            .collect();

        // The nets that have a wire, in sequence order.
        let mut sequence: Vec<(Start, NetId, Option<u32>)> =
            nets.nets()
                .iter()
                .filter(|net| !net.wires.is_empty())
                .map(|net| {
                    let points =
                        net.pins
                            .iter()
                            .filter_map(|pin| layout.pin_position(pin))
                            .chain(net.junctions.iter().filter_map(|id| {
                                layout.endpoint_position(&Endpoint::Junction(*id))
                            }));
                    let mut first: Option<(Start, Option<u32>)> = None;
                    for point in points {
                        let (sheet, zone) = match sheet_at(doc, point) {
                            Some(sheet) => (
                                Some(sheet.number),
                                sheet.zone_of(point).map_or((0, 0), |z| (z.row, z.column)),
                            ),
                            None => (None, (0, 0)),
                        };
                        let start = (sheet.unwrap_or(u32::MAX), zone.0, zone.1, point.y, point.x);
                        if first.is_none_or(|(best, _)| start < best) {
                            first = Some((start, sheet));
                        }
                    }
                    let (start, sheet) =
                        first.unwrap_or(((u32::MAX, 0, 0, i32::MAX, i32::MAX), None));
                    (start, net.id, sheet)
                })
                .collect();
        sequence.sort_unstable_by_key(|(start, id, _)| (*start, *id));

        let rule = doc.project.wire_numbering.as_ref();
        let mut next_in_group: BTreeMap<Option<u32>, u64> = BTreeMap::new();
        let mut by_net = BTreeMap::new();
        for (_, id, sheet) in &sequence {
            let Some(net) = nets.net(*id) else {
                continue;
            };
            // The counter belongs to a sheet only when the rule restarts on
            // each sheet.
            let group: Option<u32> = if rule.is_some_and(|rule| rule.per_sheet) {
                *sheet
            } else {
                None
            };
            let position = {
                let slot = next_in_group.entry(group).or_insert(0);
                let position = *slot;
                *slot = slot.saturating_add(1);
                position
            };
            let overridden = net
                .wires
                .iter()
                .find_map(|wire| wire_override.get(wire).copied());
            let text = match (overridden, rule) {
                (Some(text), _) => Some(text.to_owned()),
                (None, Some(rule)) => {
                    let value = u64::from(rule.start)
                        .saturating_add(position.saturating_mul(u64::from(rule.step.get())));
                    Some(match group {
                        Some(sheet) => format!("{}{sheet}.{value}", rule.prefix),
                        None => format!("{}{value}", rule.prefix),
                    })
                }
                (None, None) => None,
            };
            if let Some(text) = text {
                by_net.insert(*id, text);
            }
        }

        let mut by_wire = BTreeMap::new();
        for (id, text) in &by_net {
            if let Some(net) = nets.net(*id) {
                for wire in &net.wires {
                    by_wire.insert(*wire, text.clone());
                }
            }
        }
        Self { by_net, by_wire }
    }

    /// The number of a net; `None` for a net without a wire, or when the
    /// project has no rule and no override on it.
    #[must_use]
    pub fn of_net(&self, net: NetId) -> Option<&str> {
        self.by_net.get(&net).map(String::as_str)
    }

    /// The number of a wire, which is that of its net.
    #[must_use]
    pub fn of_wire(&self, wire: WireId) -> Option<&str> {
        self.by_wire.get(&wire).map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use super::*;
    use crate::builder::{SchematicBuilder, pin};
    use crate::documentation::WireNumbering;
    use crate::geometry::GridPoint;
    use crate::ids::SymbolId;
    use crate::sheet::{Orientation, Sheet, SheetSize, Zones};

    /// Three pushbuttons in a row, S1 to S3, at x = 0, 20 and 40.
    fn row() -> (SchematicBuilder, [SymbolId; 3]) {
        let mut b = SchematicBuilder::new("numbers");
        let mut symbols = [SymbolId(0); 3];
        for (index, x) in [0, 20, 40].into_iter().enumerate() {
            let device = b
                .device(&format!("S{}", index + 1), "pushbutton_no", &[])
                .unwrap();
            symbols[index] = b
                .symbol(device, "contact", None, GridPoint::new(x, 0))
                .unwrap();
        }
        (b, symbols)
    }

    fn rule(prefix: &str, start: u32, step: u32, per_sheet: bool) -> WireNumbering {
        WireNumbering {
            prefix: prefix.to_owned(),
            start,
            step: NonZeroU32::new(step).unwrap(),
            per_sheet,
        }
    }

    fn numbers(doc: &Schematic) -> WireNumbers {
        let catalog = ComponentCatalog::builtin();
        WireNumbers::build(doc, &catalog, &StaticNets::build(doc, &catalog))
    }

    fn sheet(id: u64, number: u32, x: i32) -> Sheet {
        Sheet {
            id: crate::ids::SheetId(id),
            number,
            title: String::new(),
            origin: GridPoint::new(x, -10),
            size: SheetSize::Custom {
                width: 40,
                height: 30,
            },
            orientation: Orientation::Landscape,
            zones: Zones {
                columns: 8,
                rows: 6,
            },
        }
    }

    #[test]
    fn nets_are_numbered_in_reading_order_from_the_start_with_the_step() {
        let (mut b, [s1, s2, s3]) = row();
        b.wire(pin(s2, "p2"), pin(s3, "p1")).unwrap();
        b.wire(pin(s1, "p2"), pin(s2, "p1")).unwrap();
        let mut doc = b.finish();
        doc.project.wire_numbering = Some(rule("W", 100, 5, false));

        let numbers = numbers(&doc);

        // Both nets start on the row of the upper terminals; the one that
        // starts further left (S1 to S2) is numbered first.
        assert_eq!(numbers.of_wire(doc.wires[1].id), Some("W100"));
        assert_eq!(numbers.of_wire(doc.wires[0].id), Some("W105"));
    }

    #[test]
    fn every_wire_of_a_net_carries_the_same_number() {
        let (mut b, [s1, s2, s3]) = row();
        let junction = b.junction(GridPoint::new(10, 5)).unwrap();
        b.wire(pin(s1, "p2"), junction).unwrap();
        b.wire(junction, pin(s2, "p1")).unwrap();
        b.wire(junction, pin(s3, "p1")).unwrap();
        let mut doc = b.finish();
        doc.project.wire_numbering = Some(rule("", 1, 1, false));

        let numbers = numbers(&doc);

        for wire in &doc.wires {
            assert_eq!(numbers.of_wire(wire.id), Some("1"));
        }
    }

    #[test]
    fn an_override_wins_and_does_not_shift_the_other_numbers() {
        let (mut b, [s1, s2, s3]) = row();
        b.wire(pin(s1, "p2"), pin(s2, "p1")).unwrap();
        b.wire(pin(s2, "p2"), pin(s3, "p1")).unwrap();
        let mut doc = b.finish();
        doc.project.wire_numbering = Some(rule("W", 1, 1, false));
        assert_eq!(numbers(&doc).of_wire(doc.wires[1].id), Some("W2"));

        doc.wires[0].number = Some("L1".to_owned());

        let numbers = numbers(&doc);
        assert_eq!(numbers.of_wire(doc.wires[0].id), Some("L1"));
        assert_eq!(numbers.of_wire(doc.wires[1].id), Some("W2"));
    }

    #[test]
    fn without_a_rule_only_the_overrides_are_numbered() {
        let (mut b, [s1, s2, s3]) = row();
        b.wire(pin(s1, "p2"), pin(s2, "p1")).unwrap();
        b.wire(pin(s2, "p2"), pin(s3, "p1")).unwrap();
        let mut doc = b.finish();
        doc.wires[1].number = Some("N".to_owned());

        let numbers = numbers(&doc);

        assert_eq!(numbers.of_wire(doc.wires[0].id), None);
        assert_eq!(numbers.of_wire(doc.wires[1].id), Some("N"));
    }

    #[test]
    fn an_empty_override_is_no_override() {
        let (mut b, [s1, s2, _]) = row();
        b.wire(pin(s1, "p2"), pin(s2, "p1")).unwrap();
        let mut doc = b.finish();
        doc.project.wire_numbering = Some(rule("W", 7, 1, false));
        doc.wires[0].number = Some(String::new());

        assert_eq!(numbers(&doc).of_wire(doc.wires[0].id), Some("W7"));
    }

    #[test]
    fn numbers_restart_on_each_sheet_when_the_rule_says_so() {
        let (mut b, [s1, s2, s3]) = row();
        b.wire(pin(s1, "p2"), pin(s2, "p1")).unwrap();
        b.wire(pin(s2, "p2"), pin(s3, "p1")).unwrap();
        let junction = b.junction(GridPoint::new(45, 5)).unwrap();
        b.wire(pin(s3, "p2"), junction).unwrap();
        let mut doc = b.finish();
        // S1 and S2 are on sheet 1; S3 is on sheet 2, to the right.
        doc.sheets = vec![sheet(900, 1, -10), sheet(901, 2, 30)];
        doc.project.wire_numbering = Some(rule("W", 1, 1, true));

        let numbers = numbers(&doc);

        // The second net has a terminal on each sheet and starts on sheet 1,
        // so it continues that sheet's counter. The net that is only on
        // sheet 2 restarts at the first number.
        assert_eq!(numbers.of_wire(doc.wires[0].id), Some("W1.1"));
        assert_eq!(numbers.of_wire(doc.wires[1].id), Some("W1.2"));
        assert_eq!(numbers.of_wire(doc.wires[2].id), Some("W2.1"));
    }

    #[test]
    fn numbers_do_not_depend_on_the_order_of_the_collections() {
        let (mut b, [s1, s2, s3]) = row();
        let junction = b.junction(GridPoint::new(10, 5)).unwrap();
        b.wire(pin(s1, "p2"), junction).unwrap();
        b.wire(junction, pin(s2, "p1")).unwrap();
        b.wire(pin(s2, "p2"), pin(s3, "p1")).unwrap();
        let mut doc = b.finish();
        doc.project.wire_numbering = Some(rule("W", 1, 1, false));
        let expected = numbers(&doc);

        doc.devices.reverse();
        doc.symbols.reverse();
        doc.junctions.reverse();
        doc.wires.reverse();

        assert_eq!(numbers(&doc), expected);
    }
}
