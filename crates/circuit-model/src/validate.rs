//! Structural validation that needs no component catalog.
//!
//! Every check accumulates; none stops at the first problem. Subjects are
//! chosen canonically (smallest first) so the result is identical for any
//! ordering of the document collections.

use std::collections::{BTreeMap, BTreeSet};

use crate::diagnostics::{Diagnostic, DiagnosticCode, Severity, Subject, sort_diagnostics};
use crate::geometry::GridRect;
use crate::ids::WireId;
use crate::ids::{DeviceId, JunctionId, MAX_ENTITY_ID, SheetId, SymbolId};
use crate::schematic::{CURRENT_SCHEMA_VERSION, Endpoint, Schematic};
use crate::sheet::{MAX_SHEET_SIDE, MAX_ZONES};

pub(crate) fn validate(doc: &Schematic) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    check_schema_version(doc, &mut out);
    check_id_ranges(doc, &mut out);
    check_unique_ids(doc, &mut out);
    check_references(doc, &mut out);
    check_junction_degrees(doc, &mut out);
    check_designators(doc, &mut out);
    check_supply_profiles(doc, &mut out);
    check_sheets(doc, &mut out);
    check_wire_numbers(doc, &mut out);
    check_waivers(doc, &mut out);
    check_texts(doc, &mut out);
    check_layers(doc, &mut out);
    check_annotations(doc, &mut out);
    check_sheet_kinds(doc, &mut out);
    check_plc_channels(doc, &mut out);

    sort_diagnostics(&mut out);
    out
}

/// The checks of [`validate`] that report the five integrity codes
/// (`UnsupportedSchemaVersion`, `IdOutOfRange`, `DuplicateId`,
/// `DanglingReference`, `SelfLoopWire`) and nothing else. The result is the
/// same as filtering [`validate`] by those codes, at a fraction of the cost.
pub(crate) fn integrity(doc: &Schematic) -> Vec<Diagnostic> {
    let mut out = Vec::new();

    check_schema_version(doc, &mut out);
    check_id_ranges(doc, &mut out);
    check_unique_ids(doc, &mut out);
    check_references(doc, &mut out);

    sort_diagnostics(&mut out);
    out
}

/// Every entity of the document as `(raw id, subject)`.
fn entities(doc: &Schematic) -> impl Iterator<Item = (u64, Subject)> + '_ {
    let sheets = doc.sheets.iter().map(|s| (s.id.0, Subject::Sheet(s.id)));
    let supplies = doc.supplies.iter().map(|s| (s.id.0, Subject::Supply(s.id)));
    let devices = doc.devices.iter().map(|d| (d.id.0, Subject::Device(d.id)));
    let symbols = doc.symbols.iter().map(|s| (s.id.0, Subject::Symbol(s.id)));
    let junctions = doc
        .junctions
        .iter()
        .map(|j| (j.id.0, Subject::Junction(j.id)));
    let wires = doc.wires.iter().map(|w| (w.id.0, Subject::Wire(w.id)));
    let texts = doc.texts.iter().map(|t| (t.id.0, Subject::Text(t.id)));
    let layers = doc.layers.iter().map(|l| (l.id.0, Subject::Layer(l.id)));
    let shapes = doc.shapes.iter().map(|s| (s.id.0, Subject::Shape(s.id)));
    let comments = doc
        .comments
        .iter()
        .map(|c| (c.id.0, Subject::Comment(c.id)));
    let groups = doc.groups.iter().map(|g| (g.id.0, Subject::Group(g.id)));
    let views = doc.views.iter().map(|v| (v.id.0, Subject::View(v.id)));
    let plcs = doc.plcs.iter().map(|p| (p.id.0, Subject::Plc(p.id)));
    sheets
        .chain(supplies)
        .chain(devices)
        .chain(symbols)
        .chain(junctions)
        .chain(wires)
        .chain(texts)
        .chain(layers)
        .chain(shapes)
        .chain(comments)
        .chain(groups)
        .chain(views)
        .chain(plcs)
}

fn check_schema_version(doc: &Schematic, out: &mut Vec<Diagnostic>) {
    if doc.schema_version != CURRENT_SCHEMA_VERSION {
        out.push(Diagnostic::new(
            DiagnosticCode::UnsupportedSchemaVersion,
            Subject::Document,
            format!(
                "document has schema version {} but this build works on version {CURRENT_SCHEMA_VERSION}",
                doc.schema_version
            ),
        ));
    }
}

fn check_id_ranges(doc: &Schematic, out: &mut Vec<Diagnostic>) {
    // `next_id` may be one past the largest valid ID, never more.
    if doc.next_id == 0 || doc.next_id.saturating_sub(1) > MAX_ENTITY_ID {
        out.push(Diagnostic::new(
            DiagnosticCode::IdOutOfRange,
            Subject::Document,
            format!(
                "next_id {} is outside 1..={}",
                doc.next_id,
                MAX_ENTITY_ID.saturating_add(1)
            ),
        ));
    }

    for (id, subject) in entities(doc) {
        let problem = if id == 0 {
            Some("id 0 is not a valid identifier".to_owned())
        } else if id > MAX_ENTITY_ID {
            Some(format!("id {id} exceeds the maximum {MAX_ENTITY_ID}"))
        } else if id >= doc.next_id {
            Some(format!("id {id} is not below next_id {}", doc.next_id))
        } else {
            None
        };
        if let Some(message) = problem {
            out.push(Diagnostic::new(
                DiagnosticCode::IdOutOfRange,
                subject,
                message,
            ));
        }
    }
}

/// IDs come from one counter, so they must be unique across every collection,
/// not only inside each one.
fn check_unique_ids(doc: &Schematic, out: &mut Vec<Diagnostic>) {
    let mut by_id: BTreeMap<u64, BTreeSet<Subject>> = BTreeMap::new();
    let mut repeated_within_kind: BTreeSet<(u64, Subject)> = BTreeSet::new();
    for (id, subject) in entities(doc) {
        if !by_id.entry(id).or_default().insert(subject.clone()) {
            repeated_within_kind.insert((id, subject));
        }
    }

    for (id, subject) in repeated_within_kind {
        out.push(Diagnostic::new(
            DiagnosticCode::DuplicateId,
            subject,
            format!("id {id} is used more than once by the same kind of entity"),
        ));
    }

    for (id, subjects) in by_id {
        let mut subjects = subjects.into_iter();
        let (Some(first), Some(second)) = (subjects.next(), subjects.next()) else {
            continue;
        };
        let mut related = vec![second];
        related.extend(subjects);
        out.push(
            Diagnostic::new(
                DiagnosticCode::DuplicateId,
                first,
                format!("id {id} is shared by different kinds of entity"),
            )
            .with_related(related),
        );
    }
}

