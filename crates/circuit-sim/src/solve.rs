//! Discrete electrical solver.
//!
//! Given the set of closed switches, the solver decides which copper nets
//! form a group, which conductors are present in each group, which loads are
//! energized and which faults exist. It works on ideal connectivity only: no
//! voltage, current, impedance or transient is ever computed.
//!
//! The solver knows nothing about signals or devices' behaviour. The result
//! is a pure function of the switch mask, so an unchanged mask reuses the
//! previous solution.

use std::collections::{BTreeMap, BTreeSet};

use circuit_model::{
    Conductor, ConductorConflict, ConductorSet, Connection, DeviceId, Diagnostic, DiagnosticCode,
    PhaseSequence, Subject, SupplyId, SupplyKind, WindingConnection, sort_diagnostics,
};

use crate::bitset::BitSet;
use crate::compiled::{
    CompiledCircuit, CompiledLoad, GroupFeed, LoadIx, LoadRule, NetIx, SwitchIx,
    conflict_diagnostic,
};
use crate::union_find::UnionFind;

/// Which switches are closed, indexed by [`SwitchIx`], and which
/// conditional sources (transformer secondaries) are enabled, indexed by
/// source position. A source without an enabling signal always provides its
/// conductor, whatever the mask says.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SwitchMask {
    switches: BitSet,
    sources: BitSet,
}

impl SwitchMask {
    /// A mask for `switch_count` switches, all open, and no conditional
    /// source enabled.
    #[must_use]
    pub fn new(switch_count: usize) -> Self {
        Self {
            switches: BitSet::new(switch_count),
            sources: BitSet::new(0),
        }
    }

    /// A mask sized for a circuit, all switches open and every conditional
    /// source disabled.
    #[must_use]
    pub fn for_circuit(circuit: &CompiledCircuit) -> Self {
        Self {
            switches: BitSet::new(circuit.switches().len()),
            sources: BitSet::new(circuit.sources().len()),
        }
    }

    /// Open or close a switch.
    pub fn set(&mut self, switch: SwitchIx, closed: bool) {
        self.switches.set(switch.index(), closed);
    }

    /// Whether a switch is closed.
    #[must_use]
    pub fn is_closed(&self, switch: SwitchIx) -> bool {
        self.switches.get(switch.index())
    }

    pub(crate) fn set_index(&mut self, index: usize, closed: bool) {
        self.switches.set(index, closed);
    }

    pub(crate) fn closed_index(&self, index: usize) -> bool {
        self.switches.get(index)
    }

    /// Enable or disable the conditional source at `index`.
    pub(crate) fn set_source(&mut self, index: usize, enabled: bool) {
        self.sources.set(index, enabled);
    }

    pub(crate) fn source_enabled(&self, index: usize) -> bool {
        self.sources.get(index)
    }

    pub(crate) fn copy_from(&mut self, other: &Self) {
        self.switches.copy_from(&other.switches);
        self.sources.copy_from(&other.sources);
    }
}

/// Direction of rotation of a motor relative to the supply phase sequence.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Direction {
    /// With the phase sequence.
    Forward,
    /// Against the phase sequence.
    Reverse,
}

/// Whether a load is energized.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LoadState {
    /// Not energized.
    #[default]
    Off,
    /// A two-terminal load is energized.
    On,
    /// A motor is running.
    Rotating(Direction),
    /// A six-terminal motor is running with its windings connected in star
    /// or delta.
    RotatingWound(Direction, WindingConnection),
}

impl LoadState {
    /// Whether the load is energized in any way.
    #[must_use]
    pub const fn is_energized(self) -> bool {
        !matches!(self, Self::Off)
    }
}

/// Result of solving the network for one switch mask.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ElectricalSolution {
    group_of_net: Vec<NetIx>,
    conductors_of_group: Vec<ConductorSet>,
    loads: Vec<LoadState>,
    faults: Vec<Diagnostic>,
}

impl ElectricalSolution {
    /// The group a net belongs to, named by its smallest net.
    #[must_use]
    pub fn group_of(&self, net: NetIx) -> NetIx {
        self.group_of_net.get(net.index()).copied().unwrap_or(net)
    }

