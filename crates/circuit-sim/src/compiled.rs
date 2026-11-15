//! Executable representation of a schematic.
//!
//! A [`CompiledCircuit`] is produced only by [`crate::compile`], so its dense
//! indices are always consistent with each other. The indices are internal:
//! documents and snapshots use stable IDs only.

use circuit_model::{
    Conductor, ConductorConflict, ConductorSet, Connection, DeviceClass, DeviceId, Diagnostic,
    DiagnosticCode, JunctionId, PhaseSequence, PinRef, ReferenceDesignator, Subject, SupplyId,
    SupplyKind, SymbolId, SymbolKey, WireId,
};

use crate::fingerprint::Fingerprint;

macro_rules! dense_index {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(pub(crate) u32);

        impl $name {
            /// Position in the corresponding table of the compiled circuit.
            #[must_use]
            pub const fn index(self) -> usize {
                self.0 as usize
            }

            /// Raw value.
            #[must_use]
            pub const fn raw(self) -> u32 {
                self.0
            }
        }
    };
}

dense_index!(
    /// A terminal of a symbol instance.
    PinIx
);
dense_index!(
    /// A copper net: pins and junctions joined by wires. Static.
    NetIx
);
dense_index!(
    /// A device signal.
    SigIx
);
dense_index!(
    /// A device.
    DeviceIx
);
dense_index!(
    /// A switch between two copper nets.
    SwitchIx
);
dense_index!(
    /// A load.
    LoadIx
);

/// A terminal and the copper net it belongs to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledPin {
    /// The terminal.
    pub pin: PinRef,
    /// Its copper net.
    pub net: NetIx,
}

/// Signal level.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Level {
    /// The signal is false.
    Low,
    /// The signal is true.
    High,
}

/// What decides whether a switch is closed.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Drive {
    /// Permanently closed: fuse link, thermal element.
    Always,
    /// Closed while the signal has the given level.
    Signal {
        /// The driving signal.
        sig: SigIx,
        /// Level at which the switch is closed.
        closed_when: Level,
    },
}

/// Element that joins two copper nets while closed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompiledSwitch {
    /// First net.
    pub a: NetIx,
    /// Second net.
    pub b: NetIx,
    /// What closes the switch.
    pub drive: Drive,
    /// The symbol this switch belongs to.
    pub owner: SymbolId,
}

/// Point where a supply conductor enters the circuit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompiledSource {
    /// Net of the supply terminal.
    pub net: NetIx,
    /// Conductor provided.
    pub conductor: Conductor,
    /// Supply profile the conductor belongs to.
    pub supply: SupplyId,
    /// The supply terminal device.
    pub owner: DeviceId,
    /// The symbol instance of the tap.
    pub symbol: SymbolId,
    /// The signal that must be high for the source to provide its
    /// conductor, for the secondary of a transformer; `None` for a supply
    /// terminal, which always provides it.
    pub enabled_by: Option<SigIx>,
}

/// Rule that decides whether a load is energized.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LoadRule {
    /// Coil, lamp or buzzer between two nets.
    TwoTerminal {
        /// How the load connects to its supply.
        connection: Connection,
        /// Rated voltage label in millivolts.
        rated_voltage_mv: u32,
    },
    /// Simplified three-phase motor on `u1`, `v1`, `w1`.
    ThreePhaseMotor {
        /// Rated line voltage label in millivolts.
        rated_voltage_mv: u32,
    },
    /// Six-terminal motor on `u1`, `v1`, `w1`, `u2`, `v2`, `w2`, connected
    /// in star or delta by the circuit.
    SixTerminalMotor {
        /// Rated line voltage label in delta, in millivolts.
        rated_voltage_mv: u32,
    },
}

/// A load and the nets of its terminals, in terminal order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledLoad {
    /// The device the load belongs to.
    pub device: DeviceIx,
    /// The symbol instance that is the load.
    pub symbol: SymbolId,
    /// Key of that symbol in its model; used to name the load in traces.
    pub symbol_key: SymbolKey,
    /// Energization rule.
    pub rule: LoadRule,
    /// Nets of the terminals.
    pub nets: Vec<NetIx>,
}

