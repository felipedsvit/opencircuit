//! The documentation sections of the inspector (ADR-0014): the project tree,
//! the connections of a symbol, its cross-references, the catalog data of its
//! device, the number of a wire and the rule that numbers the wires.
//!
//! They sit after the sections the inspector always had, collapsed unless the
//! selection asks for them, and they change nothing themselves: a section
//! that edits returns the transaction, and one that navigates returns where
//! to go, for the window to select and show.

use std::collections::BTreeSet;
use std::num::NonZeroU32;

use circuit_commands::documentation as edits;
use circuit_commands::layers;
use circuit_model::{
    DeviceInfo, LayoutIndex, PinRef, SheetId, StaticNets, Subject, SymbolId, SymbolKind, WireId,
    WireNumbering, WireNumbers, links, pin_name, project_tree, terminal_label,
};
use eframe::egui::{self, Ui};

use crate::i18n::{tr, trf};
use crate::inspector::{InspectorState, Request};
use crate::session::Session;

/// Most rows the project tree draws, so that a huge document cannot make a
/// frame cost more than a fixed amount.
const MAX_TREE_ROWS: usize = 1_000;
/// Most buttons for the terminals that one terminal meets.
const MAX_MEETS: usize = 6;

/// Where a click in a documentation section takes the window.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Goto {
    /// Select the element and show it.
    Subject(Subject),
    /// Fit the view to a sheet.
    Sheet(SheetId),
}

/// The tree of the project: its sheets, and the devices on each. A device
/// selects its first symbol; a sheet fits the view.
pub fn project_section(
    ui: &mut Ui,
    session: &Session,
    selection: &BTreeSet<Subject>,
) -> Option<Goto> {
    let doc = session.document();
    let catalog = session.catalog();
    let mut go = None;
    let mut rows = 0_usize;
    for group in project_tree(doc) {
        egui::CollapsingHeader::new(group.title.as_str())
            .id_salt(("project-group", group.sheet, group.title.as_str()))
            .default_open(true)
            .show(ui, |ui| {
                if let Some(sheet) = group.sheet
                    && ui
                        .small_button(tr("Show sheet"))
                        .on_hover_text(tr("Fit the view to this sheet"))
                        .clicked()
                {
                    go = Some(Goto::Sheet(sheet));
                }
                for device in &group.devices {
                    rows = rows.saturating_add(1);
                    if rows > MAX_TREE_ROWS {
                        continue;
                    }
                    let title = doc
                        .devices
                        .iter()
                        .find(|candidate| candidate.id == device.device)
                        .and_then(|found| catalog.model(&found.model))
                        .map_or("", |model| model.title());
                    let text = format!("{} · {title}", device.reference);
                    let selected = device
                        .symbol
                        .is_some_and(|symbol| selection.contains(&Subject::Symbol(symbol)));
                    let response = ui.selectable_label(selected, text);
                    if response.clicked()
                        && let Some(symbol) = device.symbol
                    {
                        go = Some(Goto::Subject(Subject::Symbol(symbol)));
                    }
                }
            });
    }
    if rows > MAX_TREE_ROWS {
        ui.label(trf(
            "… and {0} more",
            &[&rows.saturating_sub(MAX_TREE_ROWS)],
        ));
    }
    go
}

/// The terminals of a symbol: the number of the wire on each, and buttons for
/// the terminals it meets.
pub fn connections_section(ui: &mut Ui, session: &Session, symbol_id: SymbolId) -> Option<Goto> {
    let doc = session.document();
    let catalog = session.catalog();
    let layout = LayoutIndex::new(doc, catalog);
    let symbol = layout.symbol(symbol_id)?;
    let definition = layout.definition(symbol_id)?;
    let device = layout.device(symbol.device_id)?;
    let class = catalog.model(&device.model)?.class();
    let nets = StaticNets::build(doc, catalog);
    let numbers = WireNumbers::build(doc, catalog, &nets);

    let mut go = None;
    for terminal in definition.terminals() {
        let pin = PinRef {
            symbol_id,
            terminal: terminal.key.clone(),
        };
        let label = terminal_label(class, definition.kind(), symbol.slot, &terminal.key)
            .unwrap_or_else(|| terminal.key.0.clone());
        let net = nets.net_of_pin(&pin).and_then(|id| nets.net(id));
        let connected = net.is_some_and(|net| !net.wires.is_empty());
        ui.horizontal_wrapped(|ui| {
            ui.strong(&label);
            if !connected {
                ui.label(tr("not connected"));
                return;
            }
            if let Some(number) = net.and_then(|net| numbers.of_net(net.id)) {
                ui.label(trf("wire {0}", &[&number]));
            }
            let others = net.map_or_else(Vec::new, |net| {
                net.pins.iter().filter(|other| **other != pin).collect()
            });
            for other in others.iter().take(MAX_MEETS) {
                if ui
                    .small_button(pin_name(doc, catalog, other))
                    .on_hover_text(tr("Select the other end"))
                    .clicked()
                {
                    go = Some(Goto::Subject(Subject::Symbol(other.symbol_id)));
                }
            }
            if others.len() > MAX_MEETS {
                ui.label(format!("+{}", others.len().saturating_sub(MAX_MEETS)));
            }
        });
    }
    go
}

