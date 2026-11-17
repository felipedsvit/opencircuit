//! What a person decides about the documentation of a drawing (ADR-0014).
//!
//! Wire numbers, reports and the list of nets are derived from the document
//! and never stored. What cannot be derived is stored here: the catalog data
//! of a physical device, the rule that numbers the wires, and the reason a
//! warning was accepted. An override of one wire number is `Wire::number`.

use std::num::NonZeroU32;

use serde::{Deserialize, Serialize};

use std::collections::BTreeMap;

use crate::diagnostics::{Diagnostic, DiagnosticCode, Severity, Subject};
use crate::geometry::GridPoint;
use crate::ids::TextId;
use crate::schematic::Schematic;

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

/// Most characters of a text note (ADR-0015).
pub const MAX_NOTE_CHARS: usize = 120;

/// Where a text note sits on its anchor point.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAnchor {
    /// The text starts at the point.
    #[default]
    Left,
    /// The text is centred on the point.
    Center,
}

/// A line of text on the drawing: a title, a remark, a label (ADR-0015).
///
/// It is documentation only: it has no electrical meaning, joins nothing and
/// is not in any report. The text is one line of at most [`MAX_NOTE_CHARS`]
/// printable characters and 256 bytes.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TextNote {
    /// Stable identifier, from the document counter.
    pub id: TextId,
    /// The anchor point.
    pub position: GridPoint,
    /// The text.
    pub text: String,
    /// Where the text sits on the point.
    pub anchor: TextAnchor,
}

impl TextNote {
    /// Whether `text` is acceptable: not empty, at most [`MAX_NOTE_CHARS`]
    /// characters and at most [`crate::limits::MAX_TEXT_BYTES`] bytes (the
    /// bound of every text of a project file), and no control character.
    #[must_use]
    pub fn is_valid_text(text: &str) -> bool {
        !text.is_empty()
            && text.chars().count() <= MAX_NOTE_CHARS
            && text.len() <= crate::limits::MAX_TEXT_BYTES
            && !text.chars().any(char::is_control)
    }
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

/// The waivers of a document, to ask whether a diagnostic is waived.
///
/// Only a warning or an informational diagnostic can be waived: an error
/// never is, and neither is [`DiagnosticCode::InvalidWaiver`], which is how a
/// bad waiver is reported.
#[derive(Clone, Debug, Default)]
pub struct WaiverIndex<'a> {
    by_key: BTreeMap<(&'a str, &'a Subject), &'a Waiver>,
}

impl<'a> WaiverIndex<'a> {
    /// Index the waivers of a document. When a code and a subject have more
    /// than one waiver (a structural warning), the first in the list wins.
    #[must_use]
    pub fn new(doc: &'a Schematic) -> Self {
        let mut by_key = BTreeMap::new();
        for waiver in &doc.waivers {
            by_key
                .entry((waiver.code.as_str(), &waiver.subject))
                .or_insert(waiver);
        }
        Self { by_key }
    }

    /// The waiver that covers this diagnostic, if there is one and the
    /// diagnostic may be waived.
    #[must_use]
    pub fn of(&self, diagnostic: &Diagnostic) -> Option<&'a Waiver> {
        if diagnostic.severity() == Severity::Error
            || diagnostic.code() == DiagnosticCode::InvalidWaiver
        {
            return None;
        }
        self.by_key
            .get(&(diagnostic.code().as_str(), diagnostic.subject()))
            .copied()
    }

    /// Whether the document has no waiver.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::WireId;

    #[test]
    fn a_text_is_one_to_120_characters_without_control_characters() {
        let valid = |text: &str| TextNote::is_valid_text(text);

        assert!(valid("Motor 1"));
        assert!(valid(&"x".repeat(MAX_NOTE_CHARS)));
        assert!(valid("Ação de partida"));
        assert!(!valid(""));
        assert!(!valid(&"x".repeat(MAX_NOTE_CHARS + 1)));
        assert!(!valid("two\nlines"));
        assert!(!valid("tab\there"));
        // Few characters, too many bytes for any text of a project file.
        assert!(!valid(&"€".repeat(MAX_NOTE_CHARS)));
    }

    #[test]
    fn device_info_is_empty_until_a_field_is_set() {
        let mut info = DeviceInfo::default();
        assert!(info.is_empty());

        info.location = Some("Panel 1".to_owned());
        assert!(!info.is_empty());
    }

    fn finding(code: DiagnosticCode, subject: Subject) -> Diagnostic {
        Diagnostic::new(code, subject, "test")
    }

    fn doc_with(code: &str, subject: Subject) -> Schematic {
        let mut doc = Schematic::new("waivers");
        doc.waivers.push(Waiver {
            code: code.to_owned(),
            subject,
            reason: "checked".to_owned(),
        });
        doc
    }

    #[test]
    fn a_waiver_covers_the_diagnostic_of_its_code_and_subject_and_no_other() {
        let doc = doc_with("OC-STR-018", Subject::Wire(WireId(5)));
        let index = WaiverIndex::new(&doc);

        let covered = finding(DiagnosticCode::LooseConnection, Subject::Wire(WireId(5)));
        assert_eq!(
            index.of(&covered).map(|w| w.reason.as_str()),
            Some("checked")
        );
        assert!(
            index
                .of(&finding(
                    DiagnosticCode::LooseConnection,
                    Subject::Wire(WireId(6))
                ))
                .is_none()
        );
        assert!(
            index
                .of(&finding(
                    DiagnosticCode::OverlappingWires,
                    Subject::Wire(WireId(5))
                ))
                .is_none()
        );
    }

    #[test]
    fn an_error_and_an_invalid_waiver_are_never_waived() {
        let doc = doc_with("OC-STR-013", Subject::Wire(WireId(5)));
        let index = WaiverIndex::new(&doc);
        assert!(
            index
                .of(&finding(
                    DiagnosticCode::SelfLoopWire,
                    Subject::Wire(WireId(5))
                ))
                .is_none()
        );

        let doc = doc_with("OC-STR-020", Subject::Wire(WireId(5)));
        assert!(
            WaiverIndex::new(&doc)
                .of(&finding(
                    DiagnosticCode::InvalidWaiver,
                    Subject::Wire(WireId(5))
                ))
                .is_none()
        );
    }

    #[test]
    fn a_document_with_no_waiver_has_an_empty_index() {
        assert!(WaiverIndex::new(&Schematic::new("none")).is_empty());
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