fn check_references(doc: &Schematic, out: &mut Vec<Diagnostic>) {
    let devices: BTreeSet<DeviceId> = doc.devices.iter().map(|d| d.id).collect();
    let symbols: BTreeSet<SymbolId> = doc.symbols.iter().map(|s| s.id).collect();
    let junctions: BTreeSet<JunctionId> = doc.junctions.iter().map(|j| j.id).collect();

    let layers: BTreeSet<crate::ids::LayerId> = doc.layers.iter().map(|l| l.id).collect();
    let on_missing_layer =
        |layer: Option<crate::ids::LayerId>| layer.filter(|id| !layers.contains(id));
    for (subject, layer) in doc
        .symbols
        .iter()
        .map(|s| (Subject::Symbol(s.id), s.layer))
        .chain(doc.wires.iter().map(|w| (Subject::Wire(w.id), w.layer)))
        .chain(doc.texts.iter().map(|t| (Subject::Text(t.id), t.layer)))
        .chain(doc.shapes.iter().map(|s| (Subject::Shape(s.id), s.layer)))
        .chain(doc.views.iter().map(|v| (Subject::View(v.id), v.layer)))
    {
        if let Some(missing) = on_missing_layer(layer) {
            out.push(
                Diagnostic::new(
                    DiagnosticCode::DanglingReference,
                    subject.clone(),
                    format!("{subject} is on layer {missing} which does not exist"),
                )
                .with_related(vec![Subject::Layer(missing)]),
            );
        }
    }

    check_annotation_references(doc, &devices, out);

    for symbol in &doc.symbols {
        if !devices.contains(&symbol.device_id) {
            out.push(
                Diagnostic::new(
                    DiagnosticCode::DanglingReference,
                    Subject::Symbol(symbol.id),
                    format!(
                        "symbol {} refers to device {} which does not exist",
                        symbol.id, symbol.device_id
                    ),
                )
                .with_related(vec![Subject::Device(symbol.device_id)]),
            );
        }
    }

    for wire in &doc.wires {
        if wire.from == wire.to {
            out.push(Diagnostic::new(
                DiagnosticCode::SelfLoopWire,
                Subject::Wire(wire.id),
                format!("wire {} starts and ends at the same point", wire.id),
            ));
        }

        for endpoint in [&wire.from, &wire.to] {
            match endpoint {
                Endpoint::Terminal(pin) if !symbols.contains(&pin.symbol_id) => out.push(
                    Diagnostic::new(
                        DiagnosticCode::DanglingReference,
                        Subject::Wire(wire.id),
                        format!(
                            "wire {} ends at symbol {} which does not exist",
                            wire.id, pin.symbol_id
                        ),
                    )
                    .with_related(vec![Subject::Symbol(pin.symbol_id)]),
                ),
                Endpoint::Junction(junction) if !junctions.contains(junction) => out.push(
                    Diagnostic::new(
                        DiagnosticCode::DanglingReference,
                        Subject::Wire(wire.id),
                        format!(
                            "wire {} ends at junction {junction} which does not exist",
                            wire.id
                        ),
                    )
                    .with_related(vec![Subject::Junction(*junction)]),
                ),
                Endpoint::Terminal(_) | Endpoint::Junction(_) => {}
            }
        }
    }
}

fn check_junction_degrees(doc: &Schematic, out: &mut Vec<Diagnostic>) {
    let mut wires_at: BTreeMap<JunctionId, u32> = doc.junctions.iter().map(|j| (j.id, 0)).collect();
    for wire in &doc.wires {
        let mut touched = BTreeSet::new();
        for endpoint in [&wire.from, &wire.to] {
            if let Endpoint::Junction(junction) = endpoint {
                touched.insert(*junction);
            }
        }
        for junction in touched {
            if let Some(count) = wires_at.get_mut(&junction) {
                *count = count.saturating_add(1);
            }
        }
    }

    for (junction, count) in wires_at {
        match count {
            0 => out.push(Diagnostic::new(
                DiagnosticCode::JunctionUnconnected,
                Subject::Junction(junction),
                format!("junction {junction} has no wire"),
            )),
            1 => out.push(Diagnostic::new(
                DiagnosticCode::JunctionDangling,
                Subject::Junction(junction),
                format!("junction {junction} has a single wire"),
            )),
            _ => {}
        }
    }
}

fn check_designators(doc: &Schematic, out: &mut Vec<Diagnostic>) {
    let mut by_reference: BTreeMap<&str, BTreeSet<DeviceId>> = BTreeMap::new();
    for device in &doc.devices {
        if !device.reference.is_valid() {
            out.push(Diagnostic::new(
                DiagnosticCode::InvalidReference,
                Subject::Device(device.id),
                "reference designator must be an uppercase letter followed by up to 15 uppercase letters, digits or underscores",
            ));
        }
        by_reference
            .entry(device.reference.0.as_str())
            .or_default()
            .insert(device.id);
    }

    for devices in by_reference.into_values() {
        let mut devices = devices.into_iter();
        let (Some(first), Some(second)) = (devices.next(), devices.next()) else {
            continue;
        };
        let mut related = vec![Subject::Device(second)];
        related.extend(devices.map(Subject::Device));
        out.push(
            Diagnostic::new(
                DiagnosticCode::DuplicateReference,
                Subject::Device(first),
                "reference designator is used by more than one device",
            )
            .with_related(related),
        );
    }
}

/// Sheet numbers are unique and positive; a frame has zones, a size within
/// [`MAX_SHEET_SIDE`], stays in the coordinate range and does not overlap
/// another frame.
fn check_sheets(doc: &Schematic, out: &mut Vec<Diagnostic>) {
    let mut numbers: BTreeMap<u32, BTreeSet<SheetId>> = BTreeMap::new();
    for sheet in &doc.sheets {
        numbers.entry(sheet.number).or_default().insert(sheet.id);
    }
    for (number, ids) in numbers {
        let mut ids = ids.into_iter();
        let Some(first) = ids.next() else {
            continue;
        };
        let others: Vec<Subject> = ids.map(Subject::Sheet).collect();
        if number == 0 || !others.is_empty() {
            let text = if number == 0 {
                "sheet number 0: sheets are numbered from 1".to_owned()
            } else {
                format!("sheet number {number} is used by more than one sheet")
            };
            out.push(
                Diagnostic::new(
                    DiagnosticCode::DuplicateSheetNumber,
                    Subject::Sheet(first),
                    text,
                )
                .with_related(others),
            );
        }
    }

    let mut frames: Vec<(SheetId, GridRect)> = Vec::new();
    for sheet in &doc.sheets {
        let invalid = |text: String| {
            Diagnostic::new(DiagnosticCode::InvalidSheet, Subject::Sheet(sheet.id), text)
        };
        let zones = sheet.zones;
        if !(1..=MAX_ZONES).contains(&zones.columns) || !(1..=MAX_ZONES).contains(&zones.rows) {
            out.push(invalid(format!(
                "sheet {} must have 1 to {MAX_ZONES} zone columns and rows",
                sheet.number
            )));
        }
        let (width, height) = sheet.extent();
        if width == 0 || height == 0 || width > MAX_SHEET_SIDE || height > MAX_SHEET_SIDE {
            out.push(invalid(format!(
                "sheet {} must be 1 to {MAX_SHEET_SIDE} grid units on each side",
                sheet.number
            )));
            continue;
        }
        let Some(frame) = sheet.frame() else {
            out.push(invalid(format!(
                "sheet {} leaves the coordinate range",
                sheet.number
            )));
            continue;
        };
        frames.push((sheet.id, frame));
    }
    frames.sort_by_key(|(id, _)| *id);
    for (index, (id, frame)) in frames.iter().enumerate() {
        let overlapping: Vec<Subject> = frames
            .iter()
            .skip(index.saturating_add(1))
            .filter(|(_, other)| {
                frame.min.x < other.max.x
                    && other.min.x < frame.max.x
                    && frame.min.y < other.max.y
                    && other.min.y < frame.max.y
            })
            .map(|(other, _)| Subject::Sheet(*other))
            .collect();
        if !overlapping.is_empty() {
            out.push(
                Diagnostic::new(
                    DiagnosticCode::InvalidSheet,
                    Subject::Sheet(*id),
                    "the frame of this sheet overlaps another sheet",
                )
                .with_related(overlapping),
            );
        }
    }
}

