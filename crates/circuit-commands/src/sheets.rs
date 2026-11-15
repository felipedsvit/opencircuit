//! Operations on the sheets and the information of a project, and a bare
//! junction: what a person does in the title block and what a script does
//! before it draws.

use circuit_model::{
    DiagnosticCode, GridPoint, IdAllocator, Junction, JunctionId, Orientation, ProjectMeta,
    Schematic, Sheet, SheetId, SheetSize, Subject, Zones,
};

use crate::edit::Edit;
use crate::error::CommandError;
use crate::transaction::Transaction;

/// What a new sheet looks like, before it has an ID.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SheetSpec {
    /// The number printed in the title block, or `None` for the next free
    /// one after the largest.
    pub number: Option<u32>,
    /// Title of the sheet.
    pub title: String,
    /// Top-left corner of the frame.
    pub origin: GridPoint,
    /// Paper size.
    pub size: SheetSize,
    /// Orientation of the paper.
    pub orientation: Orientation,
    /// Zones along the frame.
    pub zones: Zones,
}

impl SheetSpec {
    /// An A4 landscape sheet with the usual 8 by 6 zones.
    #[must_use]
    pub fn a4(title: impl Into<String>, origin: GridPoint) -> Self {
        Self {
            number: None,
            title: title.into(),
            origin,
            size: SheetSize::A4,
            orientation: Orientation::Landscape,
            zones: Zones {
                columns: 8,
                rows: 6,
            },
        }
    }
}

/// Refuse `candidate` when the sheet `id` of it breaks a rule of
/// ADR-0011: the same checks as [`Schematic::validate`], read from its
/// result so that the two cannot disagree.
fn check(candidate: &Schematic, id: SheetId) -> Result<(), CommandError> {
    let mentions = |diagnostic: &circuit_model::Diagnostic| {
        diagnostic.subject() == &Subject::Sheet(id)
            || diagnostic.related().contains(&Subject::Sheet(id))
    };
    match candidate.validate().into_iter().find(|diagnostic| {
        matches!(
            diagnostic.code(),
            DiagnosticCode::DuplicateSheetNumber | DiagnosticCode::InvalidSheet
        ) && mentions(diagnostic)
    }) {
        Some(found) => Err(CommandError::InvalidSheet(found.message().to_owned())),
        None => Ok(()),
    }
}

/// Add a sheet. It takes the next number after the largest unless the spec
/// names one.
///
/// # Errors
///
/// Returns [`CommandError::InvalidSheet`] for a number that is 0 or used, a
/// frame without zones, of a size the format does not allow, beyond the
/// coordinate range or over another sheet; and [`CommandError::Ids`] when the
/// document has no ID left.
pub fn add_sheet(doc: &Schematic, spec: &SheetSpec) -> Result<Transaction, CommandError> {
    let number = spec.number.unwrap_or_else(|| {
        doc.sheets
            .iter()
            .map(|sheet| sheet.number)
            .max()
            .map_or(1, |largest| largest.saturating_add(1))
    });
    let mut ids = IdAllocator::starting_at(doc.next_id);
    let id = SheetId(ids.alloc()?);
    let sheet = Sheet {
        id,
        number,
        title: spec.title.clone(),
        origin: spec.origin,
        size: spec.size,
        orientation: spec.orientation,
        zones: spec.zones,
    };
    let mut candidate = doc.clone();
    candidate.sheets.push(sheet.clone());
    check(&candidate, id)?;

    let mut tx = Transaction::new(format!("Add sheet {number}"), ids.next_id());
    tx.edits.push(Edit::InsertSheet(sheet));
    Ok(tx)
}

/// Replace a sheet, found by its ID, with `sheet`.
///
/// # Errors
///
/// Returns [`CommandError::Missing`] for a sheet the document does not have
/// and [`CommandError::InvalidSheet`] as [`add_sheet`] does.
pub fn set_sheet(doc: &Schematic, sheet: Sheet) -> Result<Transaction, CommandError> {
    let id = sheet.id;
    if !doc.sheets.iter().any(|existing| existing.id == id) {
        return Err(CommandError::Missing(Subject::Sheet(id)));
    }
    let mut candidate = doc.clone();
    if let Some(slot) = candidate
        .sheets
        .iter_mut()
        .find(|existing| existing.id == id)
    {
        *slot = sheet.clone();
    }
    check(&candidate, id)?;

    let mut tx = Transaction::new(format!("Edit sheet {}", sheet.number), doc.next_id);
    if doc.sheets.iter().any(|existing| existing == &sheet) {
        return Ok(tx);
    }
    tx.edits.push(Edit::SetSheet(sheet));
    Ok(tx)
}

/// Remove a sheet. What was drawn on it stays on the canvas.
///
/// # Errors
///
/// Returns [`CommandError::Missing`] for a sheet the document does not have.
pub fn remove_sheet(doc: &Schematic, id: SheetId) -> Result<Transaction, CommandError> {
    let sheet = doc
        .sheets
        .iter()
        .find(|sheet| sheet.id == id)
        .ok_or(CommandError::Missing(Subject::Sheet(id)))?;
    let mut tx = Transaction::new(format!("Remove sheet {}", sheet.number), doc.next_id);
    tx.edits.push(Edit::RemoveSheet(id));
    Ok(tx)
}

/// Replace the project information: the name, and the author, company,
/// revision and date of the title block. The grid unit is kept.
///
/// # Errors
///
/// Returns [`CommandError::Limit`] from the transaction when a text is longer
/// than the format allows.
pub fn set_project_info(
    doc: &Schematic,
    name: &str,
    author: Option<&str>,
    company: Option<&str>,
    revision: Option<&str>,
    date: Option<&str>,
) -> Result<Transaction, CommandError> {
    let info = ProjectMeta {
        name: name.to_owned(),
        grid_unit: doc.project.grid_unit,
        author: author.map(str::to_owned),
        company: company.map(str::to_owned),
        revision: revision.map(str::to_owned),
        date: date.map(str::to_owned),
    };
    let mut tx = Transaction::new("Edit project information", doc.next_id);
    if info != doc.project {
        tx.edits.push(Edit::SetProject(info));
    }
    Ok(tx)
}

/// Add a junction at `at`, with no wire yet.
///
/// # Errors
///
/// Returns [`CommandError::Ids`] when the document has no ID left.
pub fn add_junction(doc: &Schematic, at: GridPoint) -> Result<Transaction, CommandError> {
    let mut ids = IdAllocator::starting_at(doc.next_id);
    let id = JunctionId(ids.alloc()?);
    let mut tx = Transaction::new("Add junction", ids.next_id());
    tx.edits
        .push(Edit::InsertJunction(Junction { id, position: at }));
    Ok(tx)
}
