//! The inspector: the device of the selected symbol, its parameters and
//! symbols, and the supply profiles of the document.
//!
//! Nothing here changes the document. Each widget that the user commits
//! returns the transaction of the change, built by `circuit_commands::ops`,
//! which checks it against the catalog; the window applies it.

use std::collections::{BTreeMap, BTreeSet};

use circuit_commands::arrange::{self, Align, Axis};
use circuit_commands::documentation::InfoField;
use circuit_commands::sheets::{self, SheetSpec};
use circuit_commands::{CommandError, SupplyPreset, Transaction, ops};
use circuit_model::{
    DeviceClass, DeviceId, GridPoint, LayoutIndex, ParamType, ParamValue, QuarterTurn, Subject,
    SupplyKind, SupplyProfile,
};
use eframe::egui::{self, RichText, Ui};

use crate::documentation::{self, Goto};
use crate::i18n::{tr, trf};
use crate::session::Session;

/// What the user asked for: the transaction of a change, or why it cannot
/// be made.
pub type Request = Result<Transaction, CommandError>;

/// Where a new symbol of a device goes, relative to the selected one.
const NEXT_SYMBOL: GridPoint = GridPoint::new(6, 0);

/// Texts being typed in the inspector, by field.
#[derive(Debug, Default)]
pub struct InspectorState {
    texts: BTreeMap<String, String>,
    /// Where a click in a documentation section asked to go; the window
    /// takes it after drawing the inspector.
    goto: Option<Goto>,
}

impl InspectorState {
    /// Forget what was typed; the fields show the document again. Call after
    /// every change to the document.
    pub fn clear(&mut self) {
        self.texts.clear();
    }

    /// Where a click asked to go since the last call, if anywhere.
    pub fn take_goto(&mut self) -> Option<Goto> {
        self.goto.take()
    }

    /// A single-line text field showing `current` until the user types.
    /// Returns the text when the user leaves the field after changing it.
    pub(crate) fn text_field(
        &mut self,
        ui: &mut Ui,
        key: String,
        label: &str,
        current: &str,
    ) -> Option<String> {
        let response = ui.horizontal(|ui| {
            let name = ui.label(label);
            let buffer = self
                .texts
                .entry(key.clone())
                .or_insert_with(|| current.to_owned());
            ui.add(egui::TextEdit::singleline(buffer).desired_width(110.0))
                .labelled_by(name.id)
        });
        let edited = self.texts.get(&key).map(|text| text.trim().to_owned());
        match edited {
            Some(text) if response.inner.lost_focus() && text != current => Some(text),
            _ => None,
        }
    }
}

fn kind_label(kind: SupplyKind) -> &'static str {
    match kind {
        SupplyKind::Dc => "DC",
        SupplyKind::SinglePhase => tr("single phase"),
        SupplyKind::ThreePhase => tr("three phase"),
    }
}

fn supply_label(profile: &SupplyProfile) -> String {
    trf(
        "Supply {0} ({1}, {2} mV)",
        &[
            &profile.id,
            &kind_label(profile.kind),
            &profile.nominal_voltage_mv,
        ],
    )
}

