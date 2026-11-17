//! Reports of a project (ADR-0014).
//!
//! Every report is derived from the document, its catalog and, for the list
//! of problems, the diagnostics somebody else produced; none is stored. Each
//! comes out as a [`Table`], which the command line writes as text, CSV or
//! JSON and the exporter draws as PDF pages.
//!
//! | Report | One row per |
//! | --- | --- |
//! | [`bom`] | kind of part: model, manufacturer, part number and description |
//! | [`wires`] | net with a wire: its number and where it goes |
//! | [`terminals`] | terminal of every symbol, with its wire and the terminals it meets |
//! | [`strips`] | terminal of a terminal strip: where each side comes from and goes to |
//! | [`xref`] | cross-reference exception: a coil with no contact, a contact with no coil, a symbol off the sheets |
//! | [`io`] | address of the Modbus map (ADR-0009) |
//! | [`problems`] | diagnostic |
//! | [`devices`] | device: its model, symbols, sheets and catalog data |
//! | [`diff`] | element added, removed or changed between two versions of a project |
//!
//! Rows are in reading order (designators in natural order, so `K2` comes
//! before `K10`), and nothing depends on the order of the collections of the
//! document.

#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]
#![cfg_attr(
    not(test),
    deny(clippy::indexing_slicing, clippy::arithmetic_side_effects)
)]

use std::collections::BTreeMap;

mod diff;
pub use diff::diff;

use circuit_io::IoMap;
use circuit_model::{
    ComponentCatalog, Device, DeviceClass, DeviceId, Diagnostic, PinRef, Schematic, StaticNets,
    SymbolId, SymbolInstance, SymbolKind, Table, WaiverIndex, WireId, WireNumbers, WireSpec,
    natural_key as natural, sheet_at, strip_terminals, terminal_label,
};

/// The reports, by the name the command line uses.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Kind {
    /// [`bom`].
    Bom,
    /// [`wires`].
    Wires,
    /// [`terminals`].
    Terminals,
    /// [`strips`].
    Strips,
    /// [`xref`].
    Xref,
    /// [`io`].
    Io,
    /// [`problems`].
    Problems,
    /// [`devices`].
    Devices,
}

impl Kind {
    /// Every report, in the order the command line lists them.
    pub const ALL: [Self; 8] = [
        Self::Bom,
        Self::Wires,
        Self::Terminals,
        Self::Strips,
        Self::Xref,
        Self::Io,
        Self::Problems,
        Self::Devices,
    ];

    /// The name of the report: `bom`, `wires`, `terminals`, `strips`, `xref`,
    /// `io`, `problems` or `devices`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bom => "bom",
            Self::Wires => "wires",
            Self::Terminals => "terminals",
            Self::Strips => "strips",
            Self::Xref => "xref",
            Self::Io => "io",
            Self::Problems => "problems",
            Self::Devices => "devices",
        }
    }

    /// The report with this name.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }
}

/// A designator in natural order.
type Natural = (String, u64, String);

/// One exception of the cross-reference report, in sort order: the natural
/// designator, the kind of exception, the symbol, then the three text cells.
type Exception = (Natural, u8, SymbolId, String, String, &'static str);

/// The lookups every report needs.
struct View<'a> {
    catalog: &'a ComponentCatalog,
    devices: BTreeMap<DeviceId, &'a Device>,
    symbols: BTreeMap<SymbolId, &'a SymbolInstance>,
}

impl<'a> View<'a> {
    fn new(doc: &'a Schematic, catalog: &'a ComponentCatalog) -> Self {
        let mut devices = BTreeMap::new();
        for device in &doc.devices {
            devices.entry(device.id).or_insert(device);
        }
        let mut symbols = BTreeMap::new();
        for symbol in &doc.symbols {
            symbols.entry(symbol.id).or_insert(symbol);
        }
        Self {
            catalog,
            devices,
            symbols,
        }
    }