/// Text notes that are empty, too long or have a control character
/// (ADR-0015).
fn check_texts(doc: &Schematic, out: &mut Vec<Diagnostic>) {
    for note in &doc.texts {
        if !crate::documentation::TextNote::is_valid_text(&note.text) {
            out.push(Diagnostic::new(
                DiagnosticCode::InvalidText,
                Subject::Text(note.id),
                format!(
                    "text {} must be 1 to {} characters, with no control character",
                    note.id,
                    crate::documentation::MAX_NOTE_CHARS
                ),
            ));
        }
    }
}

/// References of the documentation of ADR-0018: the device of a view or a
/// channel, the element of a comment and the members of a group.
fn check_annotation_references(
    doc: &Schematic,
    devices: &BTreeSet<DeviceId>,
    out: &mut Vec<Diagnostic>,
) {
    let dangling = |out: &mut Vec<Diagnostic>, subject: Subject, missing: Subject| {
        out.push(
            Diagnostic::new(
                DiagnosticCode::DanglingReference,
                subject.clone(),
                format!("{subject} refers to {missing}, which does not exist"),
            )
            .with_related(vec![missing]),
        );
    };
    for view in &doc.views {
        if !devices.contains(&view.device_id) {
            dangling(out, Subject::View(view.id), Subject::Device(view.device_id));
        }
    }
    for plc in &doc.plcs {
        let missing: BTreeSet<DeviceId> = plc
            .channels
            .iter()
            .filter_map(|channel| channel.device)
            .filter(|device| !devices.contains(device))
            .collect();
        for device in missing {
            dangling(out, Subject::Plc(plc.id), Subject::Device(device));
        }
    }
    if doc.comments.is_empty() && doc.groups.is_empty() {
        return;
    }
    let known: BTreeSet<Subject> = entities(doc).map(|(_, subject)| subject).collect();
    let exists = |subject: &Subject| match subject {
        Subject::Document => true,
        Subject::Pin(pin) => known.contains(&Subject::Symbol(pin.symbol_id)),
        other => known.contains(other),
    };
    for comment in &doc.comments {
        if !exists(&comment.subject) || matches!(comment.subject, Subject::Comment(_)) {
            dangling(out, Subject::Comment(comment.id), comment.subject.clone());
        }
    }
    for group in &doc.groups {
        let missing: BTreeSet<&Subject> = group
            .members
            .iter()
            .filter(|member| !exists(member))
            .collect();
        for member in missing {
            dangling(out, Subject::Group(group.id), member.clone());
        }
    }
}

/// The bounds of ADR-0018: the client, wire specifications, datasheets,
/// shapes, comments, groups, views and PLC modules.
fn check_annotations(doc: &Schematic, out: &mut Vec<Diagnostic>) {
    use crate::annotations::{
        Group, MAX_AUTHOR_CHARS, MAX_CLIENT_CHARS, MAX_COMMENT_CHARS, MAX_GROUP_MEMBERS,
        MAX_GROUP_NAME_CHARS, is_valid_datasheet, is_valid_text,
    };
    let mut warn = |subject: Subject, message: String| {
        out.push(Diagnostic::new(
            DiagnosticCode::InvalidAnnotation,
            subject,
            message,
        ));
    };
    if let Some(client) = &doc.project.client
        && !is_valid_text(client, MAX_CLIENT_CHARS)
    {
        warn(
            Subject::Document,
            format!(
                "the client must be 1 to {MAX_CLIENT_CHARS} characters, with no control character"
            ),
        );
    }
    for wire in &doc.wires {
        if wire.spec.as_ref().is_some_and(|spec| !spec.is_valid()) {
            warn(
                Subject::Wire(wire.id),
                format!(
                    "each field of the specification of wire {} must be 1 to {} characters",
                    wire.id,
                    crate::annotations::MAX_SPEC_CHARS
                ),
            );
        }
    }
    for device in &doc.devices {
        if let Some(link) = device
            .info
            .as_ref()
            .and_then(|info| info.datasheet.as_deref())
            && !is_valid_datasheet(link)
        {
            warn(
                Subject::Device(device.id),
                format!(
                    "the datasheet of {} must be an http, https or file link, or a relative path",
                    device.reference.0
                ),
            );
        }
    }
    for shape in &doc.shapes {
        if let Some(problem) = shape.kind.problem() {
            warn(
                Subject::Shape(shape.id),
                format!("shape {}: {problem}", shape.id),
            );
        }
    }
    for comment in &doc.comments {
        let author_bad = comment
            .author
            .as_deref()
            .is_some_and(|author| !is_valid_text(author, MAX_AUTHOR_CHARS));
        if !is_valid_text(&comment.text, MAX_COMMENT_CHARS) || author_bad {
            warn(
                Subject::Comment(comment.id),
                format!(
                    "comment {} must be 1 to {MAX_COMMENT_CHARS} characters, its author at most {MAX_AUTHOR_CHARS}",
                    comment.id
                ),
            );
        }
    }
    let mut owner: BTreeMap<&Subject, crate::ids::GroupId> = BTreeMap::new();
    for group in &doc.groups {
        let problem = if !is_valid_text(&group.name, MAX_GROUP_NAME_CHARS) {
            Some(format!(
                "a group name is 1 to {MAX_GROUP_NAME_CHARS} characters"
            ))
        } else if group.members.is_empty() || group.members.len() > MAX_GROUP_MEMBERS {
            Some(format!("a group has 1 to {MAX_GROUP_MEMBERS} members"))
        } else if let Some(member) = group.members.iter().find(|member| !Group::may_hold(member)) {
            Some(format!("a group cannot hold {member}"))
        } else {
            group.members.iter().find_map(|member| {
                owner
                    .insert(member, group.id)
                    .filter(|other| *other != group.id)
                    .map(|other| format!("{member} is also in group {other}"))
            })
        };
        if let Some(problem) = problem {
            warn(
                Subject::Group(group.id),
                format!("group {}: {problem}", group.id),
            );
        }
    }
    for view in &doc.views {
        if !view.has_valid_size() || view.bounds().is_none() {
            warn(
                Subject::View(view.id),
                format!(
                    "view {} must be 1 to {} grid units wide and high",
                    view.id,
                    crate::annotations::MAX_VIEW_SIDE
                ),
            );
        }
    }
    for plc in &doc.plcs {
        if let Some(problem) = plc.problem() {
            warn(Subject::Plc(plc.id), format!("{}: {problem}", plc.name));
        }
    }
}

