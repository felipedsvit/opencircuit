//! Macros (M9.2, issue #118): the edits of a session, recorded as an edit
//! file ([`crate::script`]) that the window, `opencircuit edit --apply` and
//! the MCP server play back.
//!
//! Each transaction the window commits is translated into operations of the
//! edit file, and the operations are run on the project as it was before
//! the transaction: they are kept only when they give exactly the project
//! the transaction gave. A change that cannot be written that way (the
//! route of a wire dragged by hand, a layer, a supply) is counted as left
//! out instead of being recorded wrong. Elements the macro creates are
//! named after their IDs (`s12`, `j13`, `w14`, `t15`), so that later
//! operations find them when the macro runs on another project; the others
//! are named by their IDs, and devices by their designators.

use std::collections::{BTreeMap, BTreeSet};

use circuit_model::{
    ComponentCatalog, Device, DeviceId, Endpoint, GridPoint, JunctionId, ParamType, ParamValue,
    QuarterTurn, Schematic, SymbolId, TextAnchor, TextId, WireId,
};
use serde_json::{Value, json};

use crate::script::{Created, SCHEMA_VERSION, run_values};
use crate::{Edit, Transaction, ops};

/// Most transactions one macro records.
pub const MAX_STEPS: usize = 1000;

/// The macro being recorded.
#[derive(Clone, Debug, Default)]
pub struct Recorder {
    /// The operations of each recorded transaction, oldest first.
    steps: Vec<Vec<Value>>,
    /// Steps taken back by an undo, newest last, for a redo.
    undone: Vec<Vec<Value>>,
    /// Whether each transaction of the history was recorded, in the same
    /// order, so that an undo takes back the right step.
    kept: Vec<bool>,
    /// Kept flags of the undone transactions.
    kept_undone: Vec<bool>,
    /// The names of the elements the macro created.
    names: BTreeMap<String, Created>,
    /// Transactions that could not be written as operations.
    left_out: usize,
}

impl Recorder {
    /// A recorder with nothing recorded.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How many transactions are recorded.
    #[must_use]
    pub fn steps(&self) -> usize {
        self.steps.len()
    }

    /// How many transactions could not be recorded.
    #[must_use]
    pub const fn left_out(&self) -> usize {
        self.left_out
    }

    /// Record `tx`, committed on `before`. Returns whether it was recorded;
    /// a transaction that changes nothing is neither recorded nor left out.
    pub fn record(
        &mut self,
        before: &Schematic,
        catalog: &ComponentCatalog,
        tx: &Transaction,
    ) -> bool {
        self.undone.clear();
        self.kept_undone.clear();
        if tx.edits.is_empty() {
            self.kept.push(false);
            return false;
        }
        let recorded = self.steps.len() < MAX_STEPS
            && tx
                .apply_to(before)
                .ok()
                .and_then(|(after, _)| {
                    let operations = translate(before, catalog, tx, &self.names)?;
                    let (ran, names) =
                        run_values(before, catalog, &self.names, &operations).ok()?;
                    (ran == after).then_some((operations, names))
                })
                .map(|(operations, names)| {
                    self.steps.push(operations);
                    self.names = names;
                })
                .is_some();
        if !recorded {
            self.left_out = self.left_out.saturating_add(1);
        }
        self.kept.push(recorded);
        recorded
    }

    /// The window took back its last transaction.
    pub fn undo(&mut self) {
        // No `let` chain: the core builds on Rust 1.85.
        if let Some(kept) = self.kept.pop() {
            if kept {
                if let Some(step) = self.steps.pop() {
                    self.undone.push(step);
                }
            }
            self.kept_undone.push(kept);
        }
    }

    /// The window did again the transaction it took back last.
    pub fn redo(&mut self) {
        if let Some(kept) = self.kept_undone.pop() {
            if kept {
                if let Some(step) = self.undone.pop() {
                    self.steps.push(step);
                }
            }
            self.kept.push(kept);
        }
    }

    /// The edit file of the recorded steps.
    #[must_use]
    pub fn to_edit_file(&self) -> String {
        let operations: Vec<&Value> = self.steps.iter().flatten().collect();
        let file = json!({
            "schema_version": SCHEMA_VERSION,
            "operations": operations,
        });
        serde_json::to_string_pretty(&file).unwrap_or_default() + "\n"
    }
}

fn point(at: GridPoint) -> Value {
    json!([at.x, at.y])
}

const fn degrees(rotation: QuarterTurn) -> u16 {
    match rotation {
        QuarterTurn::Deg0 => 0,
        QuarterTurn::Deg90 => 90,
        QuarterTurn::Deg180 => 180,
        QuarterTurn::Deg270 => 270,
    }
}

