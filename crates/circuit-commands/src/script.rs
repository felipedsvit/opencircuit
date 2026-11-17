//! Edit files: a list of operations on a project, in JSON, applied through
//! the same operations the editor uses (EXTENDING.md, "Editing from a
//! script"). `opencircuit edit --apply`, the MCP server and the macros of
//! the window read them here.
//!
//! The file holds data only: nothing in it is executed. It is read strictly
//! (unknown fields and operations are refused) with limits on its size and on
//! every list, applied operation by operation to a copy of the project, and
//! the project is replaced only when every operation succeeded.
//!
//! Elements an operation creates can be given a name (`"as": "coil"`) that
//! later operations use, since their IDs are not known before the file runs.
//! A device is named by its designator, so `"K1"` is the device `K1`.

use std::collections::BTreeMap;
use std::fmt;

use crate::sheets::{self, SheetSpec};
use crate::{Anchor, CommandError, Edit, SupplyPreset, Transaction, ops};
use crate::{designators, documentation, layers, waivers};
use circuit_model::{
    ComponentCatalog, DeviceId, Endpoint, GridPoint, JunctionId, LayerId, ModelRef, Orientation,
    ParamValue, PinRef, QuarterTurn, Schematic, SheetSize, Subject, SupplyId, SymbolId, SymbolKey,
    TerminalKey, TextId, WireId, Zones,
};
use serde::Deserialize;
use serde_json::Value;

/// Largest edit file accepted, in bytes.
pub const MAX_EDIT_BYTES: usize = 1024 * 1024;
/// Most operations in one file.
const MAX_OPERATIONS: usize = 10_000;
/// Most elements in one list of an operation, and waypoints in one wire.
const MAX_LIST: usize = 10_000;
/// Longest name an operation may give an element.
const MAX_NAME: usize = 64;
/// The version of the file layout this build reads.
pub(crate) const SCHEMA_VERSION: u32 = 1;

/// A field that may be left alone, cleared or set.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
enum Field {
    /// Absent from the operation: keep what the project has.
    #[default]
    Keep,
    /// `null`: clear it.
    Clear,
    /// A text.
    Set(String),
}

impl<'de> Deserialize<'de> for Field {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match Option::<String>::deserialize(deserializer)? {
            None => Self::Clear,
            Some(text) => Self::Set(text),
        })
    }
}

impl Field {
    fn apply(&self, current: Option<&String>) -> Option<String> {
        match self {
            Self::Keep => current.cloned(),
            Self::Clear => None,
            Self::Set(text) => Some(text.clone()),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EditFile {
    schema_version: u32,
    operations: Vec<Value>,
}

/// One operation of the file. The elements it refers to are JSON values,
/// resolved when it runs, so that an error can name what was wrong.
#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Operation {
    /// Set the name and the fields of the title block.
    Project {
        name: Option<String>,
        #[serde(default)]
        author: Field,
        #[serde(default)]
        company: Field,
        #[serde(default)]
        revision: Field,
        #[serde(default)]
        date: Field,
    },
    /// Add a supply profile: `dc_24v`, `single_phase_230v` or
    /// `three_phase_400v`.
    AddSupply {
        preset: String,
        #[serde(rename = "as")]
        name: Option<String>,
    },
    /// Place a new device drawn as one of its symbols.
    Place {
        model: String,
        version: Option<u32>,
        symbol: String,
        at: [i32; 2],
        rotation: Option<u16>,
        #[serde(rename = "as")]
        name: Option<String>,
        reference: Option<String>,
    },
    /// Add another symbol to a device.
    AddSymbol {
        device: String,
        symbol: String,
        at: [i32; 2],
        rotation: Option<u16>,
        #[serde(rename = "as")]
        name: Option<String>,
    },
    /// Set a parameter of a device; `null` clears it.
    SetParam {
        device: String,
        key: String,
        value: Value,
    },
    /// Rename a device.
    SetReference { device: String, to: String },
    /// Add a junction.
    Junction {
        at: [i32; 2],
        #[serde(rename = "as")]
        name: Option<String>,
    },
    /// Wire two endpoints, through waypoints if given.
    Connect {
        from: Value,
        to: Value,
        #[serde(default)]
        via: Vec<[i32; 2]>,
        #[serde(rename = "as")]
        name: Option<String>,
    },
    /// Move symbols, junctions and the symbols of devices.
    Move { elements: Vec<Value>, by: [i32; 2] },
    /// Turn symbols a quarter turn clockwise.
    Rotate { symbols: Vec<Value> },
    /// Delete elements and what hangs on them.
    Delete { elements: Vec<Value> },
    /// Add a sheet.
    AddSheet {
        number: Option<u32>,
        title: Option<String>,
        origin: [i32; 2],
        size: Option<Value>,
        orientation: Option<String>,
        zones: Option<[u8; 2]>,
    },
    /// Remove the sheet with this number.
    RemoveSheet { number: u32 },
    /// Set the catalog data of a device (ADR-0014); a field left out is
    /// kept and `null` clears it.
    SetDeviceInfo {
        device: String,
        #[serde(default)]
        manufacturer: Field,
        #[serde(default)]
        part_no: Field,
        #[serde(default)]
        description: Field,
        #[serde(default)]
        location: Field,
    },
    /// Set the number that replaces the generated one on a wire; `null`, or
    /// leaving it out, clears it.
    SetWireNumber { wire: Value, number: Option<String> },
    /// Set the rule that numbers the wires. What is left out keeps its value
    /// (an empty prefix, 1, 1 and a single counter for a new rule);
    /// `"enabled": false` removes the rule.
    Numbering {
        prefix: Option<String>,
        start: Option<u32>,
        step: Option<u32>,
        per_sheet: Option<bool>,
        enabled: Option<bool>,
    },
    /// Waive a warning or informational diagnostic, with a reason. The
    /// subject is a name, `{ "device": "K1" }`, `{ "wire": ... }`,
    /// `{ "junction": ... }`, `{ "symbol": ..., "terminal": "p2" }` for a
    /// terminal, or `"document"`.
    Waive {
        code: String,
        subject: Value,
        reason: String,
    },
    /// Take a waiver away.
    Unwaive { code: String, subject: Value },
    /// Renumber the designators in reading order, on every sheet or on the
    /// sheet with this number.
    Renumber { sheet: Option<u32> },
    /// Number the sheets 1, 2, 3 in reading order of their frames.
    RenumberSheets {},
    /// Give wires an orthogonal route around symbols, by name or ID.
    Route { wires: Vec<Value> },
    /// Replace a text in the designators, the text notes and the text
    /// parameters; each kind can be left out.
    Replace {
        find: String,
        with: String,
        designators: Option<bool>,
        notes: Option<bool>,
        parameters: Option<bool>,
    },
    /// Add a text note (ADR-0015): one line of at most 120 characters.
    AddText {
        at: [i32; 2],
        text: String,
        anchor: Option<String>,
        #[serde(rename = "as")]
        name: Option<String>,
    },
    /// Change the words of a text note, by name or ID.
    SetText { text: Value, to: String },
    /// Add a layer (ADR-0015), visible and unlocked unless said otherwise.
    AddLayer {
        name: String,
        visible: Option<bool>,
        locked: Option<bool>,
        #[serde(rename = "as")]
        alias: Option<String>,
    },
    /// Rename, show, hide, lock or unlock a layer; what is left out stays.
    SetLayer {
        layer: Value,
        name: Option<String>,
        visible: Option<bool>,
        locked: Option<bool>,
    },
    /// Remove a layer nothing is on.
    RemoveLayer { layer: Value },
    /// Put symbols, wires, text notes and the symbols of devices on a layer;
    /// `null` is the default layer.
    AssignLayer { layer: Value, elements: Vec<Value> },
}

impl Operation {
    fn name(&self) -> &'static str {
        match self {
            Self::Project { .. } => "project",
            Self::AddSupply { .. } => "add_supply",
            Self::Place { .. } => "place",
            Self::AddSymbol { .. } => "add_symbol",
            Self::SetParam { .. } => "set_param",
            Self::SetReference { .. } => "set_reference",
            Self::Junction { .. } => "junction",
            Self::Connect { .. } => "connect",
            Self::Move { .. } => "move",
            Self::Rotate { .. } => "rotate",
            Self::Delete { .. } => "delete",
            Self::AddSheet { .. } => "add_sheet",
            Self::RemoveSheet { .. } => "remove_sheet",
            Self::SetDeviceInfo { .. } => "set_device_info",
            Self::SetWireNumber { .. } => "set_wire_number",
            Self::Numbering { .. } => "numbering",
            Self::Waive { .. } => "waive",
            Self::Unwaive { .. } => "unwaive",
            Self::Renumber { .. } => "renumber",
            Self::RenumberSheets {} => "renumber_sheets",
            Self::Replace { .. } => "replace",
            Self::Route { .. } => "route",
            Self::AddText { .. } => "add_text",
            Self::SetText { .. } => "set_text",
            Self::AddLayer { .. } => "add_layer",
            Self::SetLayer { .. } => "set_layer",
            Self::RemoveLayer { .. } => "remove_layer",
            Self::AssignLayer { .. } => "assign_layer",
        }
    }
}

/// An element an operation created and named.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Created {
    /// A supply profile.
    Supply(SupplyId),
    /// A symbol.
    Symbol(SymbolId),
    /// A junction.
    Junction(JunctionId),
    /// A wire.
    Wire(WireId),
    /// A text note.
    Text(TextId),
    /// A layer.
    Layer(LayerId),
}