/// The coil of a contact, or the contacts of a coil. Draws nothing, not even
/// its header, for a symbol that is neither; the links are worked out only
/// while the section is open.
pub fn xref_section(ui: &mut Ui, session: &Session, symbol: SymbolId) -> Option<Goto> {
    let doc = session.document();
    let catalog = session.catalog();
    let kind = LayoutIndex::new(doc, catalog)
        .definition(symbol)
        .map(|definition| definition.kind())?;
    let linked = matches!(
        kind,
        SymbolKind::Coil
            | SymbolKind::TransformerPrimary
            | SymbolKind::ContactNo
            | SymbolKind::ContactNc
            | SymbolKind::Changeover
            | SymbolKind::PowerContacts { .. }
            | SymbolKind::TransformerSecondary
            | SymbolKind::SignalArrow
    );
    if !linked {
        return None;
    }
    let mut go = None;
    egui::CollapsingHeader::new(tr("Cross-references"))
        .id_salt("Cross-references")
        .default_open(false)
        .show(ui, |ui| {
            let found = links(doc, catalog, symbol);
            if found.is_empty() {
                ui.label(tr("Nothing to go to."));
            }
            for link in &found {
                if ui.button(&link.text).clicked() {
                    go = Some(Goto::Subject(Subject::Symbol(link.symbol)));
                }
            }
        });
    go
}

/// The catalog data of the device of a symbol: manufacturer, part number,
/// description and location, each committed when the field is left.
pub fn catalog_section(
    ui: &mut Ui,
    session: &Session,
    symbol: SymbolId,
    state: &mut InspectorState,
) -> Option<Request> {
    let doc = session.document();
    let layout = LayoutIndex::new(doc, session.catalog());
    let device = layout
        .symbol(symbol)
        .and_then(|s| layout.device(s.device_id))?;
    let info = device.info.clone().unwrap_or_default();
    let mut request = None;
    egui::CollapsingHeader::new(tr("Catalog data"))
        .id_salt("Catalog data")
        .default_open(false)
        .show(ui, |ui| {
            let fields = [
                (
                    "manufacturer",
                    tr("Manufacturer"),
                    info.manufacturer.as_deref(),
                ),
                ("part_no", tr("Part no"), info.part_no.as_deref()),
                (
                    "description",
                    tr("Description"),
                    info.description.as_deref(),
                ),
                ("location", tr("Location"), info.location.as_deref()),
                ("datasheet", tr("Datasheet"), info.datasheet.as_deref()),
            ];
            for (key, label, current) in fields {
                let Some(text) = state.text_field(
                    ui,
                    format!("info:{}:{key}", device.id),
                    label,
                    current.unwrap_or(""),
                ) else {
                    continue;
                };
                let mut changed: DeviceInfo = info.clone();
                let value = Some(text);
                match key {
                    "manufacturer" => changed.manufacturer = value,
                    "part_no" => changed.part_no = value,
                    "description" => changed.description = value,
                    "datasheet" => changed.datasheet = value,
                    _ => changed.location = value,
                }
                request = Some(edits::set_device_info(doc, device.id, changed));
            }
            // The datasheet opens with the system, never in the window
            // (ADR-0018).
            if let Some(url) = info
                .datasheet
                .as_deref()
                .and_then(|link| datasheet_url(link, session.path()))
                && ui
                    .button(tr("Open datasheet"))
                    .on_hover_text(url.as_str())
                    .clicked()
            {
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            }
        });
    request
}

