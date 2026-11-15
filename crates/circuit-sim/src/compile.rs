//! Topology compilation: schematic plus catalog in, executable circuit out.
//!
//! The pipeline is: validate structure, build the pin table, join pins and
//! junctions into copper nets, resolve devices and signals, assemble switches,
//! sources and loads, run the static checks, emit.
//!
//! Everything is processed in stable-ID order and every result is sorted, so
//! the output is identical for any ordering of the document collections.

use std::collections::{BTreeMap, BTreeSet};

use circuit_model::catalog::{
    PARAM_CONDUCTOR, PARAM_CONNECTION, PARAM_FIRST, PARAM_INITIAL_CLOSED, PARAM_PRESET_MS,
    PARAM_RATED_VOLTAGE_MV, PARAM_SECOND, PARAM_SUPPLY,
};
use circuit_model::{
    ComponentCatalog, ComponentModel, Conductor, ConductorSet, Connection, Device, DeviceClass,
    DeviceId, Diagnostic, DiagnosticCode, Endpoint, JunctionId, LayoutIndex, PhaseSequence, PinRef,
    Schematic, Subject, SupplyId, SupplyKind, SupplyProfile, SymbolDefinition, SymbolId,
    SymbolInstance, SymbolKey, SymbolKind, TimerMode, has_errors, is_orthogonal, sort_diagnostics,
};

use crate::compiled::{
    CompiledCircuit, CompiledDevice, CompiledLoad, CompiledPin, CompiledSource, CompiledSupply,
    CompiledSwitch, DeviceIx, Drive, Level, LoadIx, LoadRule, NetIx, SigIx, SignalDef, SignalKind,
    conflict_diagnostic,
};
use crate::fingerprint::{Fingerprint, Fnv64};
use crate::union_find::UnionFind;

/// Result of compiling a schematic.
///
/// Warnings survive a successful compilation and the diagnostics are complete
/// even when compilation fails, which is why this is not a `Result`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompileOutput {
    /// `Some` exactly when no diagnostic has error severity.
    pub circuit: Option<CompiledCircuit>,
    /// Every diagnostic, warnings included, in canonical order.
    pub diagnostics: Vec<Diagnostic>,
}

impl CompileOutput {
    /// Whether any diagnostic has error severity.
    #[must_use]
    pub fn has_errors(&self) -> bool {
        has_errors(&self.diagnostics)
    }
}

/// Compile a schematic against a component catalog.
///
/// The structural phase runs to completion. If it finds an error the later
/// phases, whose tables it would invalidate, do not run. Otherwise every
/// later check runs and accumulates its diagnostics.
#[must_use]
pub fn compile(schematic: &Schematic, catalog: &ComponentCatalog) -> CompileOutput {
    let mut diagnostics = schematic.validate();
    let resolved = resolve(schematic, catalog, &mut diagnostics);
    report_non_orthogonal_wires(schematic, catalog, &mut diagnostics);

    let circuit = if has_errors(&diagnostics) {
        None
    } else {
        build(schematic, &resolved, &mut diagnostics)
    };

    sort_diagnostics(&mut diagnostics);
    let circuit = if has_errors(&diagnostics) {
        None
    } else {
        circuit
    };
    CompileOutput {
        circuit,
        diagnostics,
    }
}

/// Warn about every wire with a segment that is neither horizontal nor
/// vertical. Geometry has no electrical meaning, so this never blocks the
/// compilation; a wire whose ends do not resolve is reported elsewhere.
fn report_non_orthogonal_wires(
    schematic: &Schematic,
    catalog: &ComponentCatalog,
    out: &mut Vec<Diagnostic>,
) {
    let layout = LayoutIndex::new(schematic, catalog);
    for wire in &schematic.wires {
        let Some(path) = layout.wire_polyline(wire) else {
            continue;
        };
        if !is_orthogonal(&path) {
            out.push(Diagnostic::new(
                DiagnosticCode::NonOrthogonalWire,
                Subject::Wire(wire.id),
                format!(
                    "wire {} has a segment that is neither horizontal nor vertical",
                    wire.id
                ),
            ));
        }
    }
}

struct ResolvedDevice<'a> {
    device: &'a Device,
    model: &'a ComponentModel,
}

struct ResolvedSymbol<'a> {
    instance: &'a SymbolInstance,
    /// Position of the owning device in `Resolved::devices`.
    device: usize,
    definition: &'a SymbolDefinition,
}

/// Devices and symbols matched with their catalog definitions, sorted by ID.
/// Complete only when the structural phase reported no error.
struct Resolved<'a> {
    devices: Vec<ResolvedDevice<'a>>,
    symbols: Vec<ResolvedSymbol<'a>>,
}