/// Draw the inspector. Returns the change the user committed this frame.
pub fn show(
    ui: &mut Ui,
    session: &Session,
    selection: &BTreeSet<Subject>,
    state: &mut InspectorState,
) -> Option<Request> {
    let mut request = None;
    ui.label(selection_summary(session, selection));
    egui::CollapsingHeader::new(tr("Properties"))
        .id_salt("Properties")
        .default_open(true)
        .show(ui, |ui| {
            let mut selected = selection.iter();
            match (selected.next(), selected.next()) {
                (Some(Subject::Symbol(id)), None) => {
                    request = device_section(ui, session, *id, state);
                }
                (None, _) => {
                    ui.label(tr("Select a symbol to edit its device."));
                }
                _ => {
                    request = selection_section(ui, session, selection, state);
                }
            }
        });
    egui::CollapsingHeader::new(tr("Supplies"))
        .id_salt("Supplies")
        .default_open(true)
        .show(ui, |ui| {
            if let Some(change) = supplies_section(ui, session, state) {
                request = Some(change);
            }
        });
    egui::CollapsingHeader::new(tr("Sheets and title block"))
        .id_salt("Sheets and title block")
        .default_open(true)
        .show(ui, |ui| {
            if let Some(change) = sheets_section(ui, session, state) {
                request = Some(change);
            }
            if let Some(change) = documentation::numbering_block(ui, session, state) {
                request = Some(change);
            }
        });
    // What M7 added (ADR-0014), after what was always here.
    let mut selected = selection.iter();
    match (selected.next(), selected.next()) {
        (Some(Subject::Symbol(symbol)), None) => {
            egui::CollapsingHeader::new(tr("Connections"))
                .id_salt("Connections")
                .default_open(false)
                .show(ui, |ui| {
                    state.goto = documentation::connections_section(ui, session, *symbol)
                        .or(state.goto.take());
                });
            if let Some(go) = documentation::xref_section(ui, session, *symbol) {
                state.goto = Some(go);
            }
            if let Some(change) = documentation::catalog_section(ui, session, *symbol, state) {
                request = Some(change);
            }
        }
        (Some(Subject::Wire(wire)), None) => {
            if let Some(change) = documentation::wire_section(ui, session, *wire, state) {
                request = Some(change);
            }
        }
        (Some(Subject::Text(text)), None) => {
            if let Some(change) = documentation::text_section(ui, session, *text, state) {
                request = Some(change);
            }
        }
        _ => {}
    }
    egui::CollapsingHeader::new(tr("Project tree"))
        .id_salt("Project tree")
        .default_open(false)
        .show(ui, |ui| {
            if let Some(go) = documentation::project_section(ui, session, selection) {
                state.goto = Some(go);
            }
            // The layers organise the project, and live in its section: the
            // inspector of a fresh window gets no new row (ADR-0015).
            egui::CollapsingHeader::new(tr("Layers"))
                .id_salt("Layers")
                .default_open(false)
                .show(ui, |ui| {
                    if let Some(change) =
                        documentation::layers_section(ui, session, selection, state)
                    {
                        request = Some(change);
                    }
                });
            // The data of schema version 5 (ADR-0018), closed here too.
            egui::CollapsingHeader::new(tr("Project data"))
                .id_salt("Project data")
                .default_open(false)
                .show(ui, |ui| {
                    if let Some(change) = project_data_section(ui, session, state) {
                        request = Some(change);
                    }
                });
            egui::CollapsingHeader::new(tr("Comments"))
                .id_salt("Comments")
                .default_open(false)
                .show(ui, |ui| {
                    if let Some(change) = comments_section(ui, session, selection, state) {
                        request = Some(change);
                    }
                });
        });
    request
}

/// The comments about the selected element, or about the project when
/// nothing is selected (ADR-0018): each with a button that deletes it, and a
/// field that adds one.
fn comments_section(
    ui: &mut Ui,
    session: &Session,
    selection: &BTreeSet<Subject>,
    state: &mut InspectorState,
) -> Option<Request> {
    let doc = session.document();
    let mut selected = selection.iter();
    let subject = match (selected.next(), selected.next()) {
        (None, _) => Subject::Document,
        (Some(subject), None) => subject.clone(),
        (Some(_), Some(_)) => {
            ui.label(tr("Select one element to see its comments."));
            return None;
        }
    };
    let mut request = None;
    let comments = circuit_commands::annotations::comments_of(doc, &subject);
    if comments.is_empty() {
        ui.label(RichText::new(tr("No comment")).weak());
    }
    for comment in comments {
        ui.horizontal(|ui| {
            let text = match &comment.author {
                Some(author) => format!("{author}: {}", comment.text),
                None => comment.text.clone(),
            };
            ui.label(text);
            if ui
                .small_button("×")
                .on_hover_text(tr("Delete this comment"))
                .clicked()
            {
                request = Some(circuit_commands::ops::delete(
                    doc,
                    session.catalog(),
                    &BTreeSet::from([Subject::Comment(comment.id)]),
                ));
            }
        });
    }
    if let Some(text) =
        state.text_field(ui, format!("comment:new:{subject}"), tr("New comment"), "")
    {
        request = Some(
            circuit_commands::annotations::add_comment(doc, subject, &text, None).map(|(tx, _)| tx),
        );
    }
    request
}