/// The URL a datasheet link opens: a web or file URL as it is, and a path
/// relative to the folder of the project as a file URL. `None` for a link
/// of any other kind, or a relative path of a project never saved.
pub fn datasheet_url(link: &str, project: Option<&std::path::Path>) -> Option<String> {
    if !circuit_model::annotations::is_valid_datasheet(link) {
        return None;
    }
    let lower = link.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("file:") {
        return Some(link.to_owned());
    }
    let folder = project?.parent()?;
    let path = folder.join(link);
    let text = path.to_str()?.replace('\\', "/");
    Some(if text.starts_with('/') {
        format!("file://{text}")
    } else {
        format!("file:///{text}")
    })
}

/// The number of the selected wire, and the text that overrides it.
pub fn wire_section(
    ui: &mut Ui,
    session: &Session,
    wire: WireId,
    state: &mut InspectorState,
) -> Option<Request> {
    let doc = session.document();
    let found = doc.wires.iter().find(|candidate| candidate.id == wire)?;
    let nets = StaticNets::build(doc, session.catalog());
    let numbers = WireNumbers::build(doc, session.catalog(), &nets);
    let mut request = None;
    egui::CollapsingHeader::new(tr("Wire number"))
        .id_salt("Wire number")
        .default_open(true)
        .show(ui, |ui| {
            match numbers.of_wire(wire) {
                Some(number) => ui.label(trf("Number {0}", &[&number])),
                None => ui.label(tr(
                    "No number: set a rule under Sheets and title block, or type one here.",
                )),
            };
            if let Some(text) = state.text_field(
                ui,
                format!("wire:{wire}:number"),
                tr("Own number"),
                found.number.as_deref().unwrap_or(""),
            ) {
                request = Some(edits::set_wire_number(doc, wire, Some(&text)));
            }
        });
    // What the wire is made of (ADR-0018), closed: selecting a wire shows
    // one row more than before, and no field.
    egui::CollapsingHeader::new(tr("Wire data"))
        .id_salt("Wire data")
        .default_open(false)
        .show(ui, |ui| {
            let spec = found.spec.clone().unwrap_or_default();
            let labels = [
                tr("Colour"),
                tr("Cross-section"),
                tr("Type"),
                tr("Material"),
                tr("Insulation"),
            ];
            for (name, label) in circuit_model::WireSpec::FIELDS.into_iter().zip(labels) {
                let current = spec
                    .clone()
                    .field_mut(name)
                    .and_then(|value| value.take())
                    .unwrap_or_default();
                if let Some(text) =
                    state.text_field(ui, format!("wire:{wire}:{name}"), label, &current)
                {
                    let mut changed = spec.clone();
                    if let Some(field) = changed.field_mut(name) {
                        *field = Some(text);
                    }
                    request = Some(edits::set_wire_spec(doc, wire, changed));
                }
            }
        });
    request
}

/// The text of a note, after the other sections: its words, and where it sits
/// on its point.
pub fn text_section(
    ui: &mut Ui,
    session: &Session,
    text: circuit_model::TextId,
    state: &mut InspectorState,
) -> Option<Request> {
    let doc = session.document();
    let found = doc.texts.iter().find(|candidate| candidate.id == text)?;
    let mut request = None;
    egui::CollapsingHeader::new(tr("Text"))
        .id_salt("Text")
        .default_open(true)
        .show(ui, |ui| {
            if let Some(words) =
                state.text_field(ui, format!("text:{text}:words"), tr("Words"), &found.text)
            {
                request = Some(edits::set_text(doc, text, &words));
            }
        });
    request
}

