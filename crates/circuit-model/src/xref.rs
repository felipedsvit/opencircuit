//! Cross-references between the coil of a device and its contacts.
//!
//! On a drawing printed over several sheets, a reader of a coil wants to know
//! where its contacts are, and a reader of a contact wants to know where its
//! coil is. Both are derived from the document and its sheets (ADR-0011) and
//! never stored.
//!
//! A location is the number of the sheet and the zone of the symbol's origin
//! on it, such as `2/4B`. A document without sheets has no locations and
//! therefore no cross-references.

use std::collections::BTreeMap;
use std::fmt;

use crate::catalog::{ComponentCatalog, SymbolKind};
use crate::geometry::GridPoint;
use crate::ids::{DeviceId, SymbolId};
use crate::labels::terminal_label;
use crate::schematic::{Device, Schematic, SymbolInstance};
use crate::sheet::{Zone, sheet_at};

/// Where a symbol is: a sheet and a zone of it. A symbol outside every frame
/// has neither.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Location {
    /// Number of the sheet.
    pub sheet: u32,
    /// Zone of the frame.
    pub zone: Zone,
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.sheet, self.zone)
    }
}

/// A contact (or secondary winding) of a device, as listed under its coil.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContactRef {
    /// The contact symbol.
    pub symbol: SymbolId,
    /// Its terminal numbers, first and last: `13-14`, `1-6`. Empty when its
    /// terminals have no marking.
    pub terminals: String,
    /// Where it is; `None` when it is outside every sheet.
    pub location: Option<Location>,
}

/// The cross-references of a document.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CrossReferences {
    /// For each coil symbol, the contacts of its device, ordered by
    /// location and then by symbol ID.
    pub contacts: BTreeMap<SymbolId, Vec<ContactRef>>,
    /// For each contact symbol of a device that has a coil, where the coil
    /// is: `None` when the coil is outside every sheet.
    pub coil: BTreeMap<SymbolId, Option<Location>>,
}

/// Whether a symbol kind is one that a coil drives, or that a transformer
/// primary feeds.
const fn is_driven(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::ContactNo
            | SymbolKind::ContactNc
            | SymbolKind::Changeover
            | SymbolKind::PowerContacts { .. }
            | SymbolKind::TransformerSecondary
    )
}

/// Compute the cross-references of `doc`. A document with no sheet has
/// none. A device with several coil symbols lists its contacts under each,
/// and its contacts point to the coil with the smallest symbol ID.
#[must_use]
pub fn cross_references(doc: &Schematic, catalog: &ComponentCatalog) -> CrossReferences {
    if doc.sheets.is_empty() {
        return CrossReferences::default();
    }
    derive(doc, catalog).0
}

/// The cross-references, and for each contact symbol the coil symbol it
/// points to, whether or not the document has sheets (without them nothing
/// has a location).
fn derive(
    doc: &Schematic,
    catalog: &ComponentCatalog,
) -> (CrossReferences, BTreeMap<SymbolId, SymbolId>) {
    let mut found = CrossReferences::default();
    let mut coil_of: BTreeMap<SymbolId, SymbolId> = BTreeMap::new();
    let location_of = |point: GridPoint| {
        let sheet = sheet_at(doc, point)?;
        Some(Location {
            sheet: sheet.number,
            zone: sheet.zone_of(point)?,
        })
    };

    let mut by_device: BTreeMap<DeviceId, Vec<&SymbolInstance>> = BTreeMap::new();
    for symbol in &doc.symbols {
        by_device.entry(symbol.device_id).or_default().push(symbol);
    }
    let devices: BTreeMap<DeviceId, &Device> = doc.devices.iter().map(|d| (d.id, d)).collect();

    for (device_id, symbols) in by_device {
        let Some(device) = devices.get(&device_id) else {
            continue;
        };
        let Some(model) = catalog.model(&device.model) else {
            continue;
        };
        let kind_of = |symbol: &SymbolInstance| {
            model
                .symbol(&symbol.symbol_key)
                .map(|definition| definition.kind())
        };
        let mut coils: Vec<&SymbolInstance> = symbols
            .iter()
            .copied()
            .filter(|symbol| {
                matches!(
                    kind_of(symbol),
                    Some(SymbolKind::Coil | SymbolKind::TransformerPrimary)
                )
            })
            .collect();
        coils.sort_by_key(|symbol| symbol.id);
        let Some(first_coil) = coils.first() else {
            continue;
        };
        let coil_location = location_of(first_coil.position);

        let mut contacts: Vec<ContactRef> = Vec::new();
        for symbol in &symbols {
            let Some(definition) = model.symbol(&symbol.symbol_key) else {
                continue;
            };
            if !is_driven(definition.kind()) {
                continue;
            }
            let labels: Vec<String> = definition
                .terminals()
                .iter()
                .filter_map(|terminal| {
                    terminal_label(model.class(), definition.kind(), symbol.slot, &terminal.key)
                })
                .collect();
            let terminals = match (labels.first(), labels.last()) {
                (Some(first), Some(last)) if first != last => format!("{first}-{last}"),
                (Some(only), _) => only.clone(),
                _ => String::new(),
            };
            contacts.push(ContactRef {
                symbol: symbol.id,
                terminals,
                location: location_of(symbol.position),
            });
            found.coil.insert(symbol.id, coil_location);
            coil_of.insert(symbol.id, first_coil.id);
        }
        // Located contacts first, in sheet and zone order, then the ones
        // outside every sheet, each group by symbol ID.
        contacts
            .sort_by_key(|contact| (contact.location.is_none(), contact.location, contact.symbol));
        for coil in coils {
            found.contacts.insert(coil.id, contacts.clone());
        }
    }
    (found, coil_of)
}

