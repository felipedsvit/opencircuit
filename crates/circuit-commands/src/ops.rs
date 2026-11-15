//! The actions of a user, turned into transactions.
//!
//! Every operation reads the document and the catalog and returns a
//! [`Transaction`]; none changes the document. New IDs come from the
//! document counter, so the transaction can be applied, undone and redone
//! with the same IDs. An operation refuses up front what it can check, and
//! [`Transaction::apply_to`] checks the rest.

use std::collections::{BTreeMap, BTreeSet};

use circuit_model::catalog::{PARAM_FIRST, PARAM_SECOND, PARAM_SUPPLY};
use circuit_model::{
    ComponentCatalog, ComponentModel, Device, DeviceClass, DeviceId, Endpoint, GridPoint,
    IdAllocator, Junction, JunctionId, LayoutIndex, ModelRef, Operator, ParamType, ParamValue,
    PhaseSequence, QuarterTurn, ReferenceDesignator, Schematic, Subject, SupplyId, SupplyKind,
    SupplyProfile, SymbolId, SymbolInstance, SymbolKey, Wire, WireId, orthogonal_route, segment_at,
};

use crate::edit::Edit;
use crate::error::CommandError;
use crate::transaction::Transaction;

/// Where a new wire ends.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Anchor {
    /// An existing terminal or junction.
    Endpoint(Endpoint),
    /// A point on an existing wire. Unless it is one of the ends of the wire,
    /// the wire is split there by a new junction, which is how a branch (a
    /// T connection) is drawn.
    OnWire {
        /// The wire.
        wire: WireId,
        /// The point on it.
        at: GridPoint,
    },
}

/// Supply profiles offered when adding one; every field can be edited after.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SupplyPreset {
    /// 24 V DC.
    Dc24V,
    /// 230 V, 50 Hz, single phase.
    SinglePhase230V,
    /// 400 V line, 230 V phase, 50 Hz, phase order L1-L2-L3.
    ThreePhase400V,
}

impl SupplyPreset {
    /// Every preset.
    pub const ALL: [Self; 3] = [Self::Dc24V, Self::SinglePhase230V, Self::ThreePhase400V];

    /// A short name for a menu.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Dc24V => "24 V DC",
            Self::SinglePhase230V => "230 V 50 Hz single phase",
            Self::ThreePhase400V => "400/230 V 50 Hz three phase",
        }
    }

    /// The profile of this preset with the given ID.
    #[must_use]
    pub const fn profile(self, id: SupplyId) -> SupplyProfile {
        match self {
            Self::Dc24V => SupplyProfile {
                id,
                kind: SupplyKind::Dc,
                nominal_voltage_mv: 24_000,
                phase_voltage_mv: None,
                frequency_millihz: None,
                phase_sequence: None,
            },
            Self::SinglePhase230V => SupplyProfile {
                id,
                kind: SupplyKind::SinglePhase,
                nominal_voltage_mv: 230_000,
                phase_voltage_mv: None,
                frequency_millihz: Some(50_000),
                phase_sequence: None,
            },
            Self::ThreePhase400V => SupplyProfile {
                id,
                kind: SupplyKind::ThreePhase,
                nominal_voltage_mv: 400_000,
                phase_voltage_mv: Some(230_000),
                frequency_millihz: Some(50_000),
                phase_sequence: Some(PhaseSequence::L1L2L3),
            },
        }
    }
}

fn model_of<'a>(
    catalog: &'a ComponentCatalog,
    model: &ModelRef,
) -> Result<&'a ComponentModel, CommandError> {
    catalog
        .model(model)
        .ok_or_else(|| CommandError::UnknownModel(model.id.clone()))
}

fn device_of(doc: &Schematic, device: DeviceId) -> Result<&Device, CommandError> {
    doc.devices
        .iter()
        .find(|d| d.id == device)
        .ok_or(CommandError::Missing(Subject::Device(device)))
}