impl Created {
    /// The kind of element, as the output names it.
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::Supply(_) => "supply",
            Self::Symbol(_) => "symbol",
            Self::Junction(_) => "junction",
            Self::Wire(_) => "wire",
            Self::Text(_) => "text",
            Self::Layer(_) => "layer",
        }
    }

    /// The ID of the element.
    #[must_use]
    pub const fn id(self) -> u64 {
        match self {
            Self::Supply(id) => id.0,
            Self::Symbol(id) => id.0,
            Self::Junction(id) => id.0,
            Self::Wire(id) => id.0,
            Self::Text(id) => id.0,
            Self::Layer(id) => id.0,
        }
    }
}

/// What applying a file produced.
#[derive(Debug)]
pub struct Applied {
    /// The edited project.
    pub doc: Schematic,
    /// How many operations ran.
    pub operations: usize,
    /// The named elements, with their IDs.
    pub created: BTreeMap<String, Created>,
    /// Every edit of the file in one transaction: applied to the project the
    /// file was applied to, it gives `doc`, and one undo takes it back.
    pub transaction: Transaction,
}

/// Why a file could not be applied.
#[derive(Debug, Eq, PartialEq)]
pub enum EditError {
    /// The file is not a valid edit file: not JSON, an unknown field or
    /// operation, a list over its limit. Nothing was applied.
    Invalid(String),
    /// An operation was refused. Nothing was applied.
    Refused {
        /// Its position in the file, from 1.
        operation: usize,
        /// Its name.
        op: &'static str,
        /// Why.
        message: String,
    },
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => f.write_str(message),
            Self::Refused {
                operation,
                op,
                message,
            } => write!(f, "operation {operation} ({op}): {message}"),
        }
    }
}

/// Read an edit file.
fn parse(text: &str) -> Result<Vec<Operation>, EditError> {
    if text.len() > MAX_EDIT_BYTES {
        return Err(EditError::Invalid(format!(
            "the edit file is larger than {MAX_EDIT_BYTES} bytes"
        )));
    }
    let file: EditFile = serde_json::from_str(text)
        .map_err(|error| EditError::Invalid(format!("not a valid edit file: {error}")))?;
    if file.schema_version != SCHEMA_VERSION {
        return Err(EditError::Invalid(format!(
            "schema version {} is not supported; this build reads version {SCHEMA_VERSION}",
            file.schema_version
        )));
    }
    if file.operations.len() > MAX_OPERATIONS {
        return Err(EditError::Invalid(format!(
            "the edit file has more than {MAX_OPERATIONS} operations"
        )));
    }
    let mut operations = Vec::with_capacity(file.operations.len());
    for (index, value) in file.operations.into_iter().enumerate() {
        let number = index.saturating_add(1);
        let operation: Operation = serde_json::from_value(value)
            .map_err(|error| EditError::Invalid(format!("operation {number}: {error}")))?;
        check_lists(&operation)
            .map_err(|message| EditError::Invalid(format!("operation {number}: {message}")))?;
        operations.push(operation);
    }
    Ok(operations)
}

/// The limits on the lists of an operation and on the names it gives.
fn check_lists(operation: &Operation) -> Result<(), String> {
    let too_long = |what: &str, len: usize| {
        (len > MAX_LIST).then(|| format!("{what} has more than {MAX_LIST} entries"))
    };
    let name = |name: &Option<String>| -> Result<(), String> {
        let Some(name) = name else {
            return Ok(());
        };
        let fine = !name.is_empty()
            && name.len() <= MAX_NAME
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
        if fine {
            Ok(())
        } else {
            Err(format!(
                "the name `{name}` must be 1 to {MAX_NAME} letters, digits, `_`, `-` or `.`"
            ))
        }
    };
    let problem = match operation {
        Operation::AddSupply { name: n, .. }
        | Operation::Place { name: n, .. }
        | Operation::AddSymbol { name: n, .. }
        | Operation::Junction { name: n, .. } => return name(n),
        Operation::Connect { via, name: n, .. } => {
            name(n)?;
            too_long("`via`", via.len())
        }
        Operation::Move { elements, .. } | Operation::Delete { elements } => {
            too_long("`elements`", elements.len())
        }
        Operation::Rotate { symbols } => too_long("`symbols`", symbols.len()),
        _ => None,
    };
    problem.map_or(Ok(()), Err)
}

/// Apply the file to `doc`. Operations run in order, each one on the result
/// of the one before; the first refusal stops everything and `doc` is not
/// changed, because the caller keeps its own copy.
///
/// # Errors
///
/// Returns [`EditError::Invalid`] for a file that cannot be read as an edit
/// file and [`EditError::Refused`] for the operation that was refused.
pub fn apply(doc: Schematic, catalog: &ComponentCatalog, text: &str) -> Result<Applied, EditError> {
    let operations = parse(text)?;
    let start = doc.next_id;
    let mut run = Run {
        doc,
        catalog,
        names: BTreeMap::new(),
        transaction: Transaction::new("Apply edit file", start),
    };
    let count = operations.len();
    for (index, operation) in operations.iter().enumerate() {
        run.step(operation).map_err(|message| EditError::Refused {
            operation: index.saturating_add(1),
            op: operation.name(),
            message,
        })?;
    }
    let mut transaction = run.transaction;
    transaction.next_id = run.doc.next_id;
    Ok(Applied {
        doc: run.doc,
        operations: count,
        created: run.names,
        transaction,
    })
}

/// Run `operations` on `doc` with the names already given in `names`, for
/// the recorder of macros: the project after them and every name, or why an
/// operation failed.
pub(crate) fn run_values(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    names: &BTreeMap<String, Created>,
    operations: &[Value],
) -> Result<(Schematic, BTreeMap<String, Created>), String> {
    let mut run = Run {
        doc: doc.clone(),
        catalog,
        names: names.clone(),
        transaction: Transaction::new("Record", doc.next_id),
    };
    for value in operations {
        let operation: Operation =
            serde_json::from_value(value.clone()).map_err(|error| error.to_string())?;
        check_lists(&operation)?;
        run.step(&operation)?;
    }
    Ok((run.doc, run.names))
}

struct Run<'a> {
    doc: Schematic,
    catalog: &'a ComponentCatalog,
    names: BTreeMap<String, Created>,
    transaction: Transaction,
}

fn point([x, y]: [i32; 2]) -> GridPoint {
    GridPoint::new(x, y)
}

fn refused(error: CommandError) -> String {
    error.to_string()
}