    /// Conductors present on a net.
    #[must_use]
    pub fn conductors(&self, net: NetIx) -> ConductorSet {
        self.conductors_of_group
            .get(self.group_of(net).index())
            .copied()
            .unwrap_or_default()
    }

    /// State of every load, indexed by [`LoadIx`].
    #[must_use]
    pub fn loads(&self) -> &[LoadState] {
        &self.loads
    }

    /// State of one load; `Off` for an index outside the circuit.
    #[must_use]
    pub fn load(&self, load: LoadIx) -> LoadState {
        self.loads.get(load.index()).copied().unwrap_or_default()
    }

    /// Active faults and warnings, in canonical order.
    #[must_use]
    pub fn faults(&self) -> &[Diagnostic] {
        &self.faults
    }
}

/// Voltage labels of the supply that provides a conductor.
#[derive(Clone, Copy, Debug)]
struct SupplyLabel {
    kind: SupplyKind,
    nominal_voltage_mv: u32,
    phase_voltage_mv: Option<u32>,
    phase_sequence: Option<PhaseSequence>,
}

/// Loads that share conductor-less groups, and what feeds them from outside.
#[derive(Default)]
struct DeadComponent {
    /// Conductors and section of the first live group reached through a
    /// load.
    boundary: Option<(ConductorSet, Option<SupplyId>)>,
    /// Whether live groups with different conductors were reached.
    mixed: bool,
    loads: BTreeSet<usize>,
    devices: BTreeSet<DeviceId>,
}

/// Reusable solver for one compiled circuit.
#[derive(Clone, Debug)]
pub struct Solver {
    groups: UnionFind,
    dead: UnionFind,
    faulted: Vec<bool>,
    /// The supply section of the live conductors of each group, by group.
    sections: Vec<Option<SupplyId>>,
    labels: BTreeMap<SupplyId, SupplyLabel>,
    solution: ElectricalSolution,
    last_mask: Option<SwitchMask>,
    solve_count: u64,
    /// What the enabled sources provide to each group, by group. Only the
    /// groups listed in `fed` hold a value; the others are default.
    feeds: Vec<GroupFeed>,
    /// The groups with at least one enabled source, in the order found.
    fed: Vec<u32>,
}

impl Solver {
    /// Prepare buffers for a circuit. The solver must be used with this
    /// circuit only.
    #[must_use]
    pub fn new(circuit: &CompiledCircuit) -> Self {
        let nets = circuit.net_count();
        let mut labels = BTreeMap::new();
        for source in circuit.sources() {
            if let Some(supply) = circuit.supply(source.supply) {
                labels.entry(supply.id).or_insert(SupplyLabel {
                    kind: supply.kind,
                    nominal_voltage_mv: supply.nominal_voltage_mv,
                    phase_voltage_mv: supply.phase_voltage_mv,
                    phase_sequence: supply.phase_sequence,
                });
            }
        }
        Self {
            groups: UnionFind::new(nets),
            dead: UnionFind::new(nets),
            faulted: vec![false; nets as usize],
            sections: vec![None; nets as usize],
            labels,
            solution: ElectricalSolution {
                group_of_net: (0..nets).map(NetIx).collect(),
                conductors_of_group: vec![ConductorSet::EMPTY; nets as usize],
                loads: vec![LoadState::Off; circuit.loads().len()],
                faults: Vec::new(),
            },
            last_mask: None,
            solve_count: 0,
            feeds: vec![GroupFeed::default(); nets as usize],
            fed: Vec::new(),
        }
    }

    /// The most recent solution.
    #[must_use]
    pub const fn solution(&self) -> &ElectricalSolution {
        &self.solution
    }

    /// How many times the network was actually solved, not counting calls
    /// that reused the previous solution.
    #[must_use]
    pub const fn solve_count(&self) -> u64 {
        self.solve_count
    }