    fn device_of(&self, symbol: SymbolId) -> Option<&'a Device> {
        let symbol = self.symbols.get(&symbol)?;
        self.devices.get(&symbol.device_id).copied()
    }

    /// The designator of the device a symbol belongs to.
    fn reference(&self, symbol: SymbolId) -> &'a str {
        self.device_of(symbol)
            .map_or("?", |device| device.reference.0.as_str())
    }

    /// `K1:A1`: the designator and the printed number of a terminal, or its
    /// key when it has no printed number.
    fn pin_name(&self, pin: &PinRef) -> String {
        let label = self.symbols.get(&pin.symbol_id).and_then(|symbol| {
            let device = self.devices.get(&symbol.device_id)?;
            let model = self.catalog.model(&device.model)?;
            let definition = model.symbol(&symbol.symbol_key)?;
            terminal_label(model.class(), definition.kind(), symbol.slot, &pin.terminal)
        });
        format!(
            "{}:{}",
            self.reference(pin.symbol_id),
            label.unwrap_or_else(|| pin.terminal.0.clone())
        )
    }

    /// Whether a symbol is a signal arrow: a name on the drawing, not a part
    /// with terminals to list.
    fn is_arrow(&self, symbol: SymbolId) -> bool {
        self.symbols.get(&symbol).is_some_and(|instance| {
            self.devices
                .get(&instance.device_id)
                .and_then(|device| self.catalog.model(&device.model))
                .and_then(|model| model.symbol(&instance.symbol_key))
                .is_some_and(|definition| definition.kind() == SymbolKind::SignalArrow)
        })
    }

    /// Every pin of a list, named and in reading order; the terminals of signal
    /// arrows are left out (ADR-0016).
    fn names_in_order(&self, pins: &[PinRef]) -> Vec<String> {
        let mut named: Vec<(Natural, SymbolId, usize, String)> = pins
            .iter()
            .filter(|pin| !self.is_arrow(pin.symbol_id))
            .zip(0_usize..)
            .map(|(pin, index)| {
                (
                    natural(self.reference(pin.symbol_id)),
                    pin.symbol_id,
                    index,
                    self.pin_name(pin),
                )
            })
            .collect();
        named.sort();
        named.into_iter().map(|(_, _, _, name)| name).collect()
    }
}

/// The bill of materials: one row for every kind of part, with how many
/// there are and where. Two devices are the same part when their model, its
/// version and their catalog data (`Device::info`) are equal. A tap of a
/// supply conductor is a point of the drawing, not a part, and is left out.
#[must_use]
pub fn bom(doc: &Schematic, catalog: &ComponentCatalog) -> Table {
    let view = View::new(doc, catalog);
    type Key = (String, u32, String, String, String);
    let mut groups: BTreeMap<Key, Vec<&str>> = BTreeMap::new();
    for device in view.devices.values() {
        let is_tap = catalog.model(&device.model).is_some_and(|model| {
            matches!(
                model.class(),
                DeviceClass::PowerSupply | DeviceClass::SignalArrow
            )
        });
        if is_tap {
            continue;
        }
        let info = device.info.clone().unwrap_or_default();
        let key = (
            device.model.id.clone(),
            device.model.version,
            info.manufacturer.unwrap_or_default(),
            info.part_no.unwrap_or_default(),
            info.description.unwrap_or_default(),
        );
        groups
            .entry(key)
            .or_default()
            .push(device.reference.0.as_str());
    }
    let mut parts: Vec<(Vec<&str>, Key)> = groups
        .into_iter()
        .map(|(key, mut references)| {
            references.sort_by_key(|reference| natural(reference));
            (references, key)
        })
        .collect();
    parts.sort_by_key(|(references, key)| {
        (references.first().map(|first| natural(first)), key.clone())
    });

    let mut table = Table::new(
        "Bill of materials",
        &[
            "Item",
            "Qty",
            "References",
            "Model",
            "Manufacturer",
            "Part no",
            "Description",
        ],
    );
    for (item, (references, key)) in (1_u64..).zip(parts) {
        let (model, version, manufacturer, part_no, description) = key;
        table.push(vec![
            item.to_string(),
            references.len().to_string(),
            references.join(" "),
            format!("{model} v{version}"),
            manufacturer,
            part_no,
            description,
        ]);
    }
    table
}