/// Meaning of a device signal. Every signal has exactly one writer.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SignalKind {
    /// Pushbutton held or latching switch closed. Written by inputs; effective
    /// in the tick of the action.
    Actuated,
    /// Thermal overload tripped. Written by inputs; effective in the tick of
    /// the action.
    Tripped,
    /// Contactor or relay picked up. Written by the logic at commit.
    CoilActive,
    /// Timer output. Written by the logic at commit.
    TimerOut,
    /// Transformer powered: its secondary provides its section. Written by
    /// the logic at commit, with the actuation delays of coils.
    Powered,
}

impl SignalKind {
    /// Whether the signal is written by external inputs rather than derived
    /// by the logic.
    #[must_use]
    pub const fn is_input(self) -> bool {
        matches!(self, Self::Actuated | Self::Tripped)
    }
}

/// A signal and the device that owns it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SignalDef {
    /// Owning device.
    pub owner: DeviceId,
    /// Dense index of the owning device.
    pub device: DeviceIx,
    /// Meaning of the signal.
    pub kind: SignalKind,
}

/// A device as the runtime sees it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledDevice {
    /// Stable identifier.
    pub id: DeviceId,
    /// User-visible label.
    pub reference: ReferenceDesignator,
    /// Behaviour class.
    pub class: DeviceClass,
    /// The signal of the device, when its class has one.
    pub signal: Option<SigIx>,
    /// The coil load of a contactor, relay or timer.
    pub coil: Option<LoadIx>,
    /// Timer preset in milliseconds.
    pub preset_ms: Option<u32>,
    /// Whether a latching switch starts closed.
    pub initial_closed: bool,
}

/// The labels of a supply profile the solver reads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompiledSupply {
    /// Stable identifier.
    pub id: SupplyId,
    /// Kind of supply system.
    pub kind: SupplyKind,
    /// Nominal voltage label in millivolts.
    pub nominal_voltage_mv: u32,
    /// Phase-to-neutral voltage label of a three-phase supply.
    pub phase_voltage_mv: Option<u32>,
    /// Phase order of a three-phase supply.
    pub phase_sequence: Option<PhaseSequence>,
}

/// A schematic compiled for execution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledCircuit {
    pub(crate) pins: Vec<CompiledPin>,
    pub(crate) net_count: u32,
    pub(crate) switches: Vec<CompiledSwitch>,
    pub(crate) sources: Vec<CompiledSource>,
    pub(crate) loads: Vec<CompiledLoad>,
    pub(crate) signals: Vec<SignalDef>,
    pub(crate) devices: Vec<CompiledDevice>,
    pub(crate) supplies: Vec<CompiledSupply>,
    pub(crate) wires: Vec<(WireId, NetIx)>,
    pub(crate) junctions: Vec<(JunctionId, NetIx)>,
    pub(crate) interlocks: Vec<(DeviceIx, DeviceIx)>,
    pub(crate) fingerprint: Fingerprint,
}

impl CompiledCircuit {
    /// Every terminal, indexed by [`PinIx`].
    #[must_use]
    pub fn pins(&self) -> &[CompiledPin] {
        &self.pins
    }

    /// Number of copper nets.
    #[must_use]
    pub const fn net_count(&self) -> u32 {
        self.net_count
    }

    /// Every switch, indexed by [`SwitchIx`].
    #[must_use]
    pub fn switches(&self) -> &[CompiledSwitch] {
        &self.switches
    }

    /// Every supply tap.
    #[must_use]
    pub fn sources(&self) -> &[CompiledSource] {
        &self.sources
    }

    /// Every load, indexed by [`LoadIx`].
    #[must_use]
    pub fn loads(&self) -> &[CompiledLoad] {
        &self.loads
    }

    /// Every signal, indexed by [`SigIx`].
    #[must_use]
    pub fn signals(&self) -> &[SignalDef] {
        &self.signals
    }

