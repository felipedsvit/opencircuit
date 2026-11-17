//! Editing the documentation data of a project (ADR-0014): the catalog data
//! of a device, the override of a wire number and the numbering rule.
//!
//! Text is trimmed and an empty text is no text, so the file never holds an
//! empty field. A change that changes nothing is an empty transaction.

use circuit_model::{DeviceId, DeviceInfo, Schematic, WireId, WireNumbering};

use crate::{CommandError, Edit, Transaction};

fn clean(text: Option<String>) -> Option<String> {
    text.map(|text| text.trim().to_owned())
        .filter(|text| !text.is_empty())
}

/// Set the catalog data of a device. Every field that is empty is cleared,
/// and a device with no field left has no data at all.
///
/// # Errors
///
/// Returns [`CommandError::Missing`] for a device that does not exist, and the
/// limit when a text is longer than the format allows.
pub fn set_device_info(
    doc: &Schematic,
    device: DeviceId,
    info: DeviceInfo,
) -> Result<Transaction, CommandError> {
    let found = doc
        .devices
        .iter()
        .find(|candidate| candidate.id == device)
        .ok_or(CommandError::Missing(circuit_model::Subject::Device(
            device,
        )))?;
    crate::layers::ensure_unlocked(doc, [&circuit_model::Subject::Device(device)])?;
    let info = DeviceInfo {
        manufacturer: clean(info.manufacturer),
        part_no: clean(info.part_no),
        description: clean(info.description),
        location: clean(info.location),
        datasheet: None,
    };
    let info = (!info.is_empty()).then_some(info);
    let mut tx = Transaction::new("Edit catalog data", doc.next_id);
    if info != found.info {
        tx.edits.push(Edit::SetDeviceInfo { device, info });
    }
    Ok(tx)
}

/// One field of the catalog data of a device.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InfoField {
    /// The manufacturer.
    Manufacturer,
    /// The part number.
    PartNo,
    /// The description.
    Description,
    /// Where the device is mounted.
    Location,
}

impl InfoField {
    /// Every field, in the order the window shows them.
    pub const ALL: [Self; 4] = [
        Self::Manufacturer,
        Self::PartNo,
        Self::Description,
        Self::Location,
    ];

    /// The field of `info`.
    #[must_use]
    pub fn of(self, info: &DeviceInfo) -> Option<&str> {
        match self {
            Self::Manufacturer => info.manufacturer.as_deref(),
            Self::PartNo => info.part_no.as_deref(),
            Self::Description => info.description.as_deref(),
            Self::Location => info.location.as_deref(),
        }
    }

    fn set(self, info: &mut DeviceInfo, value: Option<String>) {
        match self {
            Self::Manufacturer => info.manufacturer = value,
            Self::PartNo => info.part_no = value,
            Self::Description => info.description = value,
            Self::Location => info.location = value,
        }
    }
}

/// Set one field of the catalog data of several devices to the same text
/// (an empty text clears it), keeping their other fields, in one
/// transaction.
///
/// # Errors
///
/// As [`set_device_info`], for the first device that refuses.
pub fn set_common_info(
    doc: &Schematic,
    devices: &std::collections::BTreeSet<DeviceId>,
    field: InfoField,
    value: &str,
) -> Result<Transaction, CommandError> {
    let mut tx = Transaction::new("Edit catalog data", doc.next_id);
    for device in devices {
        let current = doc
            .devices
            .iter()
            .find(|candidate| candidate.id == *device)
            .ok_or(CommandError::Missing(circuit_model::Subject::Device(
                *device,
            )))?;
        let mut info = current.info.clone().unwrap_or_default();
        field.set(&mut info, Some(value.to_owned()));
        tx.edits.extend(set_device_info(doc, *device, info)?.edits);
    }
    Ok(tx)
}

/// Set (`Some`) or clear (`None`, or an empty text) the number that replaces
/// the generated one on a wire.
///
/// # Errors
///
/// Returns [`CommandError::Missing`] for a wire that does not exist.
pub fn set_wire_number(
    doc: &Schematic,
    wire: WireId,
    number: Option<&str>,
) -> Result<Transaction, CommandError> {
    let found = doc
        .wires
        .iter()
        .find(|candidate| candidate.id == wire)
        .ok_or(CommandError::Missing(circuit_model::Subject::Wire(wire)))?;
    crate::layers::ensure_wire_unlocked(doc, wire)?;
    let number = clean(number.map(str::to_owned));
    let mut tx = Transaction::new("Set wire number", doc.next_id);
    if number != found.number {
        tx.edits.push(Edit::SetWireNumber { wire, number });
    }
    Ok(tx)
}