/// The devices: one row for every device in natural order of designators,
/// with its model, how many symbols draw it, the sheets they are on and its
/// catalog data. Unlike the bill of materials, two devices of one part are
/// two rows. Supply taps and signal arrows, which are points of the drawing
/// and not parts, are left out.
#[must_use]
pub fn devices(doc: &Schematic, catalog: &ComponentCatalog) -> Table {
    let view = View::new(doc, catalog);
    let mut rows: Vec<(Natural, DeviceId, Vec<String>)> = Vec::new();
    for device in view.devices.values() {
        let model = catalog.model(&device.model);
        if model.is_some_and(|model| {
            matches!(
                model.class(),
                DeviceClass::PowerSupply | DeviceClass::SignalArrow
            )
        }) {
            continue;
        }
        let drawn: Vec<&SymbolInstance> = view
            .symbols
            .values()
            .filter(|symbol| symbol.device_id == device.id)
            .copied()
            .collect();
        let mut sheets: Vec<u32> = drawn
            .iter()
            .filter_map(|symbol| sheet_at(doc, symbol.position).map(|sheet| sheet.number))
            .collect();
        sheets.sort_unstable();
        sheets.dedup();
        let info = device.info.clone().unwrap_or_default();
        rows.push((
            natural(&device.reference.0),
            device.id,
            vec![
                device.reference.0.clone(),
                format!("{} v{}", device.model.id, device.model.version),
                drawn.len().to_string(),
                sheets
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" "),
                info.manufacturer.unwrap_or_default(),
                info.part_no.unwrap_or_default(),
                info.description.unwrap_or_default(),
            ],
        ));
    }
    rows.sort();
    let mut table = Table::new(
        "Devices",
        &[
            "Reference",
            "Model",
            "Symbols",
            "Sheets",
            "Manufacturer",
            "Part no",
            "Description",
        ],
    );
    for (_, _, cells) in rows {
        table.push(cells);
    }
    table
}

/// The wires: one row for every net that has a wire and a terminal, in the
/// order the nets are numbered. `From` and `To` are the first two terminals
/// in reading order, and `Also` lists the others.
#[must_use]
pub fn wires(doc: &Schematic, catalog: &ComponentCatalog) -> Table {
    let view = View::new(doc, catalog);
    let nets = StaticNets::build(doc, catalog);
    let numbers = WireNumbers::build(doc, catalog, &nets);

    // The data of the wires (ADR-0018) adds five columns, only to a project
    // that has some: the report of any other project is as before.
    let with_data = doc.wires.iter().any(|wire| wire.spec.is_some());
    let mut columns = vec!["Number", "Sheet", "From", "To", "Also"];
    if with_data {
        columns.extend(["Colour", "Cross-section", "Type", "Material", "Insulation"]);
    }
    let mut table = Table::new("Wires", &columns);
    let specs: BTreeMap<WireId, &WireSpec> = doc
        .wires
        .iter()
        .filter_map(|wire| wire.spec.as_ref().map(|spec| (wire.id, spec)))
        .collect();
    for (id, sheet) in numbers.sequence() {
        let Some(net) = nets.net(*id) else {
            continue;
        };
        let names = view.names_in_order(&net.pins);
        let mut names = names.into_iter();
        let Some(from) = names.next() else {
            continue;
        };
        let to = names.next().unwrap_or_default();
        let also: Vec<String> = names.collect();
        let mut row = vec![
            numbers.of_net(*id).unwrap_or_default().to_owned(),
            sheet.map(|sheet| sheet.to_string()).unwrap_or_default(),
            from,
            to,
            also.join(" "),
        ];
        if with_data {
            // The wires of one net may be made of different things: each
            // distinct value once, in the order of the wire IDs.
            for field in 0..WireSpec::FIELDS.len() {
                let mut values: Vec<&str> = Vec::new();
                for wire in &net.wires {
                    let value = specs
                        .get(wire)
                        .and_then(|spec| spec.values().get(field).copied())
                        .and_then(|value| value.as_deref());
                    if let Some(value) = value
                        && !values.contains(&value)
                    {
                        values.push(value);
                    }
                }
                row.push(values.join(" / "));
            }
        }
        table.push(row);
    }
    table
}