    /// Every device, indexed by [`DeviceIx`] and sorted by [`DeviceId`].
    #[must_use]
    pub fn devices(&self) -> &[CompiledDevice] {
        &self.devices
    }

    /// Every supply profile, sorted by [`SupplyId`].
    #[must_use]
    pub fn supplies(&self) -> &[CompiledSupply] {
        &self.supplies
    }

    /// The copper net of every wire, sorted by [`WireId`].
    #[must_use]
    pub fn wires(&self) -> &[(WireId, NetIx)] {
        &self.wires
    }

    /// The copper net of every junction, sorted by [`JunctionId`].
    #[must_use]
    pub fn junctions(&self) -> &[(JunctionId, NetIx)] {
        &self.junctions
    }

    /// The contactor pairs of the mechanical interlocks, each pair in
    /// ascending order and the list sorted. A contactor is in at most one
    /// pair.
    #[must_use]
    pub fn interlocks(&self) -> &[(DeviceIx, DeviceIx)] {
        &self.interlocks
    }

    /// Stable hash of this circuit.
    #[must_use]
    pub const fn fingerprint(&self) -> Fingerprint {
        self.fingerprint
    }

    /// Dense index of a device.
    #[must_use]
    pub fn device_index(&self, id: DeviceId) -> Option<DeviceIx> {
        let position = self
            .devices
            .binary_search_by_key(&id, |device| device.id)
            .ok()?;
        u32::try_from(position).ok().map(DeviceIx)
    }

    /// A device by dense index.
    #[must_use]
    pub fn device(&self, index: DeviceIx) -> Option<&CompiledDevice> {
        self.devices.get(index.index())
    }

    /// A device by stable ID.
    #[must_use]
    pub fn device_by_id(&self, id: DeviceId) -> Option<&CompiledDevice> {
        self.device_index(id).and_then(|index| self.device(index))
    }

    /// The device that carries a reference designator.
    #[must_use]
    pub fn device_by_reference(&self, reference: &str) -> Option<&CompiledDevice> {
        self.devices
            .iter()
            .find(|device| device.reference.0 == reference)
    }

    /// A supply profile by ID.
    #[must_use]
    pub fn supply(&self, id: SupplyId) -> Option<&CompiledSupply> {
        self.supplies
            .binary_search_by_key(&id, |supply| supply.id)
            .ok()
            .and_then(|position| self.supplies.get(position))
    }
}

/// Diagnostic for a group whose conductors conflict, or `None` when they do
/// not. `sources` are the supply terminal devices feeding the group; the
/// smallest becomes the subject so the result is canonical.
///
/// Shared by the static check at compile time and by the solver at run time
/// so both report the same thing for the same condition.
/// What the sources reaching one group provide: their conductors, the
/// supply section of the live ones, and whether live conductors of two
/// sections meet (ADR-0005 §1). Protective earth is common to every section
/// and never sets or mixes one.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct GroupFeed {
    /// Conductors present.
    pub(crate) conductors: ConductorSet,
    /// The section of the first live conductor added.
    pub(crate) section: Option<SupplyId>,
    /// Whether live conductors of two sections were added.
    pub(crate) bridged: bool,
}

impl GroupFeed {
    /// Add a conductor provided by `supply`.
    pub(crate) fn add(&mut self, conductor: Conductor, supply: SupplyId) {
        self.conductors.insert(conductor);
        if conductor == Conductor::Pe {
            return;
        }
        match self.section {
            None => self.section = Some(supply),
            Some(section) if section != supply => self.bridged = true,
            Some(_) => {}
        }
    }

    /// The conflict of the group: bridged sections are a short circuit;
    /// within one section the conductor table applies.
    pub(crate) fn conflict(&self) -> ConductorConflict {
        if self.bridged {
            ConductorConflict::ShortCircuit
        } else {
            self.conductors.conflict()
        }
    }
}