/// Refuse a placement whose symbol area would leave the coordinate range:
/// such a symbol could be neither drawn nor clicked.
fn fits(
    definition: &circuit_model::SymbolDefinition,
    at: GridPoint,
    rotation: QuarterTurn,
) -> Result<(), CommandError> {
    definition
        .kind()
        .bounds()
        .placed(rotation, at)
        .map(|_| ())
        .ok_or(CommandError::OutOfRange)
}

/// Check that a symbol of the document still fits once placed at `at` with
/// `rotation`. A symbol whose model is unknown has no known area and passes.
fn still_fits(
    layout: &LayoutIndex<'_>,
    symbol: SymbolId,
    at: GridPoint,
    rotation: QuarterTurn,
) -> Result<(), CommandError> {
    match layout.definition(symbol) {
        Some(definition) => fits(definition, at, rotation),
        None => Ok(()),
    }
}

/// The letters a new device of this model is named with: K for contactors
/// and relays, KT for timers, S for pushbuttons and switches, Q for a main
/// switch, F for thermal relays and fuses, M for motors, H for lamps and
/// buzzers, X for supply terminals, KX for mechanical interlocks, T for
/// transformers, B for position and level switches.
#[must_use]
pub fn designator_prefix(model: &ComponentModel) -> &'static str {
    match model.class() {
        DeviceClass::Contactor | DeviceClass::Relay => "K",
        DeviceClass::Timer(_) => "KT",
        DeviceClass::Pushbutton => {
            if model.operator() == Operator::Position {
                "B"
            } else {
                "S"
            }
        }
        DeviceClass::LatchingSwitch => {
            if model.id().starts_with("main_switch") {
                "Q"
            } else {
                "S"
            }
        }
        DeviceClass::ThermalOverload | DeviceClass::Fuse => "F",
        DeviceClass::Motor => "M",
        DeviceClass::Lamp | DeviceClass::Buzzer => "H",
        DeviceClass::PowerSupply => "X",
        DeviceClass::MechanicalInterlock => "KX",
        DeviceClass::Transformer => "T",
    }
}

/// The first `prefix` + number not used by any device.
fn free_designator(doc: &Schematic, prefix: &str) -> Result<ReferenceDesignator, CommandError> {
    let used: BTreeSet<&str> = doc.devices.iter().map(|d| d.reference.0.as_str()).collect();
    // With n devices, one of the numbers 1..=n+1 is free.
    let last = u64::try_from(doc.devices.len())
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    (1..=last)
        .map(|n| format!("{prefix}{n}"))
        .find(|candidate| !used.contains(candidate.as_str()))
        .map(ReferenceDesignator)
        .ok_or_else(|| CommandError::InvalidReference(prefix.to_owned()))
}

/// Place a new device of `model`, drawn as its symbol `symbol` at `at`.
///
/// The device gets the first free designator of its kind (see
/// [`designator_prefix`]) and no parameters: the model defaults apply, and
/// a parameter without a default shows up as a compile diagnostic until it
/// is set. A slotted symbol takes slot 1.
///
/// # Errors
///
/// Returns [`CommandError`] for a model or symbol the catalog does not have,
/// or when the document has no ID left.
pub fn place(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    model: &ModelRef,
    symbol: &SymbolKey,
    at: GridPoint,
    rotation: QuarterTurn,
) -> Result<Transaction, CommandError> {
    let definition_model = model_of(catalog, model)?;
    let definition =
        definition_model
            .symbol(symbol)
            .ok_or_else(|| CommandError::UnknownSymbol {
                model: model.id.clone(),
                symbol: symbol.0.clone(),
            })?;
    fits(definition, at, rotation)?;
    let mut ids = IdAllocator::starting_at(doc.next_id);
    let device = DeviceId(ids.alloc()?);
    let symbol_id = SymbolId(ids.alloc()?);
    let reference = free_designator(doc, designator_prefix(definition_model))?;

    let mut tx = Transaction::new(format!("Place {}", reference.0), ids.next_id());
    tx.edits.push(Edit::InsertDevice(Device {
        id: device,
        reference,
        model: model.clone(),
        params: BTreeMap::new(),
    }));
    tx.edits.push(Edit::InsertSymbol(SymbolInstance {
        id: symbol_id,
        device_id: device,
        symbol_key: symbol.clone(),
        slot: definition.slotted().then_some(1),
        position: at,
        rotation,
    }));
    Ok(tx)
}

