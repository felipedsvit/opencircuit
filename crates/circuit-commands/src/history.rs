//! Undo and redo.

use std::collections::VecDeque;

use circuit_model::Schematic;

use crate::error::CommandError;
use crate::transaction::Transaction;

/// A transaction that was applied, with the transaction that undoes it.
#[must_use]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Applied {
    /// The transaction as the user made it; redo applies it again, with the
    /// same IDs.
    pub forward: Transaction,
    /// The transaction that restores the document as it was before.
    pub inverse: Transaction,
    /// The revision of the document after `forward`.
    pub revision: u64,
}

/// The undo and redo stacks of one document.
///
/// Every change to the document must go through [`History::execute`], so
/// that each inverse applies to exactly the document it was computed for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct History {
    undo: VecDeque<Applied>,
    redo: Vec<Applied>,
    limit: usize,
    /// The last revision handed out; revisions never repeat.
    last_revision: u64,
    /// The revision of the oldest state that can still be reached.
    base_revision: u64,
    /// The revision last saved, if any.
    saved_revision: Option<u64>,
}

impl Default for History {
    fn default() -> Self {
        Self::with_limit(Self::DEFAULT_LIMIT)
    }
}

impl History {
    /// Number of actions kept for undo by default.
    pub const DEFAULT_LIMIT: usize = 1_000;

    /// A history of a document as loaded or created, which counts as saved.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// A history keeping at most `limit` actions for undo (at least one).
    #[must_use]
    pub fn with_limit(limit: usize) -> Self {
        Self {
            undo: VecDeque::new(),
            redo: Vec::new(),
            limit: limit.max(1),
            last_revision: 0,
            base_revision: 0,
            saved_revision: Some(0),
        }
    }

    /// Apply a transaction to `doc` and record it. An empty transaction
    /// changes nothing and is not recorded.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] when the transaction is refused; `doc` and
    /// the history are then unchanged.
    pub fn execute(&mut self, doc: &mut Schematic, tx: Transaction) -> Result<(), CommandError> {
        if tx.is_empty() {
            return Ok(());
        }
        let (edited, inverse) = tx.apply_to(doc)?;
        *doc = edited;
        self.last_revision = self.last_revision.saturating_add(1);
        self.undo.push_back(Applied {
            forward: tx,
            inverse,
            revision: self.last_revision,
        });
        self.redo.clear();
        while self.undo.len() > self.limit {
            if let Some(dropped) = self.undo.pop_front() {
                self.base_revision = dropped.revision;
            }
        }
        Ok(())
    }

    /// Undo the last action. `Ok(false)` when there is nothing to undo.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] if the inverse no longer applies, which
    /// happens only when the document was changed outside the history; the
    /// document and the history are then unchanged.
    pub fn undo(&mut self, doc: &mut Schematic) -> Result<bool, CommandError> {
        let Some(last) = self.undo.pop_back() else {
            return Ok(false);
        };
        match last.inverse.apply_to(doc) {
            Ok((restored, _)) => {
                *doc = restored;
                self.redo.push(last);
                Ok(true)
            }
            Err(error) => {
                self.undo.push_back(last);
                Err(error)
            }
        }
    }

    /// Redo the last undone action, with the same IDs. `Ok(false)` when
    /// there is nothing to redo.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] if the action no longer applies; the
    /// document and the history are then unchanged.
    pub fn redo(&mut self, doc: &mut Schematic) -> Result<bool, CommandError> {
        let Some(next) = self.redo.pop() else {
            return Ok(false);
        };
        match next.forward.apply_to(doc) {
            Ok((edited, _)) => {
                *doc = edited;
                self.undo.push_back(next);
                Ok(true)
            }
            Err(error) => {
                self.redo.push(next);
                Err(error)
            }
        }
    }

    /// Whether there is an action to undo.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// Whether there is an action to redo.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The label of the action undo would revert.
    #[must_use]
    pub fn undo_label(&self) -> Option<&str> {
        self.undo
            .back()
            .map(|applied| applied.forward.label.as_str())
    }

    /// The label of the action redo would apply.
    #[must_use]
    pub fn redo_label(&self) -> Option<&str> {
        self.redo
            .last()
            .map(|applied| applied.forward.label.as_str())
    }

