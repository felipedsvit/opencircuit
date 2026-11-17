//! The terminals of the terminal strips of a drawing (ADR-0016).
//!
//! A terminal of a strip is a device of the class
//! [`DeviceClass::TerminalBlock`] whose
//! parameters `strip` and `number` say where it is on the panel. The list is
//! derived from the document and its catalog, ordered by strip (naturally, so
//! `X2` comes before `X10`) and by number, and does not depend on the order of
//! the collections.

use std::collections::BTreeMap;

use crate::catalog::{ComponentCatalog, DeviceClass};
use crate::diagnostics::{Diagnostic, DiagnosticCode, Subject};
use crate::ids::{DeviceId, SymbolId};
use crate::labels::natural_key;
use crate::schematic::Schematic;

/// The parameter that names the strip of a terminal.
pub const PARAM_STRIP: &str = "strip";
/// The parameter that numbers a terminal on its strip.
pub const PARAM_NUMBER: &str = "number";

/// One terminal of a strip.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StripTerminal {
    /// The device.
    pub device: DeviceId,
    /// Its first symbol, in symbol-ID order; `None` when it has no symbol.
    pub symbol: Option<SymbolId>,
    /// The strip, such as `X1`.
    pub strip: String,
    /// The number on the strip.
    pub number: i64,
}

/// Every terminal of every strip, by strip and number, then by device.
#[must_use]
pub fn strip_terminals(doc: &Schematic, catalog: &ComponentCatalog) -> Vec<StripTerminal> {
    let mut first_symbol: BTreeMap<DeviceId, SymbolId> = BTreeMap::new();
    for symbol in &doc.symbols {
        first_symbol
            .entry(symbol.device_id)
            .and_modify(|best| *best = (*best).min(symbol.id))
            .or_insert(symbol.id);
    }
    let mut found: Vec<StripTerminal> = doc
        .devices
        .iter()
        .filter_map(|device| {
            let model = catalog.model(&device.model)?;
            if model.class() != DeviceClass::TerminalBlock {
                return None;
            }
            Some(StripTerminal {
                device: device.id,
                symbol: first_symbol.get(&device.id).copied(),
                strip: model.text_param(&device.params, PARAM_STRIP)?.to_owned(),
                number: model.int_param(&device.params, PARAM_NUMBER)?,
            })
        })
        .collect();
    found.sort_by(|a, b| {
        (natural_key(&a.strip), a.number, a.device).cmp(&(
            natural_key(&b.strip),
            b.number,
            b.device,
        ))
    });
    found
}