/// How the operations name the elements: the macro's name of an element it
/// created, or the ID.
struct Names<'a> {
    names: &'a BTreeMap<String, Created>,
}

impl Names<'_> {
    fn named(&self, name: String, created: Created) -> Option<Value> {
        (self.names.get(&name) == Some(&created)).then_some(Value::String(name))
    }

    fn symbol(&self, id: SymbolId) -> Value {
        self.named(format!("s{}", id.0), Created::Symbol(id))
            .unwrap_or_else(|| json!(id.0))
    }

    fn junction(&self, id: JunctionId) -> Value {
        self.named(format!("j{}", id.0), Created::Junction(id))
            .unwrap_or_else(|| json!(id.0))
    }

    fn wire(&self, id: WireId) -> Value {
        self.named(format!("w{}", id.0), Created::Wire(id))
            .unwrap_or_else(|| json!(id.0))
    }

    fn text(&self, id: TextId) -> Value {
        self.named(format!("t{}", id.0), Created::Text(id))
            .unwrap_or_else(|| json!(id.0))
    }
}

/// The operations of `tx`, best effort; `None` for an edit no operation
/// makes. Whether they are right is checked by running them.
fn translate(
    before: &Schematic,
    catalog: &ComponentCatalog,
    tx: &Transaction,
    names: &BTreeMap<String, Created>,
) -> Option<Vec<Value>> {
    let names = Names { names };
    let created: BTreeMap<DeviceId, &Device> = tx
        .edits
        .iter()
        .filter_map(|edit| match edit {
            Edit::InsertDevice(device) => Some((device.id, device)),
            _ => None,
        })
        .collect();
    // The designator a device has before the transaction, or gets in it.
    let designator = |id: DeviceId| -> Option<String> {
        before
            .devices
            .iter()
            .find(|device| device.id == id)
            .or_else(|| created.get(&id).copied())
            .map(|device| device.reference.0.clone())
    };
    let param = |device: &Device, key: &str, value: &ParamValue| -> Option<Value> {
        let ty = catalog
            .model(&device.model)
            .and_then(|model| model.params().iter().find(|d| d.key == key))
            .map(|definition| &definition.ty);
        Some(match (ty, value) {
            (Some(ParamType::SupplyRef), ParamValue::Int(id)) => json!({ "supply": id }),
            (Some(ParamType::DeviceRef), ParamValue::Int(id)) => {
                json!({ "device": designator(DeviceId(u64::try_from(*id).ok()?))? })
            }
            (_, ParamValue::Bool(flag)) => json!(flag),
            (_, ParamValue::Int(number)) => json!(number),
            (_, ParamValue::Text(text)) => json!(text),
        })
    };
    let end = |endpoint: &Endpoint| -> Value {
        match endpoint {
            Endpoint::Terminal(pin) => {
                json!({ "symbol": names.symbol(pin.symbol_id), "terminal": pin.terminal.0 })
            }
            Endpoint::Junction(id) => json!({ "junction": names.junction(*id) }),
        }
    };

    let mut operations = Vec::new();
    let mut removed: Vec<Value> = Vec::new();
    let mut removed_at = None;
    let removed_symbol_devices: BTreeSet<DeviceId> = tx
        .edits
        .iter()
        .filter_map(|edit| match edit {
            Edit::RemoveSymbol(id) => before
                .symbols
                .iter()
                .find(|symbol| symbol.id == *id)
                .map(|symbol| symbol.device_id),
            _ => None,
        })
        .collect();
    let mut placed: Vec<(SymbolId, GridPoint, QuarterTurn)> = Vec::new();
    let mut moved: Vec<(Value, GridPoint, GridPoint)> = Vec::new();
    for edit in &tx.edits {
        let mut remove = |value: Value, operations: &Vec<Value>| {
            removed_at.get_or_insert(operations.len());
            removed.push(value);
        };
        match edit {
            Edit::InsertDevice(_) | Edit::SetRoute { .. } => {}
            Edit::InsertSymbol(symbol) => {
                if let Some(device) = created.get(&symbol.device_id) {
                    operations.push(json!({
                        "op": "place",
                        "model": device.model.id,
                        "version": device.model.version,
                        "symbol": symbol.symbol_key.0,
                        "at": point(symbol.position),
                        "rotation": degrees(symbol.rotation),
                        "reference": device.reference.0,
                        "as": format!("s{}", symbol.id.0),
                    }));
                    for (key, value) in &device.params {
                        operations.push(json!({
                            "op": "set_param",
                            "device": device.reference.0,
                            "key": key,
                            "value": param(device, key, value)?,
                        }));
                    }
                    if let Some(info) = &device.info {
                        operations.push(json!({
                            "op": "set_device_info",
                            "device": device.reference.0,
                            "manufacturer": info.manufacturer,
                            "part_no": info.part_no,
                            "description": info.description,
                            "location": info.location,
                        }));
                    }
                } else {
                    operations.push(json!({
                        "op": "add_symbol",
                        "device": designator(symbol.device_id)?,
                        "symbol": symbol.symbol_key.0,
                        "at": point(symbol.position),
                        "rotation": degrees(symbol.rotation),
                        "as": format!("s{}", symbol.id.0),
                    }));
                }
            }
            Edit::InsertJunction(junction) => operations.push(json!({
                "op": "junction",
                "at": point(junction.position),
                "as": format!("j{}", junction.id.0),
            })),
            Edit::InsertWire(wire) => {
                operations.push(json!({
                    "op": "connect",
                    "from": end(&wire.from),
                    "to": end(&wire.to),
                    "via": wire.route.iter().map(|at| [at.x, at.y]).collect::<Vec<_>>(),
                    "as": format!("w{}", wire.id.0),
                }));
                if let Some(number) = &wire.number {
                    operations.push(json!({
                        "op": "set_wire_number",
                        "wire": format!("w{}", wire.id.0),
                        "number": number,
                    }));
                }
            }
            Edit::InsertText(note) => operations.push(json!({
                "op": "add_text",
                "at": point(note.position),
                "text": note.text,
                "anchor": match note.anchor {
                    TextAnchor::Left => "left",
                    TextAnchor::Center => "center",
                },
                "as": format!("t{}", note.id.0),
            })),
            Edit::RemoveWire(id) => remove(json!({ "wire": names.wire(*id) }), &operations),
            Edit::RemoveSymbol(id) => remove(names.symbol(*id), &operations),
            Edit::RemoveJunction(id) => {
                remove(json!({ "junction": names.junction(*id) }), &operations);
            }
            Edit::RemoveText(id) => remove(json!({ "text": names.text(*id) }), &operations),
            Edit::RemoveDevice(id) => {
                if !removed_symbol_devices.contains(id) {
                    remove(json!({ "device": designator(*id)? }), &operations);
                }
            }
            Edit::SetReference { device, reference } => operations.push(json!({
                "op": "set_reference",
                "device": designator(*device)?,
                "to": reference.0,
            })),
            Edit::SetParam { device, key, value } => {
                let owner = before.devices.iter().find(|d| d.id == *device)?;
                let value = match value {
                    Some(value) => param(owner, key, value)?,
                    None => Value::Null,
                };
                operations.push(json!({
                    "op": "set_param",
                    "device": owner.reference.0,
                    "key": key,
                    "value": value,
                }));
            }
            Edit::SetDeviceInfo { device, info } => {
                let info = info.clone().unwrap_or_default();
                operations.push(json!({
                    "op": "set_device_info",
                    "device": designator(*device)?,
                    "manufacturer": info.manufacturer,
                    "part_no": info.part_no,
                    "description": info.description,
                    "location": info.location,
                }));
            }
            Edit::SetWireNumber { wire, number } => operations.push(json!({
                "op": "set_wire_number",
                "wire": names.wire(*wire),
                "number": number,
            })),
            Edit::SetText { text, content } => operations.push(json!({
                "op": "set_text",
                "text": names.text(*text),
                "to": content,
            })),
            Edit::Place {
                symbol,
                position,
                rotation,
            } => placed.push((*symbol, *position, *rotation)),
            Edit::MoveJunction { junction, position } => {
                let from = before
                    .junctions
                    .iter()
                    .find(|j| j.id == *junction)?
                    .position;
                moved.push((
                    json!({ "junction": names.junction(*junction) }),
                    from,
                    *position,
                ));
            }
            Edit::MoveText { text, position } => {
                let from = before.texts.iter().find(|t| t.id == *text)?.position;
                moved.push((json!({ "text": names.text(*text) }), from, *position));
            }
            _ => return None,
        }
    }
    if let Some(at) = removed_at {
        let at = at.min(operations.len());
        operations.insert(at, json!({ "op": "delete", "elements": removed }));
    }
    if !placed.is_empty() || !moved.is_empty() {
        operations.extend(arrange(before, catalog, &names, &placed, &moved)?);
    }
    Some(operations)
}