/// Structural checks that need the catalog: models, parameters, symbol keys,
/// slots, instance limits and terminal keys.
fn resolve<'a>(
    doc: &'a Schematic,
    catalog: &'a ComponentCatalog,
    out: &mut Vec<Diagnostic>,
) -> Resolved<'a> {
    let supplies: BTreeMap<SupplyId, &SupplyProfile> =
        doc.supplies.iter().map(|s| (s.id, s)).collect();

    let mut sorted_devices: Vec<&Device> = doc.devices.iter().collect();
    sorted_devices.sort_by_key(|device| device.id);
    let mut devices = Vec::with_capacity(sorted_devices.len());
    let mut device_position: BTreeMap<DeviceId, usize> = BTreeMap::new();
    for device in sorted_devices {
        let Some(model) = catalog.model(&device.model) else {
            out.push(Diagnostic::new(
                DiagnosticCode::UnknownModel,
                Subject::Device(device.id),
                format!(
                    "model `{}` version {} is not in the catalog",
                    device.model.id, device.model.version
                ),
            ));
            continue;
        };
        let problems = model.check_params(&device.params);
        if problems.is_empty() {
            check_supply_terminal(device, model, &supplies, out);
        }
        for problem in problems {
            out.push(Diagnostic::new(
                DiagnosticCode::InvalidParameter,
                Subject::Device(device.id),
                problem,
            ));
        }
        device_position.entry(device.id).or_insert(devices.len());
        devices.push(ResolvedDevice { device, model });
    }
    check_interlocks(&devices, out);

    let mut sorted_symbols: Vec<&SymbolInstance> = doc.symbols.iter().collect();
    sorted_symbols.sort_by_key(|symbol| symbol.id);
    let mut symbols = Vec::with_capacity(sorted_symbols.len());
    let mut definitions: BTreeMap<SymbolId, &SymbolDefinition> = BTreeMap::new();
    let mut instances: BTreeMap<(DeviceId, &SymbolKey), u32> = BTreeMap::new();
    let mut slots: BTreeMap<(DeviceId, &SymbolKey, u8), Vec<SymbolId>> = BTreeMap::new();
    for instance in sorted_symbols {
        // A symbol of a missing device or of an unknown model was reported
        // already; nothing more can be said about it.
        let Some(&device) = device_position.get(&instance.device_id) else {
            continue;
        };
        let Some(owner) = devices.get(device) else {
            continue;
        };
        let Some(definition) = owner.model.symbol(&instance.symbol_key) else {
            out.push(Diagnostic::new(
                DiagnosticCode::UnknownSymbolKey,
                Subject::Symbol(instance.id),
                format!(
                    "model `{}` has no symbol `{}`",
                    owner.model.id(),
                    instance.symbol_key.0
                ),
            ));
            continue;
        };

        match (definition.slotted(), instance.slot) {
            (true, Some(slot)) if slot >= 1 => {
                slots
                    .entry((instance.device_id, definition.key(), slot))
                    .or_default()
                    .push(instance.id);
            }
            (true, _) => out.push(Diagnostic::new(
                DiagnosticCode::SymbolInstanceLimit,
                Subject::Symbol(instance.id),
                format!(
                    "symbol `{}` needs a slot of 1 or more",
                    instance.symbol_key.0
                ),
            )),
            (false, Some(_)) => out.push(Diagnostic::new(
                DiagnosticCode::SymbolInstanceLimit,
                Subject::Symbol(instance.id),
                format!("symbol `{}` does not take a slot", instance.symbol_key.0),
            )),
            (false, None) => {}
        }

        let count = instances
            .entry((instance.device_id, definition.key()))
            .or_insert(0);
        *count = count.saturating_add(1);
        definitions.entry(instance.id).or_insert(definition);
        symbols.push(ResolvedSymbol {
            instance,
            device,
            definition,
        });
    }

    for sharing in slots.into_values() {
        let mut sharing = sharing.into_iter();
        let (Some(first), Some(second)) = (sharing.next(), sharing.next()) else {
            continue;
        };
        let mut related = vec![Subject::Symbol(second)];
        related.extend(sharing.map(Subject::Symbol));
        out.push(
            Diagnostic::new(
                DiagnosticCode::SymbolInstanceLimit,
                Subject::Symbol(first),
                "slot is used by more than one instance of the same symbol",
            )
            .with_related(related),
        );
    }

    for resolved in &devices {
        for definition in resolved.model.symbols() {
            let count = instances
                .get(&(resolved.device.id, definition.key()))
                .copied()
                .unwrap_or(0);
            let (min, max) = (
                u32::from(definition.min_instances()),
                u32::from(definition.max_instances()),
            );
            if count < min || count > max {
                out.push(Diagnostic::new(
                    DiagnosticCode::SymbolInstanceLimit,
                    Subject::Device(resolved.device.id),
                    format!(
                        "symbol `{}` needs between {min} and {max} instances, found {count}",
                        definition.key().0
                    ),
                ));
            }
        }
    }

    for wire in &doc.wires {
        for endpoint in [&wire.from, &wire.to] {
            let Endpoint::Terminal(pin) = endpoint else {
                continue;
            };
            let Some(definition) = definitions.get(&pin.symbol_id) else {
                continue;
            };
            if definition.terminal_index(&pin.terminal).is_none() {
                out.push(
                    Diagnostic::new(
                        DiagnosticCode::UnknownTerminal,
                        Subject::Wire(wire.id),
                        format!(
                            "symbol `{}` has no terminal `{}`",
                            definition.key().0,
                            pin.terminal.0
                        ),
                    )
                    .with_related(vec![Subject::Pin(pin.clone())]),
                );
            }
        }
    }

    Resolved { devices, symbols }
}