/// The data of the project that schema version 5 added (ADR-0018): the
/// client of the title block.
fn project_data_section(
    ui: &mut Ui,
    session: &Session,
    state: &mut InspectorState,
) -> Option<Request> {
    let doc = session.document();
    // An empty text clears the client.
    state
        .text_field(
            ui,
            "project:client".to_owned(),
            tr("Client"),
            doc.project.client.as_deref().unwrap_or(""),
        )
        .map(|text| sheets::set_client(doc, Some(&text)))
}

/// One line about the selection: nothing, one symbol of a part, or how many.
fn selection_summary(session: &Session, selection: &BTreeSet<Subject>) -> String {
    let mut selected = selection.iter();
    match (selected.next(), selected.next()) {
        (None, _) => tr("Nothing selected").to_owned(),
        (Some(Subject::Symbol(id)), None) => {
            let layout = LayoutIndex::new(session.document(), session.catalog());
            layout
                .symbol(*id)
                .and_then(|symbol| layout.device(symbol.device_id))
                .map_or_else(
                    || tr("1 symbol selected").to_owned(),
                    |device| {
                        let title = session
                            .catalog()
                            .model(&device.model)
                            .map_or(device.model.id.as_str(), |model| model.title());
                        format!("{} · {title}", device.reference.0)
                    },
                )
        }
        (Some(Subject::Text(_)), None) => tr("1 text selected").to_owned(),
        (Some(_), None) => tr("1 element selected").to_owned(),
        _ => trf("{0} elements selected", &[&selection.len()]),
    }
}

fn device_section(
    ui: &mut Ui,
    session: &Session,
    symbol_id: circuit_model::SymbolId,
    state: &mut InspectorState,
) -> Option<Request> {
    let doc = session.document();
    let catalog = session.catalog();
    let layout = LayoutIndex::new(doc, catalog);
    let symbol = layout.symbol(symbol_id)?;
    let device = layout.device(symbol.device_id)?;
    let mut request = None;

    ui.label(trf(
        "{0} · {1} v{2} · symbol `{3}`",
        &[
            &device.reference.0,
            &device.model.id,
            &device.model.version,
            &symbol.symbol_key.0,
        ],
    ));
    if let Some(text) = state.text_field(
        ui,
        format!("reference:{}", device.id),
        tr("Reference"),
        &device.reference.0,
    ) {
        request = Some(ops::set_reference(doc, device.id, &text));
    }

    let Some(model) = catalog.model(&device.model) else {
        ui.colored_label(ui.visuals().error_fg_color, tr("Model not in the catalog."));
        return request;
    };
    if let Some(newer) = catalog.upgrade(&device.model)
        && ui
            .button(trf("Upgrade to version {0}", &[&newer.version()]))
            .on_hover_text(tr(
                "Keeps every symbol, wire and parameter and offers the newer symbols",
            ))
            .clicked()
    {
        request = Some(ops::upgrade_model(doc, catalog, device.id));
    }
    if !model.params().is_empty() {
        ui.strong(tr("Parameters"));
    }
    let devices = BTreeSet::from([device.id]);
    for definition in model.params() {
        let current = device.params.get(&definition.key);
        if let Some(change) = param_row(ui, session, &devices, definition, current, state) {
            request = Some(change);
        }
    }

    ui.strong(tr("Symbols"));
    for definition in model.symbols() {
        let count = doc
            .symbols
            .iter()
            .filter(|s| s.device_id == device.id && &s.symbol_key == definition.key())
            .count();
        ui.horizontal(|ui| {
            ui.label(trf(
                "{0}: {1} of {2}",
                &[&definition.key().0, &count, &definition.max_instances()],
            ));
            if count < usize::from(definition.max_instances())
                && ui
                    .button(trf(
                        "Add {0} to {1}",
                        &[&definition.key().0, &device.reference.0],
                    ))
                    .clicked()
            {
                request = Some(
                    symbol
                        .position
                        .checked_add(NEXT_SYMBOL)
                        .ok_or(CommandError::OutOfRange)
                        .and_then(|at| {
                            ops::add_symbol(
                                doc,
                                catalog,
                                device.id,
                                definition.key(),
                                at,
                                QuarterTurn::Deg0,
                            )
                        }),
                );
            }
        });
    }

    ui.horizontal(|ui| {
        if ui.button(tr("Rotate symbol")).clicked() {
            request = Some(ops::rotate(doc, catalog, &BTreeSet::from([symbol_id])));
        }
        if ui.button(tr("Delete symbol")).clicked() {
            request = Some(ops::delete(
                doc,
                catalog,
                &BTreeSet::from([Subject::Symbol(symbol_id)]),
            ));
        }
    });
    request
}