/// Add another symbol of an existing device, such as an auxiliary contact
/// of a contactor. A slotted symbol takes the first free slot.
///
/// # Errors
///
/// Returns [`CommandError`] for a missing device, an unknown model or
/// symbol, a device that already has as many such symbols as its model
/// allows, or when the document has no ID left.
pub fn add_symbol(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    device: DeviceId,
    symbol: &SymbolKey,
    at: GridPoint,
    rotation: QuarterTurn,
) -> Result<Transaction, CommandError> {
    let owner = device_of(doc, device)?;
    let model = model_of(catalog, &owner.model)?;
    let definition = model
        .symbol(symbol)
        .ok_or_else(|| CommandError::UnknownSymbol {
            model: owner.model.id.clone(),
            symbol: symbol.0.clone(),
        })?;
    let limit = CommandError::InstanceLimit {
        device,
        symbol: symbol.0.clone(),
    };
    let existing: Vec<&SymbolInstance> = doc
        .symbols
        .iter()
        .filter(|s| s.device_id == device && &s.symbol_key == symbol)
        .collect();
    if existing.len() >= usize::from(definition.max_instances()) {
        return Err(limit);
    }
    let slot = if definition.slotted() {
        let used: BTreeSet<u8> = existing.iter().filter_map(|s| s.slot).collect();
        Some(
            (1..=definition.max_instances())
                .find(|slot| !used.contains(slot))
                .ok_or(limit)?,
        )
    } else {
        None
    };
    fits(definition, at, rotation)?;

    let mut ids = IdAllocator::starting_at(doc.next_id);
    let id = SymbolId(ids.alloc()?);
    let mut tx = Transaction::new(
        format!("Add {} to {}", symbol.0, owner.reference.0),
        ids.next_id(),
    );
    tx.edits.push(Edit::InsertSymbol(SymbolInstance {
        id,
        device_id: device,
        symbol_key: symbol.clone(),
        slot,
        position: at,
        rotation,
    }));
    Ok(tx)
}

fn subject_of(endpoint: &Endpoint) -> Subject {
    match endpoint {
        Endpoint::Terminal(pin) => Subject::Symbol(pin.symbol_id),
        Endpoint::Junction(id) => Subject::Junction(*id),
    }
}