/// A supply terminal must name an existing supply and a conductor that this
/// kind of supply provides.
fn check_supply_terminal(
    device: &Device,
    model: &ComponentModel,
    supplies: &BTreeMap<SupplyId, &SupplyProfile>,
    out: &mut Vec<Diagnostic>,
) {
    if model.class() != DeviceClass::PowerSupply {
        return;
    }
    let Some(supply) = supply_param(device, model) else {
        return;
    };
    let Some(profile) = supplies.get(&supply) else {
        out.push(
            Diagnostic::new(
                DiagnosticCode::DanglingReference,
                Subject::Device(device.id),
                format!("supply {supply} does not exist"),
            )
            .with_related(vec![Subject::Supply(supply)]),
        );
        return;
    };
    let Some(conductor) = conductor_param(device, model) else {
        return;
    };
    if !profile.kind.provides(conductor) {
        out.push(
            Diagnostic::new(
                DiagnosticCode::InvalidParameter,
                Subject::Device(device.id),
                format!(
                    "supply {supply} is {} and does not provide conductor `{conductor}`",
                    supply_kind_name(profile.kind)
                ),
            )
            .with_related(vec![Subject::Supply(supply)]),
        );
    }
}

const fn supply_kind_name(kind: SupplyKind) -> &'static str {
    match kind {
        SupplyKind::Dc => "a DC supply",
        SupplyKind::SinglePhase => "a single-phase supply",
        SupplyKind::ThreePhase => "a three-phase supply",
    }
}

/// The two contactors a mechanical interlock names, when both parameters
/// are set.
fn interlock_params(device: &Device, model: &ComponentModel) -> Option<(DeviceId, DeviceId)> {
    let id = |key| {
        let raw = model.int_param(&device.params, key)?;
        u64::try_from(raw).ok().map(DeviceId)
    };
    Some((id(PARAM_FIRST)?, id(PARAM_SECOND)?))
}

