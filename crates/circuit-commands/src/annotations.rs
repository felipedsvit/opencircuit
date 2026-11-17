//! Edits of the documentation of schema version 5 (ADR-0018): shapes and
//! QR codes, comments, groups, mirrored symbols, device views and PLC
//! modules, and what follows when an element they name is deleted.
//!
//! Every function is a pure transformation of the document into a
//! [`Transaction`]; none of it reaches the compiled circuit.

use std::collections::BTreeSet;

use circuit_model::annotations::{
    MAX_AUTHOR_CHARS, MAX_COMMENT_CHARS, MAX_GROUP_MEMBERS, MAX_GROUP_NAME_CHARS, MAX_VIEW_SIDE,
    is_valid_text,
};
use circuit_model::{
    Comment, CommentId, DeviceId, DeviceView, GridPoint, Group, GroupId, IdAllocator, LayerId,
    PlcChannel, PlcId, PlcModule, Schematic, Shape, ShapeId, ShapeKind, Subject, ViewId,
};

use crate::{CommandError, Edit, Transaction};

/// Whether `subject`, one of the documentation subjects of ADR-0018, is in
/// the document.
#[must_use]
pub fn exists(doc: &Schematic, subject: &Subject) -> bool {
    match subject {
        Subject::Shape(id) => doc.shapes.iter().any(|x| x.id == *id),
        Subject::Comment(id) => doc.comments.iter().any(|x| x.id == *id),
        Subject::Group(id) => doc.groups.iter().any(|x| x.id == *id),
        Subject::View(id) => doc.views.iter().any(|x| x.id == *id),
        Subject::Plc(id) => doc.plcs.iter().any(|x| x.id == *id),
        _ => false,
    }
}

/// The edit that removes one documentation subject, if it is one.
pub(crate) fn remove_edit(subject: &Subject) -> Option<Edit> {
    match subject {
        Subject::Shape(id) => Some(Edit::RemoveShape(*id)),
        Subject::Comment(id) => Some(Edit::RemoveComment(*id)),
        Subject::Group(id) => Some(Edit::RemoveGroup(*id)),
        Subject::View(id) => Some(Edit::RemoveView(*id)),
        Subject::Plc(id) => Some(Edit::RemovePlc(*id)),
        _ => None,
    }
}

/// What follows the removal of `removed`: the comments about a removed
/// element go, a group lets a removed member go (and goes when it has none
/// left), the views of a removed device go, and a channel that named a
/// removed device names none. What `removed` already holds is not repeated.
#[must_use]
pub fn follow_removal(doc: &Schematic, removed: &BTreeSet<Subject>) -> Vec<Edit> {
    let gone = |subject: &Subject| match subject {
        Subject::Pin(pin) => removed.contains(&Subject::Symbol(pin.symbol_id)),
        other => removed.contains(other),
    };
    let mut edits = Vec::new();
    let devices: BTreeSet<DeviceId> = removed
        .iter()
        .filter_map(|subject| match subject {
            Subject::Device(id) => Some(*id),
            _ => None,
        })
        .collect();
    let mut views: BTreeSet<ViewId> = BTreeSet::new();
    for view in &doc.views {
        if devices.contains(&view.device_id) && !removed.contains(&Subject::View(view.id)) {
            views.insert(view.id);
        }
    }
    let view_gone = |subject: &Subject| match subject {
        Subject::View(id) => views.contains(id),
        _ => false,
    };
    for comment in &doc.comments {
        if !removed.contains(&Subject::Comment(comment.id))
            && (gone(&comment.subject) || view_gone(&comment.subject))
        {
            edits.push(Edit::RemoveComment(comment.id));
        }
    }
    for group in &doc.groups {
        if removed.contains(&Subject::Group(group.id)) {
            continue;
        }
        let kept: Vec<Subject> = group
            .members
            .iter()
            .filter(|member| !gone(member))
            .cloned()
            .collect();
        if kept.is_empty() {
            edits.push(Edit::RemoveGroup(group.id));
        } else if kept.len() != group.members.len() {
            edits.push(Edit::SetGroup(Group {
                members: kept,
                ..group.clone()
            }));
        }
    }
    for plc in &doc.plcs {
        if removed.contains(&Subject::Plc(plc.id)) {
            continue;
        }
        let names_gone = plc.channels.iter().any(|channel| {
            channel
                .device
                .is_some_and(|device| devices.contains(&device))
        });
        if names_gone {
            let channels = plc
                .channels
                .iter()
                .map(|channel| PlcChannel {
                    device: channel.device.filter(|device| !devices.contains(device)),
                    ..channel.clone()
                })
                .collect();
            edits.push(Edit::SetPlc(PlcModule {
                channels,
                ..plc.clone()
            }));
        }
    }
    edits.extend(views.into_iter().map(Edit::RemoveView));
    edits
}

