//! Size limits of a document.
//!
//! A project file may come from anyone. The limits keep a hostile file from
//! turning into unbounded memory or work, and they are far above any
//! schematic a person draws. The loader refuses a document beyond them, and
//! the editor refuses an edit that would exceed them, so a document that was
//! edited can always be saved and read back.

use crate::documentation::MAX_WIRE_NUMBER_PREFIX_BYTES;
use crate::schematic::{Endpoint, ParamValue, Schematic};

/// Largest accepted project text, in bytes.
pub const MAX_PROJECT_BYTES: usize = 16 * 1024 * 1024;
/// Most entities in any one collection of the document.
pub const MAX_ENTITIES_PER_COLLECTION: usize = 100_000;
/// Most intermediate waypoints of one wire.
pub const MAX_ROUTE_POINTS: usize = 1_024;
/// Most parameters of one device.
pub const MAX_PARAMS_PER_DEVICE: usize = 64;
/// Longest text value (name, designator, key, text parameter), in bytes.
pub const MAX_TEXT_BYTES: usize = 256;
/// Most sheets in one document.
pub const MAX_SHEETS: usize = 1_000;
/// Most waivers in one document.
pub const MAX_WAIVERS: usize = 10_000;

/// A document beyond one of the limits.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("{what} exceeds the limit of {limit}")]
pub struct LimitExceeded {
    /// What is too large or too many.
    pub what: &'static str,
    /// The limit.
    pub limit: usize,
}

fn count(what: &'static str, len: usize, limit: usize) -> Result<(), LimitExceeded> {
    if len > limit {
        Err(LimitExceeded { what, limit })
    } else {
        Ok(())
    }
}

fn text(what: &'static str, value: &str) -> Result<(), LimitExceeded> {
    count(what, value.len(), MAX_TEXT_BYTES)
}

/// Check a document against every limit except the size of its text, which
/// only exists once it is serialized.
///
/// # Errors
///
/// Returns the first limit the document exceeds.
pub fn check(doc: &Schematic) -> Result<(), LimitExceeded> {
    count("sheets", doc.sheets.len(), MAX_SHEETS)?;
    count("supplies", doc.supplies.len(), MAX_ENTITIES_PER_COLLECTION)?;
    count("devices", doc.devices.len(), MAX_ENTITIES_PER_COLLECTION)?;
    count("symbols", doc.symbols.len(), MAX_ENTITIES_PER_COLLECTION)?;
    count(
        "junctions",
        doc.junctions.len(),
        MAX_ENTITIES_PER_COLLECTION,
    )?;
    count("wires", doc.wires.len(), MAX_ENTITIES_PER_COLLECTION)?;
    count("waivers", doc.waivers.len(), MAX_WAIVERS)?;

    text("project name", &doc.project.name)?;
    for (what, value) in [
        ("project author", &doc.project.author),
        ("project company", &doc.project.company),
        ("project revision", &doc.project.revision),
        ("project date", &doc.project.date),
    ] {
        if let Some(value) = value {
            text(what, value)?;
        }
    }
    if let Some(numbering) = &doc.project.wire_numbering {
        count(
            "wire number prefix",
            numbering.prefix.len(),
            MAX_WIRE_NUMBER_PREFIX_BYTES,
        )?;
    }
    for sheet in &doc.sheets {
        text("sheet title", &sheet.title)?;
    }
    for device in &doc.devices {
        text("reference designator", &device.reference.0)?;
        text("model id", &device.model.id)?;
        count(
            "device parameters",
            device.params.len(),
            MAX_PARAMS_PER_DEVICE,
        )?;
        for (key, value) in &device.params {
            text("parameter key", key)?;
            if let ParamValue::Text(value) = value {
                text("parameter value", value)?;
            }
        }
        if let Some(info) = &device.info {
            for (what, value) in [
                ("device manufacturer", &info.manufacturer),
                ("device part number", &info.part_no),
                ("device description", &info.description),
                ("device location", &info.location),
            ] {
                if let Some(value) = value {
                    text(what, value)?;
                }
            }
        }
    }
    for waiver in &doc.waivers {
        text("waiver code", &waiver.code)?;
        text("waiver reason", &waiver.reason)?;
    }
    for symbol in &doc.symbols {
        text("symbol key", &symbol.symbol_key.0)?;
    }
    for wire in &doc.wires {
        count("wire route points", wire.route.len(), MAX_ROUTE_POINTS)?;
        if let Some(number) = &wire.number {
            text("wire number", number)?;
        }
        for endpoint in [&wire.from, &wire.to] {
            if let Endpoint::Terminal(pin) = endpoint {
                text("terminal key", &pin.terminal.0)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schematic::Wire;
    use crate::{GridPoint, JunctionId, WireId};

    #[test]
    fn empty_document_is_within_every_limit() {
        assert_eq!(check(&Schematic::new("empty")), Ok(()));
    }

    #[test]
    fn too_many_sheets_and_long_title_block_texts_are_refused() {
        let mut doc = Schematic::new("sheets");
        doc.project.author = Some("a".repeat(MAX_TEXT_BYTES + 1));
        assert_eq!(
            check(&doc),
            Err(LimitExceeded {
                what: "project author",
                limit: MAX_TEXT_BYTES
            })
        );
        doc.project.author = None;
        let sheet = crate::Sheet {
            id: crate::SheetId(1),
            number: 1,
            title: String::new(),
            origin: GridPoint::new(0, 0),
            size: crate::SheetSize::A4,
            orientation: crate::Orientation::Landscape,
            zones: crate::Zones {
                columns: 1,
                rows: 1,
            },
        };
        doc.sheets = vec![sheet; MAX_SHEETS + 1];
        assert_eq!(
            check(&doc),
            Err(LimitExceeded {
                what: "sheets",
                limit: MAX_SHEETS
            })
        );
    }

    #[test]
    fn long_project_name_is_refused() {
        let doc = Schematic::new("x".repeat(MAX_TEXT_BYTES + 1));

        assert_eq!(
            check(&doc),
            Err(LimitExceeded {
                what: "project name",
                limit: MAX_TEXT_BYTES
            })
        );
    }

    #[test]
    fn wire_with_too_many_waypoints_is_refused() {
        let mut doc = Schematic::new("route");
        doc.wires.push(Wire {
            id: WireId(1),
            from: Endpoint::Junction(JunctionId(2)),
            to: Endpoint::Junction(JunctionId(3)),
            route: vec![GridPoint::default(); MAX_ROUTE_POINTS + 1],
            number: None,
        });

        assert_eq!(check(&doc).map_err(|e| e.what), Err("wire route points"));
    }
}