/// The endpoint and position of an anchor, adding the edits that split a
/// wire when the anchor is in the middle of one.
fn resolve(
    doc: &Schematic,
    layout: &LayoutIndex<'_>,
    anchor: &Anchor,
    ids: &mut IdAllocator,
    edits: &mut Vec<Edit>,
) -> Result<(Endpoint, GridPoint), CommandError> {
    match anchor {
        Anchor::Endpoint(endpoint) => {
            let at = layout
                .endpoint_position(endpoint)
                .ok_or_else(|| CommandError::Missing(subject_of(endpoint)))?;
            Ok((endpoint.clone(), at))
        }
        Anchor::OnWire { wire, at } => {
            let missing = || CommandError::Missing(Subject::Wire(*wire));
            let split = doc
                .wires
                .iter()
                .find(|w| w.id == *wire)
                .ok_or_else(missing)?;
            let path = layout.wire_polyline(split).ok_or_else(missing)?;
            if path.first() == Some(at) {
                return Ok((split.from.clone(), *at));
            }
            if path.last() == Some(at) {
                return Ok((split.to.clone(), *at));
            }
            let segment = segment_at(&path, *at).ok_or(CommandError::NotOnWire(*wire))?;
            let junction = JunctionId(ids.alloc()?);
            let first = WireId(ids.alloc()?);
            let second = WireId(ids.alloc()?);
            // `path` is the start, the waypoints, then the end; the split
            // point lies on segment `segment`, between path[segment] and
            // path[segment + 1].
            let before: Vec<GridPoint> = path
                .iter()
                .skip(1)
                .take(segment)
                .copied()
                .filter(|p| p != at)
                .collect();
            let after: Vec<GridPoint> = path
                .iter()
                .skip(segment.saturating_add(1))
                .take(path.len().saturating_sub(segment.saturating_add(2)))
                .copied()
                .filter(|p| p != at)
                .collect();
            edits.push(Edit::RemoveWire(*wire));
            edits.push(Edit::InsertJunction(Junction {
                id: junction,
                position: *at,
            }));
            edits.push(Edit::InsertWire(Wire {
                id: first,
                from: split.from.clone(),
                to: Endpoint::Junction(junction),
                route: before,
            }));
            edits.push(Edit::InsertWire(Wire {
                id: second,
                from: Endpoint::Junction(junction),
                to: split.to.clone(),
                route: after,
            }));
            Ok((Endpoint::Junction(junction), *at))
        }
    }
}

/// Draw a wire from one anchor to another through `via`. With no waypoint
/// given, the wire takes an orthogonal route with at most one bend.
///
/// # Errors
///
/// Returns [`CommandError`] when an anchor does not exist or is not on its
/// wire, when both anchors are the same point or on the same wire, or when
/// the document has no ID left.
pub fn connect(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    from: &Anchor,
    to: &Anchor,
    via: &[GridPoint],
) -> Result<Transaction, CommandError> {
    if let (Anchor::OnWire { wire: a, .. }, Anchor::OnWire { wire: b, .. }) = (from, to) {
        if a == b {
            return Err(CommandError::SelfLoop);
        }
    }
    let layout = LayoutIndex::new(doc, catalog);
    let mut ids = IdAllocator::starting_at(doc.next_id);
    let mut edits = Vec::new();
    let (start, start_at) = resolve(doc, &layout, from, &mut ids, &mut edits)?;
    let (end, end_at) = resolve(doc, &layout, to, &mut ids, &mut edits)?;
    if start == end {
        return Err(CommandError::SelfLoop);
    }
    let route = if via.is_empty() {
        orthogonal_route(start_at, end_at)
    } else {
        via.to_vec()
    };
    let id = WireId(ids.alloc()?);
    edits.push(Edit::InsertWire(Wire {
        id,
        from: start,
        to: end,
        route,
    }));
    let mut tx = Transaction::new("Connect", ids.next_id());
    tx.edits = edits;
    Ok(tx)
}

fn ends_at_junction(wire: &Wire, junction: JunctionId) -> bool {
    wire.from == Endpoint::Junction(junction) || wire.to == Endpoint::Junction(junction)
}

fn touches(
    endpoint: &Endpoint,
    symbols: &BTreeSet<SymbolId>,
    junctions: &BTreeSet<JunctionId>,
) -> bool {
    match endpoint {
        Endpoint::Terminal(pin) => symbols.contains(&pin.symbol_id),
        Endpoint::Junction(id) => junctions.contains(id),
    }
}