/// The edits that move the shapes and device views of `subjects` by
/// `delta`.
///
/// # Errors
///
/// [`CommandError::Missing`] for a shape or view that does not exist, and
/// [`CommandError::OutOfRange`] when one would leave the coordinate range.
pub(crate) fn move_edits(
    doc: &Schematic,
    subjects: &BTreeSet<Subject>,
    delta: GridPoint,
) -> Result<Vec<Edit>, CommandError> {
    let mut edits = Vec::new();
    for subject in subjects {
        match subject {
            Subject::Shape(id) => {
                let shape = doc
                    .shapes
                    .iter()
                    .find(|shape| shape.id == *id)
                    .ok_or_else(|| CommandError::Missing(subject.clone()))?;
                let kind = shape
                    .kind
                    .moved(delta.x, delta.y)
                    .ok_or(CommandError::OutOfRange)?;
                edits.push(Edit::SetShape(Shape {
                    kind,
                    ..shape.clone()
                }));
            }
            Subject::View(id) => {
                let view = doc
                    .views
                    .iter()
                    .find(|view| view.id == *id)
                    .ok_or_else(|| CommandError::Missing(subject.clone()))?;
                let position = view
                    .position
                    .checked_add(delta)
                    .ok_or(CommandError::OutOfRange)?;
                edits.push(Edit::SetView(DeviceView {
                    position,
                    ..view.clone()
                }));
            }
            _ => {}
        }
    }
    Ok(edits)
}

/// Add a shape (ADR-0018) on `layer`, or on the default layer.
///
/// # Errors
///
/// [`CommandError::Invalid`] for a shape that breaks a bound of ADR-0018,
/// [`CommandError::Missing`] for a layer that does not exist, and
/// [`CommandError::Ids`] when the document has no ID left.
pub fn add_shape(
    doc: &Schematic,
    kind: ShapeKind,
    layer: Option<LayerId>,
) -> Result<(Transaction, ShapeId), CommandError> {
    if let Some(problem) = kind.problem() {
        return Err(CommandError::Invalid(problem));
    }
    if let Some(layer) = layer
        && !doc.layers.iter().any(|candidate| candidate.id == layer)
    {
        return Err(CommandError::Missing(Subject::Layer(layer)));
    }
    let mut ids = IdAllocator::starting_at(doc.next_id);
    let id = ShapeId(ids.alloc()?);
    let mut tx = Transaction::new(format!("Add {}", kind.name()), ids.next_id());
    tx.edits.push(Edit::InsertShape(Shape { id, kind, layer }));
    Ok((tx, id))
}

/// Replace the geometry of a shape.
///
/// # Errors
///
/// [`CommandError::Missing`] for a shape that does not exist,
/// [`CommandError::Locked`] for one on a locked layer and
/// [`CommandError::Invalid`] for a geometry that breaks a bound.
pub fn set_shape(
    doc: &Schematic,
    id: ShapeId,
    kind: ShapeKind,
) -> Result<Transaction, CommandError> {
    let shape = doc
        .shapes
        .iter()
        .find(|shape| shape.id == id)
        .ok_or(CommandError::Missing(Subject::Shape(id)))?;
    crate::layers::ensure_unlocked(doc, [&Subject::Shape(id)])?;
    if let Some(problem) = kind.problem() {
        return Err(CommandError::Invalid(problem));
    }
    let mut tx = Transaction::new(format!("Change {}", kind.name()), doc.next_id);
    if kind != shape.kind {
        tx.edits.push(Edit::SetShape(Shape {
            kind,
            ..shape.clone()
        }));
    }
    Ok(tx)
}

fn check_comment(text: &str, author: Option<&str>) -> Result<(), CommandError> {
    if !is_valid_text(text, MAX_COMMENT_CHARS) {
        return Err(CommandError::Invalid(format!(
            "a comment is 1 to {MAX_COMMENT_CHARS} characters with no control character"
        )));
    }
    if author.is_some_and(|author| !is_valid_text(author, MAX_AUTHOR_CHARS)) {
        return Err(CommandError::Invalid(format!(
            "the author of a comment is 1 to {MAX_AUTHOR_CHARS} characters"
        )));
    }
    Ok(())
}

