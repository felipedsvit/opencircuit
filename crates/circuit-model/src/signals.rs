//! Signal arrows: names that join nets (ADR-0016).
//!
//! A signal arrow is a device of the class
//! [`DeviceClass::SignalArrow`] with one terminal and a `name`. Every arrow with
//! the same name is one net, whether it is on the same sheet or on another, so a
//! circuit can continue where a wire cannot go. Which arrows there are, which
//! ones stand alone and where the others of each one are is derived from the
//! document and its catalog, never stored.

use std::collections::BTreeMap;

use crate::catalog::{ComponentCatalog, DeviceClass, PARAM_NAME};
use crate::diagnostics::{Diagnostic, DiagnosticCode, Subject};
use crate::geometry::GridPoint;
use crate::ids::{DeviceId, SymbolId};
use crate::schematic::Schematic;
use crate::sheet::sheet_at;
use crate::xref::Location;

/// One signal arrow on the drawing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArrowAt {
    /// The device.
    pub device: DeviceId,
    /// Its symbol.
    pub symbol: SymbolId,
    /// Where it is.
    pub position: GridPoint,
}

/// Every arrow, by name; the arrows of a name are in symbol-ID order.
#[must_use]
pub fn signal_arrows(
    doc: &Schematic,
    catalog: &ComponentCatalog,
) -> BTreeMap<String, Vec<ArrowAt>> {
    let mut found: BTreeMap<String, Vec<ArrowAt>> = BTreeMap::new();
    for symbol in &doc.symbols {
        let Some(device) = doc.devices.iter().find(|d| d.id == symbol.device_id) else {
            continue;
        };
        let Some(model) = catalog.model(&device.model) else {
            continue;
        };
        if model.class() != DeviceClass::SignalArrow {
            continue;
        }
        let Some(name) = model.text_param(&device.params, PARAM_NAME) else {
            continue;
        };
        found.entry(name.to_owned()).or_default().push(ArrowAt {
            device: device.id,
            symbol: symbol.id,
            position: symbol.position,
        });
    }
    for arrows in found.values_mut() {
        arrows.sort_by_key(|arrow| arrow.symbol);
    }
    found
}

/// `OC-SEM-014`: an arrow whose name no other arrow has, so it joins nothing.
/// The subject is its device.
#[must_use]
pub fn lone_arrows(doc: &Schematic, catalog: &ComponentCatalog) -> Vec<Diagnostic> {
    signal_arrows(doc, catalog)
        .into_iter()
        .filter_map(|(name, arrows)| match arrows.as_slice() {
            [only] => Some(Diagnostic::new(
                DiagnosticCode::LoneSignalArrow,
                Subject::Device(only.device),
                format!("no other arrow is named `{name}`, so this one joins nothing"),
            )),
            _ => None,
        })
        .collect()
}