/// Delete elements and everything that cannot exist without them: the
/// symbols of a deleted device and the wires at a deleted symbol or
/// junction. A junction whose wires are all deleted and a device whose
/// symbols are all deleted go too, and so does a mechanical interlock that
/// names a deleted device. Pins, supplies and the document are not deleted
/// this way.
///
/// # Errors
///
/// Returns [`CommandError::Missing`] for an element that does not exist.
pub fn delete(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    subjects: &BTreeSet<Subject>,
) -> Result<Transaction, CommandError> {
    let mut devices = BTreeSet::new();
    let mut symbols = BTreeSet::new();
    let mut junctions = BTreeSet::new();
    let mut wires = BTreeSet::new();
    for subject in subjects {
        let missing = || CommandError::Missing(subject.clone());
        match subject {
            Subject::Device(id) => {
                doc.devices
                    .iter()
                    .find(|d| d.id == *id)
                    .ok_or_else(missing)?;
                devices.insert(*id);
            }
            Subject::Symbol(id) => {
                doc.symbols
                    .iter()
                    .find(|s| s.id == *id)
                    .ok_or_else(missing)?;
                symbols.insert(*id);
            }
            Subject::Junction(id) => {
                doc.junctions
                    .iter()
                    .find(|j| j.id == *id)
                    .ok_or_else(missing)?;
                junctions.insert(*id);
            }
            Subject::Wire(id) => {
                doc.wires.iter().find(|w| w.id == *id).ok_or_else(missing)?;
                wires.insert(*id);
            }
            Subject::Pin(_) | Subject::Supply(_) | Subject::Document => {}
        }
    }

    symbols.extend(
        doc.symbols
            .iter()
            .filter(|s| devices.contains(&s.device_id))
            .map(|s| s.id),
    );
    wires.extend(
        doc.wires
            .iter()
            .filter(|w| {
                touches(&w.from, &symbols, &junctions) || touches(&w.to, &symbols, &junctions)
            })
            .map(|w| w.id),
    );
    for junction in &doc.junctions {
        let mut attached = doc
            .wires
            .iter()
            .filter(|w| ends_at_junction(w, junction.id))
            .peekable();
        if attached.peek().is_some() && attached.all(|w| wires.contains(&w.id)) {
            junctions.insert(junction.id);
        }
    }
    for device in &doc.devices {
        let mut own = doc
            .symbols
            .iter()
            .filter(|s| s.device_id == device.id)
            .peekable();
        if own.peek().is_some() && own.all(|s| symbols.contains(&s.id)) {
            devices.insert(device.id);
        }
    }

    // An interlock cannot outlive a contactor it names.
    for device in &doc.devices {
        let names_deleted = catalog.model(&device.model).is_some_and(|model| {
            model.class() == DeviceClass::MechanicalInterlock
                && [PARAM_FIRST, PARAM_SECOND].iter().any(|key| {
                    model
                        .int_param(&device.params, key)
                        .and_then(|raw| u64::try_from(raw).ok())
                        .is_some_and(|id| devices.contains(&DeviceId(id)))
                })
        });
        if names_deleted && devices.insert(device.id) {
            symbols.extend(
                doc.symbols
                    .iter()
                    .filter(|s| s.device_id == device.id)
                    .map(|s| s.id),
            );
        }
    }

    let count = wires
        .len()
        .saturating_add(junctions.len())
        .saturating_add(symbols.len())
        .saturating_add(devices.len());
    let mut tx = Transaction::new(format!("Delete {count} items"), doc.next_id);
    tx.edits.extend(wires.into_iter().map(Edit::RemoveWire));
    tx.edits
        .extend(junctions.into_iter().map(Edit::RemoveJunction));
    tx.edits.extend(symbols.into_iter().map(Edit::RemoveSymbol));
    tx.edits.extend(devices.into_iter().map(Edit::RemoveDevice));
    Ok(tx)
}