/// The several elements of a selection: how many, buttons acting on all of
/// them, and the parameters shared by their devices when every device has
/// the same model.
fn selection_section(
    ui: &mut Ui,
    session: &Session,
    selection: &BTreeSet<Subject>,
    state: &mut InspectorState,
) -> Option<Request> {
    let doc = session.document();
    let catalog = session.catalog();
    let mut request = None;
    ui.label(trf("{0} elements selected.", &[&selection.len()]));
    let symbols: BTreeSet<circuit_model::SymbolId> = selection
        .iter()
        .filter_map(|subject| match subject {
            Subject::Symbol(id) => Some(*id),
            _ => None,
        })
        .collect();
    ui.horizontal(|ui| {
        if !symbols.is_empty() && ui.button(tr("Rotate all")).clicked() {
            request = Some(ops::rotate(doc, catalog, &symbols));
        }
        if ui.button(tr("Delete all")).clicked() {
            request = Some(ops::delete(doc, catalog, selection));
        }
    });
    ui.horizontal(|ui| {
        ui.label(tr("Align"));
        for (label, edge) in [
            (tr("Left"), Align::Left),
            (tr("Right"), Align::Right),
            (tr("Top"), Align::Top),
            (tr("Bottom"), Align::Bottom),
        ] {
            if ui.button(label).clicked() {
                request = Some(arrange::align(doc, catalog, selection, edge));
            }
        }
    });
    ui.horizontal(|ui| {
        ui.label(tr("Distribute"));
        for (label, axis) in [
            (tr("Across"), Axis::Horizontal),
            (tr("Down"), Axis::Vertical),
        ] {
            if ui.button(label).clicked() {
                request = Some(arrange::distribute(doc, catalog, selection, axis));
            }
        }
    });

    // Parameters shared by devices of one model. A device reference names
    // one other device, so it is not offered for several at once.
    let devices: BTreeSet<DeviceId> = doc
        .symbols
        .iter()
        .filter(|symbol| symbols.contains(&symbol.id))
        .map(|symbol| symbol.device_id)
        .collect();
    let owners: Vec<&circuit_model::Device> = doc
        .devices
        .iter()
        .filter(|device| devices.contains(&device.id))
        .collect();
    let first_model = owners.first().map(|device| &device.model);
    let model = first_model
        .filter(|model| owners.iter().all(|device| &device.model == *model))
        .and_then(|model| catalog.model(model));
    if let (Some(model), true) = (model, owners.len() >= 2) {
        ui.strong(trf("Parameters of {0} {1}", &[&owners.len(), &model.id()]));
        for definition in model.params() {
            if definition.ty == ParamType::DeviceRef {
                continue;
            }
            let values: BTreeSet<Option<&ParamValue>> = owners
                .iter()
                .map(|device| device.params.get(&definition.key))
                .collect();
            let common = if values.len() == 1 {
                values.into_iter().next().flatten()
            } else {
                None
            };
            if let Some(change) = param_row(ui, session, &devices, definition, common, state) {
                request = Some(change);
            }
        }
    }

    // The catalog data of every device of the selection, of any model: a
    // field shows its text when all have the same, and a text typed in it
    // goes to all of them.
    let devices: BTreeSet<DeviceId> = devices
        .into_iter()
        .chain(selection.iter().filter_map(|subject| match subject {
            Subject::Device(id) => Some(*id),
            _ => None,
        }))
        .collect();
    if devices.len() >= 2 {
        egui::CollapsingHeader::new(trf("Catalog data of {0} devices", &[&devices.len()]))
            .id_salt("Catalog data of {0} devices")
            .default_open(false)
            .show(ui, |ui| {
                for (field, label) in [
                    (InfoField::Manufacturer, tr("Manufacturer")),
                    (InfoField::PartNo, tr("Part no")),
                    (InfoField::Description, tr("Description")),
                    (InfoField::Location, tr("Location")),
                ] {
                    let values: BTreeSet<Option<&str>> = doc
                        .devices
                        .iter()
                        .filter(|device| devices.contains(&device.id))
                        .map(|device| device.info.as_ref().and_then(|info| field.of(info)))
                        .collect();
                    let common = match values.len() {
                        1 => values.into_iter().next().flatten().unwrap_or(""),
                        _ => "",
                    };
                    if let Some(text) =
                        state.text_field(ui, format!("common info:{field:?}"), label, common)
                    {
                        request = Some(circuit_commands::documentation::set_common_info(
                            doc, &devices, field, &text,
                        ));
                    }
                }
            });
    }
    request
}