/// Every mechanical interlock must name two distinct contactors of the
/// document, and a contactor may belong to one interlock only.
fn check_interlocks(devices: &[ResolvedDevice<'_>], out: &mut Vec<Diagnostic>) {
    let class_of: BTreeMap<DeviceId, DeviceClass> = devices
        .iter()
        .map(|resolved| (resolved.device.id, resolved.model.class()))
        .collect();
    let mut owner_of: BTreeMap<DeviceId, DeviceId> = BTreeMap::new();
    for ResolvedDevice { device, model } in devices {
        if model.class() != DeviceClass::MechanicalInterlock {
            continue;
        }
        // Missing or ill-typed parameters were reported as InvalidParameter.
        let Some((first, second)) = interlock_params(device, model) else {
            continue;
        };
        let report = |message: String, related: Vec<Subject>, out: &mut Vec<Diagnostic>| {
            out.push(
                Diagnostic::new(
                    DiagnosticCode::InvalidInterlock,
                    Subject::Device(device.id),
                    message,
                )
                .with_related(related),
            );
        };
        if first == second {
            report(
                format!("the interlock names device {first} twice"),
                vec![Subject::Device(first)],
                out,
            );
            continue;
        }
        let mut valid = true;
        for target in [first, second] {
            if class_of.get(&target) != Some(&DeviceClass::Contactor) {
                valid = false;
                report(
                    format!("device {target} is not a contactor of the document"),
                    vec![Subject::Device(target)],
                    out,
                );
            }
        }
        if !valid {
            continue;
        }
        for target in [first, second] {
            if let Some(other) = owner_of.get(&target) {
                report(
                    format!("contactor {target} already belongs to interlock {other}"),
                    vec![Subject::Device(target), Subject::Device(*other)],
                    out,
                );
            } else {
                owner_of.insert(target, device.id);
            }
        }
    }
}

fn supply_param(device: &Device, model: &ComponentModel) -> Option<SupplyId> {
    let raw = model.int_param(&device.params, PARAM_SUPPLY)?;
    u64::try_from(raw).ok().map(SupplyId)
}

fn conductor_param(device: &Device, model: &ComponentModel) -> Option<Conductor> {
    model
        .text_param(&device.params, PARAM_CONDUCTOR)
        .and_then(Conductor::from_key)
}

fn rated_voltage_param(device: &Device, model: &ComponentModel) -> Option<u32> {
    let raw = model.int_param(&device.params, PARAM_RATED_VOLTAGE_MV)?;
    u32::try_from(raw).ok()
}

fn connection_param(device: &Device, model: &ComponentModel) -> Option<Connection> {
    model
        .text_param(&device.params, PARAM_CONNECTION)
        .and_then(Connection::from_key)
}

/// The signal a class owns. Contacts and main contacts of a device are driven
/// by this signal.
const fn signal_kind(class: DeviceClass) -> Option<SignalKind> {
    match class {
        DeviceClass::Contactor | DeviceClass::Relay => Some(SignalKind::CoilActive),
        DeviceClass::Timer(_) => Some(SignalKind::TimerOut),
        DeviceClass::Pushbutton | DeviceClass::LatchingSwitch => Some(SignalKind::Actuated),
        DeviceClass::ThermalOverload => Some(SignalKind::Tripped),
        DeviceClass::Fuse
        | DeviceClass::Motor
        | DeviceClass::Lamp
        | DeviceClass::Buzzer
        | DeviceClass::PowerSupply
        | DeviceClass::MechanicalInterlock => None,
    }
}

fn too_large(out: &mut Vec<Diagnostic>) {
    out.push(Diagnostic::new(
        DiagnosticCode::CircuitTooLarge,
        Subject::Document,
        "the document has more elements than the compiler can index",
    ));
}

fn dense(len: usize, out: &mut Vec<Diagnostic>) -> Option<u32> {
    let value = u32::try_from(len).ok();
    if value.is_none() {
        too_large(out);
    }
    value
}

/// Phases 2 to 7. Runs only on a document without structural errors, so every
/// lookup is expected to succeed; a lookup that fails anyway skips the element
/// instead of panicking.
#[expect(
    clippy::too_many_lines,
    reason = "the phases share the pin, net and device tables; splitting them would only pass the same tables around"
)]
fn build(
    doc: &Schematic,
    resolved: &Resolved<'_>,
    out: &mut Vec<Diagnostic>,
) -> Option<CompiledCircuit> {
    // Phase 2: pin table, in (symbol id, terminal) order.
    let mut pins: Vec<CompiledPin> = Vec::new();
    let mut first_pin: Vec<u32> = Vec::with_capacity(resolved.symbols.len());
    let mut symbol_position: BTreeMap<SymbolId, usize> = BTreeMap::new();
    for (position, symbol) in resolved.symbols.iter().enumerate() {
        first_pin.push(dense(pins.len(), out)?);
        symbol_position.insert(symbol.instance.id, position);
        for terminal in symbol.definition.terminals() {
            pins.push(CompiledPin {
                pin: PinRef {
                    symbol_id: symbol.instance.id,
                    terminal: terminal.key.clone(),
                },
                net: NetIx(0),
            });
        }
    }
    let pin_count = dense(pins.len(), out)?;

    let mut junction_ids: Vec<JunctionId> = doc.junctions.iter().map(|j| j.id).collect();
    junction_ids.sort_unstable();
    let junction_count = dense(junction_ids.len(), out)?;
    let junction_position: BTreeMap<JunctionId, u32> =
        junction_ids.iter().copied().zip(0_u32..).collect();
    let Some(node_count) = pin_count.checked_add(junction_count) else {
        too_large(out);
        return None;
    };

    let node_of = |endpoint: &Endpoint| -> Option<u32> {
        match endpoint {
            Endpoint::Terminal(pin) => {
                let position = *symbol_position.get(&pin.symbol_id)?;
                let symbol = resolved.symbols.get(position)?;
                let terminal = symbol.definition.terminal_index(&pin.terminal)?;
                first_pin
                    .get(position)?
                    .checked_add(u32::try_from(terminal).ok()?)
            }
            Endpoint::Junction(junction) => {
                pin_count.checked_add(*junction_position.get(junction)?)
            }
        }
    };

    // Phase 3: copper nets. Pins and junctions joined by wires.
    let mut sorted_wires: Vec<_> = doc.wires.iter().collect();
    sorted_wires.sort_by_key(|wire| wire.id);
    let mut copper = UnionFind::new(node_count);
    let mut pin_degree = vec![0_u32; pins.len()];
    let mut wire_nodes = Vec::with_capacity(sorted_wires.len());
    for wire in &sorted_wires {
        let (Some(from), Some(to)) = (node_of(&wire.from), node_of(&wire.to)) else {
            continue;
        };
        copper.union(from, to);
        for node in [from, to] {
            if let Some(degree) = pin_degree.get_mut(node as usize) {
                *degree = degree.saturating_add(1);
            }
        }
        wire_nodes.push((wire.id, from));
    }

    // The representative of a set is its smallest node, and nodes are visited
    // in ascending order, so a net is numbered by its smallest pin.
    let mut net_of_node: Vec<u32> = Vec::with_capacity(node_count as usize);
    let mut net_count = 0_u32;
    for node in 0..node_count {
        let root = copper.find(node);
        let net = if root == node {
            let net = net_count;
            net_count = net_count.saturating_add(1);
            net
        } else {
            net_of_node.get(root as usize).copied().unwrap_or(0)
        };
        net_of_node.push(net);
    }
    let net_of = |node: u32| net_of_node.get(node as usize).copied().map(NetIx);
    for (pin, net) in pins.iter_mut().zip(net_of_node.iter()) {
        pin.net = NetIx(*net);
    }

    // Phase 4: devices and their signals.
    let mut devices: Vec<CompiledDevice> = Vec::with_capacity(resolved.devices.len());
    let mut signals: Vec<SignalDef> = Vec::new();
    for (position, resolved_device) in resolved.devices.iter().enumerate() {
        let ResolvedDevice { device, model } = resolved_device;
        let index = DeviceIx(dense(position, out)?);
        let signal = match signal_kind(model.class()) {
            Some(kind) => {
                let sig = SigIx(dense(signals.len(), out)?);
                signals.push(SignalDef {
                    owner: device.id,
                    device: index,
                    kind,
                });
                Some(sig)
            }
            None => None,
        };
        devices.push(CompiledDevice {
            id: device.id,
            reference: device.reference.clone(),
            class: model.class(),
            signal,
            coil: None,
            preset_ms: model
                .int_param(&device.params, PARAM_PRESET_MS)
                .and_then(|raw| u32::try_from(raw).ok()),
            initial_closed: model
                .bool_param(&device.params, PARAM_INITIAL_CLOSED)
                .unwrap_or(false),
        });
    }

    // Mechanical interlocks, checked in the structural phase.
    let mut interlocks: Vec<(DeviceIx, DeviceIx)> = Vec::new();
    for ResolvedDevice { device, model } in &resolved.devices {
        if model.class() != DeviceClass::MechanicalInterlock {
            continue;
        }
        let Some((first, second)) = interlock_params(device, model) else {
            continue;
        };
        let index = |id: DeviceId| {
            let position = devices.binary_search_by_key(&id, |d| d.id).ok()?;
            u32::try_from(position).ok().map(DeviceIx)
        };
        if let (Some(a), Some(b)) = (index(first), index(second)) {
            interlocks.push((a.min(b), a.max(b)));
        }
    }
    interlocks.sort_unstable();

    // Phase 5: switches, sources and loads, in symbol order.
    let mut switches: Vec<CompiledSwitch> = Vec::new();
    let mut sources: Vec<CompiledSource> = Vec::new();
    let mut loads: Vec<CompiledLoad> = Vec::new();
    let mut symbols_of_device = vec![0_u32; devices.len()];
    for (position, symbol) in resolved.symbols.iter().enumerate() {
        let Some(owner) = resolved.devices.get(symbol.device) else {
            continue;
        };
        let (Some(&base), Some(device_ix)) = (
            first_pin.get(position),
            u32::try_from(symbol.device).ok().map(DeviceIx),
        ) else {
            continue;
        };
        if let Some(count) = symbols_of_device.get_mut(symbol.device) {
            *count = count.saturating_add(1);
        }
        let terminal_net = |terminal: u32| base.checked_add(terminal).and_then(net_of);
        let signal = devices.get(symbol.device).and_then(|device| device.signal);
        let id = symbol.instance.id;

        match symbol.definition.kind() {
            kind @ (SymbolKind::Coil | SymbolKind::Lamp | SymbolKind::Buzzer) => {
                let (Some(a), Some(b), Some(connection), Some(rated_voltage_mv)) = (
                    terminal_net(0),
                    terminal_net(1),
                    connection_param(owner.device, owner.model),
                    rated_voltage_param(owner.device, owner.model),
                ) else {
                    continue;
                };
                let load = LoadIx(dense(loads.len(), out)?);
                loads.push(CompiledLoad {
                    device: device_ix,
                    symbol: id,
                    symbol_key: symbol.definition.key().clone(),
                    rule: LoadRule::TwoTerminal {
                        connection,
                        rated_voltage_mv,
                    },
                    nets: vec![a, b],
                });
                if kind == SymbolKind::Coil {
                    if let Some(device) = devices.get_mut(symbol.device) {
                        device.coil = Some(load);
                    }
                }
            }
            SymbolKind::Motor => {
                let (Some(u), Some(v), Some(w), Some(rated_voltage_mv)) = (
                    terminal_net(0),
                    terminal_net(1),
                    terminal_net(2),
                    rated_voltage_param(owner.device, owner.model),
                ) else {
                    continue;
                };
                loads.push(CompiledLoad {
                    device: device_ix,
                    symbol: id,
                    symbol_key: symbol.definition.key().clone(),
                    rule: LoadRule::ThreePhaseMotor { rated_voltage_mv },
                    nets: vec![u, v, w],
                });
            }
            kind @ (SymbolKind::ContactNo | SymbolKind::ContactNc) => {
                let (Some(a), Some(b), Some(sig)) = (terminal_net(0), terminal_net(1), signal)
                else {
                    continue;
                };
                let closed_when = if kind == SymbolKind::ContactNo {
                    Level::High
                } else {
                    Level::Low
                };
                switches.push(CompiledSwitch {
                    a,
                    b,
                    drive: Drive::Signal { sig, closed_when },
                    owner: id,
                });
            }
            SymbolKind::Changeover => {
                // Two switches on one signal: `com`-`nc` at rest, `com`-`no`
                // while the signal is high. The `nc` side comes first, in
                // terminal order.
                let Some(sig) = signal else {
                    continue;
                };
                let Some(common) = terminal_net(0) else {
                    continue;
                };
                for (terminal, closed_when) in [(1, Level::Low), (2, Level::High)] {
                    let Some(side) = terminal_net(terminal) else {
                        continue;
                    };
                    switches.push(CompiledSwitch {
                        a: common,
                        b: side,
                        drive: Drive::Signal { sig, closed_when },
                        owner: id,
                    });
                }
            }
            SymbolKind::PowerContacts { poles } => {
                let Some(sig) = signal else {
                    continue;
                };
                for pole in 0..u32::from(poles) {
                    let first = pole.saturating_mul(2);
                    let (Some(a), Some(b)) =
                        (terminal_net(first), terminal_net(first.saturating_add(1)))
                    else {
                        continue;
                    };
                    switches.push(CompiledSwitch {
                        a,
                        b,
                        drive: Drive::Signal {
                            sig,
                            closed_when: Level::High,
                        },
                        owner: id,
                    });
                }
            }
            SymbolKind::ThermalElement { poles } => {
                for pole in 0..u32::from(poles) {
                    let first = pole.saturating_mul(2);
                    let (Some(a), Some(b)) =
                        (terminal_net(first), terminal_net(first.saturating_add(1)))
                    else {
                        continue;
                    };
                    switches.push(CompiledSwitch {
                        a,
                        b,
                        drive: Drive::Always,
                        owner: id,
                    });
                }
            }
            SymbolKind::FuseLink => {
                let (Some(a), Some(b)) = (terminal_net(0), terminal_net(1)) else {
                    continue;
                };
                switches.push(CompiledSwitch {
                    a,
                    b,
                    drive: Drive::Always,
                    owner: id,
                });
            }
            // A mark on the drawing; the pair is compiled from the device.
            SymbolKind::InterlockLink => {}
            SymbolKind::SupplyTerminal => {
                let (Some(net), Some(conductor), Some(supply)) = (
                    terminal_net(0),
                    conductor_param(owner.device, owner.model),
                    supply_param(owner.device, owner.model),
                ) else {
                    continue;
                };
                sources.push(CompiledSource {
                    net,
                    conductor,
                    supply,
                    owner: owner.device.id,
                    symbol: id,
                });
            }
        }
    }
    dense(switches.len(), out)?;
    dense(loads.len(), out)?;

    // Phase 6: static checks.
    for (position, symbol) in resolved.symbols.iter().enumerate() {
        let Some(&base) = first_pin.get(position) else {
            continue;
        };
        for (terminal, offset) in symbol.definition.terminals().iter().zip(0_u32..) {
            let unconnected = base
                .checked_add(offset)
                .and_then(|node| pin_degree.get(node as usize))
                .is_some_and(|degree| *degree == 0);
            if terminal.required && unconnected {
                out.push(Diagnostic::new(
                    DiagnosticCode::MissingRequiredTerminal,
                    Subject::Pin(PinRef {
                        symbol_id: symbol.instance.id,
                        terminal: terminal.key.clone(),
                    }),
                    format!("terminal `{}` must be connected", terminal.key.0),
                ));
            }
        }
    }

    for (device, count) in devices.iter().zip(symbols_of_device.iter()) {
        if *count == 0 {
            out.push(Diagnostic::new(
                DiagnosticCode::DeviceWithoutSymbols,
                Subject::Device(device.id),
                "device has no symbol",
            ));
        }
    }

    check_conflicting_supplies(&sources, out);

    if !loads.is_empty() && sources.is_empty() {
        out.push(Diagnostic::new(
            DiagnosticCode::NoSupply,
            Subject::Document,
            "the document has loads and no supply",
        ));
    }

    // Permanent conflicts: only the always-closed switches conduct.
    let mut permanent = UnionFind::new(net_count);
    for switch in &switches {
        if switch.drive == Drive::Always {
            permanent.union(switch.a.0, switch.b.0);
        }
    }
    let mut permanent_groups: BTreeMap<u32, (ConductorSet, Vec<DeviceId>)> = BTreeMap::new();
    for source in &sources {
        let group = permanent_groups
            .entry(permanent.find(source.net.0))
            .or_default();
        group.0.insert(source.conductor);
        group.1.push(source.owner);
    }
    for (conductors, owners) in permanent_groups.into_values() {
        out.extend(conflict_diagnostic(conductors, &owners));
    }

    // Reachability with every switch closed.
    let mut closed = UnionFind::new(net_count);
    for switch in &switches {
        closed.union(switch.a.0, switch.b.0);
    }
    let fed: BTreeSet<u32> = sources.iter().map(|s| closed.find(s.net.0)).collect();
    let loaded: BTreeSet<u32> = loads
        .iter()
        .flat_map(|load| load.nets.iter())
        .map(|net| closed.find(net.0))
        .collect();

    let mut unused: BTreeMap<DeviceId, bool> = BTreeMap::new();
    for source in &sources {
        let reaches_load = loaded.contains(&closed.find(source.net.0));
        let entry = unused.entry(source.owner).or_insert(true);
        *entry = *entry && !reaches_load;
    }
    for (device, is_unused) in unused {
        if is_unused {
            out.push(Diagnostic::new(
                DiagnosticCode::UnusedSupply,
                Subject::Device(device),
                "supply terminal reaches no load",
            ));
        }
    }

    if !sources.is_empty() {
        report_floating_nets(
            &pins,
            &junction_ids,
            &net_of_node,
            net_count,
            &mut closed,
            &fed,
            out,
        );
    }

    report_static_series_loads(&switches, &sources, &loads, &devices, net_count, out);

    // Phase 7: emit.
    let supplies: Vec<CompiledSupply> = {
        let mut supplies: Vec<CompiledSupply> = doc
            .supplies
            .iter()
            .map(|supply| CompiledSupply {
                id: supply.id,
                kind: supply.kind,
                nominal_voltage_mv: supply.nominal_voltage_mv,
                phase_voltage_mv: supply.phase_voltage_mv,
                phase_sequence: supply.phase_sequence,
            })
            .collect();
        supplies.sort_by_key(|supply| supply.id);
        supplies
    };
    let wires = wire_nodes
        .into_iter()
        .filter_map(|(wire, node)| net_of(node).map(|net| (wire, net)))
        .collect();
    let junctions = junction_ids
        .iter()
        .zip(0_u32..)
        .filter_map(|(junction, offset)| {
            pin_count
                .checked_add(offset)
                .and_then(net_of)
                .map(|net| (*junction, net))
        })
        .collect();

    let mut circuit = CompiledCircuit {
        pins,
        net_count,
        switches,
        sources,
        loads,
        signals,
        devices,
        supplies,
        wires,
        junctions,
        interlocks,
        fingerprint: Fingerprint(0),
    };
    circuit.fingerprint = fingerprint(&circuit);
    Some(circuit)
}