fn rotation(degrees: Option<u16>) -> Result<QuarterTurn, String> {
    match degrees.unwrap_or(0) {
        0 => Ok(QuarterTurn::Deg0),
        90 => Ok(QuarterTurn::Deg90),
        180 => Ok(QuarterTurn::Deg180),
        270 => Ok(QuarterTurn::Deg270),
        other => Err(format!("a rotation is 0, 90, 180 or 270, not {other}")),
    }
}

impl Run<'_> {
    /// Apply one operation to the working document.
    fn step(&mut self, operation: &Operation) -> Result<(), String> {
        match operation {
            Operation::Project {
                name,
                author,
                company,
                revision,
                date,
            } => {
                let project = &self.doc.project;
                let tx = sheets::set_project_info(
                    &self.doc,
                    name.as_deref().unwrap_or(&project.name),
                    author.apply(project.author.as_ref()).as_deref(),
                    company.apply(project.company.as_ref()).as_deref(),
                    revision.apply(project.revision.as_ref()).as_deref(),
                    date.apply(project.date.as_ref()).as_deref(),
                )
                .map_err(refused)?;
                self.commit(&tx)
            }
            Operation::AddSupply { preset, name } => {
                let preset = match preset.as_str() {
                    "dc_24v" => SupplyPreset::Dc24V,
                    "single_phase_230v" => SupplyPreset::SinglePhase230V,
                    "three_phase_400v" => SupplyPreset::ThreePhase400V,
                    other => {
                        return Err(format!(
                            "unknown preset `{other}`; use `dc_24v`, `single_phase_230v` or `three_phase_400v`"
                        ));
                    }
                };
                let tx = ops::add_supply(&self.doc, preset).map_err(refused)?;
                let id = tx.edits.iter().find_map(|edit| match edit {
                    Edit::InsertSupply(supply) => Some(supply.id),
                    _ => None,
                });
                self.commit(&tx)?;
                self.remember(name.as_deref(), id.map(Created::Supply))
            }
            Operation::Place {
                model,
                version,
                symbol,
                at,
                rotation: turn,
                name,
                reference,
            } => {
                let version = match version {
                    Some(version) => *version,
                    None => self
                        .catalog
                        .latest()
                        .find(|candidate| candidate.id() == model)
                        .map(circuit_model::ComponentModel::version)
                        .ok_or_else(|| format!("the catalog has no model `{model}`"))?,
                };
                let model = ModelRef {
                    id: model.clone(),
                    version,
                };
                let tx = ops::place(
                    &self.doc,
                    self.catalog,
                    &model,
                    &SymbolKey(symbol.clone()),
                    point(*at),
                    rotation(*turn)?,
                )
                .map_err(refused)?;
                let placed = first_symbol(&tx);
                self.commit(&tx)?;
                self.remember(name.as_deref(), placed.map(Created::Symbol))?;
                if let (Some(reference), Some(symbol)) = (reference, placed) {
                    let device = self.device_of_symbol(symbol)?;
                    let tx = ops::set_reference(&self.doc, device, reference).map_err(refused)?;
                    self.commit(&tx)?;
                }
                Ok(())
            }
            Operation::AddSymbol {
                device,
                symbol,
                at,
                rotation: turn,
                name,
            } => {
                let device = self.device(device)?;
                let tx = ops::add_symbol(
                    &self.doc,
                    self.catalog,
                    device,
                    &SymbolKey(symbol.clone()),
                    point(*at),
                    rotation(*turn)?,
                )
                .map_err(refused)?;
                let added = first_symbol(&tx);
                self.commit(&tx)?;
                self.remember(name.as_deref(), added.map(Created::Symbol))
            }
            Operation::SetParam { device, key, value } => {
                let device = self.device(device)?;
                let value = self.param_value(value)?;
                let tx =
                    ops::set_param(&self.doc, self.catalog, device, key, value).map_err(refused)?;
                self.commit(&tx)
            }
            Operation::SetReference { device, to } => {
                let device = self.device(device)?;
                let tx = ops::set_reference(&self.doc, device, to).map_err(refused)?;
                self.commit(&tx)
            }
            Operation::Junction { at, name } => {
                let tx = sheets::add_junction(&self.doc, point(*at)).map_err(refused)?;
                let id = tx.edits.iter().find_map(|edit| match edit {
                    Edit::InsertJunction(junction) => Some(junction.id),
                    _ => None,
                });
                self.commit(&tx)?;
                self.remember(name.as_deref(), id.map(Created::Junction))
            }
            Operation::Connect {
                from,
                to,
                via,
                name,
            } => {
                let from = self.anchor(from)?;
                let to = self.anchor(to)?;
                let via: Vec<GridPoint> = via.iter().copied().map(point).collect();
                let tx =
                    ops::connect(&self.doc, self.catalog, &from, &to, &via).map_err(refused)?;
                let wire = tx.edits.iter().rev().find_map(|edit| match edit {
                    Edit::InsertWire(wire) => Some(wire.id),
                    _ => None,
                });
                self.commit(&tx)?;
                self.remember(name.as_deref(), wire.map(Created::Wire))
            }
            Operation::Move { elements, by } => {
                let subjects = self.subjects(elements)?;
                let tx = ops::move_by(&self.doc, self.catalog, &subjects, point(*by))
                    .map_err(refused)?;
                self.commit(&tx)
            }
            Operation::Rotate { symbols } => {
                let ids = symbols
                    .iter()
                    .map(|value| self.symbol(value))
                    .collect::<Result<_, _>>()?;
                let tx = ops::rotate(&self.doc, self.catalog, &ids).map_err(refused)?;
                self.commit(&tx)
            }
            Operation::Delete { elements } => {
                let subjects = self.subjects(elements)?;
                let tx = ops::delete(&self.doc, self.catalog, &subjects).map_err(refused)?;
                self.commit(&tx)
            }
            Operation::AddSheet {
                number,
                title,
                origin,
                size,
                orientation,
                zones,
            } => {
                let spec = SheetSpec {
                    number: *number,
                    title: title.clone().unwrap_or_default(),
                    origin: point(*origin),
                    size: sheet_size(size.as_ref())?,
                    orientation: match orientation.as_deref() {
                        None | Some("landscape") => Orientation::Landscape,
                        Some("portrait") => Orientation::Portrait,
                        Some(other) => {
                            return Err(format!(
                                "unknown orientation `{other}`; use `landscape` or `portrait`"
                            ));
                        }
                    },
                    zones: zones.map_or(
                        Zones {
                            columns: 8,
                            rows: 6,
                        },
                        |[columns, rows]| Zones { columns, rows },
                    ),
                };
                let tx = sheets::add_sheet(&self.doc, &spec).map_err(refused)?;
                self.commit(&tx)
            }
            Operation::RemoveSheet { number } => {
                let id = self
                    .doc
                    .sheets
                    .iter()
                    .find(|sheet| sheet.number == *number)
                    .map(|sheet| sheet.id)
                    .ok_or_else(|| format!("the project has no sheet number {number}"))?;
                let tx = sheets::remove_sheet(&self.doc, id).map_err(refused)?;
                self.commit(&tx)
            }
            Operation::SetDeviceInfo {
                device,
                manufacturer,
                part_no,
                description,
                location,
            } => {
                let device = self.device(device)?;
                let current = self
                    .doc
                    .devices
                    .iter()
                    .find(|candidate| candidate.id == device)
                    .and_then(|found| found.info.clone())
                    .unwrap_or_default();
                let info = circuit_model::DeviceInfo {
                    manufacturer: manufacturer.apply(current.manufacturer.as_ref()),
                    part_no: part_no.apply(current.part_no.as_ref()),
                    description: description.apply(current.description.as_ref()),
                    location: location.apply(current.location.as_ref()),
                };
                let tx =
                    documentation::set_device_info(&self.doc, device, info).map_err(refused)?;
                self.commit(&tx)
            }
            Operation::SetWireNumber { wire, number } => {
                let wire = WireId(self.named_or_id(wire, "wire")?);
                let tx = documentation::set_wire_number(&self.doc, wire, number.as_deref())
                    .map_err(refused)?;
                self.commit(&tx)
            }
            Operation::Numbering {
                prefix,
                start,
                step,
                per_sheet,
                enabled,
            } => {
                let rule = if *enabled == Some(false) {
                    None
                } else {
                    let current = self.doc.project.wire_numbering.clone();
                    let step = match step {
                        Some(step) => std::num::NonZeroU32::new(*step)
                            .ok_or("the step of the numbering must be 1 or more")?,
                        None => current
                            .as_ref()
                            .map_or(std::num::NonZeroU32::MIN, |rule| rule.step),
                    };
                    Some(circuit_model::WireNumbering {
                        prefix: prefix
                            .clone()
                            .or_else(|| current.as_ref().map(|rule| rule.prefix.clone()))
                            .unwrap_or_default(),
                        start: start
                            .or(current.as_ref().map(|rule| rule.start))
                            .unwrap_or(1),
                        step,
                        per_sheet: per_sheet
                            .or(current.as_ref().map(|rule| rule.per_sheet))
                            .unwrap_or(false),
                    })
                };
                let tx = documentation::set_wire_numbering(&self.doc, rule).map_err(refused)?;
                self.commit(&tx)
            }
            Operation::Waive {
                code,
                subject,
                reason,
            } => {
                let subject = self.waiver_subject(subject)?;
                let tx = waivers::waive(&self.doc, code, subject, reason).map_err(refused)?;
                self.commit(&tx)
            }
            Operation::Unwaive { code, subject } => {
                let subject = self.waiver_subject(subject)?;
                let tx = waivers::unwaive(&self.doc, code, subject).map_err(refused)?;
                self.commit(&tx)
            }
            Operation::AddText {
                at,
                text,
                anchor,
                name,
            } => {
                let anchor = match anchor.as_deref() {
                    None | Some("left") => circuit_model::TextAnchor::Left,
                    Some("center") => circuit_model::TextAnchor::Center,
                    Some(other) => {
                        return Err(format!("unknown anchor `{other}`; use `left` or `center`"));
                    }
                };
                let tx = ops::add_text(&self.doc, point(*at), text, anchor).map_err(refused)?;
                let id = tx.edits.iter().find_map(|edit| match edit {
                    Edit::InsertText(note) => Some(note.id),
                    _ => None,
                });
                self.commit(&tx)?;
                self.remember(name.as_deref(), id.map(Created::Text))
            }
            Operation::SetText { text, to } => {
                let id = TextId(self.named_or_id(text, "text")?);
                let tx = ops::set_text(&self.doc, id, to).map_err(refused)?;
                self.commit(&tx)
            }
            Operation::AddLayer {
                name,
                visible,
                locked,
                alias,
            } => {
                let tx = layers::add_layer(
                    &self.doc,
                    name,
                    visible.unwrap_or(true),
                    locked.unwrap_or(false),
                )
                .map_err(refused)?;
                let id = tx.edits.iter().find_map(|edit| match edit {
                    Edit::InsertLayer(layer) => Some(layer.id),
                    _ => None,
                });
                self.commit(&tx)?;
                self.remember(alias.as_deref(), id.map(Created::Layer))
            }
            Operation::SetLayer {
                layer,
                name,
                visible,
                locked,
            } => {
                let id = self.layer(layer)?;
                let tx = layers::set_layer(&self.doc, id, name.as_deref(), *visible, *locked)
                    .map_err(refused)?;
                self.commit(&tx)
            }
            Operation::RemoveLayer { layer } => {
                let id = self.layer(layer)?;
                let tx = layers::remove_layer(&self.doc, id).map_err(refused)?;
                self.commit(&tx)
            }
            Operation::AssignLayer { layer, elements } => {
                let target = match layer {
                    Value::Null => None,
                    other => Some(self.layer(other)?),
                };
                let subjects = self.subjects(elements)?;
                let tx = layers::assign_layer(&self.doc, &subjects, target).map_err(refused)?;
                self.commit(&tx)
            }
            Operation::Renumber { sheet } => {
                let scope = sheet.map_or(designators::Scope::All, designators::Scope::Sheet);
                let tx = designators::renumber(&self.doc, scope).map_err(refused)?;
                self.commit(&tx)
            }
            Operation::RenumberSheets {} => self.commit(&sheets::renumber_sheets(&self.doc)),
            Operation::Route { wires } => {
                let ids = wires
                    .iter()
                    .map(|wire| self.named_or_id(wire, "wire").map(WireId))
                    .collect::<Result<Vec<_>, _>>()?;
                let tx = crate::route::route(&self.doc, self.catalog, &ids).map_err(refused)?;
                self.commit(&tx)
            }
            Operation::Replace {
                find,
                with,
                designators,
                notes,
                parameters,
            } => {
                let fields = crate::find::Fields {
                    designators: designators.unwrap_or(true),
                    notes: notes.unwrap_or(true),
                    parameters: parameters.unwrap_or(true),
                };
                let tx = crate::find::replace(&self.doc, self.catalog, fields, find, with)
                    .map_err(refused)?;
                self.commit(&tx)
            }
        }
    }

    /// The subject of a waiver: `"document"`, a terminal `{ "symbol": ...,
    /// "terminal": "p2" }`, or an element as for `move` and `delete`.
    fn waiver_subject(&self, value: &Value) -> Result<Subject, String> {
        if value.as_str() == Some("document") {
            return Ok(Subject::Document);
        }
        let terminal_of = value.as_object().and_then(|object| {
            Some((
                object.get("symbol")?,
                object.get("terminal").and_then(Value::as_str)?,
            ))
        });
        if let Some((symbol, terminal)) = terminal_of {
            return Ok(Subject::Pin(PinRef {
                symbol_id: self.symbol(symbol)?,
                terminal: TerminalKey(terminal.to_owned()),
            }));
        }
        let mut subjects = self.subjects(std::slice::from_ref(value))?.into_iter();
        match (subjects.next(), subjects.next()) {
            (Some(subject), None) => Ok(subject),
            _ => Err("a waiver is about one element".to_owned()),
        }
    }

    /// Apply a transaction to the working document, with every check of the
    /// editor.
    fn commit(&mut self, tx: &Transaction) -> Result<(), String> {
        let (next, _) = tx.apply_to(&self.doc).map_err(refused)?;
        self.doc = next;
        self.transaction.edits.extend(tx.edits.iter().cloned());
        Ok(())
    }

    fn remember(&mut self, name: Option<&str>, created: Option<Created>) -> Result<(), String> {
        let (Some(name), Some(created)) = (name, created) else {
            return Ok(());
        };
        if self.names.insert(name.to_owned(), created).is_some() {
            return Err(format!("the name `{name}` is already used"));
        }
        Ok(())
    }

    /// A layer: a name given with `as`, the name it has in the project, or
    /// an ID.
    fn layer(&self, value: &Value) -> Result<LayerId, String> {
        match value {
            Value::String(name) => match self.names.get(name) {
                Some(Created::Layer(id)) => Ok(*id),
                Some(other) => Err(format!("`{name}` is a {}, not a layer", other.kind())),
                None => self
                    .doc
                    .layers
                    .iter()
                    .find(|layer| layer.name == *name)
                    .map(|layer| layer.id)
                    .ok_or_else(|| format!("the project has no layer `{name}`")),
            },
            Value::Number(number) => number
                .as_u64()
                .map(LayerId)
                .ok_or_else(|| "a layer ID is a whole number".to_owned()),
            _ => Err("a layer is a name or an ID".to_owned()),
        }
    }

    /// A device by designator.
    fn device(&self, reference: &str) -> Result<DeviceId, String> {
        self.doc
            .devices
            .iter()
            .find(|device| device.reference.0 == reference)
            .map(|device| device.id)
            .ok_or_else(|| format!("the project has no device `{reference}`"))
    }

    fn device_of_symbol(&self, symbol: SymbolId) -> Result<DeviceId, String> {
        self.doc
            .symbols
            .iter()
            .find(|candidate| candidate.id == symbol)
            .map(|symbol| symbol.device_id)
            .ok_or_else(|| format!("symbol {symbol} does not exist"))
    }

    /// A symbol: a name given with `as`, an ID, or `{ "device": "K1",
    /// "symbol": "coil", "slot": 1 }`.
    fn symbol(&self, value: &Value) -> Result<SymbolId, String> {
        match value {
            Value::String(name) => match self.names.get(name) {
                Some(Created::Symbol(id)) => Ok(*id),
                Some(other) => Err(format!("`{name}` is a {}, not a symbol", other.kind())),
                None => Err(format!("no element is named `{name}`")),
            },
            Value::Number(number) => number
                .as_u64()
                .map(SymbolId)
                .ok_or_else(|| "a symbol ID is a whole number".to_owned()),
            Value::Object(object) => {
                let text = |key: &str| object.get(key).and_then(Value::as_str);
                let (Some(device), Some(key)) = (text("device"), text("symbol")) else {
                    return Err(
                        "a symbol is a name, an ID or { \"device\", \"symbol\", \"slot\" }"
                            .to_owned(),
                    );
                };
                let slot = object.get("slot").and_then(Value::as_u64);
                let device = self.device(device)?;
                let mut found = self.doc.symbols.iter().filter(|symbol| {
                    symbol.device_id == device
                        && symbol.symbol_key.0 == key
                        && slot.is_none_or(|slot| symbol.slot.map(u64::from) == Some(slot))
                });
                match (found.next(), found.next()) {
                    (Some(symbol), None) => Ok(symbol.id),
                    (None, _) => Err(format!(
                        "device `{}` has no such symbol `{key}`",
                        text("device").unwrap_or("")
                    )),
                    (Some(_), Some(_)) => Err(format!(
                        "device `{}` has several `{key}` symbols; name one with `slot`",
                        text("device").unwrap_or("")
                    )),
                }
            }
            _ => Err("a symbol is a name, an ID or { \"device\", \"symbol\" }".to_owned()),
        }
    }

    /// A junction or a wire by name or ID, in `{ "junction": ... }`.
    fn named_or_id(&self, value: &Value, expect: &str) -> Result<u64, String> {
        match value {
            Value::String(name) => match (self.names.get(name), expect) {
                (Some(Created::Junction(id)), "junction") => Ok(id.0),
                (Some(Created::Wire(id)), "wire") => Ok(id.0),
                (Some(Created::Text(id)), "text") => Ok(id.0),
                (Some(other), _) => Err(format!("`{name}` is a {}, not a {expect}", other.kind())),
                (None, _) => Err(format!("no element is named `{name}`")),
            },
            Value::Number(number) => number
                .as_u64()
                .ok_or_else(|| format!("a {expect} ID is a whole number")),
            _ => Err(format!("a {expect} is a name or an ID")),
        }
    }

    /// Where a wire starts or ends: `{ "symbol": ..., "terminal": "a1" }`,
    /// `{ "junction": ... }` or `{ "wire": ..., "at": [x, y] }`.
    fn anchor(&self, value: &Value) -> Result<Anchor, String> {
        let object = value
            .as_object()
            .ok_or("an end of a wire is an object with `symbol` and `terminal`, `junction`, or `wire` and `at`")?;
        if let Some(junction) = object.get("junction") {
            let id = self.named_or_id(junction, "junction")?;
            return Ok(Anchor::Endpoint(Endpoint::Junction(JunctionId(id))));
        }
        if let Some(wire) = object.get("wire") {
            let id = self.named_or_id(wire, "wire")?;
            let at = object
                .get("at")
                .and_then(|at| serde_json::from_value::<[i32; 2]>(at.clone()).ok())
                .ok_or("a point on a wire needs `at`: [x, y]")?;
            return Ok(Anchor::OnWire {
                wire: WireId(id),
                at: point(at),
            });
        }
        let (Some(symbol), Some(terminal)) = (
            object.get("symbol"),
            object.get("terminal").and_then(Value::as_str),
        ) else {
            return Err(
                "an end of a wire is `symbol` and `terminal`, `junction`, or `wire` and `at`"
                    .to_owned(),
            );
        };
        Ok(Anchor::Endpoint(Endpoint::Terminal(PinRef {
            symbol_id: self.symbol(symbol)?,
            terminal: TerminalKey(terminal.to_owned()),
        })))
    }

    /// Elements for `move` and `delete`: a name, a symbol, `{ "junction": ...
    /// }`, `{ "wire": ... }` or `{ "device": "K1" }` alone for a whole device.
    fn subjects(&self, values: &[Value]) -> Result<std::collections::BTreeSet<Subject>, String> {
        let mut subjects = std::collections::BTreeSet::new();
        for value in values {
            let subject = match value {
                Value::String(name) => match self.names.get(name) {
                    Some(Created::Symbol(id)) => Subject::Symbol(*id),
                    Some(Created::Junction(id)) => Subject::Junction(*id),
                    Some(Created::Wire(id)) => Subject::Wire(*id),
                    Some(Created::Text(id)) => Subject::Text(*id),
                    Some(Created::Layer(_)) => {
                        return Err(format!("`{name}` is a layer, which cannot be selected"));
                    }
                    Some(Created::Supply(_)) => {
                        return Err(format!("`{name}` is a supply, which cannot be selected"));
                    }
                    None => return Err(format!("no element is named `{name}`")),
                },
                Value::Object(object) if object.len() == 1 && object.contains_key("junction") => {
                    Subject::Junction(JunctionId(self.named_or_id(
                        object.get("junction").unwrap_or(&Value::Null),
                        "junction",
                    )?))
                }
                Value::Object(object) if object.len() == 1 && object.contains_key("wire") => {
                    Subject::Wire(WireId(
                        self.named_or_id(object.get("wire").unwrap_or(&Value::Null), "wire")?,
                    ))
                }
                Value::Object(object) if object.len() == 1 && object.contains_key("text") => {
                    Subject::Text(TextId(
                        self.named_or_id(object.get("text").unwrap_or(&Value::Null), "text")?,
                    ))
                }
                Value::Object(object) if object.len() == 1 && object.contains_key("device") => {
                    let reference = object
                        .get("device")
                        .and_then(Value::as_str)
                        .ok_or("`device` is a designator")?;
                    Subject::Device(self.device(reference)?)
                }
                other => Subject::Symbol(self.symbol(other)?),
            };
            subjects.insert(subject);
        }
        Ok(subjects)
    }

    /// A parameter value: a boolean, an integer, a text, `{ "supply": name or
    /// ID }`, `{ "device": designator }`, or `null` to clear it.
    fn param_value(&self, value: &Value) -> Result<Option<ParamValue>, String> {
        match value {
            Value::Null => Ok(None),
            Value::Bool(flag) => Ok(Some(ParamValue::Bool(*flag))),
            Value::Number(number) => number
                .as_i64()
                .map(|integer| Some(ParamValue::Int(integer)))
                .ok_or_else(|| "a parameter is a whole number, not a decimal".to_owned()),
            Value::String(text) => Ok(Some(ParamValue::Text(text.clone()))),
            Value::Object(object) => {
                let reference = |id: u64| {
                    i64::try_from(id)
                        .map(|raw| Some(ParamValue::Int(raw)))
                        .map_err(|_| "the ID is too large".to_owned())
                };
                if let Some(supply) = object.get("supply") {
                    return match supply {
                        Value::String(name) => match self.names.get(name) {
                            Some(Created::Supply(id)) => reference(id.0),
                            _ => Err(format!("`{name}` is not a supply made by this file")),
                        },
                        Value::Number(number) => number
                            .as_u64()
                            .ok_or_else(|| "a supply ID is a whole number".to_owned())
                            .and_then(reference),
                        _ => Err("`supply` is a name or an ID".to_owned()),
                    };
                }
                if let Some(device) = object.get("device").and_then(Value::as_str) {
                    return reference(self.device(device)?.0);
                }
                Err("an object value is `{ \"supply\": ... }` or `{ \"device\": ... }`".to_owned())
            }
            Value::Array(_) => Err("a parameter is not a list".to_owned()),
        }
    }
}

