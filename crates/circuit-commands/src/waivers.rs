//! Waiving a warning with a reason (ADR-0014).
//!
//! A waiver says that a person has seen a warning or an informational
//! diagnostic and accepts it. It hides nothing from the compiler: whoever
//! presents the diagnostics marks the waived ones. An error cannot be waived.

use circuit_model::{DiagnosticCode, Schematic, Severity, Subject, Waiver};

use crate::{CommandError, Edit, Transaction};

/// Waive the diagnostic `code` about `subject`, because of `reason`. A waiver
/// that exists already is given the new reason; the same reason changes
/// nothing, and the transaction is empty.
///
/// # Errors
///
/// Returns [`CommandError::CannotWaive`] for a text that is not a diagnostic
/// code, for an error, for `OC-STR-020` (the report of a bad waiver) and for
/// a reason that is empty. A reason over the text limit is refused with the
/// limit.
pub fn waive(
    doc: &Schematic,
    code: &str,
    subject: Subject,
    reason: &str,
) -> Result<Transaction, CommandError> {
    let parsed = DiagnosticCode::parse(code)
        .ok_or_else(|| CommandError::CannotWaive(format!("`{code}` is not a diagnostic code")))?;
    if parsed.default_severity() == Severity::Error || parsed == DiagnosticCode::InvalidWaiver {
        return Err(CommandError::CannotWaive(format!(
            "{} is an error or reports a bad waiver, and cannot be waived",
            parsed.as_str()
        )));
    }
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(CommandError::CannotWaive(
            "a waiver needs a reason".to_owned(),
        ));
    }

    let mut tx = Transaction::new(format!("Waive {}", parsed.as_str()), doc.next_id);
    let existing = doc
        .waivers
        .iter()
        .find(|waiver| waiver.code == parsed.as_str() && waiver.subject == subject);
    if let Some(existing) = existing {
        if existing.reason == reason {
            return Ok(tx);
        }
        tx.edits.push(Edit::RemoveWaiver {
            code: parsed.as_str().to_owned(),
            subject: subject.clone(),
        });
    }
    tx.edits.push(Edit::InsertWaiver(Waiver {
        code: parsed.as_str().to_owned(),
        subject,
        reason: reason.to_owned(),
    }));
    Ok(tx)
}

/// Take the waiver of `code` about `subject` away.
///
/// # Errors
///
/// Returns [`CommandError::Missing`] when there is no such waiver.
pub fn unwaive(doc: &Schematic, code: &str, subject: Subject) -> Result<Transaction, CommandError> {
    let found = DiagnosticCode::parse(code).and_then(|parsed| {
        doc.waivers
            .iter()
            .find(|waiver| waiver.code == parsed.as_str() && waiver.subject == subject)
    });
    let Some(found) = found else {
        return Err(CommandError::Missing(subject));
    };
    let mut tx = Transaction::new(format!("Restore {}", found.code), doc.next_id);
    tx.edits.push(Edit::RemoveWaiver {
        code: found.code.clone(),
        subject,
    });
    Ok(tx)
}

#[cfg(test)]
mod tests {
    use circuit_model::WireId;

    use super::*;
    use crate::History;

    fn wire(id: u64) -> Subject {
        Subject::Wire(WireId(id))
    }

    #[test]
    fn a_waiver_is_added_given_a_new_reason_and_taken_away_in_one_step_each() {
        let mut doc = Schematic::new("waive");
        let mut history = History::new();

        let tx = waive(&doc, "OC-STR-018", wire(5), "  test point  ").unwrap();
        history.execute(&mut doc, tx).unwrap();
        assert_eq!(doc.waivers.len(), 1);
        assert_eq!(doc.waivers[0].reason, "test point");

        // The same reason changes nothing.
        assert!(
            waive(&doc, "OC-STR-018", wire(5), "test point")
                .unwrap()
                .is_empty()
        );

        // Another reason replaces it, in one transaction.
        let tx = waive(&doc, "OC-STR-018", wire(5), "checked on site").unwrap();
        assert_eq!(tx.edits.len(), 2);
        history.execute(&mut doc, tx).unwrap();
        assert_eq!(doc.waivers.len(), 1);
        assert_eq!(doc.waivers[0].reason, "checked on site");
        history.undo(&mut doc).unwrap();
        assert_eq!(doc.waivers[0].reason, "test point");

        let tx = unwaive(&doc, "OC-STR-018", wire(5)).unwrap();
        history.execute(&mut doc, tx).unwrap();
        assert!(doc.waivers.is_empty());
    }

    #[test]
    fn what_cannot_be_waived_is_refused_with_the_reason_why() {
        let doc = Schematic::new("waive");

        for (code, reason) in [
            ("OC-NOPE-1", "x"),
            ("OC-STR-013", "x"),
            ("OC-ELEC-001", "x"),
            ("OC-STR-020", "x"),
            ("OC-STR-018", "   "),
        ] {
            assert!(
                matches!(
                    waive(&doc, code, wire(5), reason),
                    Err(CommandError::CannotWaive(_))
                ),
                "{code} {reason:?}"
            );
        }
    }

    #[test]
    fn restoring_what_is_not_waived_is_refused_and_a_long_reason_is_a_limit() {
        let doc = Schematic::new("waive");

        assert!(matches!(
            unwaive(&doc, "OC-STR-018", wire(5)),
            Err(CommandError::Missing(_))
        ));
        let long = "x".repeat(300);
        let tx = waive(&doc, "OC-STR-018", wire(5), &long).unwrap();
        assert!(matches!(tx.apply_to(&doc), Err(CommandError::Limit(_))));
    }

    #[test]
    fn waiving_keeps_the_waivers_sorted() {
        let mut doc = Schematic::new("waive");
        let mut history = History::new();
        for (code, subject) in [
            ("OC-STR-019", wire(1)),
            ("OC-STR-018", wire(9)),
            ("OC-STR-018", wire(2)),
        ] {
            let tx = waive(&doc, code, subject, "ok").unwrap();
            history.execute(&mut doc, tx).unwrap();
        }

        assert!(doc.is_canonical());
        assert_eq!(doc.waivers[0].subject, wire(2));
    }
}