/// Whether `subject` is an element of the document a comment may be about.
fn commentable(doc: &Schematic, subject: &Subject) -> bool {
    match subject {
        Subject::Document => true,
        Subject::Device(id) => doc.devices.iter().any(|x| x.id == *id),
        Subject::Symbol(id) => doc.symbols.iter().any(|x| x.id == *id),
        Subject::Pin(pin) => doc.symbols.iter().any(|x| x.id == pin.symbol_id),
        Subject::Wire(id) => doc.wires.iter().any(|x| x.id == *id),
        Subject::Junction(id) => doc.junctions.iter().any(|x| x.id == *id),
        Subject::Sheet(id) => doc.sheets.iter().any(|x| x.id == *id),
        Subject::Supply(id) => doc.supplies.iter().any(|x| x.id == *id),
        Subject::Text(id) => doc.texts.iter().any(|x| x.id == *id),
        Subject::Layer(id) => doc.layers.iter().any(|x| x.id == *id),
        Subject::Comment(_) => false,
        other => exists(doc, other),
    }
}

/// Add a comment about `subject`.
///
/// # Errors
///
/// [`CommandError::Missing`] for an element that does not exist (a comment
/// is never about a comment), [`CommandError::Invalid`] for a text or an
/// author out of bounds, and [`CommandError::Ids`] when no ID is left.
pub fn add_comment(
    doc: &Schematic,
    subject: Subject,
    text: &str,
    author: Option<&str>,
) -> Result<(Transaction, CommentId), CommandError> {
    let text = text.trim();
    let author = author.map(str::trim).filter(|author| !author.is_empty());
    check_comment(text, author)?;
    if !commentable(doc, &subject) {
        return Err(CommandError::Missing(subject));
    }
    let mut ids = IdAllocator::starting_at(doc.next_id);
    let id = CommentId(ids.alloc()?);
    let mut tx = Transaction::new("Add comment", ids.next_id());
    tx.edits.push(Edit::InsertComment(Comment {
        id,
        subject,
        text: text.to_owned(),
        author: author.map(str::to_owned),
    }));
    Ok((tx, id))
}

/// Change the words of a comment.
///
/// # Errors
///
/// [`CommandError::Missing`] for a comment that does not exist and
/// [`CommandError::Invalid`] for a text out of bounds.
pub fn set_comment(
    doc: &Schematic,
    id: CommentId,
    text: &str,
) -> Result<Transaction, CommandError> {
    let comment = doc
        .comments
        .iter()
        .find(|comment| comment.id == id)
        .ok_or(CommandError::Missing(Subject::Comment(id)))?;
    let text = text.trim();
    check_comment(text, None)?;
    let mut tx = Transaction::new("Change comment", doc.next_id);
    if text != comment.text {
        tx.edits.push(Edit::SetComment(Comment {
            text: text.to_owned(),
            ..comment.clone()
        }));
    }
    Ok(tx)
}

/// The comments about `subject`, oldest first.
#[must_use]
pub fn comments_of<'a>(doc: &'a Schematic, subject: &Subject) -> Vec<&'a Comment> {
    doc.comments
        .iter()
        .filter(|comment| &comment.subject == subject)
        .collect()
}

/// Make a group of `members` called `name`. The members are symbols, wires,
/// junctions, texts and shapes; an element already in a group is refused, so
/// that no element is in two.
///
/// # Errors
///
/// [`CommandError::Invalid`] for a bad name, no member, too many, a member
/// of another kind or one already in a group; [`CommandError::Missing`] for
/// a member that does not exist; [`CommandError::Ids`] when no ID is left.
pub fn group(
    doc: &Schematic,
    name: &str,
    members: &BTreeSet<Subject>,
) -> Result<(Transaction, GroupId), CommandError> {
    let name = name.trim();
    if !is_valid_text(name, MAX_GROUP_NAME_CHARS) {
        return Err(CommandError::Invalid(format!(
            "a group name is 1 to {MAX_GROUP_NAME_CHARS} characters"
        )));
    }
    if members.is_empty() || members.len() > MAX_GROUP_MEMBERS {
        return Err(CommandError::Invalid(format!(
            "a group has 1 to {MAX_GROUP_MEMBERS} members"
        )));
    }
    for member in members {
        if !Group::may_hold(member) {
            return Err(CommandError::Invalid(format!(
                "a group cannot hold {member}"
            )));
        }
        if !commentable(doc, member) {
            return Err(CommandError::Missing(member.clone()));
        }
        if let Some(other) = group_of(doc, member) {
            return Err(CommandError::Invalid(format!(
                "{member} is already in group {}",
                other.name
            )));
        }
    }
    let mut ids = IdAllocator::starting_at(doc.next_id);
    let id = GroupId(ids.alloc()?);
    let mut tx = Transaction::new(format!("Group {}", members.len()), ids.next_id());
    tx.edits.push(Edit::InsertGroup(Group {
        id,
        name: name.to_owned(),
        members: members.iter().cloned().collect(),
    }));
    Ok((tx, id))
}