fn param_row(
    ui: &mut Ui,
    session: &Session,
    devices: &BTreeSet<DeviceId>,
    definition: &circuit_model::ParamDefinition,
    current: Option<&ParamValue>,
    state: &mut InspectorState,
) -> Option<Request> {
    let doc = session.document();
    let catalog = session.catalog();
    let key = definition.key.as_str();
    let set = |value: Option<ParamValue>| match devices.first() {
        Some(device) if devices.len() == 1 => ops::set_param(doc, catalog, *device, key, value),
        _ => arrange::set_param_many(doc, catalog, devices, key, value),
    };
    // One field per set of devices, so that typing for one selection never
    // shows up for another.
    let device = devices
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let shown = current.or(definition.default.as_ref());
    let mut request = None;
    match &definition.ty {
        ParamType::Bool => {
            let mut value = matches!(shown, Some(ParamValue::Bool(true)));
            if ui.checkbox(&mut value, key).changed() {
                request = Some(set(Some(ParamValue::Bool(value))));
            }
        }
        ParamType::Int { min, max } => {
            let text = match shown {
                Some(ParamValue::Int(value)) => value.to_string(),
                _ => String::new(),
            };
            if let Some(typed) = state.text_field(ui, format!("param:{device}:{key}"), key, &text) {
                request = Some(if typed.is_empty() {
                    set(None)
                } else {
                    typed.parse::<i64>().map_or_else(
                        |_| {
                            Err(CommandError::InvalidParameter(format!(
                                "`{key}` must be a whole number between {min} and {max}"
                            )))
                        },
                        |value| set(Some(ParamValue::Int(value))),
                    )
                });
            }
        }
        ParamType::Name => {
            let text = match shown {
                Some(ParamValue::Text(text)) => text.clone(),
                _ => String::new(),
            };
            if let Some(typed) = state.text_field(ui, format!("param:{device}:{key}"), key, &text) {
                request = Some(if typed.is_empty() {
                    set(None)
                } else {
                    set(Some(ParamValue::Text(typed)))
                });
            }
        }
        ParamType::Enum(choices) => {
            let selected = match shown {
                Some(ParamValue::Text(text)) => text.clone(),
                _ => tr("(not set)").to_owned(),
            };
            egui::ComboBox::new(format!("param:{device}:{key}"), key)
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for choice in choices {
                        if ui
                            .selectable_label(
                                shown == Some(&ParamValue::Text(choice.clone())),
                                choice,
                            )
                            .clicked()
                        {
                            request = Some(set(Some(ParamValue::Text(choice.clone()))));
                        }
                    }
                });
        }
        ParamType::SupplyRef => {
            let chosen = match shown {
                Some(ParamValue::Int(raw)) => doc
                    .supplies
                    .iter()
                    .find(|s| i64::try_from(s.id.0).ok() == Some(*raw))
                    .map_or_else(|| trf("missing supply {0}", &[raw]), supply_label),
                _ => tr("(not set)").to_owned(),
            };
            egui::ComboBox::new(format!("param:{device}:{key}"), key)
                .selected_text(chosen)
                .show_ui(ui, |ui| {
                    for supply in &doc.supplies {
                        let Ok(raw) = i64::try_from(supply.id.0) else {
                            continue;
                        };
                        if ui
                            .selectable_label(
                                shown == Some(&ParamValue::Int(raw)),
                                supply_label(supply),
                            )
                            .clicked()
                        {
                            request = Some(set(Some(ParamValue::Int(raw))));
                        }
                    }
                });
        }
        ParamType::DeviceRef => {
            // Offer the contactors: the only devices an interlock may name.
            // The compiler checks the choice again.
            let contactors: Vec<&circuit_model::Device> = doc
                .devices
                .iter()
                .filter(|d| {
                    !devices.contains(&d.id)
                        && catalog
                            .model(&d.model)
                            .is_some_and(|m| m.class() == DeviceClass::Contactor)
                })
                .collect();
            let chosen = match shown {
                Some(ParamValue::Int(raw)) => doc
                    .devices
                    .iter()
                    .find(|d| i64::try_from(d.id.0).ok() == Some(*raw))
                    .map_or_else(
                        || trf("missing device {0}", &[raw]),
                        |d| d.reference.0.clone(),
                    ),
                _ => tr("(not set)").to_owned(),
            };
            egui::ComboBox::new(format!("param:{device}:{key}"), key)
                .selected_text(chosen)
                .show_ui(ui, |ui| {
                    for target in contactors {
                        let Ok(raw) = i64::try_from(target.id.0) else {
                            continue;
                        };
                        if ui
                            .selectable_label(
                                shown == Some(&ParamValue::Int(raw)),
                                &target.reference.0,
                            )
                            .clicked()
                        {
                            request = Some(set(Some(ParamValue::Int(raw))));
                        }
                    }
                });
        }
    }
    request
}