/// The symbols and junctions a selection moves: the symbols named, every
/// symbol of the devices named, and the junctions named.
fn movable(
    doc: &Schematic,
    subjects: &BTreeSet<Subject>,
) -> (BTreeSet<SymbolId>, BTreeSet<JunctionId>) {
    let mut symbols = BTreeSet::new();
    let mut junctions = BTreeSet::new();
    for subject in subjects {
        match subject {
            Subject::Symbol(id) => {
                symbols.insert(*id);
            }
            Subject::Device(id) => symbols.extend(
                doc.symbols
                    .iter()
                    .filter(|s| s.device_id == *id)
                    .map(|s| s.id),
            ),
            Subject::Junction(id) => {
                junctions.insert(*id);
            }
            Subject::Wire(_) | Subject::Pin(_) | Subject::Supply(_) | Subject::Document => {}
        }
    }
    (symbols, junctions)
}

/// New routes for the wires whose ends were moved: shifted by `shift` when
/// both ends moved together, routed again when only one did or when there
/// is no shift (a rotation). `moved` is the document after the move.
fn reroute(
    before: &Schematic,
    moved: &Schematic,
    catalog: &ComponentCatalog,
    symbols: &BTreeSet<SymbolId>,
    junctions: &BTreeSet<JunctionId>,
    shift: Option<GridPoint>,
) -> Result<Vec<Edit>, CommandError> {
    let layout = LayoutIndex::new(moved, catalog);
    let mut edits = Vec::new();
    for wire in &before.wires {
        let from = touches(&wire.from, symbols, junctions);
        let to = touches(&wire.to, symbols, junctions);
        let route = match (from, to, shift) {
            (false, false, _) => continue,
            (true, true, Some(delta)) => wire
                .route
                .iter()
                .map(|p| p.checked_add(delta).ok_or(CommandError::OutOfRange))
                .collect::<Result<Vec<_>, _>>()?,
            _ => {
                let ends = (
                    layout.endpoint_position(&wire.from),
                    layout.endpoint_position(&wire.to),
                );
                let (Some(start), Some(end)) = ends else {
                    continue;
                };
                orthogonal_route(start, end)
            }
        };
        if route != wire.route {
            edits.push(Edit::SetRoute {
                wire: wire.id,
                route,
            });
        }
    }
    Ok(edits)
}

/// Move the symbols and junctions of a selection by `delta` grid units.
/// Wires between two moved elements keep their shape; a wire with one end
/// moved is routed again.
///
/// # Errors
///
/// Returns [`CommandError`] when a position would leave the coordinate
/// range or an element does not exist.
pub fn move_by(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    subjects: &BTreeSet<Subject>,
    delta: GridPoint,
) -> Result<Transaction, CommandError> {
    let mut tx = Transaction::new("Move", doc.next_id);
    if delta == GridPoint::default() {
        return Ok(tx);
    }
    let (symbols, junctions) = movable(doc, subjects);
    let layout = LayoutIndex::new(doc, catalog);
    for id in &symbols {
        let symbol = doc
            .symbols
            .iter()
            .find(|s| s.id == *id)
            .ok_or(CommandError::Missing(Subject::Symbol(*id)))?;
        let position = symbol
            .position
            .checked_add(delta)
            .ok_or(CommandError::OutOfRange)?;
        still_fits(&layout, *id, position, symbol.rotation)?;
        tx.edits.push(Edit::Place {
            symbol: *id,
            position,
            rotation: symbol.rotation,
        });
    }
    for id in &junctions {
        let junction = doc
            .junctions
            .iter()
            .find(|j| j.id == *id)
            .ok_or(CommandError::Missing(Subject::Junction(*id)))?;
        tx.edits.push(Edit::MoveJunction {
            junction: *id,
            position: junction
                .position
                .checked_add(delta)
                .ok_or(CommandError::OutOfRange)?,
        });
    }
    let (moved, _) = tx.apply_edits(doc)?;
    let routes = reroute(doc, &moved, catalog, &symbols, &junctions, Some(delta))?;
    tx.edits.extend(routes);
    Ok(tx)
}

