//! Renumbering of reference designators (ADR-0014).
//!
//! After a drawing has been edited for a while its designators are out of
//! order: `K7` is above `K3`. [`renumber`] gives every device the next free
//! number of its own prefix, in reading order, as one transaction and so one
//! undo.
//!
//! The order is that of the pages: the first symbol of a device places it by
//! sheet, then zone row, zone column, `y` and `x` (`circuit_model::reading_key`);
//! a device with no symbol comes last, by ID. Only the number changes: `KM9`
//! becomes `KM1`, never `K1`. A designator that is not letters followed by
//! digits (`K1_A`, `Q`) is left as it is and its text stays taken, so the
//! result never has a duplicate and never depends on the order of the
//! collections of the document.

use std::collections::{BTreeMap, BTreeSet};

use circuit_model::{DeviceId, ReferenceDesignator, Schematic, reading_key};

use crate::{CommandError, Edit, Transaction};

/// Which devices [`renumber`] touches.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Scope {
    /// Every device of the document.
    All,
    /// The devices whose first symbol is on the sheet with this number. The
    /// numbers they get skip the designators of every other device.
    Sheet(u32),
}

/// A designator split into its letters and its number: `KM12` is `KM` and 12.
/// `None` when it is not letters (or underscores) and then digits only.
fn split(reference: &str) -> Option<&str> {
    let digits_at = reference.find(|c: char| c.is_ascii_digit())?;
    let prefix = reference.get(..digits_at)?;
    let number = reference.get(digits_at..)?;
    let letters = !prefix.is_empty() && prefix.bytes().all(|b| b.is_ascii_uppercase() || b == b'_');
    (letters && number.bytes().all(|b| b.is_ascii_digit())).then_some(prefix)
}

/// Where a device's first symbol is in reading order, and that symbol's ID.
type Place = (circuit_model::ReadingKey, u64);

/// Renumber the designators of the devices in `scope`.
///
/// An empty transaction means nothing needs to change.
///
/// # Errors
///
/// Returns [`CommandError::InvalidSheet`] for a sheet number the document does
/// not have, and [`CommandError::InvalidReference`] when a prefix is so long
/// that the new designator would not be valid.
pub fn renumber(doc: &Schematic, scope: Scope) -> Result<Transaction, CommandError> {
    let missing_sheet = match scope {
        Scope::Sheet(number) => !doc.sheets.iter().any(|sheet| sheet.number == number),
        Scope::All => false,
    };
    if let (true, Scope::Sheet(number)) = (missing_sheet, scope) {
        return Err(CommandError::InvalidSheet(format!(
            "there is no sheet number {number}"
        )));
    }

    // Where each device is: its first symbol in reading order.
    let mut place: BTreeMap<DeviceId, Place> = BTreeMap::new();
    for symbol in &doc.symbols {
        let key = (reading_key(doc, symbol.position), symbol.id.0);
        place
            .entry(symbol.device_id)
            .and_modify(|best| *best = (*best).min(key))
            .or_insert(key);
    }
    let never: Place = ((u32::MAX, u8::MAX, u8::MAX, i32::MAX, i32::MAX), u64::MAX);

    let mut used: BTreeSet<&str> = BTreeSet::new();
    let mut moving: Vec<_> = Vec::new();
    for device in &doc.devices {
        let first = place.get(&device.id).copied().unwrap_or(never);
        let in_scope = match scope {
            Scope::All => true,
            Scope::Sheet(number) => first.0.0 == number,
        };
        match split(&device.reference.0) {
            Some(prefix) if in_scope => moving.push((first, device.id, prefix, device)),
            _ => {
                used.insert(device.reference.0.as_str());
            }
        }
    }
    moving.sort_by_key(|(first, id, _, _)| (*first, *id));

    let mut next: BTreeMap<&str, u64> = BTreeMap::new();
    let mut tx = Transaction::new("Renumber designators", doc.next_id);
    let mut taken: BTreeSet<String> = BTreeSet::new();
    for (_, _, prefix, device) in moving {
        let counter = next.entry(prefix).or_insert(1);
        let reference = loop {
            let candidate = format!("{prefix}{counter}");
            *counter = counter.saturating_add(1);
            if !used.contains(candidate.as_str()) && !taken.contains(&candidate) {
                break candidate;
            }
        };
        let designator = ReferenceDesignator(reference.clone());
        if !designator.is_valid() {
            return Err(CommandError::InvalidReference(reference));
        }
        taken.insert(reference);
        if designator != device.reference {
            tx.edits.push(Edit::SetReference {
                device: device.id,
                reference: designator,
            });
        }
    }
    Ok(tx)
}