/// The information of the project and its sheets (ADR-0011): the name and
/// the fields of the title block, the title of each sheet, and buttons to
/// add and remove sheets.
fn sheets_section(ui: &mut Ui, session: &Session, state: &mut InspectorState) -> Option<Request> {
    let doc = session.document();
    let mut request = None;
    let project = &doc.project;
    let fields = [
        ("name", tr("Project"), project.name.as_str()),
        (
            "author",
            tr("Author"),
            project.author.as_deref().unwrap_or(""),
        ),
        (
            "company",
            tr("Company"),
            project.company.as_deref().unwrap_or(""),
        ),
        (
            "revision",
            tr("Revision"),
            project.revision.as_deref().unwrap_or(""),
        ),
        ("date", tr("Date"), project.date.as_deref().unwrap_or("")),
    ];
    for (key, label, current) in fields {
        if let Some(text) = state.text_field(ui, format!("project:{key}"), label, current) {
            // An empty text clears an optional field; the name stays.
            let value = |own: &str, field: Option<&String>| -> Option<String> {
                if own == key {
                    (!text.is_empty()).then(|| text.clone())
                } else {
                    field.cloned()
                }
            };
            let name = if key == "name" {
                text.as_str()
            } else {
                project.name.as_str()
            };
            request = Some(ops_info(
                doc,
                name,
                value("author", project.author.as_ref()),
                value("company", project.company.as_ref()),
                value("revision", project.revision.as_ref()),
                value("date", project.date.as_ref()),
            ));
        }
    }

    let mut sheets: Vec<&circuit_model::Sheet> = doc.sheets.iter().collect();
    sheets.sort_by_key(|sheet| (sheet.number, sheet.id));
    for sheet in sheets {
        ui.horizontal(|ui| {
            ui.label(trf("Sheet {0}", &[&sheet.number]));
            if ui.button(tr("Remove")).clicked() {
                request = Some(sheets::remove_sheet(doc, sheet.id));
            }
        });
        if let Some(title) = state.text_field(
            ui,
            format!("sheet:{}:title", sheet.id),
            tr("Title"),
            &sheet.title,
        ) {
            request = Some(sheets::set_sheet(
                doc,
                circuit_model::Sheet {
                    title,
                    ..sheet.clone()
                },
            ));
        }
    }
    if ui
        .button(tr("Add A4 sheet"))
        .on_hover_text(tr("A new landscape sheet to the right of the others"))
        .clicked()
    {
        request = Some(next_sheet(doc));
    }
    request
}