/// Each conductor may be provided by one supply profile only, so that the
/// solver can carry conductor bits without losing where the voltage label
/// comes from.
fn check_conflicting_supplies(sources: &[CompiledSource], out: &mut Vec<Diagnostic>) {
    let mut providers: BTreeMap<Conductor, BTreeMap<SupplyId, BTreeSet<DeviceId>>> =
        BTreeMap::new();
    for source in sources {
        providers
            .entry(source.conductor)
            .or_default()
            .entry(source.supply)
            .or_default()
            .insert(source.owner);
    }
    for (conductor, by_supply) in providers {
        if by_supply.len() < 2 {
            continue;
        }
        let mut devices: BTreeSet<DeviceId> = BTreeSet::new();
        let mut related: Vec<Subject> = Vec::new();
        for (supply, owners) in by_supply {
            related.push(Subject::Supply(supply));
            devices.extend(owners);
        }
        let mut devices = devices.into_iter();
        let Some(first) = devices.next() else {
            continue;
        };
        related.extend(devices.map(Subject::Device));
        out.push(
            Diagnostic::new(
                DiagnosticCode::ConflictingSupplies,
                Subject::Device(first),
                format!("conductor `{conductor}` is provided by more than one supply"),
            )
            .with_related(related),
        );
    }
}

