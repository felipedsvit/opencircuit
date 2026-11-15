//! Transactions: edits applied all together or not at all.

use std::collections::BTreeSet;

use circuit_model::{DiagnosticCode, Schematic, Subject};

use crate::edit::Edit;
use crate::error::CommandError;

/// Structural errors an edit must never add: a document with one of them
/// cannot be compiled, saved meaningfully or edited safely.
const INTEGRITY: [DiagnosticCode; 5] = [
    DiagnosticCode::UnsupportedSchemaVersion,
    DiagnosticCode::IdOutOfRange,
    DiagnosticCode::DuplicateId,
    DiagnosticCode::DanglingReference,
    DiagnosticCode::SelfLoopWire,
];

/// The integrity errors of a document, as `(code, subject)` pairs.
fn integrity_errors(doc: &Schematic) -> BTreeSet<(DiagnosticCode, Subject)> {
    doc.validate()
        .into_iter()
        .filter(|diagnostic| INTEGRITY.contains(&diagnostic.code()))
        .map(|diagnostic| (diagnostic.code(), diagnostic.subject().clone()))
        .collect()
}

/// Edits that form one user action, such as placing a contactor or deleting
/// a selection.
#[must_use]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transaction {
    /// What the action was, for the undo and redo labels.
    pub label: String,
    /// The edits, applied in order.
    pub edits: Vec<Edit>,
    /// The document counter after the action. Applying a transaction raises
    /// `next_id` to at least this value and never lowers it, so an ID handed
    /// out once is never handed out again, even after an undo.
    pub next_id: u64,
}

impl Transaction {
    /// An empty transaction for a document whose counter is `next_id`.
    pub fn new(label: impl Into<String>, next_id: u64) -> Self {
        Self {
            label: label.into(),
            edits: Vec::new(),
            next_id,
        }
    }

    /// Whether the transaction changes nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.edits.is_empty()
    }

    /// The symbols, junctions and wires the transaction creates, in order:
    /// what an editor selects once the transaction is applied.
    #[must_use]
    pub fn inserted(&self) -> Vec<circuit_model::Subject> {
        self.edits
            .iter()
            .filter_map(|edit| match edit {
                Edit::InsertSymbol(symbol) => Some(Subject::Symbol(symbol.id)),
                Edit::InsertJunction(junction) => Some(Subject::Junction(junction.id)),
                Edit::InsertWire(wire) => Some(Subject::Wire(wire.id)),
                Edit::InsertSupply(_)
                | Edit::RemoveSupply(_)
                | Edit::SetSupply(_)
                | Edit::InsertDevice(_)
                | Edit::RemoveDevice(_)
                | Edit::SetReference { .. }
                | Edit::SetParam { .. }
                | Edit::SetModel { .. }
                | Edit::RemoveSymbol(_)
                | Edit::Place { .. }
                | Edit::RemoveJunction(_)
                | Edit::MoveJunction { .. }
                | Edit::RemoveWire(_)
                | Edit::SetRoute { .. } => None,
            })
            .collect()
    }

    /// Apply the edits to a copy of `doc` without the size and integrity
    /// checks of [`apply_to`](Self::apply_to).
    ///
    /// Returns the copy, sorted by ID, and the edits that undo the
    /// transaction. It is for intermediate documents that only feed a
    /// geometric computation, such as routing the wires of a move; whatever
    /// is committed goes through `apply_to`.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] when an edit is refused.
    pub(crate) fn apply_edits(
        &self,
        doc: &Schematic,
    ) -> Result<(Schematic, Vec<Edit>), CommandError> {
        let mut draft = doc.clone();
        draft.sort_canonical();

        let mut inverse = Vec::with_capacity(self.edits.len());
        for edit in &self.edits {
            inverse.push(edit.clone().apply(&mut draft)?);
        }
        inverse.reverse();
        draft.next_id = draft.next_id.max(self.next_id);
        Ok((draft, inverse))
    }

    /// Apply the transaction to a copy of `doc`.
    ///
    /// Returns the edited copy and the transaction that undoes it. The
    /// collections of the copy are sorted by ID. `doc` itself is never
    /// touched, so a refused transaction leaves nothing half applied.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] when an edit is refused, when the result
    /// would exceed a size limit of the format, or when it would contain an
    /// integrity error (`DuplicateId`, `DanglingReference`, `IdOutOfRange`,
    /// `SelfLoopWire`, `UnsupportedSchemaVersion`) that `doc` does not
    /// already have.
    pub fn apply_to(&self, doc: &Schematic) -> Result<(Schematic, Self), CommandError> {
        let (draft, inverse) = self.apply_edits(doc)?;

        circuit_model::limits::check(&draft)?;
        // A document without integrity errors cannot have gained one, so the
        // original is only inspected when the result has some. This keeps the
        // common case, which runs on every frame of a drag, to one validation.
        let after = integrity_errors(&draft);
        if !after.is_empty() {
            let before = integrity_errors(doc);
            if let Some((code, subject)) = after.difference(&before).next() {
                return Err(CommandError::BreaksIntegrity(format!(
                    "{code} on {subject}"
                )));
            }
        }

        let undo = Self {
            label: self.label.clone(),
            edits: inverse,
            next_id: draft.next_id,
        };
        Ok((draft, undo))
    }
}

