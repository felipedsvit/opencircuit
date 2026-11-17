//! What a person decides about the documentation of a drawing (ADR-0014).
//!
//! Wire numbers, reports and the list of nets are derived from the document
//! and never stored. What cannot be derived is stored here: the catalog data
//! of a physical device, the rule that numbers the wires, and the reason a
//! warning was accepted. An override of one wire number is `Wire::number`.

use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};

use crate::diagnostics::Subject;

/// Longest prefix of a wire number, in bytes.
pub const MAX_WIRE_NUMBER_PREFIX_BYTES: usize = 16;

/// Catalog data of the physical device, for bills of materials.
///
/// It describes the equipment on the shelf, not the behaviour of its model.
/// Every field is plain text and optional; a field that is empty is `None`.
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceInfo {
    /// Maker of the device.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manufacturer: Option<String>,
    /// The maker's part or order number.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_no: Option<String>,
    /// What the device is, in the author's words.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Where the device is installed, such as `Panel 1` or `Field`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
}

impl DeviceInfo {
    /// Whether no field is set.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.manufacturer.is_none()
            && self.part_no.is_none()
            && self.description.is_none()
            && self.location.is_none()
    }
}

/// The rule that numbers the wires of a project.
///
/// It is stored in the project so that the editor, the exports and the
/// command line draw the same numbers. A number is the prefix followed by a
/// decimal counter that starts at `start` and grows by `step` for each net.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WireNumbering {
    /// Text before the counter; may be empty.
    pub prefix: String,
    /// The first number.
    pub start: u32,
    /// The distance between two numbers; never zero.
    pub step: NonZeroU32,
    /// Whether the counter starts again on every sheet, and the number is
    /// written `sheet.counter`.
    pub per_sheet: bool,
}

/// A warning or informational diagnostic that a person accepted, with the
/// reason.
///
/// Waiving hides nothing from the compiler: the diagnostic is still
/// produced, and whoever shows it marks it as waived. Errors cannot be
/// waived.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Waiver {
    /// The diagnostic code, such as `OC-STR-018`.
    pub code: String,
    /// The element the diagnostic is about.
    pub subject: Subject,
    /// Why it is accepted.
    pub reason: String,
}

impl Waiver {
    /// The order of waivers in a saved file: by code, then by subject.
    #[must_use]
    pub fn sort_key(&self) -> (&str, &Subject) {
        (&self.code, &self.subject)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::WireId;

    #[test]
    fn device_info_is_empty_until_a_field_is_set() {
        let mut info = DeviceInfo::default();
        assert!(info.is_empty());

        info.location = Some("Panel 1".to_owned());
        assert!(!info.is_empty());
    }

    #[test]
    fn waivers_sort_by_code_then_subject() {
        let waiver = |code: &str, wire: u64| Waiver {
            code: code.to_owned(),
            subject: Subject::Wire(WireId(wire)),
            reason: String::new(),
        };
        let mut waivers = [
            waiver("OC-STR-019", 1),
            waiver("OC-STR-018", 9),
            waiver("OC-STR-018", 2),
        ];

        waivers.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));

        let order: Vec<_> = waivers
            .iter()
            .map(|w| (w.code.as_str(), &w.subject))
            .collect();
        assert_eq!(
            order,
            [
                ("OC-STR-018", &Subject::Wire(WireId(2))),
                ("OC-STR-018", &Subject::Wire(WireId(9))),
                ("OC-STR-019", &Subject::Wire(WireId(1))),
            ]
        );
    }
}