/// The layers (ADR-0015), at the end of the inspector: a switch to show and
/// one to lock each layer, a button to put the selection on it, a field for a
/// new one, and a way to put the selection back on the default layer.
pub fn layers_section(
    ui: &mut Ui,
    session: &Session,
    selection: &BTreeSet<Subject>,
    state: &mut InspectorState,
) -> Option<Request> {
    let doc = session.document();
    let mut request = None;
    if doc.layers.is_empty() {
        ui.label(tr("No layers: every element is on the default layer."));
    }
    for layer in &doc.layers {
        ui.horizontal(|ui| {
            ui.label(&layer.name);
            let mut visible = layer.visible;
            if ui
                .checkbox(&mut visible, trf("Show {0}", &[&layer.name]))
                .changed()
            {
                request = Some(layers::set_layer(doc, layer.id, None, Some(visible), None));
            }
            let mut locked = layer.locked;
            if ui
                .checkbox(&mut locked, trf("Lock {0}", &[&layer.name]))
                .changed()
            {
                request = Some(layers::set_layer(doc, layer.id, None, None, Some(locked)));
            }
        });
        if !selection.is_empty()
            && ui
                .button(trf("Put selection on {0}", &[&layer.name]))
                .clicked()
        {
            request = Some(layers::assign_layer(doc, selection, Some(layer.id)));
        }
    }
    if !selection.is_empty()
        && !doc.layers.is_empty()
        && ui
            .button(tr("Put selection on the default layer"))
            .clicked()
    {
        request = Some(layers::assign_layer(doc, selection, None));
    }
    if let Some(name) = state.text_field(ui, "layers:new".to_owned(), tr("New layer"), "") {
        request = Some(layers::add_layer(doc, &name, true, false));
    }
    request
}

/// The rule that numbers the wires, at the end of the sheets section: a switch,
/// and its prefix, first number, step and whether it restarts on each sheet.
pub fn numbering_block(
    ui: &mut Ui,
    session: &Session,
    state: &mut InspectorState,
) -> Option<Request> {
    let doc = session.document();
    let current = doc.project.wire_numbering.clone();
    let mut request = None;
    ui.separator();
    let mut on = current.is_some();
    if ui.checkbox(&mut on, tr("Number the wires")).changed() {
        request = Some(edits::set_wire_numbering(
            doc,
            on.then(|| WireNumbering {
                prefix: String::new(),
                start: 1,
                step: NonZeroU32::MIN,
                per_sheet: false,
            }),
        ));
    }
    let Some(rule) = current else {
        return request;
    };
    if let Some(text) = state.text_field(
        ui,
        "numbering:prefix".to_owned(),
        tr("Prefix"),
        &rule.prefix,
    ) {
        request = Some(edits::set_wire_numbering(
            doc,
            Some(WireNumbering {
                prefix: text,
                ..rule.clone()
            }),
        ));
    }
    if let Some(text) = state.text_field(
        ui,
        "numbering:start".to_owned(),
        tr("First number"),
        &rule.start.to_string(),
    ) {
        request = Some(match text.parse::<u32>() {
            Ok(start) => edits::set_wire_numbering(
                doc,
                Some(WireNumbering {
                    start,
                    ..rule.clone()
                }),
            ),
            Err(_) => Err(circuit_commands::CommandError::InvalidParameter(
                "the first number must be a whole number".to_owned(),
            )),
        });
    }
    if let Some(text) = state.text_field(
        ui,
        "numbering:step".to_owned(),
        tr("Step"),
        &rule.step.to_string(),
    ) {
        request = Some(match text.parse::<u32>().ok().and_then(NonZeroU32::new) {
            Some(step) => edits::set_wire_numbering(
                doc,
                Some(WireNumbering {
                    step,
                    ..rule.clone()
                }),
            ),
            None => Err(circuit_commands::CommandError::InvalidParameter(
                "the step must be a whole number from 1".to_owned(),
            )),
        });
    }
    let mut per_sheet = rule.per_sheet;
    if ui
        .checkbox(&mut per_sheet, tr("Restart on each sheet"))
        .changed()
    {
        request = Some(edits::set_wire_numbering(
            doc,
            Some(WireNumbering {
                per_sheet,
                ..rule.clone()
            }),
        ));
    }
    request
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_datasheet_opens_as_a_web_or_file_url_and_a_path_beside_the_project() {
        assert_eq!(
            datasheet_url("https://example.com/k1.pdf", None).as_deref(),
            Some("https://example.com/k1.pdf")
        );
        let project = std::path::Path::new("/work/pump/pump.ocircuit");
        assert_eq!(
            datasheet_url("docs/k1.pdf", Some(project)).as_deref(),
            Some("file:///work/pump/docs/k1.pdf")
        );
        // A relative path needs a saved project; anything else is refused.
        assert_eq!(datasheet_url("docs/k1.pdf", None), None);
        assert_eq!(datasheet_url("javascript:alert(1)", Some(project)), None);
        assert_eq!(datasheet_url("/etc/passwd", Some(project)), None);
    }
}