#[cfg(test)]
mod tests {
    use circuit_model::builder::pin;
    use circuit_model::limits::MAX_TEXT_BYTES;
    use circuit_model::{
        Device, DeviceId, Endpoint, GridPoint, Junction, JunctionId, ModelRef, ReferenceDesignator,
        SchematicBuilder, Wire, WireId,
    };

    use super::*;

    fn doc() -> Schematic {
        let mut b = SchematicBuilder::new("transactions");
        let device = b.device("S1", "pushbutton_no", &[]).unwrap();
        let contact = b
            .symbol(device, "contact", None, GridPoint::new(0, 0))
            .unwrap();
        let junction = b.junction(GridPoint::new(0, 4)).unwrap();
        b.wire(pin(contact, "p2"), junction).unwrap();
        b.finish()
    }

    fn device(id: u64, reference: &str) -> Device {
        Device {
            id: DeviceId(id),
            reference: ReferenceDesignator(reference.to_owned()),
            model: ModelRef {
                id: "pushbutton_no".to_owned(),
                version: 1,
            },
            params: Default::default(),
        }
    }

    #[test]
    fn transaction_and_its_inverse_restore_the_document() {
        let original = doc();
        let mut tx = Transaction::new("add", original.next_id + 2);
        tx.edits
            .push(Edit::InsertDevice(device(original.next_id, "S2")));
        tx.edits.push(Edit::InsertJunction(Junction {
            id: JunctionId(original.next_id + 1),
            position: GridPoint::new(3, 3),
        }));

        let (edited, undo) = tx.apply_to(&original).unwrap();
        let (restored, _) = undo.apply_to(&edited).unwrap();

        assert_eq!(edited.devices.len(), 2);
        assert_eq!(
            Schematic {
                next_id: original.next_id,
                ..restored.clone()
            },
            original
        );
        assert_eq!(restored.next_id, original.next_id + 2);
    }

    #[test]
    fn refused_edit_in_the_middle_leaves_the_document_untouched() {
        let original = doc();
        let mut tx = Transaction::new("broken", original.next_id + 1);
        tx.edits
            .push(Edit::InsertDevice(device(original.next_id, "S2")));
        tx.edits.push(Edit::RemoveWire(WireId(999)));

        let refused = tx.apply_to(&original);

        assert!(matches!(refused, Err(CommandError::Missing(_))));
        assert_eq!(original, doc());
    }

    #[test]
    fn id_already_used_by_another_kind_of_entity_is_refused() {
        let original = doc();
        let taken = original.junctions[0].id.0;
        let mut tx = Transaction::new("clash", original.next_id);
        tx.edits.push(Edit::InsertDevice(device(taken, "S2")));

        assert!(matches!(
            tx.apply_to(&original),
            Err(CommandError::BreaksIntegrity(_))
        ));
    }

    #[test]
    fn id_at_or_above_next_id_without_raising_it_is_refused() {
        let original = doc();
        let mut tx = Transaction::new("ahead", original.next_id);
        tx.edits
            .push(Edit::InsertDevice(device(original.next_id, "S2")));

        assert!(matches!(
            tx.apply_to(&original),
            Err(CommandError::BreaksIntegrity(_))
        ));
    }

    #[test]
    fn result_beyond_a_size_limit_is_refused() {
        let original = doc();
        let mut tx = Transaction::new("long", original.next_id);
        tx.edits.push(Edit::SetReference {
            device: original.devices[0].id,
            reference: ReferenceDesignator("S".repeat(MAX_TEXT_BYTES + 1)),
        });

        assert!(matches!(
            tx.apply_to(&original),
            Err(CommandError::Limit(_))
        ));
    }

    #[test]
    fn existing_integrity_errors_do_not_block_other_edits() {
        let mut broken = doc();
        broken.wires.push(Wire {
            id: WireId(broken.next_id),
            from: Endpoint::Junction(JunctionId(500)),
            to: Endpoint::Junction(broken.junctions[0].id),
            route: Vec::new(),
        });
        broken.next_id += 1;
        let mut tx = Transaction::new("move", broken.next_id);
        tx.edits.push(Edit::MoveJunction {
            junction: broken.junctions[0].id,
            position: GridPoint::new(1, 4),
        });

        assert!(tx.apply_to(&broken).is_ok());
    }

    #[test]
    fn applying_sorts_the_collections_of_the_copy() {
        let mut unsorted = doc();
        unsorted.symbols.reverse();
        unsorted.devices.reverse();
        let tx = Transaction::new("nothing", unsorted.next_id);

        let (edited, _) = tx.apply_to(&unsorted).unwrap();

        let mut sorted = unsorted.clone();
        sorted.sort_canonical();
        assert_eq!(edited, sorted);
    }
}
