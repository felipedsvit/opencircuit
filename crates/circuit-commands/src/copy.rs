//! Copying part of a document and pasting it back, shifted.
//!
//! [`copy`] takes the elements a selection names into a [`Fragment`] that
//! keeps their original IDs; [`paste`] inserts a fragment into a document
//! with new IDs from the document counter, and [`duplicate`] does both.
//!
//! Which device a pasted symbol belongs to follows one rule. A device keeps
//! its pasted symbols when it still exists with the same model, the fragment
//! does not hold all of its symbols, and every pasted symbol is a slotted
//! one (an auxiliary contact) that its model still has room for: copying a
//! contact of K1 adds a contact to K1. Otherwise the fragment's symbols of
//! that device become a new device with the next free designator of its kind
//! and the same parameters: copying K1 with its coil and contacts makes K2,
//! and so does copying the coil of K1 alone, since K1 cannot have two coils.
//! A device parameter that names another copied device (the contactors of a
//! mechanical interlock) is pointed at the copy.

use std::collections::{BTreeMap, BTreeSet};

use circuit_model::{
    ComponentCatalog, Device, DeviceId, Endpoint, GridPoint, IdAllocator, Junction, JunctionId,
    ParamType, ParamValue, PinRef, ReferenceDesignator, Schematic, Subject, SymbolId,
    SymbolInstance, SymbolKey, Wire, WireId,
};

use crate::edit::Edit;
use crate::error::CommandError;
use crate::ops::{designator_prefix, fits, model_of, movable};
use crate::transaction::Transaction;

/// The layer a pasted element stays on: its own, when the document has it (a
/// copy pasted into another project has no such layer there).
fn keep_layer(
    doc: &Schematic,
    layer: Option<circuit_model::LayerId>,
) -> Option<circuit_model::LayerId> {
    layer.filter(|id| doc.layers.iter().any(|candidate| candidate.id == *id))
}

/// Part of a document, with its original IDs: what a copy holds.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Fragment {
    /// The devices of the copied symbols.
    pub devices: Vec<Device>,
    /// The copied symbols.
    pub symbols: Vec<SymbolInstance>,
    /// The copied junctions.
    pub junctions: Vec<Junction>,
    /// The copied wires: those whose two ends are copied.
    pub wires: Vec<Wire>,
}

impl Fragment {
    /// Whether the fragment holds nothing to paste.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty() && self.junctions.is_empty()
    }

    /// How many elements a paste inserts.
    #[must_use]
    pub fn len(&self) -> usize {
        self.symbols
            .len()
            .saturating_add(self.junctions.len())
            .saturating_add(self.wires.len())
    }
}

/// Copy the elements `subjects` name: the symbols named, every symbol of a
/// device named, the junctions named that a copied wire reaches, and every
/// wire whose two ends are copied. Elements the document does not have are
/// ignored. Everything is in ID order.
#[must_use]
pub fn copy(doc: &Schematic, subjects: &BTreeSet<Subject>) -> Fragment {
    let (symbol_ids, named_junctions) = movable(doc, subjects);
    let inside = |endpoint: &Endpoint| match endpoint {
        Endpoint::Terminal(pin) => symbol_ids.contains(&pin.symbol_id),
        Endpoint::Junction(id) => named_junctions.contains(id),
    };
    let mut wires: Vec<Wire> = doc
        .wires
        .iter()
        .filter(|wire| inside(&wire.from) && inside(&wire.to))
        .cloned()
        .collect();
    wires.sort_by_key(|wire| wire.id);
    let reached: BTreeSet<JunctionId> = wires
        .iter()
        .flat_map(|wire| [&wire.from, &wire.to])
        .filter_map(|endpoint| match endpoint {
            Endpoint::Junction(id) => Some(*id),
            Endpoint::Terminal(_) => None,
        })
        .collect();
    let mut symbols: Vec<SymbolInstance> = doc
        .symbols
        .iter()
        .filter(|symbol| symbol_ids.contains(&symbol.id))
        .cloned()
        .collect();
    symbols.sort_by_key(|symbol| symbol.id);
    let owners: BTreeSet<DeviceId> = symbols.iter().map(|symbol| symbol.device_id).collect();
    let mut devices: Vec<Device> = doc
        .devices
        .iter()
        .filter(|device| owners.contains(&device.id))
        .cloned()
        .collect();
    devices.sort_by_key(|device| device.id);
    let mut junctions: Vec<Junction> = doc
        .junctions
        .iter()
        .filter(|junction| reached.contains(&junction.id))
        .cloned()
        .collect();
    junctions.sort_by_key(|junction| junction.id);
    Fragment {
        devices,
        symbols,
        junctions,
        wires,
    }
}