/// A net with two or more points that no supply reaches even with every
/// switch closed.
fn report_floating_nets(
    pins: &[CompiledPin],
    junction_ids: &[JunctionId],
    net_of_node: &[u32],
    net_count: u32,
    closed: &mut UnionFind,
    fed: &BTreeSet<u32>,
    out: &mut Vec<Diagnostic>,
) {
    let mut points = vec![0_u32; net_count as usize];
    for net in net_of_node {
        if let Some(count) = points.get_mut(*net as usize) {
            *count = count.saturating_add(1);
        }
    }
    let floating: BTreeSet<u32> = (0..net_count)
        .filter(|net| {
            points.get(*net as usize).is_some_and(|count| *count >= 2)
                && !fed.contains(&closed.find(*net))
        })
        .collect();
    if floating.is_empty() {
        return;
    }

    let mut members: BTreeMap<u32, Vec<Subject>> = BTreeMap::new();
    for pin in pins {
        if floating.contains(&pin.net.0) {
            members
                .entry(pin.net.0)
                .or_default()
                .push(Subject::Pin(pin.pin.clone()));
        }
    }
    let junction_nets = net_of_node.iter().skip(pins.len());
    for (junction, net) in junction_ids.iter().zip(junction_nets) {
        if floating.contains(net) {
            members
                .entry(*net)
                .or_default()
                .push(Subject::Junction(*junction));
        }
    }
    for mut subjects in members.into_values() {
        subjects.sort();
        let mut subjects = subjects.into_iter();
        let Some(first) = subjects.next() else {
            continue;
        };
        out.push(
            Diagnostic::new(
                DiagnosticCode::FloatingNet,
                first,
                "net cannot be reached by any supply, even with every switch closed",
            )
            .with_related(subjects.collect()),
        );
    }
}