    /// The revision of the current document. Each new action gets a revision
    /// never used before; undo and redo move between existing ones.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.undo
            .back()
            .map_or(self.base_revision, |applied| applied.revision)
    }

    /// Record that the current document was saved.
    pub fn mark_saved(&mut self) {
        self.saved_revision = Some(self.revision());
    }

    /// Whether the document differs from the last saved one.
    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.saved_revision != Some(self.revision())
    }
}

#[cfg(test)]
mod tests {
    use circuit_model::{GridPoint, Junction, JunctionId};

    use super::*;
    use crate::edit::Edit;

    fn add_junction(doc: &Schematic, x: i32) -> Transaction {
        let mut tx = Transaction::new(format!("junction {x}"), doc.next_id + 1);
        tx.edits.push(Edit::InsertJunction(Junction {
            id: JunctionId(doc.next_id),
            position: GridPoint::new(x, 0),
        }));
        tx
    }

    fn with_actions(count: i32) -> (Schematic, History) {
        let mut doc = Schematic::new("history");
        let mut history = History::new();
        for x in 0..count {
            let tx = add_junction(&doc, x);
            history.execute(&mut doc, tx).unwrap();
        }
        (doc, history)
    }

    #[test]
    fn undo_then_redo_returns_to_the_same_document_with_the_same_ids() {
        let (mut doc, mut history) = with_actions(3);
        let edited = doc.clone();

        assert!(history.undo(&mut doc).unwrap());
        assert!(history.redo(&mut doc).unwrap());

        assert_eq!(doc, edited);
    }

    #[test]
    fn undo_never_lowers_next_id() {
        let (mut doc, mut history) = with_actions(2);
        let next_id = doc.next_id;

        while history.undo(&mut doc).unwrap() {}

        assert_eq!(doc.junctions, []);
        assert_eq!(doc.next_id, next_id);
    }

    #[test]
    fn new_action_after_undo_gets_a_fresh_id_and_clears_redo() {
        let (mut doc, mut history) = with_actions(1);
        let first = doc.junctions[0].id;
        history.undo(&mut doc).unwrap();

        let tx = add_junction(&doc, 9);
        history.execute(&mut doc, tx).unwrap();

        assert_ne!(doc.junctions[0].id, first);
        assert!(!history.can_redo());
    }

    #[test]
    fn nothing_to_undo_or_redo_is_not_an_error() {
        let mut doc = Schematic::new("empty");
        let mut history = History::new();

        assert_eq!(history.undo(&mut doc), Ok(false));
        assert_eq!(history.redo(&mut doc), Ok(false));
    }

    #[test]
    fn refused_transaction_is_not_recorded() {
        let (mut doc, mut history) = with_actions(1);
        let before = (doc.clone(), history.clone());
        let mut tx = Transaction::new("bad", doc.next_id);
        tx.edits.push(Edit::RemoveJunction(JunctionId(999)));

        assert!(history.execute(&mut doc, tx).is_err());
        assert_eq!((doc, history), before);
    }

    #[test]
    fn empty_transaction_is_not_recorded() {
        let (mut doc, mut history) = with_actions(1);
        let tx = Transaction::new("nothing", doc.next_id);

        history.execute(&mut doc, tx).unwrap();

        assert_eq!(
            (history.undo_label(), history.revision()),
            (Some("junction 0"), 1)
        );
    }

    #[test]
    fn document_is_dirty_until_saved_and_after_undo_past_the_save() {
        let (mut doc, mut history) = with_actions(2);
        assert!(history.is_dirty());

        history.mark_saved();
        assert!(!history.is_dirty());

        history.undo(&mut doc).unwrap();
        assert!(history.is_dirty());

        history.redo(&mut doc).unwrap();
        assert!(!history.is_dirty());
    }

    #[test]
    fn oldest_actions_beyond_the_limit_are_dropped() {
        let mut doc = Schematic::new("limit");
        let mut history = History::with_limit(2);
        for x in 0..3 {
            let tx = add_junction(&doc, x);
            history.execute(&mut doc, tx).unwrap();
        }

        let mut undone = 0;
        while history.undo(&mut doc).unwrap() {
            undone += 1;
        }

        assert_eq!(undone, 2);
        assert_eq!(doc.junctions.len(), 1);
        assert!(history.is_dirty());
    }

    #[test]
    fn labels_name_the_next_undo_and_redo() {
        let (mut doc, mut history) = with_actions(2);

        history.undo(&mut doc).unwrap();

        assert_eq!(
            (history.undo_label(), history.redo_label()),
            (Some("junction 0"), Some("junction 1"))
        );
    }
}