/// The ID of the first symbol a transaction inserts.
fn first_symbol(tx: &Transaction) -> Option<SymbolId> {
    tx.inserted().into_iter().find_map(|subject| match subject {
        Subject::Symbol(id) => Some(id),
        _ => None,
    })
}

/// A paper size: `"a4"`, `"a3"`, `"a2"` or `{ "custom": { "width": w,
/// "height": h } }` in grid units.
fn sheet_size(value: Option<&Value>) -> Result<SheetSize, String> {
    match value {
        None => Ok(SheetSize::A4),
        Some(Value::String(name)) => match name.as_str() {
            "a4" => Ok(SheetSize::A4),
            "a3" => Ok(SheetSize::A3),
            "a2" => Ok(SheetSize::A2),
            other => Err(format!(
                "unknown paper size `{other}`; use `a4`, `a3`, `a2` or {{ \"custom\": ... }}"
            )),
        },
        Some(object) => serde_json::from_value::<SheetSize>(object.clone())
            .map_err(|error| format!("not a paper size: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(doc: Schematic, operations: &str) -> Result<Applied, EditError> {
        let text = format!(r#"{{ "schema_version": 1, "operations": {operations} }}"#);
        apply(doc, &ComponentCatalog::builtin(), &text)
    }

    fn fresh() -> Schematic {
        Schematic::new("edit")
    }

    fn refused(result: Result<Applied, EditError>) -> (usize, String) {
        match result {
            Err(EditError::Refused {
                operation, message, ..
            }) => (operation, message),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    fn invalid(result: Result<Applied, EditError>) -> String {
        match result {
            Err(EditError::Invalid(message)) => message,
            other => panic!("expected an invalid file, got {other:?}"),
        }
    }

    /// Two pushbuttons whose terminals touch, joined by a named wire, and a
    /// third one placed first.
    const DOCUMENTED: &str = r#"[
      { "op": "place", "model": "pushbutton_no", "symbol": "contact", "at": [0, 20],
        "reference": "S9", "as": "late" },
      { "op": "place", "model": "pushbutton_no", "symbol": "contact", "at": [0, 0],
        "reference": "S7", "as": "first" },
      { "op": "place", "model": "pushbutton_no", "symbol": "contact", "at": [0, 10],
        "reference": "S8", "as": "second" },
      { "op": "connect", "from": { "symbol": "first", "terminal": "p2" },
        "to": { "symbol": "second", "terminal": "p1" }, "as": "link" }
    ]"#;

    fn documented(more: &str) -> Result<Applied, EditError> {
        let operations = format!("{}, {more}]", DOCUMENTED.trim_end_matches(']'));
        run(fresh(), &operations)
    }

    #[test]
    fn the_documentation_operations_set_info_numbers_the_rule_waivers_and_designators() {
        let applied = documented(
            r#"
            { "op": "set_device_info", "device": "S7", "manufacturer": "Acme", "part_no": "3SB3" },
            { "op": "set_device_info", "device": "S7", "part_no": null, "location": "Panel 1" },
            { "op": "set_wire_number", "wire": "link", "number": "L1" },
            { "op": "numbering", "prefix": "W", "start": 100, "step": 5 },
            { "op": "numbering", "per_sheet": true },
            { "op": "waive", "code": "OC-STR-018", "reason": "test jig",
              "subject": { "symbol": "second", "terminal": "p2" } },
            { "op": "waive", "code": "OC-SEM-005", "reason": "later", "subject": { "device": "S8" } },
            { "op": "unwaive", "code": "OC-SEM-005", "subject": { "device": "S8" } },
            { "op": "renumber" }
            "#,
        )
        .expect("the operations apply");

        let doc = applied.doc;
        let s1 = doc
            .devices
            .iter()
            .find(|d| d.reference.0 == "S1")
            .expect("renumbered");
        let info = s1.info.clone().expect("catalog data");
        // The second operation kept the manufacturer, cleared the part number
        // and set a location.
        assert_eq!(info.manufacturer.as_deref(), Some("Acme"));
        assert_eq!(info.part_no, None);
        assert_eq!(info.location.as_deref(), Some("Panel 1"));
        assert_eq!(doc.wires[0].number.as_deref(), Some("L1"));
        let rule = doc.project.wire_numbering.expect("a rule");
        assert_eq!(
            (
                rule.prefix.as_str(),
                rule.start,
                rule.step.get(),
                rule.per_sheet
            ),
            ("W", 100, 5, true)
        );
        assert_eq!(doc.waivers.len(), 1);
        assert_eq!(doc.waivers[0].reason, "test jig");
        let mut references: Vec<&str> =
            doc.devices.iter().map(|d| d.reference.0.as_str()).collect();
        references.sort_unstable();
        assert_eq!(references, ["S1", "S2", "S3"]);
    }

    #[test]
    fn the_numbering_can_be_switched_off_and_a_sheet_can_be_renumbered_alone() {
        let applied = documented(
            r#"
            { "op": "numbering", "prefix": "W" },
            { "op": "numbering", "enabled": false }
            "#,
        )
        .expect("applies");
        assert!(applied.doc.project.wire_numbering.is_none());

        let (operation, message) = refused(documented(r#"{ "op": "renumber", "sheet": 4 }"#));
        assert_eq!(operation, 5);
        assert!(message.contains("no sheet number 4"), "{message}");
    }

    #[test]
    fn what_cannot_be_documented_is_refused_naming_the_operation() {
        for (more, needle) in [
            (
                r#"{ "op": "waive", "code": "OC-STR-013", "reason": "x", "subject": "link" }"#,
                "cannot be waived",
            ),
            (
                r#"{ "op": "waive", "code": "OC-STR-018", "reason": "", "subject": "link" }"#,
                "needs a reason",
            ),
            (
                r#"{ "op": "waive", "code": "OC-NOPE-1", "reason": "x", "subject": "link" }"#,
                "not a diagnostic code",
            ),
            (
                r#"{ "op": "unwaive", "code": "OC-STR-018", "subject": "link" }"#,
                "does not exist",
            ),
            (r#"{ "op": "numbering", "step": 0 }"#, "step"),
            (
                r#"{ "op": "set_wire_number", "wire": "nope", "number": "1" }"#,
                "no element is named",
            ),
            (
                r#"{ "op": "set_device_info", "device": "S99", "part_no": "x" }"#,
                "no device",
            ),
        ] {
            let (operation, message) = refused(documented(more));
            assert_eq!(operation, 5, "{more}");
            assert!(message.contains(needle), "{more}: {message}");
        }
    }

    #[test]
    fn a_device_is_placed_named_renamed_and_its_parameters_set() {
        let applied = run(
            fresh(),
            r#"[
              { "op": "add_supply", "preset": "dc_24v", "as": "ctrl" },
              { "op": "place", "model": "contactor.generic", "symbol": "coil",
                "at": [0, 10], "reference": "K7", "as": "coil" },
              { "op": "set_param", "device": "K7", "key": "rated_voltage_mv", "value": 24000 },
              { "op": "set_param", "device": "K7", "key": "connection", "value": "dc" },
              { "op": "place", "model": "supply.terminal", "symbol": "terminal",
                "at": [0, 0], "reference": "P24" },
              { "op": "set_param", "device": "P24", "key": "supply", "value": { "supply": "ctrl" } },
              { "op": "set_param", "device": "P24", "key": "conductor", "value": "dc_pos" }
            ]"#,
        )
        .unwrap();

        assert_eq!(applied.operations, 7);
        let references: Vec<&str> = applied
            .doc
            .devices
            .iter()
            .map(|device| device.reference.0.as_str())
            .collect();
        assert_eq!(references, ["K7", "P24"]);
        let k7 = &applied.doc.devices[0];
        assert_eq!(
            k7.params.get("rated_voltage_mv"),
            Some(&ParamValue::Int(24_000))
        );
        let supply = applied.doc.supplies[0].id.0;
        assert_eq!(applied.created["ctrl"], Created::Supply(SupplyId(supply)));
        assert!(matches!(applied.created["coil"], Created::Symbol(_)));
        let p24 = &applied.doc.devices[1];
        assert_eq!(
            p24.params.get("supply"),
            Some(&ParamValue::Int(i64::try_from(supply).unwrap()))
        );
    }

    #[test]
    fn wires_junctions_moves_rotations_and_deletes_use_the_names() {
        let applied = run(
            fresh(),
            r#"[
              { "op": "place", "model": "pushbutton_no", "symbol": "contact", "at": [0, 0], "as": "a" },
              { "op": "place", "model": "pushbutton_no", "symbol": "contact", "at": [0, 8], "as": "b" },
              { "op": "junction", "at": [4, 4], "as": "j" },
              { "op": "connect", "from": { "symbol": "a", "terminal": "p2" },
                "to": { "junction": "j" }, "via": [[0, 4]], "as": "w" },
              { "op": "connect", "from": { "junction": "j" },
                "to": { "symbol": "b", "terminal": "p1" } },
              { "op": "move", "elements": ["b"], "by": [2, 0] },
              { "op": "rotate", "symbols": ["a"] }
            ]"#,
        )
        .unwrap();

        assert_eq!(applied.doc.wires.len(), 2);
        assert_eq!(applied.doc.junctions.len(), 1);
        let b = match applied.created["b"] {
            Created::Symbol(id) => id,
            other => panic!("{other:?}"),
        };
        let b = applied.doc.symbols.iter().find(|s| s.id == b).unwrap();
        assert_eq!(b.position, GridPoint::new(2, 8));
        assert!(matches!(applied.created["w"], Created::Wire(_)));

        let after = run(
            applied.doc,
            r#"[ { "op": "delete", "elements": [ { "device": "S1" } ] } ]"#,
        )
        .unwrap();
        assert_eq!(after.doc.symbols.len(), 1);
        assert_eq!(after.doc.wires.len(), 1);
    }

    #[test]
    fn symbols_of_existing_devices_are_found_by_device_symbol_and_slot() {
        let applied = run(
            fresh(),
            r#"[
              { "op": "place", "model": "contactor.generic", "symbol": "coil", "at": [0, 0] },
              { "op": "add_symbol", "device": "K1", "symbol": "aux_no", "at": [6, 0] },
              { "op": "add_symbol", "device": "K1", "symbol": "aux_no", "at": [10, 0] },
              { "op": "place", "model": "junction.none", "symbol": "x", "at": [0, 0] }
            ]"#,
        );
        // The last operation names no model: it is refused, by number.
        assert_eq!(refused(applied).0, 4);

        let doc = run(
            fresh(),
            r#"[
              { "op": "place", "model": "contactor.generic", "symbol": "coil", "at": [0, 0] },
              { "op": "add_symbol", "device": "K1", "symbol": "aux_no", "at": [6, 0] },
              { "op": "add_symbol", "device": "K1", "symbol": "aux_no", "at": [10, 0] },
              { "op": "place", "model": "lamp.generic", "symbol": "lamp", "at": [0, 8], "as": "h" }
            ]"#,
        )
        .unwrap()
        .doc;
        let wire = |terminal: &str, slot: &str| {
            format!(
                r#"[ {{ "op": "connect",
                      "from": {{ "symbol": {{ "device": "K1", "symbol": "aux_no"{slot} }}, "terminal": "{terminal}" }},
                      "to": {{ "symbol": {{ "device": "K1", "symbol": "coil" }}, "terminal": "a1" }} }} ]"#
            )
        };
        // Two aux_no symbols and no slot: ambiguous.
        let (_, message) = refused(run(doc.clone(), &wire("p1", "")));
        assert!(message.contains("several"), "{message}");
        assert!(run(doc, &wire("p1", r#", "slot": 2"#)).is_ok());
    }

    #[test]
    fn replace_changes_the_chosen_kinds_and_refuses_a_repeated_designator() {
        let doc = run(
            fresh(),
            r#"[
              { "op": "place", "model": "lamp.generic", "symbol": "lamp", "at": [0, 0] },
              { "op": "place", "model": "lamp.generic", "symbol": "lamp", "at": [10, 0] },
              { "op": "add_text", "at": [0, 10], "text": "H1 shows run" },
              { "op": "replace", "find": "H", "with": "P", "notes": false }
            ]"#,
        )
        .unwrap()
        .doc;

        let mut names: Vec<&str> = doc.devices.iter().map(|d| d.reference.0.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, ["P1", "P2"]);
        assert_eq!(doc.texts[0].text, "H1 shows run");

        let (operation, message) = refused(run(
            doc,
            r#"[ { "op": "replace", "find": "2", "with": "1" } ]"#,
        ));
        assert_eq!(operation, 1);
        assert!(message.contains("P1"), "{message}");
    }

    #[test]
    fn the_one_transaction_of_a_file_gives_the_same_project_and_undoes_in_one_step() {
        let start = fresh();
        let applied = run(start.clone(), DOCUMENTED).unwrap();

        let mut doc = start.clone();
        let mut history = crate::History::new();
        history
            .execute(&mut doc, applied.transaction.clone())
            .unwrap();
        assert_eq!(doc, applied.doc);
        history.undo(&mut doc).unwrap();
        assert_eq!(doc.devices, start.devices);
        assert_eq!(doc.wires, start.wires);
    }

    #[test]
    fn route_gives_a_named_wire_a_way_around_a_symbol() {
        let doc = run(
            fresh(),
            r#"[
              { "op": "junction", "at": [0, 0], "as": "left" },
              { "op": "junction", "at": [20, 0], "as": "right" },
              { "op": "place", "model": "lamp.generic", "symbol": "lamp", "at": [10, 0], "rotation": 90 },
              { "op": "connect", "from": { "junction": "left" }, "to": { "junction": "right" },
                "via": [], "as": "line" },
              { "op": "route", "wires": ["line"] }
            ]"#,
        )
        .unwrap()
        .doc;

        assert!(!doc.wires[0].route.is_empty());
        let (_, message) = refused(run(fresh(), r#"[ { "op": "route", "wires": [99] } ]"#));
        assert!(message.contains("does not exist"), "{message}");
    }

    #[test]
    fn renumber_sheets_numbers_the_frames_in_reading_order() {
        let doc = run(
            fresh(),
            r#"[
              { "op": "add_sheet", "number": 9, "origin": [400, 0] },
              { "op": "add_sheet", "number": 4, "origin": [0, 0] },
              { "op": "renumber_sheets" }
            ]"#,
        )
        .unwrap()
        .doc;

        let mut numbers: Vec<(i32, u32)> =
            doc.sheets.iter().map(|s| (s.origin.x, s.number)).collect();
        numbers.sort_unstable();
        assert_eq!(numbers, [(0, 1), (400, 2)]);
        let invalid = run(fresh(), r#"[ { "op": "renumber_sheets", "x": 1 } ]"#);
        assert!(
            matches!(&invalid, Err(EditError::Invalid(message)) if message.contains("unknown field")),
            "{invalid:?}"
        );
    }

    #[test]
    fn sheets_and_project_fields_are_kept_cleared_or_set() {
        let applied = run(
            fresh(),
            r#"[
              { "op": "project", "name": "Pump", "author": "Ana", "revision": "B" },
              { "op": "add_sheet", "title": "One", "origin": [0, 0] },
              { "op": "add_sheet", "title": "Two", "origin": [100, 0], "size": "a3",
                "orientation": "portrait", "zones": [4, 3] },
              { "op": "add_sheet", "number": 9, "origin": [400, 0],
                "size": { "custom": { "width": 40, "height": 30 } } }
            ]"#,
        )
        .unwrap();
        let doc = applied.doc;
        assert_eq!(
            (doc.project.name.as_str(), doc.project.author.as_deref()),
            ("Pump", Some("Ana"))
        );
        assert_eq!(
            doc.sheets.iter().map(|s| s.number).collect::<Vec<_>>(),
            [1, 2, 9]
        );
        assert_eq!(doc.sheets[1].zones.columns, 4);
        assert!(doc.validate().is_empty());

        let doc = run(
            doc,
            r#"[
              { "op": "project", "author": null, "company": "Acme" },
              { "op": "remove_sheet", "number": 2 }
            ]"#,
        )
        .unwrap()
        .doc;
        assert_eq!(doc.project.author, None);
        assert_eq!(doc.project.company.as_deref(), Some("Acme"));
        assert_eq!(doc.project.revision.as_deref(), Some("B"));
        assert_eq!(doc.sheets.len(), 2);
    }

    #[test]
    fn a_refusal_names_the_operation_and_the_reason() {
        for (operations, number, wanted) in [
            (
                r#"[ { "op": "set_param", "device": "K9", "key": "x", "value": 1 } ]"#,
                1,
                "no device `K9`",
            ),
            (
                r#"[ { "op": "junction", "at": [0,0], "as": "j" }, { "op": "junction", "at": [1,1], "as": "j" } ]"#,
                2,
                "already used",
            ),
            (
                r#"[ { "op": "move", "elements": ["nobody"], "by": [1, 0] } ]"#,
                1,
                "no element is named",
            ),
            (
                r#"[ { "op": "junction", "at": [0,0], "as": "j" }, { "op": "rotate", "symbols": ["j"] } ]"#,
                2,
                "is a junction, not a symbol",
            ),
            (
                r#"[ { "op": "place", "model": "contactor.generic", "symbol": "coil", "at": [0,0], "rotation": 45 } ]"#,
                1,
                "0, 90, 180 or 270",
            ),
            (
                r#"[ { "op": "place", "model": "nothing.here", "symbol": "x", "at": [0,0] } ]"#,
                1,
                "no model",
            ),
            (
                r#"[ { "op": "add_supply", "preset": "dc_5v" } ]"#,
                1,
                "unknown preset",
            ),
            (
                r#"[ { "op": "remove_sheet", "number": 3 } ]"#,
                1,
                "no sheet number 3",
            ),
            (
                r#"[ { "op": "add_sheet", "origin": [0,0] }, { "op": "add_sheet", "origin": [10,10] } ]"#,
                2,
                "invalid sheet",
            ),
            (
                r#"[ { "op": "set_param", "device": "K1", "key": "k", "value": [1] } ]"#,
                1,
                "no device",
            ),
        ] {
            let (found, message) = refused(run(fresh(), operations));
            assert_eq!(found, number, "{operations}: {message}");
            assert!(message.contains(wanted), "{operations}: {message}");
        }
    }

    #[test]
    fn a_file_that_is_not_an_edit_file_is_refused_whole() {
        let ok = r#"{ "schema_version": 1, "operations": [] }"#;
        assert_eq!(
            apply(fresh(), &ComponentCatalog::builtin(), ok)
                .unwrap()
                .operations,
            0
        );

        for (text, wanted) in [
            ("not json", "not a valid edit file"),
            (
                r#"{ "schema_version": 2, "operations": [] }"#,
                "schema version 2",
            ),
            (r#"{ "schema_version": 1 }"#, "operations"),
            (
                r#"{ "schema_version": 1, "operations": [], "extra": 1 }"#,
                "extra",
            ),
            (
                r#"{ "schema_version": 1, "operations": [ { "op": "format_disk" } ] }"#,
                "operation 1",
            ),
            (
                r#"{ "schema_version": 1, "operations": [ { "op": "junction", "at": [0, 0], "colour": 1 } ] }"#,
                "colour",
            ),
            (
                r#"{ "schema_version": 1, "operations": [ { "op": "junction", "at": [0] } ] }"#,
                "operation 1",
            ),
            (
                r#"{ "schema_version": 1, "operations": [ { "op": "junction", "at": [0, 0], "as": "has space" } ] }"#,
                "must be 1 to 64",
            ),
            (
                r#"{ "schema_version": 1, "operations": [ { "op": "junction", "at": [0, 0], "as": "" } ] }"#,
                "must be 1 to 64",
            ),
        ] {
            let message = invalid(apply(fresh(), &ComponentCatalog::builtin(), text));
            assert!(message.contains(wanted), "{text}: {message}");
        }
    }

    #[test]
    fn the_size_of_the_file_and_the_number_of_operations_are_limited() {
        let big = format!(
            r#"{{ "schema_version": 1, "operations": [], "pad": "{}" }}"#,
            "x".repeat(MAX_EDIT_BYTES)
        );
        assert!(invalid(apply(fresh(), &ComponentCatalog::builtin(), &big)).contains("larger"));

        let many = format!(
            r#"{{ "schema_version": 1, "operations": [{}] }}"#,
            vec![r#"{ "op": "junction", "at": [0, 0] }"#; MAX_OPERATIONS + 1].join(",")
        );
        assert!(invalid(apply(fresh(), &ComponentCatalog::builtin(), &many)).contains("more than"));
    }

    #[test]
    fn what_is_refused_halfway_leaves_the_project_to_the_caller_untouched() {
        let before = fresh();
        let copy = before.clone();

        let result = run(
            copy,
            r#"[ { "op": "junction", "at": [0, 0] }, { "op": "remove_sheet", "number": 1 } ]"#,
        );

        // The caller still has `before`: apply took its own copy.
        assert_eq!(refused(result).0, 2);
        assert!(before.junctions.is_empty());
    }

    mod hostile {
        use proptest::prelude::*;

        use super::*;

        fn operation() -> impl Strategy<Value = String> {
            let text = "[a-zA-Z0-9_.]{0,6}";
            let number = -1_000_i64..1_000;
            prop_oneof![
                (text, number.clone(), number.clone())
                    .prop_map(|(n, x, y)| format!(r#"{{ "op": "junction", "at": [{x}, {y}], "as": "{n}" }}"#)),
                (text, number.clone(), number.clone()).prop_map(|(n, x, y)| format!(
                    r#"{{ "op": "place", "model": "lamp.generic", "symbol": "lamp", "at": [{x}, {y}], "as": "{n}" }}"#
                )),
                (text, text).prop_map(|(a, b)| format!(
                    r#"{{ "op": "connect", "from": {{ "junction": "{a}" }}, "to": {{ "junction": "{b}" }} }}"#
                )),
                (text, number.clone()).prop_map(|(a, d)| format!(
                    r#"{{ "op": "move", "elements": ["{a}"], "by": [{d}, {d}] }}"#
                )),
                (text,).prop_map(|(a,)| format!(r#"{{ "op": "delete", "elements": [ "{a}", {{ "device": "{a}" }} ] }}"#)),
                (number.clone(), number).prop_map(|(x, y)| format!(
                    r#"{{ "op": "add_sheet", "origin": [{x}, {y}], "zones": [{y}, {x}] }}"#
                )),
                text.prop_map(|n| format!(r#"{{ "op": "remove_sheet", "number": {} }}"#, n.len())),
            ]
        }

        proptest! {
            /// Any sequence of operations, valid or not, ends in a result:
            /// never a panic, and a project that still validates when it is Ok.
            #[test]
            fn any_sequence_of_operations_gives_a_result(operations in prop::collection::vec(operation(), 0..12)) {
                let text = format!(
                    r#"{{ "schema_version": 1, "operations": [{}] }}"#,
                    operations.join(",")
                );
                if let Ok(applied) = apply(fresh(), &ComponentCatalog::builtin(), &text) {
                    let integrity = applied.doc.integrity_errors();
                    prop_assert!(integrity.is_empty(), "{integrity:?}");
                }
            }

            /// Any text at all is refused or applied, never a panic.
            #[test]
            fn any_text_is_a_result(text in any::<String>()) {
                let _ = apply(fresh(), &ComponentCatalog::builtin(), &text);
            }
        }
    }
}