/// Turn symbols a quarter turn clockwise about their origin. Their wires are
/// routed again.
///
/// # Errors
///
/// Returns [`CommandError::Missing`] for a symbol that does not exist.
pub fn rotate(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    symbols: &BTreeSet<SymbolId>,
) -> Result<Transaction, CommandError> {
    let mut tx = Transaction::new("Rotate", doc.next_id);
    let layout = LayoutIndex::new(doc, catalog);
    for id in symbols {
        let symbol = doc
            .symbols
            .iter()
            .find(|s| s.id == *id)
            .ok_or(CommandError::Missing(Subject::Symbol(*id)))?;
        let rotation = symbol.rotation.next();
        still_fits(&layout, *id, symbol.position, rotation)?;
        tx.edits.push(Edit::Place {
            symbol: *id,
            position: symbol.position,
            rotation,
        });
    }
    if tx.is_empty() {
        return Ok(tx);
    }
    let (turned, _) = tx.apply_edits(doc)?;
    let routes = reroute(doc, &turned, catalog, symbols, &BTreeSet::new(), None)?;
    tx.edits.extend(routes);
    Ok(tx)
}

/// Rename a device.
///
/// # Errors
///
/// Returns [`CommandError`] for a missing device, a text that is not a
/// valid designator, or a designator another device already has.
pub fn set_reference(
    doc: &Schematic,
    device: DeviceId,
    text: &str,
) -> Result<Transaction, CommandError> {
    let reference = ReferenceDesignator(text.to_owned());
    if !reference.is_valid() {
        return Err(CommandError::InvalidReference(text.to_owned()));
    }
    let current = device_of(doc, device)?;
    let mut tx = Transaction::new(
        format!("Rename {} to {text}", current.reference.0),
        doc.next_id,
    );
    if current.reference == reference {
        return Ok(tx);
    }
    if doc
        .devices
        .iter()
        .any(|d| d.id != device && d.reference == reference)
    {
        return Err(CommandError::DuplicateReference(text.to_owned()));
    }
    tx.edits.push(Edit::SetReference { device, reference });
    Ok(tx)
}

/// Set (`Some`) or clear (`None`) one parameter of a device. The value must
/// fit the definition of the parameter in the model, and a supply reference
/// must name a supply of the document.
///
/// # Errors
///
/// Returns [`CommandError`] for a missing device, an unknown model, a key
/// the model does not define, or a value that does not fit.
pub fn set_param(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    device: DeviceId,
    key: &str,
    value: Option<ParamValue>,
) -> Result<Transaction, CommandError> {
    let current = device_of(doc, device)?;
    let model = model_of(catalog, &current.model)?;
    let definition = model
        .params()
        .iter()
        .find(|definition| definition.key == key)
        .ok_or_else(|| {
            CommandError::InvalidParameter(format!(
                "parameter `{key}` is not defined by model `{}`",
                model.id()
            ))
        })?;
    if let Some(value) = &value {
        definition
            .accepts(value)
            .map_err(CommandError::InvalidParameter)?;
        if matches!(definition.ty, ParamType::DeviceRef) {
            let target = match value {
                ParamValue::Int(raw) => u64::try_from(*raw).ok().map(DeviceId),
                ParamValue::Bool(_) | ParamValue::Text(_) => None,
            };
            let exists =
                target.is_some_and(|id| id != device && doc.devices.iter().any(|d| d.id == id));
            if !exists {
                return Err(CommandError::InvalidParameter(format!(
                    "parameter `{key}` must name another device of the document"
                )));
            }
        }
        if matches!(definition.ty, ParamType::SupplyRef) {
            let supply = match value {
                ParamValue::Int(raw) => u64::try_from(*raw).ok().map(SupplyId),
                ParamValue::Bool(_) | ParamValue::Text(_) => None,
            };
            let exists = supply.is_some_and(|id| doc.supplies.iter().any(|s| s.id == id));
            if !exists {
                return Err(CommandError::InvalidParameter(format!(
                    "parameter `{key}` must name a supply of the document"
                )));
            }
        }
    }
    let mut tx = Transaction::new(format!("Set {key} of {}", current.reference.0), doc.next_id);
    if current.params.get(key) != value.as_ref() {
        tx.edits.push(Edit::SetParam {
            device,
            key: key.to_owned(),
            value,
        });
    }
    Ok(tx)
}