    /// Solve the network for a switch mask.
    ///
    /// A mask equal to the previous one returns the previous solution without
    /// recomputing anything.
    pub fn solve(&mut self, circuit: &CompiledCircuit, closed: &SwitchMask) -> &ElectricalSolution {
        if self.last_mask.as_ref() == Some(closed) {
            return &self.solution;
        }
        self.solve_count = self.solve_count.saturating_add(1);

        // Groups: copper nets joined by closed switches.
        self.groups.reset();
        for (index, switch) in circuit.switches().iter().enumerate() {
            if closed.closed_index(index) {
                self.groups.union(switch.a.raw(), switch.b.raw());
            }
        }
        for (net, group) in (0_u32..).zip(self.solution.group_of_net.iter_mut()) {
            *group = NetIx(self.groups.find(net));
        }

        // Conductors present in each group, with their supply section.
        self.solution.conductors_of_group.fill(ConductorSet::EMPTY);
        self.sections.fill(None);
        for group in self.fed.drain(..) {
            if let Some(feed) = self.feeds.get_mut(group as usize) {
                *feed = GroupFeed::default();
            }
        }
        for (index, source) in circuit.sources().iter().enumerate() {
            if source.enabled_by.is_some() && !closed.source_enabled(index) {
                continue;
            }
            let group = self.groups.find(source.net.raw());
            let Some(feed) = self.feeds.get_mut(group as usize) else {
                continue;
            };
            if feed.conductors.is_empty() {
                self.fed.push(group);
            }
            feed.add(source.conductor, source.supply);
        }

        // Conflicts. A group in fault energizes nothing.
        let mut faults = std::mem::take(&mut self.solution.faults);
        faults.clear();
        self.faulted.fill(false);
        let mut conflicting = Vec::new();
        for group in &self.fed {
            let index = *group as usize;
            let Some(feed) = self.feeds.get(index).copied() else {
                continue;
            };
            if let Some(conductors) = self.solution.conductors_of_group.get_mut(index) {
                *conductors = feed.conductors;
            }
            if let Some(section) = self.sections.get_mut(index) {
                *section = feed.section;
            }
            let conflict = feed.conflict();
            let in_fault = matches!(
                conflict,
                ConductorConflict::ShortCircuit | ConductorConflict::ProtectiveEarthFault
            );
            if let Some(flag) = self.faulted.get_mut(index) {
                *flag = in_fault;
            }
            if conflict != ConductorConflict::None {
                conflicting.push(*group);
            }
        }
        if !conflicting.is_empty() {
            // The sources of a group are only needed to name a conflict.
            conflicting.sort_unstable();
            let mut owners: BTreeMap<u32, Vec<DeviceId>> = BTreeMap::new();
            for (index, source) in circuit.sources().iter().enumerate() {
                if source.enabled_by.is_some() && !closed.source_enabled(index) {
                    continue;
                }
                let group = self.groups.find(source.net.raw());
                if conflicting.binary_search(&group).is_ok() {
                    owners.entry(group).or_default().push(source.owner);
                }
            }
            for group in &conflicting {
                let feed = self.feeds.get(*group as usize).copied().unwrap_or_default();
                let sources = owners.get(group).map_or(&[][..], Vec::as_slice);
                faults.extend(conflict_diagnostic(feed, sources));
            }
        }

        // Loads.
        for (index, load) in circuit.loads().iter().enumerate() {
            let (state, fault) = self.evaluate_load(load);
            if let Some(slot) = self.solution.loads.get_mut(index) {
                *slot = state;
            }
            if let Some((code, message)) = fault {
                faults.push(Diagnostic::new(
                    code,
                    Subject::Device(load.device_id),
                    message,
                ));
            }
        }

        self.find_series_loads(circuit, &mut faults);

        sort_diagnostics(&mut faults);
        self.solution.faults = faults;
        match &mut self.last_mask {
            Some(mask) => mask.copy_from(closed),
            None => self.last_mask = Some(closed.clone()),
        }
        &self.solution
    }

    /// The supply section of the live conductors of a group.
    fn section(&self, group: u32) -> Option<SupplyId> {
        self.sections.get(group as usize).copied().flatten()
    }

    fn group_state(&self, net: NetIx) -> (u32, ConductorSet, bool) {
        let group = self.solution.group_of(net);
        let conductors = self
            .solution
            .conductors_of_group
            .get(group.index())
            .copied()
            .unwrap_or_default();
        let faulted = self.faulted.get(group.index()).copied().unwrap_or(false);
        (group.raw(), conductors, faulted)
    }