/// `OC-SEM-013`: a strip and a number that more than one device has. The
/// subject is the device with the smallest ID and `related` the others.
#[must_use]
pub fn duplicate_terminals(doc: &Schematic, catalog: &ComponentCatalog) -> Vec<Diagnostic> {
    let mut by_place: BTreeMap<(String, i64), Vec<DeviceId>> = BTreeMap::new();
    for terminal in strip_terminals(doc, catalog) {
        by_place
            .entry((terminal.strip, terminal.number))
            .or_default()
            .push(terminal.device);
    }
    by_place
        .into_iter()
        .filter(|(_, devices)| devices.len() > 1)
        .filter_map(|((strip, number), mut devices)| {
            devices.sort_unstable();
            let (first, others) = devices.split_first()?;
            Some(
                Diagnostic::new(
                    DiagnosticCode::DuplicateTerminal,
                    Subject::Device(*first),
                    format!("terminal {strip}:{number} is given to more than one device"),
                )
                .with_related(others.iter().copied().map(Subject::Device).collect()),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::SchematicBuilder;
    use crate::geometry::GridPoint;
    use crate::schematic::ParamValue;

    /// Terminals `(strip, number)`, each a device with one symbol; the model
    /// is a test model with the class of a terminal block.
    fn catalog_and_doc(terminals: &[(&str, i64)]) -> (ComponentCatalog, Schematic) {
        let mut b = SchematicBuilder::new("strips");
        for (index, (strip, number)) in terminals.iter().enumerate() {
            let device = b
                .device(
                    &format!("XT{}", index + 1),
                    "std.terminal_block",
                    &[
                        (PARAM_STRIP, ParamValue::Text((*strip).to_owned())),
                        (PARAM_NUMBER, ParamValue::Int(*number)),
                    ],
                )
                .unwrap();
            b.symbol(
                device,
                "terminal",
                None,
                GridPoint::new(i32::try_from(index).unwrap() * 10, 0),
            )
            .unwrap();
        }
        (strip_catalog(), b.finish())
    }

    /// The built-in catalog plus the one model the tests need.
    fn strip_catalog() -> ComponentCatalog {
        use crate::catalog::{
            Category, ComponentModel, ParamDefinition, ParamType, Provenance, SymbolDefinition,
            SymbolKind,
        };
        use crate::library::Library;
        let model = ComponentModel::new(
            "std.terminal_block",
            1,
            DeviceClass::TerminalBlock,
            vec![
                ParamDefinition {
                    key: PARAM_STRIP.to_owned(),
                    ty: ParamType::Name,
                    default: Some(ParamValue::Text("X1".to_owned())),
                },
                ParamDefinition {
                    key: PARAM_NUMBER.to_owned(),
                    ty: ParamType::Int { min: 1, max: 9_999 },
                    default: Some(ParamValue::Int(1)),
                },
            ],
            vec![SymbolDefinition::new(
                "terminal",
                SymbolKind::TerminalPoint,
                1,
                1,
                false,
            )],
            Provenance {
                origin: "test".to_owned(),
                license: "MIT".to_owned(),
                standard_reference: None,
            },
        )
        .with_category(Category::Lines);
        let library = Library {
            id: "std".to_owned(),
            version: 1,
            name: "test".to_owned(),
            provenance: model_provenance(),
            models: vec![model],
        };
        ComponentCatalog::builtin().with_library(library).unwrap()
    }

    fn model_provenance() -> crate::catalog::Provenance {
        crate::catalog::Provenance {
            origin: "test".to_owned(),
            license: "MIT".to_owned(),
            standard_reference: None,
        }
    }

    #[test]
    fn strips_are_ordered_naturally_then_by_number() {
        let (catalog, doc) = catalog_and_doc(&[("X10", 1), ("X2", 5), ("X2", 3), ("X1", 9)]);

        let order: Vec<(String, i64)> = strip_terminals(&doc, &catalog)
            .into_iter()
            .map(|t| (t.strip, t.number))
            .collect();

        assert_eq!(
            order,
            [
                ("X1".to_owned(), 9),
                ("X2".to_owned(), 3),
                ("X2".to_owned(), 5),
                ("X10".to_owned(), 1)
            ]
        );
    }

    #[test]
    fn the_same_strip_and_number_twice_is_one_warning_on_the_first_device() {
        let (catalog, doc) = catalog_and_doc(&[("X1", 1), ("X1", 2), ("X1", 1), ("X2", 1)]);

        let found = duplicate_terminals(&doc, &catalog);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].code(), DiagnosticCode::DuplicateTerminal);
        assert_eq!(found[0].subject(), &Subject::Device(doc.devices[0].id));
        assert_eq!(found[0].related(), [Subject::Device(doc.devices[2].id)]);
    }

    #[test]
    fn the_result_does_not_depend_on_the_order_of_the_collections() {
        let (catalog, mut doc) = catalog_and_doc(&[("X1", 1), ("X1", 1), ("X1", 2)]);
        let expected = (
            strip_terminals(&doc, &catalog),
            duplicate_terminals(&doc, &catalog),
        );

        doc.devices.reverse();
        doc.symbols.reverse();

        assert_eq!(
            (
                strip_terminals(&doc, &catalog),
                duplicate_terminals(&doc, &catalog)
            ),
            expected
        );
    }

    #[test]
    fn a_document_without_terminals_has_none() {
        let doc = Schematic::new("none");

        assert!(strip_terminals(&doc, &ComponentCatalog::builtin()).is_empty());
        assert!(duplicate_terminals(&doc, &ComponentCatalog::builtin()).is_empty());
    }
}