/// Insert `fragment` into `doc`, every position shifted by `offset`.
///
/// Every element gets a new ID from the document counter, in the order of
/// the fragment. See the module documentation for which device a pasted
/// symbol belongs to.
///
/// # Errors
///
/// Returns [`CommandError`] for a model the catalog does not have, a symbol
/// its model does not have, a position outside the coordinate range, a
/// fragment symbol whose device is not in the fragment, or when the document
/// has no ID or designator left. An empty fragment gives an empty
/// transaction.
pub fn paste(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    fragment: &Fragment,
    offset: GridPoint,
) -> Result<Transaction, CommandError> {
    let mut ids = IdAllocator::starting_at(doc.next_id);
    let mut edits = Vec::new();
    let mut taken: BTreeSet<String> = doc
        .devices
        .iter()
        .map(|device| device.reference.0.clone())
        .collect();
    let shift = |point: GridPoint| point.checked_add(offset).ok_or(CommandError::OutOfRange);

    // Devices: kept, or new with a free designator.
    let mut device_of: BTreeMap<DeviceId, DeviceId> = BTreeMap::new();
    let mut new_devices: Vec<Device> = Vec::new();
    let mut slots_used: BTreeMap<(DeviceId, SymbolKey), BTreeSet<u8>> = BTreeMap::new();
    for device in &fragment.devices {
        let model = model_of(catalog, &device.model)?;
        let pasted: Vec<&SymbolInstance> = fragment
            .symbols
            .iter()
            .filter(|symbol| symbol.device_id == device.id)
            .collect();
        if keeps_device(doc, catalog, device, &pasted) {
            device_of.insert(device.id, device.id);
            for symbol in doc.symbols.iter().filter(|s| s.device_id == device.id) {
                if let Some(slot) = symbol.slot {
                    slots_used
                        .entry((device.id, symbol.symbol_key.clone()))
                        .or_default()
                        .insert(slot);
                }
            }
            continue;
        }
        let id = DeviceId(ids.alloc()?);
        let reference = free_designator(&taken, designator_prefix(model))?;
        taken.insert(reference.0.clone());
        device_of.insert(device.id, id);
        new_devices.push(Device {
            id,
            reference,
            model: device.model.clone(),
            params: device.params.clone(),
            info: device.info.clone(),
        });
    }
    // A device reference to a copied device follows the copy.
    for device in &mut new_devices {
        let Ok(model) = model_of(catalog, &device.model) else {
            continue;
        };
        for definition in model.params() {
            if definition.ty != ParamType::DeviceRef {
                continue;
            }
            let Some(ParamValue::Int(raw)) = device.params.get_mut(&definition.key) else {
                continue;
            };
            let target = u64::try_from(*raw)
                .ok()
                .and_then(|id| device_of.get(&DeviceId(id)))
                .and_then(|target| i64::try_from(target.0).ok());
            if let Some(target) = target {
                *raw = target;
            }
        }
    }
    edits.extend(new_devices.into_iter().map(Edit::InsertDevice));

    let mut symbol_of: BTreeMap<SymbolId, SymbolId> = BTreeMap::new();
    for symbol in &fragment.symbols {
        let source = fragment
            .devices
            .iter()
            .find(|device| device.id == symbol.device_id)
            .ok_or(CommandError::Missing(Subject::Device(symbol.device_id)))?;
        let target = device_of
            .get(&source.id)
            .copied()
            .ok_or(CommandError::Missing(Subject::Device(source.id)))?;
        let model = model_of(catalog, &source.model)?;
        let definition =
            model
                .symbol(&symbol.symbol_key)
                .ok_or_else(|| CommandError::UnknownSymbol {
                    model: source.model.id.clone(),
                    symbol: symbol.symbol_key.0.clone(),
                })?;
        let position = shift(symbol.position)?;
        fits(definition, position, symbol.rotation)?;
        let slot = if target == source.id {
            // Added to the existing device: the first free slot.
            let used = slots_used
                .entry((target, symbol.symbol_key.clone()))
                .or_default();
            let slot = (1..=definition.max_instances())
                .find(|slot| !used.contains(slot))
                .ok_or_else(|| CommandError::InstanceLimit {
                    device: target,
                    symbol: symbol.symbol_key.0.clone(),
                })?;
            used.insert(slot);
            Some(slot)
        } else {
            symbol.slot
        };
        let id = SymbolId(ids.alloc()?);
        symbol_of.insert(symbol.id, id);
        edits.push(Edit::InsertSymbol(SymbolInstance {
            id,
            device_id: target,
            symbol_key: symbol.symbol_key.clone(),
            slot,
            position,
            rotation: symbol.rotation,
            layer: keep_layer(doc, symbol.layer),
        }));
    }

    let mut junction_of: BTreeMap<JunctionId, JunctionId> = BTreeMap::new();
    for junction in &fragment.junctions {
        let id = JunctionId(ids.alloc()?);
        junction_of.insert(junction.id, id);
        edits.push(Edit::InsertJunction(Junction {
            id,
            position: shift(junction.position)?,
        }));
    }

    let endpoint = |endpoint: &Endpoint| -> Result<Endpoint, CommandError> {
        match endpoint {
            Endpoint::Terminal(pin) => symbol_of
                .get(&pin.symbol_id)
                .map(|id| {
                    Endpoint::Terminal(PinRef {
                        symbol_id: *id,
                        terminal: pin.terminal.clone(),
                    })
                })
                .ok_or(CommandError::Missing(Subject::Symbol(pin.symbol_id))),
            Endpoint::Junction(id) => junction_of
                .get(id)
                .map(|id| Endpoint::Junction(*id))
                .ok_or(CommandError::Missing(Subject::Junction(*id))),
        }
    };
    for wire in &fragment.wires {
        let route = wire
            .route
            .iter()
            .map(|point| shift(*point))
            .collect::<Result<Vec<_>, _>>()?;
        edits.push(Edit::InsertWire(Wire {
            id: WireId(ids.alloc()?),
            from: endpoint(&wire.from)?,
            to: endpoint(&wire.to)?,
            route,
            number: None,
            layer: keep_layer(doc, wire.layer),
        }));
    }

    let mut tx = Transaction::new(format!("Paste {} items", fragment.len()), ids.next_id());
    tx.edits = edits;
    Ok(tx)
}