/// Take a group apart; its elements stay.
///
/// # Errors
///
/// [`CommandError::Missing`] for a group that does not exist.
pub fn ungroup(doc: &Schematic, id: GroupId) -> Result<Transaction, CommandError> {
    if !doc.groups.iter().any(|group| group.id == id) {
        return Err(CommandError::Missing(Subject::Group(id)));
    }
    let mut tx = Transaction::new("Ungroup", doc.next_id);
    tx.edits.push(Edit::RemoveGroup(id));
    Ok(tx)
}

/// The group `subject` is in, if any.
#[must_use]
pub fn group_of<'a>(doc: &'a Schematic, subject: &Subject) -> Option<&'a Group> {
    doc.groups
        .iter()
        .find(|group| group.members.contains(subject))
}

/// The selection grown to whole groups: every member of a group one of
/// `subjects` is in.
#[must_use]
pub fn with_groups(doc: &Schematic, subjects: &BTreeSet<Subject>) -> BTreeSet<Subject> {
    let mut grown = subjects.clone();
    for group in &doc.groups {
        if group.members.iter().any(|member| subjects.contains(member)) {
            grown.extend(group.members.iter().cloned());
        }
    }
    grown
}

/// Show `device` again at `position`, `width` by `height` grid units: its
/// footprint on a panel layout or its glyph on a single-line diagram.
///
/// # Errors
///
/// [`CommandError::Missing`] for a device that does not exist,
/// [`CommandError::Invalid`] for a side out of 1 to 200, and
/// [`CommandError::Ids`] when no ID is left.
pub fn add_view(
    doc: &Schematic,
    device: DeviceId,
    position: GridPoint,
    width: u32,
    height: u32,
) -> Result<(Transaction, ViewId), CommandError> {
    if !doc.devices.iter().any(|candidate| candidate.id == device) {
        return Err(CommandError::Missing(Subject::Device(device)));
    }
    let mut ids = IdAllocator::starting_at(doc.next_id);
    let id = ViewId(ids.alloc()?);
    let view = DeviceView {
        id,
        device_id: device,
        position,
        width,
        height,
        layer: None,
    };
    if !view.has_valid_size() || view.bounds().is_none() {
        return Err(CommandError::Invalid(format!(
            "a view is 1 to {MAX_VIEW_SIDE} grid units wide and high"
        )));
    }
    let mut tx = Transaction::new("Add view", ids.next_id());
    tx.edits.push(Edit::InsertView(view));
    Ok((tx, id))
}

/// Add a PLC module with its channels; `position`, when given, draws the
/// box that lists them there.
///
/// # Errors
///
/// [`CommandError::Invalid`] for a module that breaks a bound of ADR-0018
/// or whose name another module has, [`CommandError::Missing`] for a channel
/// whose device does not exist, and [`CommandError::Ids`] when no ID is
/// left.
pub fn add_plc(
    doc: &Schematic,
    name: &str,
    description: Option<&str>,
    position: Option<GridPoint>,
    channels: Vec<PlcChannel>,
) -> Result<(Transaction, PlcId), CommandError> {
    let mut ids = IdAllocator::starting_at(doc.next_id);
    let id = PlcId(ids.alloc()?);
    let module = PlcModule {
        id,
        name: name.trim().to_owned(),
        description: description
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_owned),
        position,
        channels,
    };
    check_plc(doc, &module)?;
    let mut tx = Transaction::new(format!("Add PLC module {}", module.name), ids.next_id());
    tx.edits.push(Edit::InsertPlc(module));
    Ok((tx, id))
}