#[cfg(test)]
mod tests {
    use circuit_model::{GridPoint, SchematicBuilder};

    use super::*;

    /// Devices `(reference, x, y)`, each a pushbutton with one symbol.
    fn drawing(devices: &[(&str, i32, i32)]) -> Schematic {
        let mut b = SchematicBuilder::new("renumber");
        for (reference, x, y) in devices {
            let device = b.device(reference, "pushbutton_no", &[]).unwrap();
            b.symbol(device, "contact", None, GridPoint::new(*x, *y))
                .unwrap();
        }
        b.finish()
    }

    fn references(doc: &Schematic) -> Vec<&str> {
        let mut references: Vec<&str> =
            doc.devices.iter().map(|d| d.reference.0.as_str()).collect();
        references.sort_unstable();
        references
    }

    fn apply(doc: &mut Schematic, tx: Transaction) {
        crate::History::new().execute(doc, tx).unwrap();
    }

    #[test]
    fn devices_take_the_next_number_in_reading_order() {
        let mut doc = drawing(&[("S7", 0, 20), ("S3", 0, 10), ("S5", 10, 10)]);

        let tx = renumber(&doc, Scope::All).unwrap();
        apply(&mut doc, tx);

        let of = |x, y| {
            let symbol = doc
                .symbols
                .iter()
                .find(|s| s.position == GridPoint::new(x, y));
            let device = symbol.and_then(|s| doc.devices.iter().find(|d| d.id == s.device_id));
            device.map(|d| d.reference.0.clone())
        };
        assert_eq!(of(0, 10).as_deref(), Some("S1"));
        assert_eq!(of(10, 10).as_deref(), Some("S2"));
        assert_eq!(of(0, 20).as_deref(), Some("S3"));
    }

    #[test]
    fn the_letters_of_a_designator_stay_and_each_prefix_counts_alone() {
        let mut doc = drawing(&[("KM9", 0, 0), ("K4", 0, 10), ("KM2", 0, 20), ("S8", 0, 30)]);

        let tx = renumber(&doc, Scope::All).unwrap();
        apply(&mut doc, tx);

        assert_eq!(references(&doc), ["K1", "KM1", "KM2", "S1"]);
    }

    #[test]
    fn renumbering_twice_changes_nothing_the_second_time() {
        let mut doc = drawing(&[("S7", 0, 20), ("S3", 0, 10), ("S5", 10, 10)]);
        let tx = renumber(&doc, Scope::All).unwrap();
        apply(&mut doc, tx);

        assert!(renumber(&doc, Scope::All).unwrap().is_empty());
    }

    #[test]
    fn a_swap_is_one_transaction_and_one_undo() {
        let mut doc = drawing(&[("S2", 0, 0), ("S1", 0, 10)]);
        let original = doc.clone();
        let mut history = crate::History::new();

        let tx = renumber(&doc, Scope::All).unwrap();
        history.execute(&mut doc, tx).unwrap();
        assert_eq!(doc.devices[0].reference.0, "S1");
        assert_eq!(doc.devices[1].reference.0, "S2");

        history.undo(&mut doc).unwrap();
        assert_eq!(doc, original);
    }

    #[test]
    fn a_designator_that_is_not_letters_and_digits_is_left_alone_and_stays_taken() {
        let mut doc = drawing(&[("K1_A", 0, 0), ("K9", 0, 10), ("Q", 0, 20)]);

        let tx = renumber(&doc, Scope::All).unwrap();
        apply(&mut doc, tx);

        assert_eq!(references(&doc), ["K1", "K1_A", "Q"]);
    }