/// A copper net that touches the terminals of two or more loads and no supply
/// or switch terminal can only be fed through a load: the loads are
/// permanently in series, which the discrete solver does not represent.
fn report_static_series_loads(
    switches: &[CompiledSwitch],
    sources: &[CompiledSource],
    loads: &[CompiledLoad],
    devices: &[CompiledDevice],
    net_count: u32,
    out: &mut Vec<Diagnostic>,
) {
    let mut excluded = vec![false; net_count as usize];
    let switch_nets = switches.iter().flat_map(|switch| [switch.a, switch.b]);
    for net in switch_nets.chain(sources.iter().map(|source| source.net)) {
        if let Some(flag) = excluded.get_mut(net.index()) {
            *flag = true;
        }
    }

    let mut touching: BTreeMap<u32, BTreeSet<DeviceId>> = BTreeMap::new();
    let mut loads_on_net: BTreeMap<u32, BTreeSet<usize>> = BTreeMap::new();
    for (index, load) in loads.iter().enumerate() {
        let Some(device) = devices.get(load.device.index()) else {
            continue;
        };
        for net in &load.nets {
            if excluded.get(net.index()).copied().unwrap_or(true) {
                continue;
            }
            touching.entry(net.0).or_default().insert(device.id);
            loads_on_net.entry(net.0).or_default().insert(index);
        }
    }
    for (net, owners) in touching {
        let distinct_loads = loads_on_net.get(&net).map_or(0, BTreeSet::len);
        if distinct_loads < 2 {
            continue;
        }
        let mut owners = owners.into_iter();
        let Some(first) = owners.next() else {
            continue;
        };
        out.push(
            Diagnostic::new(
                DiagnosticCode::StaticSeriesLoads,
                Subject::Device(first),
                "loads are permanently wired in series, which the discrete model does not represent",
            )
            .with_related(owners.map(Subject::Device).collect()),
        );
    }
}

