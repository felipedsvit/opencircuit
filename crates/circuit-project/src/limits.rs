//! Size limits for project files.
//!
//! A project file may come from anyone. The limits keep a hostile file from
//! turning into unbounded memory or work; they are far above any schematic a
//! person draws. The values are provisional and will be revisited with the
//! first benchmarks.

use circuit_model::{ParamValue, Schematic};

use crate::ProjectError;

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

fn count(what: &'static str, len: usize, limit: usize) -> Result<(), ProjectError> {
    if len > limit {
        Err(ProjectError::LimitExceeded { what, limit })
    } else {
        Ok(())
    }
}

fn text(what: &'static str, value: &str) -> Result<(), ProjectError> {
    count(what, value.len(), MAX_TEXT_BYTES)
}

/// Check a document against every limit.
pub(crate) fn check(doc: &Schematic) -> Result<(), ProjectError> {
    count("supplies", doc.supplies.len(), MAX_ENTITIES_PER_COLLECTION)?;
    count("devices", doc.devices.len(), MAX_ENTITIES_PER_COLLECTION)?;
    count("symbols", doc.symbols.len(), MAX_ENTITIES_PER_COLLECTION)?;
    count(
        "junctions",
        doc.junctions.len(),
        MAX_ENTITIES_PER_COLLECTION,
    )?;
    count("wires", doc.wires.len(), MAX_ENTITIES_PER_COLLECTION)?;

    text("project name", &doc.project.name)?;
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
    }
    for symbol in &doc.symbols {
        text("symbol key", &symbol.symbol_key.0)?;
    }
    for wire in &doc.wires {
        count("wire route points", wire.route.len(), MAX_ROUTE_POINTS)?;
        for endpoint in [&wire.from, &wire.to] {
            if let circuit_model::Endpoint::Terminal(pin) = endpoint {
                text("terminal key", &pin.terminal.0)?;
            }
        }
    }
    Ok(())
}