/// The turns and moves that put symbols, junctions and notes where a
/// transaction put them: the symbols that turned are turned together, then
/// everything is moved, one operation per distance.
fn arrange(
    before: &Schematic,
    catalog: &ComponentCatalog,
    names: &Names<'_>,
    placed: &[(SymbolId, GridPoint, QuarterTurn)],
    moved: &[(Value, GridPoint, GridPoint)],
) -> Option<Vec<Value>> {
    let mut operations = Vec::new();
    // Quarter turns clockwise from one rotation to another, 0 to 3.
    let turns = |from: QuarterTurn, to: QuarterTurn| -> u16 {
        let index = |rotation| degrees(rotation).checked_div(90).unwrap_or(0);
        index(to)
            .wrapping_add(4)
            .wrapping_sub(index(from))
            .checked_rem(4)
            .unwrap_or(0)
    };
    let mut turned: BTreeSet<SymbolId> = BTreeSet::new();
    let mut quarter = None;
    for (id, _, rotation) in placed {
        let old = before.symbols.iter().find(|s| s.id == *id)?;
        let count = turns(old.rotation, *rotation);
        if count > 0 {
            if quarter.is_some_and(|q| q != count) {
                return None;
            }
            quarter = Some(count);
            turned.insert(*id);
        }
    }
    let mut after_turns = before.clone();
    if let Some(count) = quarter {
        let symbols: Vec<Value> = turned.iter().map(|id| names.symbol(*id)).collect();
        let selection: BTreeSet<SymbolId> = turned.clone();
        for _ in 0..count {
            operations.push(json!({ "op": "rotate", "symbols": symbols }));
            let tx = ops::rotate(&after_turns, catalog, &selection).ok()?;
            after_turns = tx.apply_to(&after_turns).ok()?.0;
        }
    }
    let mut by_distance: BTreeMap<(i32, i32), Vec<Value>> = BTreeMap::new();
    for (id, position, _) in placed {
        let now = after_turns.symbols.iter().find(|s| s.id == *id)?.position;
        let delta = (
            position.x.checked_sub(now.x)?,
            position.y.checked_sub(now.y)?,
        );
        if delta != (0, 0) {
            by_distance
                .entry(delta)
                .or_default()
                .push(names.symbol(*id));
        }
    }
    for (element, from, to) in moved {
        let delta = (to.x.checked_sub(from.x)?, to.y.checked_sub(from.y)?);
        if delta != (0, 0) {
            by_distance.entry(delta).or_default().push(element.clone());
        }
    }
    for ((dx, dy), elements) in by_distance {
        operations.push(json!({ "op": "move", "elements": elements, "by": [dx, dy] }));
    }
    Some(operations)
}