    #[test]
    fn a_sheet_skips_the_numbers_of_the_others_and_a_missing_sheet_is_refused() {
        let mut doc = drawing(&[("K1", 0, 0), ("K2", 5, 0), ("K9", 50, 0), ("K8", 60, 0)]);
        for (id, number, x) in [(900, 1, -5), (901, 2, 45)] {
            doc.sheets.push(circuit_model::Sheet {
                id: circuit_model::SheetId(id),
                number,
                title: String::new(),
                origin: GridPoint::new(x, -5),
                size: circuit_model::SheetSize::Custom {
                    width: 40,
                    height: 20,
                },
                orientation: circuit_model::Orientation::Landscape,
                zones: circuit_model::Zones {
                    columns: 4,
                    rows: 2,
                },
            });
        }

        let tx = renumber(&doc, Scope::Sheet(2)).unwrap();
        apply(&mut doc, tx);

        // Sheet 2 holds K9 (x = 50) and K8 (x = 60): they become K3 and K4.
        assert_eq!(references(&doc), ["K1", "K2", "K3", "K4"]);
        assert!(matches!(
            renumber(&doc, Scope::Sheet(7)),
            Err(CommandError::InvalidSheet(_))
        ));
    }

    #[test]
    fn a_device_with_no_symbol_comes_last() {
        let mut doc = drawing(&[("S5", 0, 0)]);
        let loose = circuit_model::DeviceId(doc.next_id);
        doc.next_id += 1;
        doc.devices.push(circuit_model::Device {
            id: loose,
            reference: ReferenceDesignator("S1".to_owned()),
            model: circuit_model::ModelRef {
                id: "pushbutton_no".to_owned(),
                version: 1,
            },
            params: std::collections::BTreeMap::new(),
            info: None,
        });

        let tx = renumber(&doc, Scope::All).unwrap();
        apply(&mut doc, tx);

        let reference_of = |id| {
            doc.devices
                .iter()
                .find(|d| d.id == id)
                .map(|d| d.reference.0.as_str())
        };
        assert_eq!(reference_of(doc.devices[0].id), Some("S1"));
        assert_eq!(reference_of(loose), Some("S2"));
    }

    #[test]
    fn a_number_that_would_make_the_designator_too_long_is_refused_not_cut() {
        let long: Vec<(String, i32, i32)> = (0..10)
            .map(|digit| (format!("ABCDEFGHIJKLMNO{digit}"), 0, digit * 10))
            .collect();
        let refs: Vec<(&str, i32, i32)> =
            long.iter().map(|(r, x, y)| (r.as_str(), *x, *y)).collect();
        let doc = drawing(&refs);

        // The tenth device would be `ABCDEFGHIJKLMNO10`, 17 characters.
        assert!(matches!(
            renumber(&doc, Scope::All),
            Err(CommandError::InvalidReference(_))
        ));
    }

    mod properties {
        use proptest::prelude::*;

        use super::*;

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(64))]

            #[test]
            fn the_result_has_no_duplicate_and_does_not_depend_on_the_order(
                numbers in prop::collection::vec((0_u8..4, 0_u32..40, -3_i32..3, -3_i32..3), 1..12)
            ) {
                let prefixes = ["K", "S", "KM", "H"];
                let devices: Vec<(String, i32, i32)> = numbers
                    .iter()
                    .enumerate()
                    .map(|(index, (prefix, number, x, y))| {
                        (
                            format!("{}{}", prefixes[usize::from(*prefix)], number * 20 + u32::try_from(index).unwrap()),
                            x * 10,
                            y * 10 + i32::try_from(index).unwrap() * 100,
                        )
                    })
                    .collect();
                let refs: Vec<(&str, i32, i32)> =
                    devices.iter().map(|(r, x, y)| (r.as_str(), *x, *y)).collect();
                let mut doc = drawing(&refs);
                let mut shuffled = doc.clone();
                shuffled.devices.reverse();
                shuffled.symbols.reverse();

                let first = renumber(&doc, Scope::All).unwrap();
                let second = renumber(&shuffled, Scope::All).unwrap();
                apply(&mut doc, first);
                apply(&mut shuffled, second);

                let mut all: Vec<&str> = references(&doc);
                all.dedup();
                prop_assert_eq!(all.len(), doc.devices.len());
                prop_assert_eq!(references(&doc), references(&shuffled));
                prop_assert!(renumber(&doc, Scope::All).unwrap().is_empty());
            }
        }
    }
}