/// Copy the elements `subjects` name and paste them shifted by `offset`, as
/// one transaction.
///
/// # Errors
///
/// As [`paste`].
pub fn duplicate(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    subjects: &BTreeSet<Subject>,
    offset: GridPoint,
) -> Result<Transaction, CommandError> {
    let fragment = copy(doc, subjects);
    let mut tx = paste(doc, catalog, &fragment, offset)?;
    tx.label = format!("Duplicate {} items", fragment.len());
    Ok(tx)
}

/// Whether pasted symbols stay with their device (see the module
/// documentation).
fn keeps_device(
    doc: &Schematic,
    catalog: &ComponentCatalog,
    device: &Device,
    pasted: &[&SymbolInstance],
) -> bool {
    let Some(existing) = doc
        .devices
        .iter()
        .find(|d| d.id == device.id && d.model == device.model)
    else {
        return false;
    };
    let Some(model) = catalog.model(&existing.model) else {
        return false;
    };
    let own: Vec<&SymbolInstance> = doc
        .symbols
        .iter()
        .filter(|symbol| symbol.device_id == existing.id)
        .collect();
    let pasted_ids: BTreeSet<SymbolId> = pasted.iter().map(|symbol| symbol.id).collect();
    if own.iter().all(|symbol| pasted_ids.contains(&symbol.id)) {
        return false;
    }
    let mut wanted: BTreeMap<&SymbolKey, usize> = BTreeMap::new();
    for symbol in pasted {
        let count = wanted.entry(&symbol.symbol_key).or_default();
        *count = count.saturating_add(1);
    }
    wanted.iter().all(|(key, count)| {
        model.symbol(key).is_some_and(|definition| {
            let present = own.iter().filter(|s| &&s.symbol_key == key).count();
            definition.slotted()
                && present.saturating_add(*count) <= usize::from(definition.max_instances())
        })
    })
}

/// The first `prefix` + number not in `taken`.
fn free_designator(
    taken: &BTreeSet<String>,
    prefix: &str,
) -> Result<ReferenceDesignator, CommandError> {
    // With n designators taken, one of the numbers 1..=n+1 is free.
    let last = u64::try_from(taken.len())
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    (1..=last)
        .map(|n| format!("{prefix}{n}"))
        .find(|candidate| !taken.contains(candidate))
        .map(ReferenceDesignator)
        .ok_or_else(|| CommandError::InvalidReference(prefix.to_owned()))
}