/// Upgrade a device to the newer model version its catalog registers a
/// migration for. The migration keeps every symbol and parameter key, so the
/// device, its symbols and its wires stay as they are; only the model
/// reference changes. Loading a document never does this on its own.
///
/// # Errors
///
/// Returns [`CommandError::Missing`] for a missing device,
/// [`CommandError::UnknownModel`] when its model is not in the catalog, and
/// [`CommandError::NoUpgrade`] when no migration is registered for it.
pub fn upgrade_model(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    device: DeviceId,
) -> Result<Transaction, CommandError> {
    let current = device_of(doc, device)?;
    model_of(catalog, &current.model)?;
    let target = catalog
        .upgrade(&current.model)
        .ok_or_else(|| CommandError::NoUpgrade(current.model.id.clone()))?;
    let mut tx = Transaction::new(
        format!(
            "Upgrade {} to {} version {}",
            current.reference.0,
            target.id(),
            target.version()
        ),
        doc.next_id,
    );
    tx.edits.push(Edit::SetModel {
        device,
        model: ModelRef {
            id: target.id().to_owned(),
            version: target.version(),
        },
    });
    Ok(tx)
}

/// Add a supply profile from a preset.
///
/// # Errors
///
/// Returns [`CommandError::Ids`] when the document has no ID left.
pub fn add_supply(doc: &Schematic, preset: SupplyPreset) -> Result<Transaction, CommandError> {
    let mut ids = IdAllocator::starting_at(doc.next_id);
    let id = SupplyId(ids.alloc()?);
    let mut tx = Transaction::new(format!("Add supply {}", preset.label()), ids.next_id());
    tx.edits.push(Edit::InsertSupply(preset.profile(id)));
    Ok(tx)
}

/// Replace a supply profile with an edited one of the same ID.
///
/// # Errors
///
/// Returns [`CommandError`] for a supply that does not exist or a profile
/// that is inconsistent with its kind.
pub fn set_supply(doc: &Schematic, profile: SupplyProfile) -> Result<Transaction, CommandError> {
    if let Some(reason) = profile.inconsistency() {
        return Err(CommandError::InvalidSupply(reason));
    }
    let current = doc
        .supplies
        .iter()
        .find(|s| s.id == profile.id)
        .ok_or(CommandError::Missing(Subject::Supply(profile.id)))?;
    let mut tx = Transaction::new(format!("Edit supply {}", profile.id), doc.next_id);
    if *current != profile {
        tx.edits.push(Edit::SetSupply(profile));
    }
    Ok(tx)
}

/// Remove a supply profile no supply terminal uses.
///
/// # Errors
///
/// Returns [`CommandError`] for a missing supply or one a device still uses.
pub fn remove_supply(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    supply: SupplyId,
) -> Result<Transaction, CommandError> {
    if !doc.supplies.iter().any(|s| s.id == supply) {
        return Err(CommandError::Missing(Subject::Supply(supply)));
    }
    let raw = i64::try_from(supply.0).ok();
    let user = doc.devices.iter().find(|device| {
        catalog.model(&device.model).is_some_and(|model| {
            model.class() == DeviceClass::PowerSupply
                && raw.is_some()
                && model.int_param(&device.params, PARAM_SUPPLY) == raw
        })
    });
    if let Some(device) = user {
        return Err(CommandError::InUse {
            subject: Subject::Supply(supply),
            by: Subject::Device(device.id),
        });
    }
    let mut tx = Transaction::new(format!("Remove supply {supply}"), doc.next_id);
    tx.edits.push(Edit::RemoveSupply(supply));
    Ok(tx)
}