/// Device views outside a single-line or panel-layout sheet, and symbols
/// inside one (ADR-0018).
fn check_sheet_kinds(doc: &Schematic, out: &mut Vec<Diagnostic>) {
    if doc.views.is_empty() && !doc.sheets.iter().any(|sheet| sheet.kind.shows_views()) {
        return;
    }
    let shows_views =
        |point| crate::sheet::sheet_at(doc, point).is_some_and(|sheet| sheet.kind.shows_views());
    for view in &doc.views {
        if !shows_views(view.position) {
            out.push(Diagnostic::new(
                DiagnosticCode::ViewOffItsSheet,
                Subject::View(view.id),
                format!(
                    "view {} is not on a single-line or panel-layout sheet",
                    view.id
                ),
            ));
        }
    }
    for symbol in &doc.symbols {
        if shows_views(symbol.position) {
            out.push(Diagnostic::new(
                DiagnosticCode::ViewOffItsSheet,
                Subject::Symbol(symbol.id),
                format!(
                    "symbol {} is on a single-line or panel-layout sheet, where devices are shown as views",
                    symbol.id
                ),
            ));
        }
    }
}

/// Two channels of a module with one address, or one device on two channels
/// of the same direction (ADR-0018).
fn check_plc_channels(doc: &Schematic, out: &mut Vec<Diagnostic>) {
    let mut driven: BTreeMap<(DeviceId, crate::annotations::PlcDirection), crate::ids::PlcId> =
        BTreeMap::new();
    for plc in &doc.plcs {
        let mut addresses = BTreeSet::new();
        let mut problems = BTreeSet::new();
        for channel in &plc.channels {
            if !addresses.insert(channel.address.as_str()) {
                problems.insert(format!("address {} is used twice", channel.address));
            }
            if let Some(device) = channel.device
                && driven.insert((device, channel.direction), plc.id).is_some()
            {
                problems.insert(format!(
                    "device {device} is on two {} channels",
                    channel.direction.name()
                ));
            }
        }
        for problem in problems {
            out.push(Diagnostic::new(
                DiagnosticCode::PlcChannelConflict,
                Subject::Plc(plc.id),
                format!("{}: {problem}", plc.name),
            ));
        }
    }
}

/// Layers with a bad or repeated name (ADR-0015).
fn check_layers(doc: &Schematic, out: &mut Vec<Diagnostic>) {
    let mut seen = BTreeSet::new();
    for layer in &doc.layers {
        if !crate::layers::Layer::is_valid_name(&layer.name) {
            out.push(Diagnostic::new(
                DiagnosticCode::InvalidLayer,
                Subject::Layer(layer.id),
                format!(
                    "layer {} needs a name of 1 to {} characters, with no control character or space at either end",
                    layer.id,
                    crate::layers::MAX_LAYER_NAME_CHARS
                ),
            ));
        } else if !seen.insert(layer.name.as_str()) {
            out.push(Diagnostic::new(
                DiagnosticCode::InvalidLayer,
                Subject::Layer(layer.id),
                format!("another layer is already named `{}`", layer.name),
            ));
        }
    }
}

/// Waivers that cannot work (ADR-0014): a code that is not one, an error or
/// `OC-STR-020` itself, a subject that is not in the document, or a repeat.
fn check_waivers(doc: &Schematic, out: &mut Vec<Diagnostic>) {
    if doc.waivers.is_empty() {
        return;
    }
    let known: BTreeSet<Subject> = entities(doc).map(|(_, subject)| subject).collect();
    let exists = |subject: &Subject| match subject {
        Subject::Document => true,
        Subject::Pin(pin) => known.contains(&Subject::Symbol(pin.symbol_id)),
        other => known.contains(other),
    };
    let mut seen: BTreeSet<(&str, &Subject)> = BTreeSet::new();
    for waiver in &doc.waivers {
        let problem = match DiagnosticCode::parse(&waiver.code) {
            None => "is not a diagnostic code",
            Some(DiagnosticCode::InvalidWaiver) => "cannot be waived",
            Some(code) if code.default_severity() == Severity::Error => {
                "is an error, and an error cannot be waived"
            }
            Some(_) if !exists(&waiver.subject) => "is about something that is not in the document",
            Some(_) if !seen.insert((waiver.code.as_str(), &waiver.subject)) => "is given twice",
            Some(_) => continue,
        };
        out.push(Diagnostic::new(
            DiagnosticCode::InvalidWaiver,
            waiver.subject.clone(),
            format!(
                "the waiver of `{}` for {} {problem}",
                waiver.code, waiver.subject
            ),
        ));
    }
}

/// Two wires of one net that carry different numbers (ADR-0014). The nets
/// are found from the wire endpoints alone, which is all a net with two wires
/// needs, so the check needs no catalog. The subject is the numbered wire with
/// the smallest ID, which is the one whose number wins.
fn check_wire_numbers(doc: &Schematic, out: &mut Vec<Diagnostic>) {
    let numbered: Vec<(WireId, &str)> = doc
        .wires
        .iter()
        .filter_map(|wire| match wire.number.as_deref() {
            Some(number) if !number.is_empty() => Some((wire.id, number)),
            _ => None,
        })
        .collect();
    if numbered.len() < 2 {
        return;
    }

    let mut index: BTreeMap<&Endpoint, usize> = BTreeMap::new();
    for wire in &doc.wires {
        for endpoint in [&wire.from, &wire.to] {
            let next = index.len();
            index.entry(endpoint).or_insert(next);
        }
    }
    let mut parent: Vec<usize> = (0..index.len()).collect();
    let find = |parent: &mut Vec<usize>, mut node: usize| -> usize {
        while let Some(&up) = parent.get(node) {
            if up == node {
                break;
            }
            let grand = parent.get(up).copied().unwrap_or(up);
            if let Some(slot) = parent.get_mut(node) {
                *slot = grand;
            }
            node = grand;
        }
        node
    };
    for wire in &doc.wires {
        let (Some(&a), Some(&b)) = (index.get(&wire.from), index.get(&wire.to)) else {
            continue;
        };
        let (a, b) = (find(&mut parent, a), find(&mut parent, b));
        if a == b {
            continue;
        }
        if let Some(slot) = parent.get_mut(a.max(b)) {
            *slot = a.min(b);
        }
    }

    let mut nets: BTreeMap<usize, Vec<(WireId, &str)>> = BTreeMap::new();
    for (id, number) in numbered {
        let Some(&node) = doc
            .wires
            .iter()
            .find(|wire| wire.id == id)
            .and_then(|wire| index.get(&wire.from))
        else {
            continue;
        };
        nets.entry(find(&mut parent, node))
            .or_default()
            .push((id, number));
    }
    for mut wires in nets.into_values() {
        wires.sort_unstable();
        wires.dedup();
        let Some(&(first, number)) = wires.first() else {
            continue;
        };
        let others: Vec<Subject> = wires
            .iter()
            .filter(|(_, other)| *other != number)
            .map(|(id, _)| Subject::Wire(*id))
            .collect();
        if others.is_empty() {
            continue;
        }
        out.push(
            Diagnostic::new(
                DiagnosticCode::WireNumberConflict,
                Subject::Wire(first),
                format!("wires of one net carry different numbers; `{number}` wins"),
            )
            .with_related(others),
        );
    }
}