/// Replace a PLC module: its name, description, place and channels.
///
/// # Errors
///
/// As [`add_plc`], and [`CommandError::Missing`] for a module that does not
/// exist.
pub fn set_plc(doc: &Schematic, module: PlcModule) -> Result<Transaction, CommandError> {
    let current = doc
        .plcs
        .iter()
        .find(|candidate| candidate.id == module.id)
        .ok_or(CommandError::Missing(Subject::Plc(module.id)))?;
    check_plc(doc, &module)?;
    let mut tx = Transaction::new(format!("Change PLC module {}", module.name), doc.next_id);
    if &module != current {
        tx.edits.push(Edit::SetPlc(module));
    }
    Ok(tx)
}

fn check_plc(doc: &Schematic, module: &PlcModule) -> Result<(), CommandError> {
    if let Some(problem) = module.problem() {
        return Err(CommandError::Invalid(problem));
    }
    if doc
        .plcs
        .iter()
        .any(|other| other.id != module.id && other.name == module.name)
    {
        return Err(CommandError::Invalid(format!(
            "another PLC module is named {}",
            module.name
        )));
    }
    for channel in &module.channels {
        if let Some(device) = channel.device
            && !doc.devices.iter().any(|candidate| candidate.id == device)
        {
            return Err(CommandError::Missing(Subject::Device(device)));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use circuit_model::{ComponentCatalog, PlcDirection, SchematicBuilder};

    use super::*;
    use crate::History;

    fn doc() -> Schematic {
        let mut b = SchematicBuilder::new("annotations");
        let device = b.device("K1", "contactor.generic", &[]).unwrap();
        b.symbol(device, "coil", None, GridPoint::new(0, 0))
            .unwrap();
        b.finish()
    }

    fn rect() -> ShapeKind {
        ShapeKind::Rectangle {
            from: GridPoint::new(0, 0),
            to: GridPoint::new(4, 4),
        }
    }

    #[test]
    fn a_shape_is_added_moved_and_refused_out_of_bounds() {
        let mut doc = doc();
        let mut history = History::new();
        let (tx, id) = add_shape(&doc, rect(), None).unwrap();
        history.execute(&mut doc, tx).unwrap();
        assert_eq!(doc.shapes.len(), 1);
        let moved = crate::ops::move_by(
            &doc,
            &ComponentCatalog::builtin(),
            &BTreeSet::from([Subject::Shape(id)]),
            GridPoint::new(2, 1),
        )
        .unwrap();
        history.execute(&mut doc, moved).unwrap();
        assert_eq!(doc.shapes[0].kind.anchor(), Some(GridPoint::new(2, 1)));
        history.undo(&mut doc).unwrap();
        assert_eq!(doc.shapes[0].kind.anchor(), Some(GridPoint::new(0, 0)));

        let flat = ShapeKind::Circle {
            center: GridPoint::new(0, 0),
            radius: 0,
        };
        assert!(matches!(
            add_shape(&doc, flat, None),
            Err(CommandError::Invalid(_))
        ));
        assert!(add_shape(&doc, rect(), Some(LayerId(999))).is_err());
    }

    #[test]
    fn deleting_an_element_takes_its_comments_and_its_place_in_a_group() {
        let mut doc = doc();
        let mut history = History::new();
        let symbol = Subject::Symbol(doc.symbols[0].id);
        let (tx, shape) = add_shape(&doc, rect(), None).unwrap();
        history.execute(&mut doc, tx).unwrap();
        let (tx, _) = add_comment(&doc, symbol.clone(), " check the coil ", Some("Ana")).unwrap();
        history.execute(&mut doc, tx).unwrap();
        assert_eq!(doc.comments[0].text, "check the coil");
        let both = BTreeSet::from([symbol.clone(), Subject::Shape(shape)]);
        let (tx, group) = group(&doc, "Starter", &both).unwrap();
        history.execute(&mut doc, tx).unwrap();
        // An element in a group cannot go in another.
        assert!(super::group(&doc, "Other", &BTreeSet::from([symbol.clone()])).is_err());
        assert_eq!(with_groups(&doc, &BTreeSet::from([symbol.clone()])), both);

        let delete = crate::ops::delete(
            &doc,
            &ComponentCatalog::builtin(),
            &BTreeSet::from([symbol.clone()]),
        )
        .unwrap();
        history.execute(&mut doc, delete).unwrap();
        assert!(doc.comments.is_empty());
        assert_eq!(doc.groups[0].members, [Subject::Shape(shape)]);
        assert!(doc.integrity_errors().is_empty());

        // The last member gone: the group goes too.
        let delete = crate::ops::delete(
            &doc,
            &ComponentCatalog::builtin(),
            &BTreeSet::from([Subject::Shape(shape)]),
        )
        .unwrap();
        history.execute(&mut doc, delete).unwrap();
        assert!(doc.groups.is_empty() && doc.shapes.is_empty());
        history.undo(&mut doc).unwrap();
        history.undo(&mut doc).unwrap();
        assert_eq!(doc.groups[0].id, group);
        assert_eq!(doc.comments.len(), 1);
    }

    #[test]
    fn a_mirrored_symbol_moves_its_terminals_and_keeps_its_wires() {
        let mut b = SchematicBuilder::new("mirror");
        let s1 = b.device("S1", "pushbutton_no", &[]).unwrap();
        let contact = b.symbol(s1, "contact", None, GridPoint::new(0, 0)).unwrap();
        let junction = b.junction(GridPoint::new(10, 10)).unwrap();
        b.wire(circuit_model::builder::pin(contact, "p1"), junction)
            .unwrap();
        let mut doc = b.finish();
        let catalog = ComponentCatalog::builtin();
        let layout = circuit_model::LayoutIndex::new(&doc, &catalog);
        let pins = |layout: &circuit_model::LayoutIndex<'_>| {
            ["p1", "p2"].map(|key| {
                layout.pin_position(&circuit_model::PinRef {
                    symbol_id: contact,
                    terminal: circuit_model::TerminalKey(key.to_owned()),
                })
            })
        };
        let before = pins(&layout);
        let mut history = History::new();
        let tx = crate::ops::mirror(&doc, &catalog, &BTreeSet::from([contact])).unwrap();
        history.execute(&mut doc, tx).unwrap();
        assert!(doc.symbols[0].mirrored);
        let layout = circuit_model::LayoutIndex::new(&doc, &catalog);
        let after = pins(&layout);
        for (old, new) in before.iter().zip(after.iter()) {
            let (old, new) = (old.unwrap(), new.unwrap());
            assert_eq!((new.x, new.y), (-old.x, old.y));
        }
        assert_eq!(doc.wires.len(), 1);
        assert!(doc.integrity_errors().is_empty());
        history.undo(&mut doc).unwrap();
        assert!(!doc.symbols[0].mirrored);
    }

    #[test]
    fn a_comment_is_about_something_that_exists_and_is_bounded() {
        let doc = doc();
        assert!(matches!(
            add_comment(&doc, Subject::Wire(circuit_model::WireId(99)), "x", None),
            Err(CommandError::Missing(_))
        ));
        assert!(add_comment(&doc, Subject::Document, "", None).is_err());
        assert!(add_comment(&doc, Subject::Document, &"x".repeat(201), None).is_err());
        assert!(add_comment(&doc, Subject::Document, "fine", Some(&"y".repeat(33))).is_err());
        assert!(add_comment(&doc, Subject::Document, "fine", None).is_ok());
    }

    #[test]
    fn deleting_a_device_takes_its_views_and_clears_its_channels() {
        let mut doc = doc();
        let mut history = History::new();
        let device = doc.devices[0].id;
        let (tx, view) = add_view(&doc, device, GridPoint::new(10, 0), 4, 6).unwrap();
        history.execute(&mut doc, tx).unwrap();
        let channel = PlcChannel {
            address: "Q0.0".to_owned(),
            direction: PlcDirection::Output,
            device: Some(device),
            label: None,
        };
        let (tx, plc) = add_plc(&doc, "A1", None, None, vec![channel]).unwrap();
        history.execute(&mut doc, tx).unwrap();
        assert!(add_plc(&doc, "A1", None, None, Vec::new()).is_err());
        assert!(add_view(&doc, device, GridPoint::new(0, 0), 0, 4).is_err());

        let delete = crate::ops::delete(
            &doc,
            &ComponentCatalog::builtin(),
            &BTreeSet::from([Subject::Device(device)]),
        )
        .unwrap();
        history.execute(&mut doc, delete).unwrap();
        assert!(doc.views.is_empty());
        assert_eq!(doc.plcs[0].channels[0].device, None);
        assert!(doc.integrity_errors().is_empty());
        history.undo(&mut doc).unwrap();
        assert_eq!(doc.views[0].id, view);
        assert_eq!(doc.plcs[0].id, plc);
        assert_eq!(doc.plcs[0].channels[0].device, Some(device));
    }
}