/// Somewhere to go from a symbol: the coil of a contact, or a contact of a
/// coil.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Link {
    /// The symbol it leads to.
    pub symbol: SymbolId,
    /// What to show: `13-14 2/4B` for a contact, `coil 1/3A` for a coil, with
    /// the location left out when the document has no sheets.
    pub text: String,
}

/// Where to go from `symbol`: for a coil, every contact of its device in the
/// order of [`CrossReferences::contacts`]; for a contact, its coil; for
/// anything else, nowhere. Unlike [`cross_references`] it works on a document
/// without sheets, where the links have no location.
#[must_use]
pub fn links(doc: &Schematic, catalog: &ComponentCatalog, symbol: SymbolId) -> Vec<Link> {
    let (found, coil_of) = derive(doc, catalog);
    let with_location = |text: String, location: Option<Location>| match location {
        Some(location) => format!("{text} {location}").trim().to_owned(),
        None => text,
    };
    if let Some(contacts) = found.contacts.get(&symbol) {
        return contacts
            .iter()
            .map(|contact| Link {
                symbol: contact.symbol,
                text: with_location(contact.terminals.clone(), contact.location),
            })
            .collect();
    }
    if let (Some(coil), Some(location)) = (coil_of.get(&symbol), found.coil.get(&symbol)) {
        return vec![Link {
            symbol: *coil,
            text: with_location("coil".to_owned(), *location),
        }];
    }
    // A signal arrow goes to the other arrows of its name (ADR-0016).
    let arrows = crate::signals::signal_arrows(doc, catalog);
    let name = arrows
        .iter()
        .find(|(_, list)| list.iter().any(|arrow| arrow.symbol == symbol))
        .map(|(name, _)| name.clone());
    let others = crate::signals::signal_references(doc, catalog);
    match (name, others.get(&symbol)) {
        (Some(name), Some(others)) => others
            .iter()
            .map(|(other, location)| Link {
                symbol: *other,
                text: with_location(name.clone(), *location),
            })
            .collect(),
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::SchematicBuilder;
    use crate::ids::SheetId;
    use crate::sheet::{Orientation, Sheet, SheetSize, Zones};

    fn point(x: i32, y: i32) -> GridPoint {
        GridPoint::new(x, y)
    }

    /// K1 with its coil on sheet 1 and a contact on each sheet, a lamp
    /// without any coil, and two A4 sheets side by side.
    fn two_sheet_contactor() -> (Schematic, [SymbolId; 4]) {
        let mut b = SchematicBuilder::new("xref");
        let k1 = b
            .device(
                "K1",
                "contactor.generic",
                &[
                    ("rated_voltage_mv", crate::ParamValue::Int(24_000)),
                    ("connection", crate::ParamValue::Text("dc".to_owned())),
                ],
            )
            .unwrap();
        let coil = b.symbol(k1, "coil", None, point(10, 10)).unwrap();
        let near = b.symbol(k1, "aux_no", Some(1), point(40, 10)).unwrap();
        let far = b.symbol(k1, "aux_nc", Some(2), point(110, 40)).unwrap();
        let lamp = b.device("H1", "lamp.generic", &[]).unwrap();
        let lamp = b.symbol(lamp, "lamp", None, point(20, 30)).unwrap();
        let mut doc = b.finish();
        for (number, x) in [(1_u32, 0_i32), (2, 100)] {
            let id = doc.next_id;
            doc.next_id += 1;
            doc.sheets.push(Sheet {
                id: SheetId(id),
                number,
                title: String::new(),
                origin: point(x, 0),
                size: SheetSize::A4,
                orientation: Orientation::Landscape,
                zones: Zones {
                    columns: 8,
                    rows: 6,
                },
                kind: Default::default(),
            });
        }
        (doc, [coil, near, far, lamp])
    }

    #[test]
    fn links_go_from_a_coil_to_its_contacts_and_back_with_their_locations() {
        let (doc, [coil, near, far, lamp]) = two_sheet_contactor();
        let catalog = ComponentCatalog::builtin();

        let from_coil = links(&doc, &catalog, coil);
        assert_eq!(
            from_coil,
            [
                Link {
                    symbol: near,
                    text: "13-14 1/4B".to_owned()
                },
                Link {
                    symbol: far,
                    text: "21-22 2/1E".to_owned()
                },
            ]
        );
        assert_eq!(
            links(&doc, &catalog, far),
            [Link {
                symbol: coil,
                text: "coil 1/1B".to_owned()
            }]
        );
        assert_eq!(links(&doc, &catalog, lamp), []);
    }

    #[test]
    fn links_work_without_sheets_and_then_have_no_location() {
        let (mut doc, [coil, near, _, _]) = two_sheet_contactor();
        doc.sheets.clear();
        let catalog = ComponentCatalog::builtin();

        assert_eq!(links(&doc, &catalog, coil)[0].text, "13-14");
        assert_eq!(
            links(&doc, &catalog, near),
            [Link {
                symbol: coil,
                text: "coil".to_owned()
            }]
        );
        assert_eq!(cross_references(&doc, &catalog), CrossReferences::default());
    }

    #[test]
    fn a_coil_lists_its_contacts_with_their_terminals_and_locations() {
        let (doc, [coil, near, far, _]) = two_sheet_contactor();

        let refs = cross_references(&doc, &ComponentCatalog::builtin());

        let listed = refs.contacts.get(&coil).unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].symbol, near);
        assert_eq!(listed[0].terminals, "13-14");
        assert_eq!(
            listed[0].location.map(|l| l.to_string()).as_deref(),
            Some("1/4B")
        );
        assert_eq!(listed[1].symbol, far);
        assert_eq!(listed[1].terminals, "21-22");
        assert_eq!(
            listed[1].location.map(|l| l.to_string()).as_deref(),
            Some("2/1E")
        );
    }

    #[test]
    fn a_contact_points_to_its_coil_and_a_device_without_a_coil_has_nothing() {
        let (doc, [_, near, far, lamp]) = two_sheet_contactor();

        let refs = cross_references(&doc, &ComponentCatalog::builtin());

        for contact in [near, far] {
            assert_eq!(
                refs.coil
                    .get(&contact)
                    .copied()
                    .flatten()
                    .map(|l| l.to_string())
                    .as_deref(),
                Some("1/1B")
            );
        }
        assert!(!refs.coil.contains_key(&lamp));
        assert!(!refs.contacts.contains_key(&lamp));
    }

    #[test]
    fn a_document_without_sheets_has_no_cross_references() {
        let (mut doc, _) = two_sheet_contactor();
        doc.sheets.clear();

        assert_eq!(
            cross_references(&doc, &ComponentCatalog::builtin()),
            CrossReferences::default()
        );
    }

    #[test]
    fn a_symbol_outside_every_sheet_has_no_location_and_sorts_last() {
        let (mut doc, [coil, near, far, _]) = two_sheet_contactor();
        doc.symbols
            .iter_mut()
            .find(|s| s.id == near)
            .unwrap()
            .position = point(500, 500);

        let refs = cross_references(&doc, &ComponentCatalog::builtin());

        let listed = refs.contacts.get(&coil).unwrap();
        assert_eq!(listed[0].symbol, far);
        assert_eq!((listed[1].symbol, listed[1].location), (near, None));
        assert_eq!(refs.coil.get(&near).copied(), Some(refs.coil[&far]));
    }

    #[test]
    fn the_result_does_not_depend_on_the_order_of_the_document() {
        let (doc, _) = two_sheet_contactor();
        let catalog = ComponentCatalog::builtin();
        let expected = cross_references(&doc, &catalog);

        let mut shuffled = doc.clone();
        shuffled.symbols.reverse();
        shuffled.devices.reverse();
        shuffled.sheets.reverse();

        assert_eq!(cross_references(&shuffled, &catalog), expected);
    }

    #[test]
    fn a_transformer_lists_its_secondary_under_its_primary() {
        let mut b = SchematicBuilder::new("transformer");
        let t1 = b
            .device(
                "T1",
                "transformer.control",
                &[
                    ("rated_voltage_mv", crate::ParamValue::Int(400_000)),
                    (
                        "connection",
                        crate::ParamValue::Text("line_line".to_owned()),
                    ),
                    ("secondary", crate::ParamValue::Int(1)),
                ],
            )
            .unwrap();
        let primary = b.symbol(t1, "primary", None, point(10, 10)).unwrap();
        let secondary = b.symbol(t1, "secondary", None, point(60, 10)).unwrap();
        let mut doc = b.finish();
        doc.sheets.push(Sheet {
            id: SheetId(doc.next_id),
            number: 1,
            title: String::new(),
            origin: point(0, 0),
            size: SheetSize::A4,
            orientation: Orientation::Landscape,
            zones: Zones {
                columns: 8,
                rows: 6,
            },
            kind: Default::default(),
        });
        doc.next_id += 1;

        let refs = cross_references(&doc, &ComponentCatalog::builtin());

        let listed = refs.contacts.get(&primary).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(
            (listed[0].symbol, listed[0].terminals.as_str()),
            (secondary, "X1-X2")
        );
        assert!(refs.coil.contains_key(&secondary));
    }
}