/// For each arrow, where the other arrows of its name are, by symbol ID; the
/// location is `None` for an arrow outside every sheet (or when the drawing
/// has no sheet).
#[must_use]
pub fn signal_references(
    doc: &Schematic,
    catalog: &ComponentCatalog,
) -> BTreeMap<SymbolId, Vec<(SymbolId, Option<Location>)>> {
    let location_of = |point: GridPoint| {
        let sheet = sheet_at(doc, point)?;
        Some(Location {
            sheet: sheet.number,
            zone: sheet.zone_of(point)?,
        })
    };
    let mut references = BTreeMap::new();
    for arrows in signal_arrows(doc, catalog).into_values() {
        for arrow in &arrows {
            let others: Vec<(SymbolId, Option<Location>)> = arrows
                .iter()
                .filter(|other| other.symbol != arrow.symbol)
                .map(|other| (other.symbol, location_of(other.position)))
                .collect();
            if !others.is_empty() {
                references.insert(arrow.symbol, others);
            }
        }
    }
    references
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::SchematicBuilder;
    use crate::catalog::{
        Category, ComponentModel, ParamDefinition, ParamType, Provenance, SymbolDefinition,
        SymbolKind,
    };
    use crate::library::Library;
    use crate::schematic::ParamValue;
    use crate::sheet::{Orientation, Sheet, SheetSize, Zones};

    pub(crate) fn arrow_catalog() -> ComponentCatalog {
        let provenance = Provenance {
            origin: "test".to_owned(),
            license: "MIT".to_owned(),
            standard_reference: None,
        };
        let model = ComponentModel::new(
            "std.signal_arrow",
            1,
            DeviceClass::SignalArrow,
            vec![ParamDefinition {
                key: PARAM_NAME.to_owned(),
                ty: ParamType::Name,
                default: Some(ParamValue::Text("SIGNAL".to_owned())),
            }],
            vec![SymbolDefinition::new(
                "arrow",
                SymbolKind::SignalArrow,
                1,
                1,
                false,
            )],
            provenance.clone(),
        )
        .with_category(Category::Lines);
        ComponentCatalog::builtin()
            .with_library(Library {
                id: "std".to_owned(),
                version: 1,
                name: "test".to_owned(),
                provenance,
                models: vec![model],
            })
            .unwrap()
    }

    /// Arrows `(name, x)` on a row; the sheets are two 40-wide frames.
    fn doc(arrows: &[(&str, i32)]) -> Schematic {
        let mut b = SchematicBuilder::new("arrows");
        for (index, (name, x)) in arrows.iter().enumerate() {
            let device = b
                .device(
                    &format!("SA{}", index + 1),
                    "std.signal_arrow",
                    &[(PARAM_NAME, ParamValue::Text((*name).to_owned()))],
                )
                .unwrap();
            b.symbol(device, "arrow", None, GridPoint::new(*x, 5))
                .unwrap();
        }
        let mut doc = b.finish();
        for (id, number, x) in [(900, 1, -5), (901, 2, 45)] {
            doc.sheets.push(Sheet {
                id: crate::ids::SheetId(id),
                number,
                title: String::new(),
                origin: GridPoint::new(x, -5),
                size: SheetSize::Custom {
                    width: 40,
                    height: 20,
                },
                orientation: Orientation::Landscape,
                zones: Zones {
                    columns: 4,
                    rows: 2,
                },
            });
        }
        doc
    }

    #[test]
    fn arrows_are_grouped_by_name() {
        let doc = doc(&[("FEED", 0), ("RUN", 5), ("FEED", 50)]);

        let found = signal_arrows(&doc, &arrow_catalog());

        assert_eq!(found.keys().collect::<Vec<_>>(), ["FEED", "RUN"]);
        assert_eq!(found["FEED"].len(), 2);
        assert_eq!(found["RUN"].len(), 1);
    }

    #[test]
    fn an_arrow_with_a_name_of_its_own_is_a_warning_and_a_pair_is_not() {
        let doc = doc(&[("FEED", 0), ("RUN", 5), ("FEED", 50)]);

        let lone = lone_arrows(&doc, &arrow_catalog());

        assert_eq!(lone.len(), 1);
        assert_eq!(lone[0].code(), DiagnosticCode::LoneSignalArrow);
        assert_eq!(lone[0].subject(), &Subject::Device(doc.devices[1].id));
    }

    #[test]
    fn each_arrow_knows_where_the_others_of_its_name_are() {
        let doc = doc(&[("FEED", 0), ("FEED", 50), ("FEED", 60)]);

        let references = signal_references(&doc, &arrow_catalog());

        let first = &references[&doc.symbols[0].id];
        let places: Vec<String> = first
            .iter()
            .map(|(_, location)| location.map_or_else(|| "-".to_owned(), |l| l.to_string()))
            .collect();
        // Both others are on sheet 2, in zones of its frame.
        assert!(
            places.iter().all(|place| place.starts_with("2/")),
            "{places:?}"
        );
        assert_eq!(first.len(), 2);
        assert_eq!(references[&doc.symbols[2].id].len(), 2);
    }

    #[test]
    fn a_lone_arrow_has_no_references_and_a_drawing_without_arrows_has_nothing() {
        let doc = doc(&[("A", 0), ("B", 50)]);

        assert!(signal_references(&doc, &arrow_catalog()).is_empty());
        assert!(signal_arrows(&Schematic::new("none"), &arrow_catalog()).is_empty());
    }

    #[test]
    fn links_go_from_an_arrow_to_the_others_of_its_name() {
        let doc = doc(&[("FEED", 0), ("RUN", 5), ("FEED", 50)]);
        let catalog = arrow_catalog();

        let from_first = crate::xref::links(&doc, &catalog, doc.symbols[0].id);
        let from_run = crate::xref::links(&doc, &catalog, doc.symbols[1].id);

        assert_eq!(from_first.len(), 1);
        assert_eq!(from_first[0].symbol, doc.symbols[2].id);
        assert!(
            from_first[0].text.starts_with("FEED 2/"),
            "{}",
            from_first[0].text
        );
        assert!(from_run.is_empty());
    }

    #[test]
    fn the_result_does_not_depend_on_the_order_of_the_collections() {
        let mut doc = doc(&[("FEED", 0), ("FEED", 50), ("RUN", 5)]);
        let catalog = arrow_catalog();
        let expected = (
            signal_arrows(&doc, &catalog),
            lone_arrows(&doc, &catalog),
            signal_references(&doc, &catalog),
        );

        doc.devices.reverse();
        doc.symbols.reverse();

        assert_eq!(
            (
                signal_arrows(&doc, &catalog),
                lone_arrows(&doc, &catalog),
                signal_references(&doc, &catalog),
            ),
            expected
        );
    }
}