#[cfg(test)]
mod tests {
    use circuit_model::{ModelRef, PinRef, SymbolKey, TerminalKey};

    use super::*;
    use crate::{Anchor, History, script};

    /// A session: commit `tx`, recording it.
    struct Session {
        doc: Schematic,
        history: History,
        recorder: Recorder,
        catalog: ComponentCatalog,
    }

    impl Session {
        fn new(doc: Schematic) -> Self {
            Self {
                doc,
                history: History::new(),
                recorder: Recorder::new(),
                catalog: ComponentCatalog::builtin(),
            }
        }

        fn commit(
            &mut self,
            op: impl FnOnce(&Schematic, &ComponentCatalog) -> Transaction,
        ) -> bool {
            let tx = op(&self.doc, &self.catalog);
            let before = self.doc.clone();
            self.history.execute(&mut self.doc, tx.clone()).unwrap();
            self.recorder.record(&before, &self.catalog, &tx)
        }

        fn symbol_of(&self, reference: &str) -> SymbolId {
            let device = self
                .doc
                .devices
                .iter()
                .find(|d| d.reference.0 == reference)
                .unwrap()
                .id;
            self.doc
                .symbols
                .iter()
                .find(|s| s.device_id == device)
                .unwrap()
                .id
        }
    }

    fn lamp(at: GridPoint) -> impl FnOnce(&Schematic, &ComponentCatalog) -> Transaction {
        move |doc, catalog| {
            ops::place(
                doc,
                catalog,
                &ModelRef {
                    id: "lamp.generic".to_owned(),
                    version: 1,
                },
                &SymbolKey("lamp".to_owned()),
                at,
                QuarterTurn::Deg0,
            )
            .unwrap()
        }
    }