/// The terminal plan: one row for every terminal of every symbol, by
/// designator, with the number of its wire and the terminals it meets.
#[must_use]
pub fn terminals(doc: &Schematic, catalog: &ComponentCatalog) -> Table {
    let view = View::new(doc, catalog);
    let nets = StaticNets::build(doc, catalog);
    let numbers = WireNumbers::build(doc, catalog, &nets);

    let mut symbols: Vec<&SymbolInstance> = view.symbols.values().copied().collect();
    symbols.sort_by_key(|symbol| (natural(view.reference(symbol.id)), symbol.id));

    let mut table = Table::new(
        "Terminals",
        &[
            "Reference",
            "Symbol",
            "Terminal",
            "Key",
            "Wire",
            "Connected to",
        ],
    );
    for symbol in symbols {
        if view.is_arrow(symbol.id) {
            continue;
        }
        let Some(device) = view.devices.get(&symbol.device_id) else {
            continue;
        };
        let Some(model) = catalog.model(&device.model) else {
            continue;
        };
        let Some(definition) = model.symbol(&symbol.symbol_key) else {
            continue;
        };
        for terminal in definition.terminals() {
            let pin = PinRef {
                symbol_id: symbol.id,
                terminal: terminal.key.clone(),
            };
            let label =
                terminal_label(model.class(), definition.kind(), symbol.slot, &terminal.key)
                    .unwrap_or_default();
            let net = nets.net_of_pin(&pin).and_then(|id| nets.net(id));
            let (wire, others) = match net {
                Some(net) if !net.wires.is_empty() => {
                    let others: Vec<PinRef> = net
                        .pins
                        .iter()
                        .filter(|other| **other != pin)
                        .cloned()
                        .collect();
                    (
                        numbers.of_net(net.id).unwrap_or_default().to_owned(),
                        view.names_in_order(&others).join(" "),
                    )
                }
                _ => (String::new(), String::new()),
            };
            table.push(vec![
                device.reference.0.clone(),
                symbol.symbol_key.0.clone(),
                label,
                terminal.key.0.clone(),
                wire,
                others,
            ]);
        }
    }
    table
}

/// The terminal plan: one row for every terminal of a strip, by strip and
/// number, with the number of the wire on each side and the terminals that
/// side meets. `In` is the first terminal of the symbol (the upper one) and
/// `Out` the second. The same table is the CSV for a manufacturer's import.
#[must_use]
pub fn strips(doc: &Schematic, catalog: &ComponentCatalog) -> Table {
    let view = View::new(doc, catalog);
    let nets = StaticNets::build(doc, catalog);
    let numbers = WireNumbers::build(doc, catalog, &nets);

    let mut table = Table::new(
        "Terminal plan",
        &[
            "Strip", "Terminal", "Device", "In wire", "In from", "Out wire", "Out to",
        ],
    );
    for terminal in strip_terminals(doc, catalog) {
        let reference = view
            .devices
            .get(&terminal.device)
            .map(|device| device.reference.0.clone())
            .unwrap_or_default();
        let side = |key: &str| -> (String, String) {
            let Some(symbol) = terminal.symbol else {
                return (String::new(), String::new());
            };
            let pin = PinRef {
                symbol_id: symbol,
                terminal: circuit_model::TerminalKey(key.to_owned()),
            };
            let net = nets.net_of_pin(&pin).and_then(|id| nets.net(id));
            match net {
                Some(net) if !net.wires.is_empty() => {
                    let others: Vec<PinRef> = net
                        .pins
                        .iter()
                        .filter(|other| **other != pin)
                        .cloned()
                        .collect();
                    (
                        numbers.of_net(net.id).unwrap_or_default().to_owned(),
                        view.names_in_order(&others).join(" "),
                    )
                }
                _ => (String::new(), String::new()),
            }
        };
        let (in_wire, in_from) = side("p1");
        let (out_wire, out_to) = side("p2");
        table.push(vec![
            terminal.strip,
            terminal.number.to_string(),
            reference,
            in_wire,
            in_from,
            out_wire,
            out_to,
        ]);
    }
    table
}

/// Whether a symbol kind is a coil that drives contacts.
const fn is_coil(kind: SymbolKind) -> bool {
    matches!(kind, SymbolKind::Coil | SymbolKind::TransformerPrimary)
}

/// Whether a symbol kind is driven by a coil.
const fn is_driven(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::ContactNo
            | SymbolKind::ContactNc
            | SymbolKind::Changeover
            | SymbolKind::PowerContacts { .. }
            | SymbolKind::TransformerSecondary
    )
}