/// The diagnostic for a group's conflict, naming the smallest source device
/// as subject and the others as related elements.
pub(crate) fn conflict_diagnostic(feed: GroupFeed, sources: &[DeviceId]) -> Option<Diagnostic> {
    let conductors = feed.conductors;
    let (code, text) = match feed.conflict() {
        ConductorConflict::None => return None,
        ConductorConflict::NeutralEarthBond => (
            DiagnosticCode::NeutralEarthBond,
            "neutral and protective earth are bonded",
        ),
        ConductorConflict::ProtectiveEarthFault => (
            DiagnosticCode::ProtectiveEarthFault,
            "protective earth is connected to a live conductor",
        ),
        ConductorConflict::ShortCircuit if feed.bridged => (
            DiagnosticCode::ShortCircuit,
            "conductors of separate supply sections are connected together",
        ),
        ConductorConflict::ShortCircuit => (
            DiagnosticCode::ShortCircuit,
            "incompatible conductors are connected together",
        ),
    };
    let mut devices = sources.to_vec();
    devices.sort_unstable();
    devices.dedup();
    let mut devices = devices.into_iter();
    let subject = devices.next().map_or(Subject::Document, Subject::Device);
    Some(
        Diagnostic::new(code, subject, format!("{text} ({conductors})"))
            .with_related(devices.map(Subject::Device).collect()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A group fed by one section with these conductors.
    fn conductors(list: &[Conductor]) -> GroupFeed {
        let mut feed = GroupFeed::default();
        for conductor in list {
            feed.add(*conductor, SupplyId(1));
        }
        feed
    }

    #[test]
    fn single_conductor_group_produces_no_conflict_diagnostic() {
        assert_eq!(
            conflict_diagnostic(conductors(&[Conductor::L1]), &[DeviceId(4)]),
            None
        );
    }

    #[test]
    fn short_circuit_names_the_smallest_source_as_subject() {
        let diagnostic = conflict_diagnostic(
            conductors(&[Conductor::L1, Conductor::L3]),
            &[DeviceId(9), DeviceId(4), DeviceId(9)],
        )
        .unwrap();

        assert_eq!(diagnostic.code(), DiagnosticCode::ShortCircuit);
        assert_eq!(diagnostic.subject(), &Subject::Device(DeviceId(4)));
        assert_eq!(diagnostic.related(), [Subject::Device(DeviceId(9))]);
        assert_eq!(
            diagnostic.message(),
            "incompatible conductors are connected together (l1+l3)"
        );
    }

    #[test]
    fn earth_fault_and_neutral_bond_use_their_own_codes() {
        let fault = conflict_diagnostic(
            conductors(&[Conductor::Pe, Conductor::DcPos]),
            &[DeviceId(1), DeviceId(2)],
        )
        .unwrap();
        let bond = conflict_diagnostic(
            conductors(&[Conductor::Pe, Conductor::N]),
            &[DeviceId(1), DeviceId(2)],
        )
        .unwrap();

        assert_eq!(fault.code(), DiagnosticCode::ProtectiveEarthFault);
        assert_eq!(bond.code(), DiagnosticCode::NeutralEarthBond);
    }

    #[test]
    fn same_conductor_from_two_sections_is_a_short_circuit() {
        let mut feed = GroupFeed::default();
        feed.add(Conductor::L1, SupplyId(1));
        feed.add(Conductor::L1, SupplyId(2));

        let diagnostic = conflict_diagnostic(feed, &[DeviceId(1), DeviceId(2)]).unwrap();

        assert_eq!(diagnostic.code(), DiagnosticCode::ShortCircuit);
        assert_eq!(
            diagnostic.message(),
            "conductors of separate supply sections are connected together (l1)"
        );
    }

    #[test]
    fn protective_earth_is_common_to_every_section() {
        let mut feed = GroupFeed::default();
        feed.add(Conductor::Pe, SupplyId(1));
        feed.add(Conductor::Pe, SupplyId(2));
        feed.add(Conductor::N, SupplyId(2));

        assert!(!feed.bridged);
        assert_eq!(feed.section, Some(SupplyId(2)));
        assert_eq!(feed.conflict(), ConductorConflict::NeutralEarthBond);
    }
}