    fn evaluate_load(
        &self,
        load: &CompiledLoad,
    ) -> (LoadState, Option<(DiagnosticCode, &'static str)>) {
        match (load.rule, load.nets.as_slice()) {
            (
                LoadRule::TwoTerminal {
                    connection,
                    rated_voltage_mv,
                },
                [first, second],
            ) => self.evaluate_two_terminal(connection, rated_voltage_mv, *first, *second),
            (LoadRule::ThreePhaseMotor { rated_voltage_mv }, [u, v, w]) => {
                self.evaluate_motor(rated_voltage_mv, [*u, *v, *w])
            }
            (LoadRule::SixTerminalMotor { rated_voltage_mv }, [u1, v1, w1, u2, v2, w2]) => {
                self.evaluate_six_terminal_motor(rated_voltage_mv, [*u1, *v1, *w1, *u2, *v2, *w2])
            }
            _ => (LoadState::Off, None),
        }
    }

    fn evaluate_two_terminal(
        &self,
        connection: Connection,
        rated_voltage_mv: u32,
        first: NetIx,
        second: NetIx,
    ) -> (LoadState, Option<(DiagnosticCode, &'static str)>) {
        let (group_a, set_a, faulted_a) = self.group_state(first);
        let (group_b, set_b, faulted_b) = self.group_state(second);
        if group_a == group_b {
            return (
                LoadState::Off,
                Some((
                    DiagnosticCode::LoadTerminalsShorted,
                    "both terminals of the load are connected together",
                )),
            );
        }
        if faulted_a || faulted_b {
            return (LoadState::Off, None);
        }
        match valid_pair(connection, set_a, set_b) {
            Some(_) => {
                let sections = (self.section(group_a), self.section(group_b));
                if self.expected_voltage(connection, sections) == Some(rated_voltage_mv) {
                    (LoadState::On, None)
                } else {
                    (
                        LoadState::Off,
                        Some((
                            DiagnosticCode::LoadSupplyMismatch,
                            "the supply voltage label does not match the rated voltage of the load",
                        )),
                    )
                }
            }
            None if !set_a.is_empty() && !set_b.is_empty() && set_a != set_b => (
                LoadState::Off,
                Some((
                    DiagnosticCode::LoadSupplyMismatch,
                    "the conductors across the load do not match its connection type",
                )),
            ),
            None => (LoadState::Off, None),
        }
    }

    /// Voltage label between two conductors, when one supply section
    /// provides both.
    fn expected_voltage(
        &self,
        connection: Connection,
        (section_a, section_b): (Option<SupplyId>, Option<SupplyId>),
    ) -> Option<u32> {
        let section = section_a?;
        if section_b != Some(section) {
            return None;
        }
        let label_a = self.labels.get(&section)?;
        match (connection, label_a.kind) {
            (Connection::Dc, SupplyKind::Dc)
            | (Connection::LineNeutral, SupplyKind::SinglePhase)
            | (Connection::LineLine, SupplyKind::ThreePhase) => Some(label_a.nominal_voltage_mv),
            (Connection::LineNeutral, SupplyKind::ThreePhase) => label_a.phase_voltage_mv,
            _ => None,
        }
    }

    fn evaluate_motor(
        &self,
        rated_voltage_mv: u32,
        terminals: [NetIx; 3],
    ) -> (LoadState, Option<(DiagnosticCode, &'static str)>) {
        let states = terminals.map(|net| self.group_state(net));
        if states.iter().any(|(_, _, faulted)| *faulted) {
            return (LoadState::Off, None);
        }
        if states.iter().all(|(_, set, _)| set.is_empty()) {
            return (LoadState::Off, None);
        }
        let has_foreign_conductor = states
            .iter()
            .any(|(_, set, _)| set.iter().any(|conductor| !conductor.is_phase()));
        if has_foreign_conductor {
            return (
                LoadState::Off,
                Some((
                    DiagnosticCode::LoadSupplyMismatch,
                    "a motor terminal is connected to a conductor that is not a phase",
                )),
            );
        }

        match self.rotation(rated_voltage_mv, states.map(|(group, set, _)| (group, set))) {
            Ok(Some(direction)) => (LoadState::Rotating(direction), None),
            Ok(None) => (LoadState::Off, None),
            Err(diagnostic) => (LoadState::Off, Some(diagnostic)),
        }
    }

    /// The direction a motor turns with `phases` on `u1`, `v1`, `w1`.
    /// `Ok(None)` when the supply has no phase sequence; an error when the
    /// phases are incomplete or do not match the motor.
    fn rotation(
        &self,
        rated_voltage_mv: u32,
        lines: [(u32, ConductorSet); 3],
    ) -> Result<Option<Direction>, (DiagnosticCode, &'static str)> {
        let phases = lines.map(|(_, set)| set.single());
        let [Some(u), Some(v), Some(w)] = phases else {
            return Err(phase_loss());
        };
        if u == v || v == w || u == w {
            return Err(phase_loss());
        }

        let [first, second, third] = lines.map(|(group, _)| self.section(group));
        let Some(label) = first.and_then(|section| self.labels.get(&section)) else {
            return Ok(None);
        };
        let same_supply = second == first && third == first;
        if !same_supply || label.nominal_voltage_mv != rated_voltage_mv {
            return Err((
                DiagnosticCode::LoadSupplyMismatch,
                "the supply voltage label does not match the rated voltage of the motor",
            ));
        }
        let Some(sequence) = label.phase_sequence else {
            return Ok(None);
        };

        // (u, v, w) follows L1-L2-L3 when it is one of its three rotations.
        let follows_l1_l2_l3 = matches!(
            (u, v, w),
            (Conductor::L1, Conductor::L2, Conductor::L3)
                | (Conductor::L2, Conductor::L3, Conductor::L1)
                | (Conductor::L3, Conductor::L1, Conductor::L2)
        );
        let forward = match sequence {
            PhaseSequence::L1L2L3 => follows_l1_l2_l3,
            PhaseSequence::L1L3L2 => !follows_l1_l2_l3,
        };
        Ok(Some(if forward {
            Direction::Forward
        } else {
            Direction::Reverse
        }))
    }

    /// A six-terminal motor (ADR-0005 §4): star when `u2`, `v2`, `w2` meet
    /// at a dead star point apart from the line ends, delta when each line
    /// end is joined to the second end of the next winding; open windings
    /// are off; any other energized connection is refused.
    fn evaluate_six_terminal_motor(
        &self,
        rated_voltage_mv: u32,
        terminals: [NetIx; 6],
    ) -> (LoadState, Option<(DiagnosticCode, &'static str)>) {
        let states = terminals.map(|net| self.group_state(net));
        if states.iter().any(|(_, _, faulted)| *faulted) {
            return (LoadState::Off, None);
        }
        if states.iter().all(|(_, set, _)| set.is_empty()) {
            return (LoadState::Off, None);
        }
        if states
            .iter()
            .any(|(_, set, _)| set.iter().any(|conductor| !conductor.is_phase()))
        {
            return (
                LoadState::Off,
                Some((
                    DiagnosticCode::LoadSupplyMismatch,
                    "a motor terminal is connected to a conductor that is not a phase",
                )),
            );
        }

        let [u1, v1, w1, u2, v2, w2] = states;
        let line_groups = [u1.0, v1.0, w1.0];
        let star = u2.0 == v2.0 && v2.0 == w2.0 && u2.1.is_empty() && !line_groups.contains(&u2.0);
        let delta = u1.0 == w2.0
            && v1.0 == u2.0
            && w1.0 == v2.0
            && u1.0 != v1.0
            && v1.0 != w1.0
            && u1.0 != w1.0;
        let winding = if star {
            WindingConnection::Star
        } else if delta {
            WindingConnection::Delta
        } else {
            let second_ends_dead = [u2, v2, w2].iter().all(|(_, set, _)| set.is_empty());
            let open = second_ends_dead && u2.0 != v2.0 && v2.0 != w2.0 && u2.0 != w2.0;
            if open {
                return (LoadState::Off, None);
            }
            return (
                LoadState::Off,
                Some((
                    DiagnosticCode::UnsupportedWindingConnection,
                    "the motor windings are energized in a connection that is neither star nor delta",
                )),
            );
        };
        match self.rotation(rated_voltage_mv, [(u1.0, u1.1), (v1.0, v1.1), (w1.0, w1.1)]) {
            Ok(Some(direction)) => (LoadState::RotatingWound(direction, winding), None),
            Ok(None) => (LoadState::Off, None),
            Err(diagnostic) => (LoadState::Off, Some(diagnostic)),
        }
    }

    /// Loads in series at run time.
    ///
    /// A conductor-less group that only touches loads can be fed through
    /// those loads. That matters when the far sides of the loads carry
    /// different conductors: current would flow through two or more loads in
    /// series, which this model does not represent. Loads that merely share a
    /// switched-off node (far sides on the same conductor) are fine.
    fn find_series_loads(&mut self, circuit: &CompiledCircuit, faults: &mut Vec<Diagnostic>) {
        self.dead.reset();
        for load in circuit.loads() {
            let mut first_dead = None;
            for net in &load.nets {
                let (group, conductors, _) = self.group_state(*net);
                if conductors.is_empty() {
                    match first_dead {
                        Some(first) => self.dead.union(first, group),
                        None => first_dead = Some(group),
                    }
                }
            }
        }

        let mut components: BTreeMap<u32, DeadComponent> = BTreeMap::new();
        for (index, load) in circuit.loads().iter().enumerate() {
            let states: Vec<(u32, ConductorSet, bool)> =
                load.nets.iter().map(|net| self.group_state(*net)).collect();
            let Some(dead_group) = states
                .iter()
                .find(|(_, conductors, _)| conductors.is_empty())
                .map(|(group, _, _)| *group)
            else {
                continue;
            };
            let component = components.entry(self.dead.find(dead_group)).or_default();
            component.loads.insert(index);
            if let Some(device) = circuit.device(load.device) {
                component.devices.insert(device.id);
            }
            for (group, conductors, faulted) in states {
                if conductors.is_empty() || faulted {
                    continue;
                }
                let feed = (conductors, self.section(group));
                match component.boundary {
                    None => component.boundary = Some(feed),
                    Some(boundary) if boundary != feed => component.mixed = true,
                    Some(_) => {}
                }
            }
        }

        for component in components.into_values() {
            if !component.mixed || component.loads.len() < 2 {
                continue;
            }
            let mut devices = component.devices.into_iter();
            let Some(first) = devices.next() else {
                continue;
            };
            faults.push(
                Diagnostic::new(
                    DiagnosticCode::DynamicUnsupportedTopology,
                    Subject::Device(first),
                    "loads are connected in series, which the discrete model does not represent",
                )
                .with_related(devices.map(Subject::Device).collect()),
            );
        }
    }
}

const fn phase_loss() -> (DiagnosticCode, &'static str) {
    (
        DiagnosticCode::PhaseLoss,
        "the motor is fed by only part of its phases",
    )
}

/// The conductors that energize a two-terminal load, when the two sides offer
/// a pair that suits the connection type.
fn valid_pair(
    connection: Connection,
    a: ConductorSet,
    b: ConductorSet,
) -> Option<(Conductor, Conductor)> {
    let phase = |set: ConductorSet| set.iter().find(|conductor| conductor.is_phase());
    match connection {
        Connection::Dc => {
            if a.contains(Conductor::DcPos) && b.contains(Conductor::DcNeg) {
                Some((Conductor::DcPos, Conductor::DcNeg))
            } else if a.contains(Conductor::DcNeg) && b.contains(Conductor::DcPos) {
                Some((Conductor::DcNeg, Conductor::DcPos))
            } else {
                None
            }
        }
        Connection::LineNeutral => {
            if b.contains(Conductor::N) {
                phase(a).map(|line| (line, Conductor::N))
            } else if a.contains(Conductor::N) {
                phase(b).map(|line| (line, Conductor::N))
            } else {
                None
            }
        }
        Connection::LineLine => match (phase(a), phase(b)) {
            (Some(first), Some(second)) if first != second => Some((first, second)),
            _ => None,
        },
    }
}

#[cfg(test)]
#[path = "solve_tests.rs"]
mod tests;