    /// Two lamps wired, one moved, turned and renamed, a note, and a lamp
    /// placed then deleted.
    fn session(start: Schematic) -> Session {
        let mut s = Session::new(start);
        assert!(s.commit(lamp(GridPoint::new(0, 0))));
        assert!(s.commit(lamp(GridPoint::new(20, 0))));
        let (h1, h2) = (s.symbol_of("H1"), s.symbol_of("H2"));
        let pin = |symbol, key: &str| {
            Anchor::Endpoint(Endpoint::Terminal(PinRef {
                symbol_id: symbol,
                terminal: TerminalKey(key.to_owned()),
            }))
        };
        assert!(s.commit(|doc, catalog| {
            ops::connect(doc, catalog, &pin(h1, "x2"), &pin(h2, "x1"), &[]).unwrap()
        }));
        assert!(s.commit(|doc, catalog| {
            ops::move_by(
                doc,
                catalog,
                &BTreeSet::from([circuit_model::Subject::Symbol(h2)]),
                GridPoint::new(0, 10),
            )
            .unwrap()
        }));
        assert!(s.commit(|doc, catalog| ops::rotate(doc, catalog, &BTreeSet::from([h1])).unwrap()));
        let h2_device = s.doc.symbols.iter().find(|x| x.id == h2).unwrap().device_id;
        assert!(s.commit(|doc, _| ops::set_reference(doc, h2_device, "P7").unwrap()));
        assert!(s.commit(|doc, _| {
            ops::add_text(doc, GridPoint::new(0, 30), "Pump on", TextAnchor::Left).unwrap()
        }));
        assert!(s.commit(lamp(GridPoint::new(40, 0))));
        // H2 is free again: P7 was H2.
        let h3 = s.symbol_of("H2");
        assert!(s.commit(|doc, catalog| {
            ops::delete(
                doc,
                catalog,
                &BTreeSet::from([circuit_model::Subject::Symbol(h3)]),
            )
            .unwrap()
        }));
        s
    }

    #[test]
    fn a_recorded_session_plays_back_to_the_same_project() {
        let start = Schematic::new("macro");
        let s = session(start.clone());
        assert_eq!(s.recorder.steps(), 9);
        assert_eq!(s.recorder.left_out(), 0);

        let file = s.recorder.to_edit_file();
        let played = script::apply(start, &s.catalog, &file).unwrap();

        assert_eq!(played.doc, s.doc);
    }

    #[test]
    fn a_macro_plays_on_another_project_whose_ids_differ() {
        let start = Schematic::new("macro");
        let file = session(start).recorder.to_edit_file();
        // Another project: a contactor already there takes the first IDs.
        let mut other = Schematic::new("other");
        let tx = ops::place(
            &other,
            &ComponentCatalog::builtin(),
            &ModelRef {
                id: "contactor.generic".to_owned(),
                version: 1,
            },
            &SymbolKey("coil".to_owned()),
            GridPoint::new(0, -40),
            QuarterTurn::Deg0,
        )
        .unwrap();
        other = tx.apply_to(&other).unwrap().0;

        let played = script::apply(other, &ComponentCatalog::builtin(), &file).unwrap();

        let mut names: Vec<&str> = played
            .doc
            .devices
            .iter()
            .map(|d| d.reference.0.as_str())
            .collect();
        names.sort_unstable();
        assert_eq!(names, ["H1", "K1", "P7"]);
        assert_eq!(played.doc.wires.len(), 1);
        assert_eq!(played.doc.texts.len(), 1);
    }

    #[test]
    fn undo_takes_back_the_last_step_and_redo_brings_it_again() {
        let mut s = session(Schematic::new("macro"));
        s.history.undo(&mut s.doc).unwrap();
        s.recorder.undo();
        assert_eq!(s.recorder.steps(), 8);
        s.history.redo(&mut s.doc).unwrap();
        s.recorder.redo();
        assert_eq!(s.recorder.steps(), 9);

        s.history.undo(&mut s.doc).unwrap();
        s.recorder.undo();
        let start = Schematic::new("macro");
        let played = script::apply(start, &s.catalog, &s.recorder.to_edit_file()).unwrap();
        assert_eq!(played.doc, s.doc);
    }

    #[test]
    fn a_change_no_operation_makes_is_left_out_not_recorded_wrong() {
        let mut s = Session::new(Schematic::new("macro"));
        s.commit(lamp(GridPoint::new(0, 0)));
        let recorded =
            s.commit(|doc, _| crate::layers::add_layer(doc, "Notes", true, false).unwrap());

        assert!(!recorded);
        assert_eq!((s.recorder.steps(), s.recorder.left_out()), (1, 1));
    }
}
