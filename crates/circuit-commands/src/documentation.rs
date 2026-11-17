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
    let info = DeviceInfo {
        manufacturer: clean(info.manufacturer),
        part_no: clean(info.part_no),
        description: clean(info.description),
        location: clean(info.location),
    };
    let info = (!info.is_empty()).then_some(info);
    let mut tx = Transaction::new("Edit catalog data", doc.next_id);
    if info != found.info {
        tx.edits.push(Edit::SetDeviceInfo { device, info });
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
    let number = clean(number.map(str::to_owned));
    let mut tx = Transaction::new("Set wire number", doc.next_id);
    if number != found.number {
        tx.edits.push(Edit::SetWireNumber { wire, number });
    }
    Ok(tx)
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