/// Set what a wire is made of (ADR-0018): each field of `spec` that is
/// empty is left out, and a specification with no field clears it.
///
/// # Errors
///
/// [`CommandError::Missing`] for a wire that does not exist,
/// [`CommandError::Locked`] for one on a locked layer, and
/// [`CommandError::Invalid`] for a field longer than
/// [`MAX_SPEC_CHARS`](circuit_model::annotations::MAX_SPEC_CHARS) characters
/// or with a control character.
pub fn set_wire_spec(
    doc: &Schematic,
    wire: WireId,
    spec: circuit_model::WireSpec,
) -> Result<Transaction, CommandError> {
    let found = doc
        .wires
        .iter()
        .find(|candidate| candidate.id == wire)
        .ok_or(CommandError::Missing(circuit_model::Subject::Wire(wire)))?;
    crate::layers::ensure_wire_unlocked(doc, wire)?;
    let mut spec = spec;
    for name in circuit_model::WireSpec::FIELDS {
        if let Some(field) = spec.field_mut(name) {
            *field = clean(field.take());
        }
    }
    if !spec.is_valid() {
        return Err(CommandError::Invalid(format!(
            "each field of a wire specification is 1 to {} characters with no control character",
            circuit_model::annotations::MAX_SPEC_CHARS
        )));
    }
    let spec = (!spec.is_empty()).then_some(spec);
    let mut tx = Transaction::new("Set wire data", doc.next_id);
    if spec != found.spec {
        tx.edits.push(Edit::SetWireSpec { wire, spec });
    }
    Ok(tx)
}

/// Change the words of a text note; see [`crate::ops::set_text`].
///
/// # Errors
///
/// As [`crate::ops::set_text`].
pub fn set_text(
    doc: &Schematic,
    text: circuit_model::TextId,
    words: &str,
) -> Result<Transaction, CommandError> {
    crate::ops::set_text(doc, text, words)
}