/// Hash of the canonical encoding of everything that affects execution or
/// the text of a trace. Geometry is not part of it.
fn fingerprint(circuit: &CompiledCircuit) -> Fingerprint {
    let mut hash = Fnv64::new();
    hash.str("opencircuit-compiled-v1");

    hash.u64(circuit.pins.len() as u64);
    for pin in &circuit.pins {
        hash.u64(pin.pin.symbol_id.0);
        hash.str(&pin.pin.terminal.0);
        hash.u32(pin.net.0);
    }
    hash.u32(circuit.net_count);

    hash.u64(circuit.switches.len() as u64);
    for switch in &circuit.switches {
        hash.u32(switch.a.0);
        hash.u32(switch.b.0);
        match switch.drive {
            Drive::Always => hash.u8(0),
            Drive::Signal { sig, closed_when } => {
                hash.u8(match closed_when {
                    Level::Low => 1,
                    Level::High => 2,
                });
                hash.u32(sig.0);
            }
        }
        hash.u64(switch.owner.0);
    }

    hash.u64(circuit.sources.len() as u64);
    for source in &circuit.sources {
        hash.u32(source.net.0);
        hash.str(source.conductor.key());
        hash.u64(source.supply.0);
        hash.u64(source.owner.0);
        hash.u64(source.symbol.0);
    }

    hash.u64(circuit.loads.len() as u64);
    for load in &circuit.loads {
        hash.u32(load.device.0);
        hash.u64(load.symbol.0);
        hash.str(&load.symbol_key.0);
        match load.rule {
            LoadRule::TwoTerminal {
                connection,
                rated_voltage_mv,
            } => {
                hash.u8(1);
                hash.str(connection.key());
                hash.u32(rated_voltage_mv);
            }
            LoadRule::ThreePhaseMotor { rated_voltage_mv } => {
                hash.u8(2);
                hash.u32(rated_voltage_mv);
            }
        }
        hash.u64(load.nets.len() as u64);
        for net in &load.nets {
            hash.u32(net.0);
        }
    }

    hash.u64(circuit.signals.len() as u64);
    for signal in &circuit.signals {
        hash.u64(signal.owner.0);
        hash.u8(match signal.kind {
            SignalKind::Actuated => 1,
            SignalKind::Tripped => 2,
            SignalKind::CoilActive => 3,
            SignalKind::TimerOut => 4,
        });
    }

    hash.u64(circuit.devices.len() as u64);
    for device in &circuit.devices {
        hash.u64(device.id.0);
        hash.str(&device.reference.0);
        hash.u8(match device.class {
            DeviceClass::Contactor => 1,
            DeviceClass::Relay => 2,
            DeviceClass::ThermalOverload => 3,
            DeviceClass::Timer(TimerMode::OnDelay) => 4,
            DeviceClass::Timer(TimerMode::OffDelay) => 5,
            DeviceClass::Pushbutton => 6,
            DeviceClass::LatchingSwitch => 7,
            DeviceClass::Fuse => 8,
            DeviceClass::Motor => 9,
            DeviceClass::Lamp => 10,
            DeviceClass::Buzzer => 11,
            DeviceClass::PowerSupply => 12,
            DeviceClass::MechanicalInterlock => 13,
        });
        hash.u32(device.signal.map_or(u32::MAX, |sig| sig.0));
        hash.u32(device.coil.map_or(u32::MAX, |load| load.0));
        hash.u64(device.preset_ms.map_or(u64::MAX, u64::from));
        hash.u8(u8::from(device.initial_closed));
    }

    hash.u64(circuit.supplies.len() as u64);
    for supply in &circuit.supplies {
        hash.u64(supply.id.0);
        hash.u8(match supply.kind {
            SupplyKind::Dc => 1,
            SupplyKind::SinglePhase => 2,
            SupplyKind::ThreePhase => 3,
        });
        hash.u32(supply.nominal_voltage_mv);
        hash.u64(supply.phase_voltage_mv.map_or(u64::MAX, u64::from));
        hash.u8(match supply.phase_sequence {
            None => 0,
            Some(PhaseSequence::L1L2L3) => 1,
            Some(PhaseSequence::L1L3L2) => 2,
        });
    }

    hash.u64(circuit.wires.len() as u64);
    for (wire, net) in &circuit.wires {
        hash.u64(wire.0);
        hash.u32(net.0);
    }
    hash.u64(circuit.junctions.len() as u64);
    for (junction, net) in &circuit.junctions {
        hash.u64(junction.0);
        hash.u32(net.0);
    }

    // Sections added after the first encoding are written only when they
    // are not empty, so a circuit that does not use them keeps the
    // fingerprint recorded in its goldens.
    if !circuit.interlocks.is_empty() {
        hash.str("interlocks");
        hash.u64(circuit.interlocks.len() as u64);
        for (first, second) in &circuit.interlocks {
            hash.u32(first.0);
            hash.u32(second.0);
        }
    }

    hash.finish()
}

#[cfg(test)]
#[path = "compile_tests.rs"]
mod tests;