/// A new A4 landscape sheet to the right of the last one, a few grid units
/// apart; the first one at the origin.
fn next_sheet(doc: &circuit_model::Schematic) -> Request {
    const GAP: i32 = 6;
    let origin = doc
        .sheets
        .iter()
        .filter_map(|sheet| sheet.frame())
        .max_by_key(|frame| frame.max.x)
        .map_or(Some(GridPoint::new(0, 0)), |frame| {
            frame
                .max
                .x
                .checked_add(GAP)
                .map(|x| GridPoint::new(x, frame.min.y))
        })
        .ok_or(CommandError::OutOfRange)?;
    sheets::add_sheet(doc, &SheetSpec::a4(String::new(), origin))
}

fn ops_info(
    doc: &circuit_model::Schematic,
    name: &str,
    author: Option<String>,
    company: Option<String>,
    revision: Option<String>,
    date: Option<String>,
) -> Request {
    sheets::set_project_info(
        doc,
        name,
        author.as_deref(),
        company.as_deref(),
        revision.as_deref(),
        date.as_deref(),
    )
}

fn supplies_section(ui: &mut Ui, session: &Session, state: &mut InspectorState) -> Option<Request> {
    let doc = session.document();
    let catalog = session.catalog();
    let mut request = None;
    for supply in &doc.supplies {
        ui.label(trf(
            "Supply {0} · {1}",
            &[&supply.id, &kind_label(supply.kind)],
        ));
        if let Some(text) = state.text_field(
            ui,
            format!("supply:{}:voltage", supply.id),
            tr("Voltage (mV)"),
            &supply.nominal_voltage_mv.to_string(),
        ) {
            request = Some(match text.parse::<u32>() {
                Ok(value) => ops::set_supply(
                    doc,
                    SupplyProfile {
                        nominal_voltage_mv: value,
                        ..supply.clone()
                    },
                ),
                Err(_) => Err(CommandError::InvalidSupply(
                    "the voltage must be a whole number of millivolts",
                )),
            });
        }
        if let Some(frequency) = supply.frequency_millihz
            && let Some(text) = state.text_field(
                ui,
                format!("supply:{}:frequency", supply.id),
                tr("Frequency (mHz)"),
                &frequency.to_string(),
            )
        {
            request = Some(match text.parse::<u32>() {
                Ok(value) => ops::set_supply(
                    doc,
                    SupplyProfile {
                        frequency_millihz: Some(value),
                        ..supply.clone()
                    },
                ),
                Err(_) => Err(CommandError::InvalidSupply(
                    "the frequency must be a whole number of millihertz",
                )),
            });
        }
        if ui.button(trf("Remove supply {0}", &[&supply.id])).clicked() {
            request = Some(ops::remove_supply(doc, catalog, supply.id));
        }
    }
    ui.horizontal_wrapped(|ui| {
        for preset in SupplyPreset::ALL {
            if ui.button(trf("Add {0}", &[&tr(preset.label())])).clicked() {
                request = Some(ops::add_supply(doc, preset));
            }
        }
    });
    request
}