/// Set (`Some`) or clear (`None`) the rule that numbers the wires of the
/// project. Without a rule no wire number is drawn.
///
/// # Errors
///
/// Never fails on its own; a prefix longer than the format allows is refused
/// by the transaction as a limit.
pub fn set_wire_numbering(
    doc: &Schematic,
    numbering: Option<WireNumbering>,
) -> Result<Transaction, CommandError> {
    let mut tx = Transaction::new("Set wire numbering", doc.next_id);
    if numbering != doc.project.wire_numbering {
        let mut project = doc.project.clone();
        project.wire_numbering = numbering;
        tx.edits.push(Edit::SetProject(project));
    }
    Ok(tx)
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use circuit_model::{GridPoint, SchematicBuilder, Subject};

    use super::*;
    use crate::History;

    fn doc() -> Schematic {
        let mut b = SchematicBuilder::new("documentation");
        let device = b.device("S1", "pushbutton_no", &[]).unwrap();
        let symbol = b
            .symbol(device, "contact", None, GridPoint::new(0, 0))
            .unwrap();
        let junction = b.junction(GridPoint::new(0, 10)).unwrap();
        b.wire(circuit_model::builder::pin(symbol, "p2"), junction)
            .unwrap();
        b.finish()
    }

    #[test]
    fn the_data_of_a_wire_is_cleaned_bounded_undone_and_cleared() {
        let mut doc = doc();
        let wire = doc.wires[0].id;
        let mut history = History::new();
        let spec = circuit_model::WireSpec {
            colour: Some(" BU ".to_owned()),
            kind: Some("  ".to_owned()),
            ..circuit_model::WireSpec::default()
        };
        let tx = set_wire_spec(&doc, wire, spec).unwrap();
        history.execute(&mut doc, tx).unwrap();
        assert_eq!(
            doc.wires[0].spec,
            Some(circuit_model::WireSpec {
                colour: Some("BU".to_owned()),
                ..circuit_model::WireSpec::default()
            })
        );
        let long = circuit_model::WireSpec {
            material: Some("x".repeat(33)),
            ..circuit_model::WireSpec::default()
        };
        assert!(matches!(
            set_wire_spec(&doc, wire, long),
            Err(CommandError::Invalid(_))
        ));
        assert!(set_wire_spec(&doc, WireId(999), circuit_model::WireSpec::default()).is_err());
        // Nothing left: the wire has no data.
        let tx = set_wire_spec(&doc, wire, circuit_model::WireSpec::default()).unwrap();
        history.execute(&mut doc, tx).unwrap();
        assert_eq!(doc.wires[0].spec, None);
        history.undo(&mut doc).unwrap();
        assert!(doc.wires[0].spec.is_some());
    }

    #[test]
    fn one_field_is_set_on_several_devices_keeping_the_others_in_one_step() {
        let mut b = SchematicBuilder::new("common");
        let k1 = b.device("K1", "contactor.generic", &[]).unwrap();
        let k2 = b.device("K2", "contactor.generic", &[]).unwrap();
        let mut doc = b.finish();
        doc.devices[0].info = Some(DeviceInfo {
            part_no: Some("C-9".to_owned()),
            ..DeviceInfo::default()
        });
        let mut history = History::new();
        let both = std::collections::BTreeSet::from([k1, k2]);

        let tx = set_common_info(&doc, &both, InfoField::Manufacturer, " Acme ").unwrap();
        history.execute(&mut doc, tx).unwrap();

        let infos: Vec<_> = doc
            .devices
            .iter()
            .map(|d| d.info.clone().unwrap())
            .collect();
        assert!(
            infos
                .iter()
                .all(|info| info.manufacturer.as_deref() == Some("Acme"))
        );
        assert_eq!(infos[0].part_no.as_deref(), Some("C-9"));
        assert_eq!(history.undo_label(), Some("Edit catalog data"));
        history.undo(&mut doc).unwrap();
        assert!(doc.devices[1].info.is_none());

        let clear = set_common_info(&doc, &both, InfoField::PartNo, "").unwrap();
        history.execute(&mut doc, clear).unwrap();
        assert!(doc.devices.iter().all(|d| d.info.is_none()));
        assert!(matches!(
            set_common_info(
                &doc,
                &std::collections::BTreeSet::from([DeviceId(999)]),
                InfoField::Location,
                "X"
            ),
            Err(CommandError::Missing(Subject::Device(_)))
        ));
    }

    #[test]
    fn device_info_is_trimmed_empty_fields_are_cleared_and_undo_restores_it() {
        let mut doc = doc();
        let device = doc.devices[0].id;
        let mut history = History::new();

        let tx = set_device_info(
            &doc,
            device,
            DeviceInfo {
                manufacturer: Some("  Acme ".to_owned()),
                part_no: Some("   ".to_owned()),
                ..DeviceInfo::default()
            },
        )
        .unwrap();
        history.execute(&mut doc, tx).unwrap();

        let info = doc.devices[0].info.clone().unwrap();
        assert_eq!(info.manufacturer.as_deref(), Some("Acme"));
        assert_eq!(info.part_no, None);

        // Nothing left: no data at all, and the same data again changes nothing.
        assert!(
            set_device_info(&doc, device, info.clone())
                .unwrap()
                .is_empty()
        );
        let tx = set_device_info(&doc, device, DeviceInfo::default()).unwrap();
        history.execute(&mut doc, tx).unwrap();
        assert_eq!(doc.devices[0].info, None);
        history.undo(&mut doc).unwrap();
        assert_eq!(doc.devices[0].info, Some(info));
    }

    #[test]
    fn a_missing_device_or_wire_is_refused() {
        let doc = doc();

        assert!(matches!(
            set_device_info(&doc, DeviceId(900), DeviceInfo::default()),
            Err(CommandError::Missing(Subject::Device(_)))
        ));
        assert!(matches!(
            set_wire_number(&doc, WireId(900), Some("1")),
            Err(CommandError::Missing(Subject::Wire(_)))
        ));
    }

    #[test]
    fn a_wire_number_is_set_cleared_and_empty_text_is_no_number() {
        let mut doc = doc();
        let wire = doc.wires[0].id;
        let mut history = History::new();

        let tx = set_wire_number(&doc, wire, Some(" L1 ")).unwrap();
        history.execute(&mut doc, tx).unwrap();
        assert_eq!(doc.wires[0].number.as_deref(), Some("L1"));
        assert!(set_wire_number(&doc, wire, Some("L1")).unwrap().is_empty());

        let tx = set_wire_number(&doc, wire, Some("  ")).unwrap();
        history.execute(&mut doc, tx).unwrap();
        assert_eq!(doc.wires[0].number, None);
    }

    #[test]
    fn the_numbering_rule_is_set_and_cleared_and_keeps_the_rest_of_the_project() {
        let mut doc = doc();
        doc.project.author = Some("Ana".to_owned());
        let mut history = History::new();
        let rule = WireNumbering {
            prefix: "W".to_owned(),
            start: 100,
            step: NonZeroU32::new(10).unwrap(),
            per_sheet: false,
        };

        let tx = set_wire_numbering(&doc, Some(rule.clone())).unwrap();
        history.execute(&mut doc, tx).unwrap();
        assert_eq!(doc.project.wire_numbering, Some(rule.clone()));
        assert_eq!(doc.project.author.as_deref(), Some("Ana"));
        assert!(set_wire_numbering(&doc, Some(rule)).unwrap().is_empty());

        let tx = set_wire_numbering(&doc, None).unwrap();
        history.execute(&mut doc, tx).unwrap();
        assert_eq!(doc.project.wire_numbering, None);
    }

    #[test]
    fn a_prefix_beyond_the_limit_is_refused_by_the_transaction() {
        let doc = doc();
        let tx = set_wire_numbering(
            &doc,
            Some(WireNumbering {
                prefix: "P".repeat(17),
                start: 1,
                step: NonZeroU32::MIN,
                per_sheet: false,
            }),
        )
        .unwrap();

        assert!(matches!(tx.apply_to(&doc), Err(CommandError::Limit(_))));
    }
}
