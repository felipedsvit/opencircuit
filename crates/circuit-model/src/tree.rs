//! The devices of a project, grouped by sheet (ADR-0014).
//!
//! A device is on the sheet that holds its first symbol in reading order
//! (`reading_key`); a drawing with no sheet has one group of devices. The
//! tree is derived, never stored, and does not depend on the order of the
//! collections of the document.

use std::collections::BTreeMap;

use crate::ids::{DeviceId, SheetId, SymbolId};
use crate::schematic::Schematic;
use crate::sheet::{ReadingKey, reading_key, sheet_at};

/// One device of the tree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreeDevice {
    /// The device.
    pub device: DeviceId,
    /// Its first symbol in reading order, to select and show it; `None` for a
    /// device with no symbol.
    pub symbol: Option<SymbolId>,
    /// Its designator.
    pub reference: String,
}

/// A sheet, or what is not on one, with its devices.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TreeGroup {
    /// The sheet, for a group that is a sheet.
    pub sheet: Option<SheetId>,
    /// What to call the group: `Sheet 2 · Power`, `Devices`, `Outside the
    /// sheets` or `Without symbols`.
    pub title: String,
    /// The devices, in reading order.
    pub devices: Vec<TreeDevice>,
}

/// The groups of a document: its sheets by number, then what is outside
/// them, then the devices that have no symbol. A group with no device is
/// left out, except a sheet, which is always listed.
#[must_use]
pub fn project_tree(doc: &Schematic) -> Vec<TreeGroup> {
    // The first symbol of each device, by reading order.
    let mut first: BTreeMap<DeviceId, (ReadingKey, SymbolId)> = BTreeMap::new();
    for symbol in &doc.symbols {
        let key = (reading_key(doc, symbol.position), symbol.id);
        first
            .entry(symbol.device_id)
            .and_modify(|best| *best = (*best).min(key))
            .or_insert(key);
    }

    let mut placed: Vec<_> = Vec::new();
    let mut loose: Vec<&crate::schematic::Device> = Vec::new();
    for device in &doc.devices {
        match first.get(&device.id) {
            Some((key, symbol)) => placed.push((*key, device.id, *symbol, device)),
            None => loose.push(device),
        }
    }
    placed.sort_by_key(|(key, id, _, _)| (*key, *id));
    loose.sort_by_key(|device| device.id);

    let entry = |device: &crate::schematic::Device, symbol: Option<SymbolId>| TreeDevice {
        device: device.id,
        symbol,
        reference: device.reference.0.clone(),
    };

    let mut sheets: Vec<_> = doc.sheets.iter().collect();
    sheets.sort_by_key(|sheet| (sheet.number, sheet.id));
    let mut groups: Vec<TreeGroup> = sheets
        .iter()
        .map(|sheet| TreeGroup {
            sheet: Some(sheet.id),
            title: if sheet.title.is_empty() {
                format!("Sheet {}", sheet.number)
            } else {
                format!("Sheet {} · {}", sheet.number, sheet.title)
            },
            devices: Vec::new(),
        })
        .collect();
    let mut outside = TreeGroup {
        sheet: None,
        title: if doc.sheets.is_empty() {
            "Devices".to_owned()
        } else {
            "Outside the sheets".to_owned()
        },
        devices: Vec::new(),
    };
    for (_, _, symbol, device) in &placed {
        let position = doc
            .symbols
            .iter()
            .find(|candidate| candidate.id == *symbol)
            .map(|candidate| candidate.position);
        let sheet = position.and_then(|point| sheet_at(doc, point));
        let group = sheet.and_then(|sheet| {
            groups
                .iter_mut()
                .find(|group| group.sheet == Some(sheet.id))
        });
        match group {
            Some(group) => group.devices.push(entry(device, Some(*symbol))),
            None => outside.devices.push(entry(device, Some(*symbol))),
        }
    }
    if !outside.devices.is_empty() {
        groups.push(outside);
    }
    if !loose.is_empty() {
        groups.push(TreeGroup {
            sheet: None,
            title: "Without symbols".to_owned(),
            devices: loose.iter().map(|device| entry(device, None)).collect(),
        });
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::SchematicBuilder;
    use crate::geometry::GridPoint;
    use crate::sheet::{Orientation, Sheet, SheetSize, Zones};

    fn sheet(id: u64, number: u32, title: &str, x: i32) -> Sheet {
        Sheet {
            id: SheetId(id),
            number,
            title: title.to_owned(),
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
            kind: Default::default(),
        }
    }

    fn doc() -> Schematic {
        let mut b = SchematicBuilder::new("tree");
        for (reference, x, y) in [("S2", 5, 10), ("S1", 5, 0), ("S3", 55, 0), ("S4", 200, 0)] {
            let device = b.device(reference, "pushbutton_no", &[]).unwrap();
            b.symbol(device, "contact", None, GridPoint::new(x, y))
                .unwrap();
        }
        b.device("S5", "pushbutton_no", &[]).unwrap();
        b.finish()
    }

    fn summary(groups: &[TreeGroup]) -> Vec<(String, Vec<String>)> {
        groups
            .iter()
            .map(|g| {
                (
                    g.title.clone(),
                    g.devices.iter().map(|d| d.reference.clone()).collect(),
                )
            })
            .collect()
    }

    #[test]
    fn a_drawing_without_sheets_is_one_group_in_reading_order_by_row_then_column() {
        let groups = project_tree(&doc());

        assert_eq!(
            summary(&groups),
            [
                (
                    "Devices".to_owned(),
                    ["S1", "S3", "S4", "S2"].map(str::to_owned).to_vec()
                ),
                ("Without symbols".to_owned(), vec!["S5".to_owned()]),
            ]
        );
        assert!(groups[0].sheet.is_none());
        assert!(groups[0].devices.iter().all(|d| d.symbol.is_some()));
        assert!(groups[1].devices[0].symbol.is_none());
    }

    #[test]
    fn sheets_come_by_number_with_their_devices_then_what_is_outside() {
        let mut doc = doc();
        doc.sheets = vec![sheet(901, 2, "", 45), sheet(900, 1, "Power", -5)];

        let groups = project_tree(&doc);

        assert_eq!(
            summary(&groups),
            [
                (
                    "Sheet 1 · Power".to_owned(),
                    vec!["S1".to_owned(), "S2".to_owned()]
                ),
                ("Sheet 2".to_owned(), vec!["S3".to_owned()]),
                ("Outside the sheets".to_owned(), vec!["S4".to_owned()]),
                ("Without symbols".to_owned(), vec!["S5".to_owned()]),
            ]
        );
        assert_eq!(groups[0].sheet, Some(SheetId(900)));
    }

    #[test]
    fn an_empty_sheet_is_still_listed() {
        let mut doc = Schematic::new("empty");
        doc.sheets = vec![sheet(900, 1, "", 0)];

        assert_eq!(
            summary(&project_tree(&doc)),
            [("Sheet 1".to_owned(), vec![])]
        );
    }

    #[test]
    fn the_tree_does_not_depend_on_the_order_of_the_collections() {
        let mut doc = doc();
        doc.sheets = vec![sheet(900, 1, "Power", -5), sheet(901, 2, "", 45)];
        let expected = project_tree(&doc);

        doc.devices.reverse();
        doc.symbols.reverse();
        doc.sheets.reverse();

        assert_eq!(project_tree(&doc), expected);
    }
}