fn check_supply_profiles(doc: &Schematic, out: &mut Vec<Diagnostic>) {
    for supply in &doc.supplies {
        if let Some(reason) = supply.inconsistency() {
            out.push(Diagnostic::new(
                DiagnosticCode::InvalidSupplyProfile,
                Subject::Supply(supply.id),
                reason,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::geometry::{GridPoint, QuarterTurn};
    use crate::ids::{SupplyId, WireId};
    use crate::schematic::{
        Device, Junction, ModelRef, PinRef, ReferenceDesignator, SymbolInstance, SymbolKey,
        TerminalKey, Wire,
    };
    use crate::sheet::MAX_SHEET_SIDE;
    use crate::supply::{SupplyKind, SupplyProfile};

    fn device(id: u64, reference: &str) -> Device {
        Device {
            id: DeviceId(id),
            reference: ReferenceDesignator(reference.to_owned()),
            model: ModelRef {
                id: "contactor.generic".to_owned(),
                version: 1,
            },
            params: BTreeMap::new(),
            info: None,
        }
    }

    fn symbol(id: u64, device_id: u64) -> SymbolInstance {
        SymbolInstance {
            id: SymbolId(id),
            device_id: DeviceId(device_id),
            symbol_key: SymbolKey("coil".to_owned()),
            slot: None,
            position: GridPoint::default(),
            rotation: QuarterTurn::Deg0,
            layer: None,
            mirrored: false,
        }
    }

    fn pin(symbol_id: u64, terminal: &str) -> Endpoint {
        Endpoint::Terminal(PinRef {
            symbol_id: SymbolId(symbol_id),
            terminal: TerminalKey(terminal.to_owned()),
        })
    }

    fn wire(id: u64, from: Endpoint, to: Endpoint) -> Wire {
        Wire {
            id: WireId(id),
            from,
            to,
            route: Vec::new(),
            number: None,
            layer: None,
            spec: None,
        }
    }

    /// K1 coil (symbol 3) wired from both terminals to junction 4.
    fn valid() -> Schematic {
        let mut doc = Schematic::new("valid");
        doc.supplies.push(SupplyProfile {
            id: SupplyId(1),
            kind: SupplyKind::Dc,
            nominal_voltage_mv: 24_000,
            phase_voltage_mv: None,
            frequency_millihz: None,
            phase_sequence: None,
        });
        doc.devices.push(device(2, "K1"));
        doc.symbols.push(symbol(3, 2));
        doc.junctions.push(Junction {
            id: JunctionId(4),
            position: GridPoint::new(2, 0),
        });
        doc.wires
            .push(wire(5, pin(3, "a1"), Endpoint::Junction(JunctionId(4))));
        doc.wires
            .push(wire(6, pin(3, "a2"), Endpoint::Junction(JunctionId(4))));
        doc.next_id = 7;
        doc
    }

    fn codes(doc: &Schematic) -> Vec<(DiagnosticCode, Subject)> {
        doc.validate()
            .into_iter()
            .map(|d| (d.code(), d.subject().clone()))
            .collect()
    }

    #[test]
    fn two_numbers_on_one_net_are_a_warning_on_the_wire_that_wins() {
        let mut doc = valid();
        doc.wires[0].number = Some("A".to_owned());
        doc.wires[1].number = Some("B".to_owned());

        let diagnostics = doc.validate();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code(), DiagnosticCode::WireNumberConflict);
        assert_eq!(diagnostics[0].subject(), &Subject::Wire(WireId(5)));
        assert_eq!(diagnostics[0].related(), [Subject::Wire(WireId(6))]);
    }

    #[test]
    fn the_same_number_twice_on_a_net_and_empty_numbers_are_no_conflict() {
        let mut doc = valid();
        doc.wires[0].number = Some("A".to_owned());
        doc.wires[1].number = Some("A".to_owned());
        assert_eq!(codes(&doc), []);

        doc.wires[1].number = Some(String::new());
        assert_eq!(codes(&doc), []);
    }

    #[test]
    fn different_numbers_on_different_nets_are_no_conflict() {
        let mut doc = valid();
        doc.junctions.push(Junction {
            id: JunctionId(8),
            position: GridPoint::new(9, 9),
        });
        doc.wires[1].to = Endpoint::Junction(JunctionId(8));
        doc.next_id = 9;
        doc.wires[0].number = Some("A".to_owned());
        doc.wires[1].number = Some("B".to_owned());

        assert!(
            !doc.validate()
                .iter()
                .any(|d| d.code() == DiagnosticCode::WireNumberConflict)
        );
    }

    #[test]
    fn the_wire_number_conflict_does_not_depend_on_the_order_of_the_wires() {
        let mut doc = valid();
        doc.wires[0].number = Some("A".to_owned());
        doc.wires[1].number = Some("B".to_owned());
        let expected = doc.validate();

        doc.wires.reverse();

        assert_eq!(doc.validate(), expected);
    }

    fn waived(code: &str, subject: Subject) -> crate::documentation::Waiver {
        crate::documentation::Waiver {
            code: code.to_owned(),
            subject,
            reason: "because".to_owned(),
        }
    }

    #[test]
    fn a_waiver_that_works_is_not_reported() {
        let mut doc = valid();
        doc.waivers = vec![waived("OC-STR-018", Subject::Wire(WireId(5)))];

        assert_eq!(codes(&doc), []);
    }

    #[test]
    fn a_waiver_that_cannot_work_is_a_warning_that_says_why() {
        let mut doc = valid();
        doc.waivers = vec![
            waived("OC-NOPE-1", Subject::Wire(WireId(5))),
            waived("OC-STR-013", Subject::Wire(WireId(5))),
            waived("OC-STR-020", Subject::Wire(WireId(5))),
            waived("OC-STR-018", Subject::Wire(WireId(99))),
            waived("OC-STR-018", Subject::Wire(WireId(5))),
            waived("OC-STR-018", Subject::Wire(WireId(5))),
        ];

        let messages: Vec<String> = doc
            .validate()
            .into_iter()
            .map(|d| {
                assert_eq!(d.code(), DiagnosticCode::InvalidWaiver);
                d.message().to_owned()
            })
            .collect();

        assert_eq!(messages.len(), 5);
        assert!(messages.iter().any(|m| m.contains("not a diagnostic code")));
        assert!(
            messages
                .iter()
                .any(|m| m.contains("an error cannot be waived"))
        );
        assert!(
            messages
                .iter()
                .any(|m| m.contains("OC-STR-020") && m.contains("cannot be waived"))
        );
        assert!(messages.iter().any(|m| m.contains("not in the document")));
        assert!(messages.iter().any(|m| m.contains("given twice")));
    }

    #[test]
    fn accepts_a_consistent_document() {
        assert_eq!(valid().validate(), []);
    }

    #[test]
    fn rejects_a_schema_version_other_than_current() {
        let mut doc = valid();
        doc.schema_version = 999;

        assert_eq!(
            codes(&doc),
            [(DiagnosticCode::UnsupportedSchemaVersion, Subject::Document)]
        );
    }

    #[test]
    fn rejects_id_zero() {
        let mut doc = valid();
        doc.supplies[0].id = SupplyId(0);

        assert_eq!(
            codes(&doc),
            [(DiagnosticCode::IdOutOfRange, Subject::Supply(SupplyId(0)))]
        );
    }

    #[test]
    fn rejects_id_above_max_entity_id() {
        let mut doc = valid();
        doc.supplies[0].id = SupplyId(u64::MAX);
        doc.next_id = MAX_ENTITY_ID + 1;

        assert_eq!(
            codes(&doc),
            [(
                DiagnosticCode::IdOutOfRange,
                Subject::Supply(SupplyId(u64::MAX))
            )]
        );
    }

    #[test]
    fn rejects_id_not_below_next_id() {
        let mut doc = valid();
        doc.next_id = 6;

        assert_eq!(
            codes(&doc),
            [(DiagnosticCode::IdOutOfRange, Subject::Wire(WireId(6)))]
        );
    }

    #[test]
    fn rejects_next_id_of_zero_or_beyond_the_id_space() {
        let mut doc = Schematic::new("empty");
        doc.next_id = 0;
        assert_eq!(
            codes(&doc),
            [(DiagnosticCode::IdOutOfRange, Subject::Document)]
        );

        doc.next_id = MAX_ENTITY_ID + 2;
        assert_eq!(
            codes(&doc),
            [(DiagnosticCode::IdOutOfRange, Subject::Document)]
        );

        doc.next_id = MAX_ENTITY_ID + 1;
        assert_eq!(codes(&doc), []);
    }

    #[test]
    fn rejects_id_repeated_inside_one_collection() {
        let mut doc = valid();
        doc.devices.push(device(2, "K2"));

        assert_eq!(
            codes(&doc),
            [(DiagnosticCode::DuplicateId, Subject::Device(DeviceId(2)))]
        );
    }

    #[test]
    fn rejects_id_shared_across_collections() {
        let mut doc = valid();
        doc.junctions[0].id = JunctionId(2);
        for wire in &mut doc.wires {
            wire.to = Endpoint::Junction(JunctionId(2));
        }

        let diagnostics = doc.validate();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code(), DiagnosticCode::DuplicateId);
        assert_eq!(diagnostics[0].subject(), &Subject::Device(DeviceId(2)));
        assert_eq!(diagnostics[0].related(), [Subject::Junction(JunctionId(2))]);
    }

    #[test]
    fn rejects_symbol_of_unknown_device() {
        let mut doc = valid();
        doc.symbols[0].device_id = DeviceId(99);

        let diagnostics = doc.validate();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code(), DiagnosticCode::DanglingReference);
        assert_eq!(diagnostics[0].subject(), &Subject::Symbol(SymbolId(3)));
        assert_eq!(diagnostics[0].related(), [Subject::Device(DeviceId(99))]);
    }

    #[test]
    fn rejects_wire_to_unknown_symbol() {
        let mut doc = valid();
        doc.wires[0].from = pin(99, "a1");

        assert_eq!(
            codes(&doc),
            [(DiagnosticCode::DanglingReference, Subject::Wire(WireId(5)))]
        );
    }

    #[test]
    fn rejects_wire_to_unknown_junction() {
        let mut doc = valid();
        doc.wires[0].to = Endpoint::Junction(JunctionId(99));

        assert_eq!(
            codes(&doc),
            [
                (DiagnosticCode::DanglingReference, Subject::Wire(WireId(5))),
                (
                    DiagnosticCode::JunctionDangling,
                    Subject::Junction(JunctionId(4))
                ),
            ]
        );
    }

    #[test]
    fn rejects_wire_that_starts_and_ends_at_the_same_point() {
        let mut doc = valid();
        doc.wires[0].from = Endpoint::Junction(JunctionId(4));

        assert_eq!(
            codes(&doc),
            [(DiagnosticCode::SelfLoopWire, Subject::Wire(WireId(5)))]
        );
    }

    #[test]
    fn rejects_junction_without_wires() {
        let mut doc = valid();
        doc.wires.clear();
        doc.next_id = 5;

        assert_eq!(
            codes(&doc),
            [(
                DiagnosticCode::JunctionUnconnected,
                Subject::Junction(JunctionId(4))
            )]
        );
    }

    #[test]
    fn warns_about_junction_with_a_single_wire() {
        let mut doc = valid();
        doc.wires.pop();

        let diagnostics = doc.validate();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code(), DiagnosticCode::JunctionDangling);
        assert!(!diagnostics[0].is_error());
    }

    #[test]
    fn rejects_duplicate_reference_designator() {
        let mut doc = valid();
        doc.devices.push(device(7, "K1"));
        doc.devices.push(device(8, "K1"));
        doc.next_id = 9;

        let diagnostics = doc.validate();

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code(), DiagnosticCode::DuplicateReference);
        assert_eq!(diagnostics[0].subject(), &Subject::Device(DeviceId(2)));
        assert_eq!(
            diagnostics[0].related(),
            [Subject::Device(DeviceId(7)), Subject::Device(DeviceId(8))]
        );
    }

    #[test]
    fn rejects_malformed_reference_designator() {
        let mut doc = valid();
        doc.devices[0].reference = ReferenceDesignator("k1".to_owned());

        assert_eq!(
            codes(&doc),
            [(
                DiagnosticCode::InvalidReference,
                Subject::Device(DeviceId(2))
            )]
        );
    }

    #[test]
    fn rejects_inconsistent_supply_profile() {
        let mut doc = valid();
        doc.supplies[0].frequency_millihz = Some(60_000);

        assert_eq!(
            codes(&doc),
            [(
                DiagnosticCode::InvalidSupplyProfile,
                Subject::Supply(SupplyId(1))
            )]
        );
    }

    /// The integrity diagnostics are what `validate` reports for the same
    /// five codes, so a new check that emits one of them cannot be left out
    /// of `integrity` unnoticed.
    #[test]
    fn integrity_is_the_integrity_part_of_validate() {
        const INTEGRITY: [DiagnosticCode; 5] = [
            DiagnosticCode::UnsupportedSchemaVersion,
            DiagnosticCode::IdOutOfRange,
            DiagnosticCode::DuplicateId,
            DiagnosticCode::DanglingReference,
            DiagnosticCode::SelfLoopWire,
        ];
        let mut broken: Vec<Schematic> = vec![valid()];
        let mut doc = valid();
        doc.schema_version += 1;
        broken.push(doc);
        let mut doc = valid();
        doc.symbols[0].id = SymbolId(0);
        doc.next_id = 3;
        broken.push(doc);
        let mut doc = valid();
        doc.devices.push(device(2, "K2"));
        broken.push(doc);
        let mut doc = valid();
        doc.symbols[0].device_id = DeviceId(99);
        broken.push(doc);
        let mut doc = valid();
        doc.wires[0].from = pin(99, "a1");
        doc.wires[1].to = doc.wires[1].from.clone();
        broken.push(doc);
        // Findings of the checks `integrity` leaves out.
        let mut doc = valid();
        doc.devices.push(device(7, "K1"));
        doc.junctions.push(Junction {
            id: JunctionId(8),
            position: GridPoint::default(),
        });
        doc.next_id = 9;
        broken.push(doc);

        for doc in broken {
            let expected: Vec<Diagnostic> = doc
                .validate()
                .into_iter()
                .filter(|diagnostic| INTEGRITY.contains(&diagnostic.code()))
                .collect();
            assert_eq!(doc.integrity_errors(), expected);
        }
    }

    fn sheet(id: u64, number: u32, x: i32) -> crate::Sheet {
        crate::Sheet {
            id: crate::SheetId(id),
            number,
            title: "sheet".to_owned(),
            origin: GridPoint::new(x, 0),
            size: crate::SheetSize::A4,
            orientation: crate::Orientation::Landscape,
            zones: crate::Zones {
                columns: 8,
                rows: 6,
            },
            kind: Default::default(),
        }
    }

    #[test]
    fn side_by_side_sheets_with_distinct_numbers_are_valid() {
        let mut doc = valid();
        doc.sheets = vec![sheet(7, 1, 0), sheet(8, 2, 84)];
        doc.next_id = 9;

        assert_eq!(codes(&doc), []);
    }

    #[test]
    fn sheet_numbers_are_unique_and_start_at_one() {
        let mut doc = valid();
        doc.sheets = vec![sheet(7, 1, 0), sheet(8, 1, 100), sheet(9, 0, 200)];
        doc.next_id = 10;

        assert_eq!(
            codes(&doc),
            [
                (
                    DiagnosticCode::DuplicateSheetNumber,
                    Subject::Sheet(SheetId(7))
                ),
                (
                    DiagnosticCode::DuplicateSheetNumber,
                    Subject::Sheet(SheetId(9))
                ),
            ]
        );
    }

    #[test]
    fn overlapping_empty_huge_and_out_of_range_frames_are_refused() {
        let mut doc = valid();
        let mut no_zones = sheet(9, 3, 300);
        no_zones.zones.columns = 0;
        let mut huge = sheet(10, 4, 400);
        huge.size = crate::SheetSize::Custom {
            width: MAX_SHEET_SIDE + 1,
            height: 10,
        };
        let far = sheet(11, 5, i32::MAX - 10);
        doc.sheets = vec![sheet(7, 1, 0), sheet(8, 2, 50), no_zones, huge, far];
        doc.next_id = 12;

        let found = codes(&doc);
        for id in [7, 9, 10, 11] {
            assert!(
                found.contains(&(DiagnosticCode::InvalidSheet, Subject::Sheet(SheetId(id)))),
                "sheet {id}: {found:?}"
            );
        }
        // An overlap is reported once, on the smaller ID.
        assert!(!found.contains(&(DiagnosticCode::InvalidSheet, Subject::Sheet(SheetId(8)))));
    }

    #[test]
    fn a_sheet_id_shared_with_another_entity_is_a_duplicate() {
        let mut doc = valid();
        doc.sheets = vec![sheet(2, 1, 0)];

        assert!(
            codes(&doc)
                .iter()
                .any(|(code, _)| *code == DiagnosticCode::DuplicateId)
        );
    }

    #[test]
    fn result_does_not_depend_on_collection_order() {
        let mut doc = valid();
        doc.devices.push(device(2, "K1"));
        doc.devices.push(device(7, "K1"));
        doc.symbols.push(symbol(8, 77));
        doc.junctions.push(Junction {
            id: JunctionId(9),
            position: GridPoint::default(),
        });
        doc.wires.push(wire(
            10,
            Endpoint::Junction(JunctionId(98)),
            Endpoint::Junction(JunctionId(99)),
        ));
        doc.next_id = 11;
        let forward = doc.validate();

        doc.devices.reverse();
        doc.symbols.reverse();
        doc.junctions.reverse();
        doc.wires.reverse();
        doc.supplies.reverse();

        assert!(
            forward.len() >= 5,
            "fixture should produce several findings"
        );
        assert_eq!(doc.validate(), forward);
    }

    #[test]
    fn a_text_note_that_is_empty_too_long_or_has_a_control_character_is_a_warning() {
        use crate::documentation::{MAX_NOTE_CHARS, TextAnchor, TextNote};
        use crate::ids::TextId;
        let mut doc = Schematic::new("texts");
        for (id, text) in [
            (1, "fine".to_owned()),
            (2, String::new()),
            (3, "x".repeat(MAX_NOTE_CHARS + 1)),
            (4, "a\nb".to_owned()),
        ] {
            doc.texts.push(TextNote {
                id: TextId(id),
                position: GridPoint::default(),
                text,
                anchor: TextAnchor::Left,
                layer: None,
            });
        }
        doc.next_id = 5;

        let found: Vec<_> = doc
            .validate()
            .into_iter()
            .filter(|d| d.code() == DiagnosticCode::InvalidText)
            .map(|d| d.subject().clone())
            .collect();

        assert_eq!(
            found,
            [
                Subject::Text(TextId(2)),
                Subject::Text(TextId(3)),
                Subject::Text(TextId(4))
            ]
        );
    }

    #[test]
    fn a_text_shares_the_identifier_space_and_a_repeated_id_is_a_duplicate() {
        use crate::documentation::{TextAnchor, TextNote};
        use crate::ids::TextId;
        let mut doc = Schematic::new("texts");
        doc.texts.push(TextNote {
            id: TextId(1),
            position: GridPoint::default(),
            text: "note".to_owned(),
            anchor: TextAnchor::Left,
            layer: None,
        });
        doc.sheets.push(crate::sheet::Sheet {
            id: crate::ids::SheetId(1),
            number: 1,
            title: String::new(),
            origin: GridPoint::default(),
            size: crate::sheet::SheetSize::Custom {
                width: 40,
                height: 30,
            },
            orientation: crate::sheet::Orientation::Landscape,
            zones: crate::sheet::Zones {
                columns: 4,
                rows: 2,
            },
            kind: Default::default(),
        });
        doc.next_id = 2;

        assert!(
            doc.validate()
                .iter()
                .any(|d| d.code() == DiagnosticCode::DuplicateId)
        );
    }

    #[test]
    fn a_layer_with_a_bad_or_repeated_name_is_a_warning_and_a_missing_layer_is_dangling() {
        use crate::ids::LayerId;
        use crate::layers::Layer;
        let mut doc = Schematic::new("layers");
        for (id, name) in [
            (1, "Power"),
            (2, "Power"),
            (3, ""),
            (4, " x"),
            (5, "Control"),
        ] {
            doc.layers.push(Layer {
                id: LayerId(id),
                name: name.to_owned(),
                visible: true,
                locked: false,
            });
        }
        doc.devices.push(device(10, "S1"));
        doc.symbols.push(symbol(11, 10));
        doc.symbols[0].layer = Some(LayerId(99));
        doc.next_id = 12;

        let found = doc.validate();

        let bad: Vec<_> = found
            .iter()
            .filter(|d| d.code() == DiagnosticCode::InvalidLayer)
            .map(|d| d.subject().clone())
            .collect();
        assert_eq!(
            bad,
            [
                Subject::Layer(LayerId(2)),
                Subject::Layer(LayerId(3)),
                Subject::Layer(LayerId(4))
            ]
        );
        assert!(
            found
                .iter()
                .any(|d| d.code() == DiagnosticCode::DanglingReference
                    && d.subject() == &Subject::Symbol(SymbolId(11))
                    && d.related() == [Subject::Layer(LayerId(99))])
        );
    }

    #[test]
    fn layer_ids_share_the_identifier_space() {
        use crate::ids::LayerId;
        use crate::layers::Layer;
        let mut doc = Schematic::new("layers");
        doc.devices.push(device(1, "S1"));
        doc.layers.push(Layer {
            id: LayerId(1),
            name: "Power".to_owned(),
            visible: true,
            locked: false,
        });
        doc.next_id = 2;

        assert!(
            doc.validate()
                .iter()
                .any(|d| d.code() == DiagnosticCode::DuplicateId)
        );
    }

    mod annotations {
        use super::*;
        use crate::annotations::{
            Comment, DeviceView, Group, PlcChannel, PlcDirection, PlcModule, Shape, ShapeKind,
            WireSpec,
        };
        use crate::ids::{CommentId, GroupId, PlcId, ShapeId, ViewId};
        use crate::sheet::{Orientation, Sheet, SheetKind, SheetSize, Zones};

        fn found(doc: &Schematic, code: DiagnosticCode) -> Vec<Subject> {
            doc.validate()
                .into_iter()
                .filter(|d| d.code() == code)
                .map(|d| d.subject().clone())
                .collect()
        }

        fn shape(id: u64, kind: ShapeKind) -> Shape {
            Shape {
                id: ShapeId(id),
                kind,
                layer: None,
            }
        }

        fn panel_sheet(id: u64) -> Sheet {
            Sheet {
                id: SheetId(id),
                number: 1,
                title: "Panel".to_owned(),
                origin: GridPoint::new(100, 0),
                size: SheetSize::A4,
                orientation: Orientation::Landscape,
                zones: Zones {
                    columns: 4,
                    rows: 3,
                },
                kind: SheetKind::PanelLayout,
            }
        }

        #[test]
        fn a_valid_document_with_every_kind_of_annotation_has_no_finding() {
            let mut doc = valid();
            let base = doc.next_id;
            let device = doc.devices[0].id;
            let symbol = doc.symbols[0].id;
            doc.project.client = Some("ACME".to_owned());
            doc.wires[0].spec = Some(WireSpec {
                colour: Some("BU".to_owned()),
                ..WireSpec::default()
            });
            doc.sheets.push(panel_sheet(base));
            doc.shapes.push(shape(
                base + 1,
                ShapeKind::Qr {
                    position: GridPoint::new(0, 30),
                    size: 4,
                    text: "https://example.com".to_owned(),
                },
            ));
            doc.comments.push(Comment {
                id: CommentId(base + 2),
                subject: Subject::Symbol(symbol),
                text: "check".to_owned(),
                author: None,
            });
            doc.groups.push(Group {
                id: GroupId(base + 3),
                name: "G".to_owned(),
                members: vec![Subject::Symbol(symbol), Subject::Shape(ShapeId(base + 1))],
            });
            doc.views.push(DeviceView {
                id: ViewId(base + 4),
                device_id: device,
                position: GridPoint::new(110, 10),
                width: 4,
                height: 6,
                layer: None,
            });
            doc.plcs.push(PlcModule {
                id: PlcId(base + 5),
                name: "A1".to_owned(),
                description: None,
                position: None,
                channels: vec![PlcChannel {
                    address: "I0.0".to_owned(),
                    direction: PlcDirection::Input,
                    device: Some(device),
                    label: None,
                }],
            });
            doc.next_id = base + 6;
            assert_eq!(doc.validate(), Vec::new());
        }

        #[test]
        fn references_of_annotations_to_missing_elements_are_dangling() {
            let mut doc = valid();
            let base = doc.next_id;
            doc.views.push(DeviceView {
                id: ViewId(base),
                device_id: DeviceId(999),
                position: GridPoint::new(0, 0),
                width: 2,
                height: 2,
                layer: None,
            });
            doc.comments.push(Comment {
                id: CommentId(base + 1),
                subject: Subject::Wire(WireId(998)),
                text: "x".to_owned(),
                author: None,
            });
            doc.groups.push(Group {
                id: GroupId(base + 2),
                name: "G".to_owned(),
                members: vec![Subject::Shape(ShapeId(997))],
            });
            doc.next_id = base + 3;
            let dangling = |doc: &Schematic| {
                doc.integrity_errors()
                    .into_iter()
                    .filter(|d| d.code() == DiagnosticCode::DanglingReference)
                    .map(|d| d.subject().clone())
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                dangling(&doc),
                [
                    Subject::Comment(CommentId(base + 1)),
                    Subject::Group(GroupId(base + 2)),
                    Subject::View(ViewId(base)),
                ]
            );
        }

        #[test]
        fn annotations_out_of_bounds_are_warnings() {
            let mut doc = valid();
            let base = doc.next_id;
            doc.project.client = Some("x".repeat(65));
            doc.wires[0].spec = Some(WireSpec {
                kind: Some(String::new()),
                ..WireSpec::default()
            });
            doc.shapes.push(shape(
                base,
                ShapeKind::Circle {
                    center: GridPoint::new(0, 0),
                    radius: 0,
                },
            ));
            let symbol = Subject::Symbol(doc.symbols[0].id);
            for (offset, name) in [(1, "One"), (2, "Two")] {
                doc.groups.push(Group {
                    id: GroupId(base + offset),
                    name: name.to_owned(),
                    members: vec![symbol.clone()],
                });
            }
            doc.next_id = base + 3;
            let bad = found(&doc, DiagnosticCode::InvalidAnnotation);
            assert_eq!(
                bad,
                [
                    Subject::Document,
                    Subject::Wire(doc.wires[0].id),
                    Subject::Shape(ShapeId(base)),
                    Subject::Group(GroupId(base + 2)),
                ]
            );
        }

        #[test]
        fn views_belong_on_panel_or_single_line_sheets_and_symbols_do_not() {
            let mut doc = valid();
            let base = doc.next_id;
            let device = doc.devices[0].id;
            doc.sheets.push(panel_sheet(base));
            doc.views.push(DeviceView {
                id: ViewId(base + 1),
                device_id: device,
                position: GridPoint::new(-50, -50),
                width: 2,
                height: 2,
                layer: None,
            });
            doc.symbols[0].position = GridPoint::new(120, 20);
            doc.next_id = base + 2;
            assert_eq!(
                found(&doc, DiagnosticCode::ViewOffItsSheet),
                [
                    Subject::Symbol(doc.symbols[0].id),
                    Subject::View(ViewId(base + 1)),
                ]
            );
        }

        #[test]
        fn plc_channels_with_one_address_or_one_device_twice_conflict() {
            let mut doc = valid();
            let base = doc.next_id;
            let device = doc.devices[0].id;
            let channel = |address: &str| PlcChannel {
                address: address.to_owned(),
                direction: PlcDirection::Output,
                device: Some(device),
                label: None,
            };
            doc.plcs.push(PlcModule {
                id: PlcId(base),
                name: "A1".to_owned(),
                description: None,
                position: None,
                channels: vec![channel("Q0.0"), channel("Q0.0")],
            });
            doc.next_id = base + 1;
            let found: Vec<Diagnostic> = doc
                .validate()
                .into_iter()
                .filter(|d| d.code() == DiagnosticCode::PlcChannelConflict)
                .collect();
            assert_eq!(found.len(), 2, "{found:?}");
        }

        #[test]
        fn annotation_ids_share_the_identifier_space() {
            let mut doc = valid();
            let taken = doc.symbols[0].id.0;
            doc.shapes.push(shape(
                taken,
                ShapeKind::Rectangle {
                    from: GridPoint::new(0, 0),
                    to: GridPoint::new(2, 2),
                },
            ));
            assert!(
                doc.integrity_errors()
                    .iter()
                    .any(|d| d.code() == DiagnosticCode::DuplicateId)
            );
        }
    }
}
