//! The inspector: the device of the selected symbol, its parameters and
//! symbols, and the supply profiles of the document.
//!
//! Nothing here changes the document. Each widget that the user commits
//! returns the transaction of the change, built by `circuit_commands::ops`,
//! which checks it against the catalog; the window applies it.

use std::collections::{BTreeMap, BTreeSet};

use circuit_commands::{CommandError, SupplyPreset, Transaction, ops};
use circuit_model::{
    DeviceId, GridPoint, LayoutIndex, ParamType, ParamValue, QuarterTurn, Subject, SupplyKind,
    SupplyProfile,
};
use eframe::egui::{self, Ui};

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
}

impl InspectorState {
    /// Forget what was typed; the fields show the document again. Call after
    /// every change to the document.
    pub fn clear(&mut self) {
        self.texts.clear();
    }

    /// A single-line text field showing `current` until the user types.
    /// Returns the text when the user leaves the field after changing it.
    fn text_field(
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
        SupplyKind::SinglePhase => "single phase",
        SupplyKind::ThreePhase => "three phase",
    }
}

fn supply_label(profile: &SupplyProfile) -> String {
    format!(
        "Supply {} ({}, {} mV)",
        profile.id,
        kind_label(profile.kind),
        profile.nominal_voltage_mv
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
    ui.strong("Inspector");
    let mut selected = selection.iter();
    match (selected.next(), selected.next()) {
        (Some(Subject::Symbol(id)), None) => {
            request = device_section(ui, session, *id, state);
        }
        (None, _) => {
            ui.label("Select a symbol to edit its device.");
        }
        _ => {
            ui.label(format!("{} elements selected.", selection.len()));
        }
    }
    ui.separator();
    if let Some(change) = supplies_section(ui, session, state) {
        request = Some(change);
    }
    request
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

    ui.label(format!(
        "{} · {} v{} · symbol `{}`",
        device.reference.0, device.model.id, device.model.version, symbol.symbol_key.0
    ));
    if let Some(text) = state.text_field(
        ui,
        format!("reference:{}", device.id),
        "Reference",
        &device.reference.0,
    ) {
        request = Some(ops::set_reference(doc, device.id, &text));
    }

    let Some(model) = catalog.model(&device.model) else {
        ui.colored_label(ui.visuals().error_fg_color, "Model not in the catalog.");
        return request;
    };
    if !model.params().is_empty() {
        ui.strong("Parameters");
    }
    for definition in model.params() {
        let current = device.params.get(&definition.key);
        if let Some(change) = param_row(ui, session, device.id, definition, current, state) {
            request = Some(change);
        }
    }

    ui.strong("Symbols");
    for definition in model.symbols() {
        let count = doc
            .symbols
            .iter()
            .filter(|s| s.device_id == device.id && &s.symbol_key == definition.key())
            .count();
        ui.horizontal(|ui| {
            ui.label(format!(
                "{}: {count} of {}",
                definition.key().0,
                definition.max_instances()
            ));
            if count < usize::from(definition.max_instances())
                && ui
                    .button(format!(
                        "Add {} to {}",
                        definition.key().0,
                        device.reference.0
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
        if ui.button("Rotate symbol").clicked() {
            request = Some(ops::rotate(doc, catalog, &BTreeSet::from([symbol_id])));
        }
        if ui.button("Delete symbol").clicked() {
            request = Some(ops::delete(
                doc,
                &BTreeSet::from([Subject::Symbol(symbol_id)]),
            ));
        }
    });
    request
}

fn param_row(
    ui: &mut Ui,
    session: &Session,
    device: DeviceId,
    definition: &circuit_model::ParamDefinition,
    current: Option<&ParamValue>,
    state: &mut InspectorState,
) -> Option<Request> {
    let doc = session.document();
    let catalog = session.catalog();
    let key = definition.key.as_str();
    let set = |value: Option<ParamValue>| ops::set_param(doc, catalog, device, key, value);
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
        ParamType::Enum(choices) => {
            let selected = match shown {
                Some(ParamValue::Text(text)) => text.clone(),
                _ => "(not set)".to_owned(),
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
                    .map_or_else(|| format!("missing supply {raw}"), supply_label),
                _ => "(not set)".to_owned(),
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
    }
    request
}

fn supplies_section(ui: &mut Ui, session: &Session, state: &mut InspectorState) -> Option<Request> {
    let doc = session.document();
    let catalog = session.catalog();
    let mut request = None;
    ui.strong("Supplies");
    for supply in &doc.supplies {
        ui.label(format!(
            "Supply {} · {}",
            supply.id,
            kind_label(supply.kind)
        ));
        if let Some(text) = state.text_field(
            ui,
            format!("supply:{}:voltage", supply.id),
            "Voltage (mV)",
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
                "Frequency (mHz)",
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
        if ui.button(format!("Remove supply {}", supply.id)).clicked() {
            request = Some(ops::remove_supply(doc, catalog, supply.id));
        }
    }
    ui.horizontal_wrapped(|ui| {
        for preset in SupplyPreset::ALL {
            if ui.button(format!("Add {}", preset.label())).clicked() {
                request = Some(ops::add_supply(doc, preset));
            }
        }
    });
    request
}