/// The cross-reference exceptions: a coil with no contact on the drawing, the
/// contacts of a device whose coil is not drawn, and, when the project has
/// sheets, every symbol that is outside all of them.
#[must_use]
pub fn xref(doc: &Schematic, catalog: &ComponentCatalog) -> Table {
    let view = View::new(doc, catalog);
    let mut found: Vec<Exception> = Vec::new();

    let mut by_device: BTreeMap<DeviceId, Vec<&SymbolInstance>> = BTreeMap::new();
    for symbol in view.symbols.values() {
        by_device.entry(symbol.device_id).or_default().push(symbol);
    }
    for (device_id, symbols) in &by_device {
        let Some(device) = view.devices.get(device_id) else {
            continue;
        };
        let Some(model) = catalog.model(&device.model) else {
            continue;
        };
        let kind_of = |symbol: &SymbolInstance| model.symbol(&symbol.symbol_key).map(|d| d.kind());
        let coils: Vec<&&SymbolInstance> = symbols
            .iter()
            .filter(|symbol| kind_of(symbol).is_some_and(is_coil))
            .collect();
        let driven: Vec<&&SymbolInstance> = symbols
            .iter()
            .filter(|symbol| kind_of(symbol).is_some_and(is_driven))
            .collect();
        let model_has_coil = model.symbols().iter().any(|d| is_coil(d.kind()));
        let reference = natural(&device.reference.0);
        if let (Some(coil), true) = (coils.first(), driven.is_empty()) {
            found.push((
                reference.clone(),
                0,
                coil.id,
                device.reference.0.clone(),
                coil.symbol_key.0.clone(),
                "Coil without contacts",
            ));
        }
        if let (Some(contact), true, true) = (driven.first(), coils.is_empty(), model_has_coil) {
            found.push((
                reference,
                1,
                contact.id,
                device.reference.0.clone(),
                contact.symbol_key.0.clone(),
                "Contacts without a coil",
            ));
        }
    }
    if !doc.sheets.is_empty() {
        for symbol in view.symbols.values() {
            if sheet_at(doc, symbol.position).is_none() {
                found.push((
                    natural(view.reference(symbol.id)),
                    2,
                    symbol.id,
                    view.reference(symbol.id).to_owned(),
                    symbol.symbol_key.0.clone(),
                    "Outside the sheets",
                ));
            }
        }
    }
    found.sort();

    let mut table = Table::new(
        "Cross-reference exceptions",
        &["Reference", "Symbol", "Problem"],
    );
    for (_, _, _, reference, symbol, problem) in found {
        table.push(vec![reference, symbol, problem.to_owned()]);
    }
    table
}

/// The I/O map of the Modbus server (ADR-0009), one row for every address.
#[must_use]
pub fn io(doc: &Schematic, catalog: &ComponentCatalog) -> Table {
    let mut table = Table::new("I/O map", &["Table", "Address", "Words", "Name", "Meaning"]);
    for entry in IoMap::new(doc, catalog).entries() {
        let name = match entry.table {
            circuit_io::Table::Coil => "coil",
            circuit_io::Table::DiscreteInput => "discrete input",
            circuit_io::Table::InputRegister => "input register",
        };
        table.push(vec![
            name.to_owned(),
            entry.address.to_string(),
            entry.words.to_string(),
            entry.name,
            entry.meaning.to_owned(),
        ]);
    }
    table
}

/// The diagnostics, as they are given, one row for each, with whether the
/// project waives it and why.
#[must_use]
pub fn problems(doc: &Schematic, diagnostics: &[Diagnostic]) -> Table {
    let waivers = WaiverIndex::new(doc);
    let mut table = Table::new(
        "Problems",
        &[
            "Severity", "Code", "Subject", "Message", "Related", "Waived", "Reason",
        ],
    );
    for diagnostic in diagnostics {
        let related: Vec<String> = diagnostic
            .related()
            .iter()
            .map(ToString::to_string)
            .collect();
        let waiver = waivers.of(diagnostic);
        table.push(vec![
            diagnostic.severity().as_str().to_owned(),
            diagnostic.code().as_str().to_owned(),
            diagnostic.subject().to_string(),
            diagnostic.message().to_owned(),
            related.join("; "),
            if waiver.is_some() { "yes" } else { "" }.to_owned(),
            waiver.map(|w| w.reason.clone()).unwrap_or_default(),
        ]);
    }
    table
}

#[cfg(test)]
mod tests;
