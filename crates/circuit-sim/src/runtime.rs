//! Deterministic tick runtime.
//!
//! The simulator owns a fixed tick duration and never reads the system clock.
//! One tick is:
//!
//! 1. apply the external inputs: pushbuttons, switches and thermal trips take
//!    effect in this tick;
//! 2. derive which switches are closed from the effective signals;
//! 3. solve the network (reusing the previous solution if nothing switched);
//! 4. run the device logic, which produces the coil and timer signals of the
//!    *next* tick;
//! 5. record what changed since the previous tick;
//! 6. keep what was solved so a snapshot can be built on demand;
//! 7. commit the derived signals and advance virtual time.
//!
//! A snapshot always describes the tick that was actually solved, never the
//! state committed for the tick after it.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use circuit_model::{
    DeviceClass, DeviceId, Diagnostic, DiagnosticCode, InputAction, Live, MotorState, MotorView,
    SimSnapshot, Subject, SymbolId, TimerView, sort_diagnostics,
};

use crate::bitset::BitSet;
use crate::compiled::{CompiledCircuit, Drive, Level, LoadRule, SigIx, SignalKind};
use crate::fingerprint::Fingerprint;
use crate::logic::{ActuatorState, TimerState, preset_ticks, step_actuator, step_timer};
use crate::solve::{Direction, LoadState, Solver, SwitchMask};
use crate::trace::{LoadActivity, TraceEvent, TraceHeader, TraceKind};

/// Most trace events the simulator buffers before [`Simulator::step`] refuses
/// to run until [`Simulator::take_trace`] is called. An oscillating circuit
/// emits events every tick, so the buffer must not grow without bound.
pub const MAX_PENDING_TRACE_EVENTS: usize = 1_000_000;

/// Actuation delay of contactor and relay coils, in ticks.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DelayConfig {
    /// Consecutive energized ticks before a coil picks up. At least 1.
    pub pickup_ticks: u32,
    /// Consecutive de-energized ticks before a coil drops out. At least 1.
    pub dropout_ticks: u32,
}

impl Default for DelayConfig {
    fn default() -> Self {
        Self {
            pickup_ticks: 1,
            dropout_ticks: 1,
        }
    }
}

/// What the runtime does after a latching fault.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum FaultPolicy {
    /// Fail closed: complete the faulting tick, then refuse to step until
    /// [`Simulator::reset_fault`].
    #[default]
    Halt,
    /// Report the fault and keep running. For teaching and diagnosis.
    Report,
}

impl FaultPolicy {
    /// Lowercase name used in traces and on the command line.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Halt => "halt",
            Self::Report => "report",
        }
    }
}

/// Simulator configuration.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SimConfig {
    /// Duration of one tick in microseconds. Greater than zero.
    pub tick_us: u32,
    /// Coil actuation delays.
    pub delays: DelayConfig,
    /// What happens after a latching fault.
    pub fault_policy: FaultPolicy,
    /// Whether changes are recorded for [`Simulator::take_trace`].
    pub record_trace: bool,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            tick_us: 1_000,
            delays: DelayConfig::default(),
            fault_policy: FaultPolicy::default(),
            record_trace: true,
        }
    }
}

/// A configuration the simulator refuses to run with.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ConfigError {
    /// `tick_us` is zero.
    #[error("the tick duration must be greater than zero")]
    ZeroTick,
    /// `pickup_ticks` is zero.
    #[error("the pickup delay must be at least one tick")]
    ZeroPickup,
    /// `dropout_ticks` is zero.
    #[error("the dropout delay must be at least one tick")]
    ZeroDropout,
}

impl ConfigError {
    /// The same finding as a structured diagnostic.
    #[must_use]
    pub fn diagnostic(&self) -> Diagnostic {
        Diagnostic::new(
            DiagnosticCode::InvalidConfig,
            Subject::Document,
            self.to_string(),
        )
    }
}

/// Why an input was rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum InvalidInput {
    /// The device does not exist in the circuit.
    #[error("device {0} does not exist")]
    UnknownDevice(DeviceId),
    /// The action does not apply to a device of this class.
    #[error("the action does not apply to device {0}")]
    ClassMismatch(DeviceId),
    /// Two actions target the same device in one tick.
    #[error("device {0} receives more than one action in the same tick")]
    DuplicateDevice(DeviceId),
}

/// Why a step did not run. The state is unchanged and no tick was consumed.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum SimulationError {
    /// An input was rejected; none of the inputs was applied.
    #[error("invalid input: {0}")]
    InvalidInput(#[from] InvalidInput),
    /// The runtime is latched by a fault.
    #[error("the runtime is latched by a fault; call reset_fault() first")]
    AlreadyFaulted,
    /// Too many trace events are waiting to be taken.
    #[error("the pending trace buffer is full; call take_trace() first")]
    TraceBufferFull,
    /// The tick counter cannot advance any further.
    #[error("the tick counter is exhausted")]
    TickOverflow,
}

/// Why a checkpoint was refused.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RestoreError {
    /// The checkpoint was taken from a different compiled circuit.
    #[error("the checkpoint belongs to a different circuit")]
    CircuitMismatch,
    /// The checkpoint was taken with a different configuration.
    #[error("the checkpoint was taken with a different configuration")]
    ConfigMismatch,
}

/// What a step did. A few bytes; no snapshot is allocated per tick.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StepOutcome {
    /// The last tick that was executed.
    pub tick: u64,
    /// Whether any signal, load or fault changed.
    pub changed: bool,
    /// Whether the runtime is now latched by a fault.
    pub faulted: bool,
}

/// State carried from one tick to the next.
#[derive(Clone, Debug, Eq, PartialEq)]
struct RuntimeState {
    /// The next tick to execute.
    tick: u64,
    /// Signals effective at the start of the next tick.
    signals: BitSet,
    /// Timer counters, by device index.
    timers: Vec<TimerState>,
    /// Coil counters, by device index.
    actuators: Vec<ActuatorState>,
    faulted: bool,
}

/// What was solved in the last executed tick.
#[derive(Clone, Debug, Eq, PartialEq)]
struct TickView {
    /// Whether any tick was executed since the last reset.
    solved: bool,
    tick: u64,
    /// Signals that were effective during the tick.
    signals: BitSet,
    loads: Vec<LoadState>,
    faults: Vec<Diagnostic>,
    /// Timer counters at the end of the tick.
    timers: Vec<TimerState>,
    mask: SwitchMask,
}

/// Saved simulator state for rewind and replay.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Checkpoint {
    fingerprint: Fingerprint,
    config: SimConfig,
    state: RuntimeState,
    view: TickView,
}

impl Checkpoint {
    /// The next tick the restored simulator will execute.
    #[must_use]
    pub const fn tick(&self) -> u64 {
        self.state.tick
    }
}

/// Runs a compiled circuit tick by tick.
#[derive(Clone, Debug)]
pub struct Simulator {
    circuit: CompiledCircuit,
    config: SimConfig,
    solver: Solver,
    /// Timer preset in ticks, by device index; zero for other devices.
    presets: Vec<u64>,
    /// Load indices sorted by (device id, symbol id), the trace order.
    load_order: Vec<usize>,
    config_diagnostics: Vec<Diagnostic>,
    state: RuntimeState,
    view: TickView,
    mask: SwitchMask,
    effective: BitSet,
    /// Faults of the tick being executed when the logic adds to those of
    /// the solution; reused between ticks.
    tick_faults: Vec<Diagnostic>,
    trace: Vec<TraceEvent>,
    trace_limit: usize,
}

impl Simulator {
    /// Create a simulator at tick 0.
    ///
    /// # Errors
    ///
    /// Returns [`ConfigError`] for a zero tick duration or a zero actuation
    /// delay.
    pub fn new(circuit: CompiledCircuit, config: SimConfig) -> Result<Self, ConfigError> {
        if config.tick_us == 0 {
            return Err(ConfigError::ZeroTick);
        }
        if config.delays.pickup_ticks == 0 {
            return Err(ConfigError::ZeroPickup);
        }
        if config.delays.dropout_ticks == 0 {
            return Err(ConfigError::ZeroDropout);
        }

        let mut presets = Vec::with_capacity(circuit.devices().len());
        let mut config_diagnostics = Vec::new();
        for device in circuit.devices() {
            let preset = device
                .preset_ms
                .and_then(|preset_ms| preset_ticks(preset_ms, config.tick_us));
            if preset.is_some_and(|preset| preset.quantized) {
                config_diagnostics.push(Diagnostic::new(
                    DiagnosticCode::PresetQuantized,
                    Subject::Device(device.id),
                    "the timer preset is not a multiple of the tick and was rounded up",
                ));
            }
            presets.push(preset.map_or(0, |preset| preset.ticks));
        }

        let mut load_order: Vec<usize> = (0..circuit.loads().len()).collect();
        load_order.sort_by_key(|index| {
            circuit.loads().get(*index).map(|load| {
                (
                    circuit.device(load.device).map(|device| device.id),
                    load.symbol,
                )
            })
        });

        let solver = Solver::new(&circuit);
        let state = initial_state(&circuit);
        let view = initial_view(&circuit, &state);
        let mask = SwitchMask::for_circuit(&circuit);
        let effective = state.signals.clone();
        Ok(Self {
            circuit,
            config,
            solver,
            presets,
            load_order,
            config_diagnostics,
            state,
            view,
            mask,
            effective,
            tick_faults: Vec::new(),
            trace: Vec::new(),
            trace_limit: MAX_PENDING_TRACE_EVENTS,
        })
    }

    /// The circuit being simulated.
    #[must_use]
    pub const fn circuit(&self) -> &CompiledCircuit {
        &self.circuit
    }

    /// The configuration in use.
    #[must_use]
    pub const fn config(&self) -> &SimConfig {
        &self.config
    }

    /// The next tick that will be executed.
    #[must_use]
    pub const fn tick(&self) -> u64 {
        self.state.tick
    }

    /// Whether the runtime is latched by a fault.
    #[must_use]
    pub const fn is_faulted(&self) -> bool {
        self.state.faulted
    }

    /// Configuration warnings, such as a quantized timer preset.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.config_diagnostics
    }

    /// How many times the network was actually solved.
    #[must_use]
    pub const fn solve_count(&self) -> u64 {
        self.solver.solve_count()
    }

    /// Header that makes a trace of this run reproducible.
    #[must_use]
    pub const fn trace_header(&self) -> TraceHeader {
        TraceHeader {
            fingerprint: self.circuit.fingerprint(),
            tick_us: self.config.tick_us,
            pickup_ticks: self.config.delays.pickup_ticks,
            dropout_ticks: self.config.delays.dropout_ticks,
            fault_policy: self.config.fault_policy.as_str(),
        }
    }

    /// Execute one tick with the given inputs.
    ///
    /// Every input is validated before any is applied.
    ///
    /// # Errors
    ///
    /// Returns [`SimulationError`] without changing any state and without
    /// consuming a tick when an input is invalid, the runtime is latched, the
    /// trace buffer is full or the tick counter is exhausted.
    pub fn step(&mut self, inputs: &[InputAction]) -> Result<StepOutcome, SimulationError> {
        if self.state.faulted {
            return Err(SimulationError::AlreadyFaulted);
        }
        if self.config.record_trace && self.trace.len() >= self.trace_limit {
            return Err(SimulationError::TraceBufferFull);
        }
        let tick = self.state.tick;
        let next_tick = tick.checked_add(1).ok_or(SimulationError::TickOverflow)?;
        let writes = self.resolve_inputs(inputs)?;

        // 1. Inputs take effect in this tick.
        self.effective.copy_from(&self.state.signals);
        for (signal, value) in writes {
            self.effective.set(signal.index(), value);
        }

        // 2. Closed switches follow the effective signals.
        for (index, switch) in self.circuit.switches().iter().enumerate() {
            let closed = match switch.drive {
                Drive::Always => true,
                Drive::Signal { sig, closed_when } => {
                    self.effective.get(sig.index()) == (closed_when == Level::High)
                }
            };
            self.mask.set_index(index, closed);
        }

        // 3. Solve.
        let solution = self.solver.solve(&self.circuit, &self.mask);

        // 4. Device logic: the derived signals of the next tick, computed
        // from this tick's solution. They only take effect in the next tick,
        // so computing them before recording changes nothing that is
        // recorded; it lets the interlock contention of this commit be
        // reported in this tick.
        self.state.signals.copy_from(&self.effective);
        for (index, device) in self.circuit.devices().iter().enumerate() {
            let (Some(signal), Some(coil)) = (device.signal, device.coil) else {
                continue;
            };
            let energized = solution.load(coil).is_energized();
            let effective = self.effective.get(signal.index());
            let next = match device.class {
                DeviceClass::Contactor | DeviceClass::Relay => {
                    let Some(actuator) = self.state.actuators.get_mut(index) else {
                        continue;
                    };
                    let (state, next) = step_actuator(
                        *actuator,
                        effective,
                        energized,
                        self.config.delays.pickup_ticks,
                        self.config.delays.dropout_ticks,
                    );
                    *actuator = state;
                    next
                }
                DeviceClass::Timer(mode) => {
                    let (Some(timer), Some(preset)) =
                        (self.state.timers.get_mut(index), self.presets.get(index))
                    else {
                        continue;
                    };
                    let (state, next) = step_timer(mode, *preset, *timer, energized, effective);
                    *timer = state;
                    next
                }
                _ => continue,
            };
            self.state.signals.set(signal.index(), next);
        }
        let contentions = apply_interlocks(&self.circuit, &self.effective, &mut self.state.signals);
        let logic_faults = !contentions.is_empty();
        if logic_faults {
            self.tick_faults.clear();
            self.tick_faults.extend_from_slice(solution.faults());
            self.tick_faults.extend(contentions);
            sort_diagnostics(&mut self.tick_faults);
        }
        let tick_faults: &[Diagnostic] = if logic_faults {
            &self.tick_faults
        } else {
            solution.faults()
        };

        // 5. Record what changed against the previous tick.
        let record = self.config.record_trace;
        let mut changed = false;
        for inputs_pass in [true, false] {
            for (index, signal) in self.circuit.signals().iter().enumerate() {
                if signal.kind.is_input() != inputs_pass {
                    continue;
                }
                let (from, to) = (self.view.signals.get(index), self.effective.get(index));
                if from == to {
                    continue;
                }
                changed = true;
                if record {
                    self.trace.push(TraceEvent {
                        tick,
                        kind: TraceKind::Signal {
                            device: signal.owner,
                            kind: signal.kind,
                            from,
                            to,
                        },
                    });
                }
            }
        }
        for index in &self.load_order {
            let now = solution.loads().get(*index).copied().unwrap_or_default();
            let before = self.view.loads.get(*index).copied().unwrap_or_default();
            if now == before {
                continue;
            }
            changed = true;
            if let (true, Some(load)) = (record, self.circuit.loads().get(*index)) {
                let device = self
                    .circuit
                    .device(load.device)
                    .map_or(DeviceId(0), |device| device.id);
                self.trace.push(TraceEvent {
                    tick,
                    kind: TraceKind::Load {
                        device,
                        symbol: load.symbol,
                        activity: activity(now),
                    },
                });
            }
        }
        let faults_changed = record_fault_changes(
            &self.view.faults,
            tick_faults,
            tick,
            record.then_some(&mut self.trace),
        );
        changed |= faults_changed;

        let latches = self.config.fault_policy == FaultPolicy::Halt
            && tick_faults
                .iter()
                .any(|fault| fault.code().latches_runtime());
        if latches {
            changed = true;
            if record {
                self.trace.push(TraceEvent {
                    tick,
                    kind: TraceKind::Latched,
                });
            }
        }

        // 6. Keep what was solved.
        self.view.solved = true;
        self.view.tick = tick;
        self.view.signals.copy_from(&self.effective);
        self.view.loads.clear();
        self.view.loads.extend_from_slice(solution.loads());
        if faults_changed {
            self.view.faults.clear();
            self.view.faults.extend_from_slice(tick_faults);
        }
        self.view.mask.copy_from(&self.mask);

        self.view.timers.clone_from(&self.state.timers);
        self.state.faulted = latches;
        self.state.tick = next_tick;

        Ok(StepOutcome {
            tick,
            changed,
            faulted: latches,
        })
    }

    /// Execute up to `ticks` ticks. The inputs apply to the first tick only.
    /// Stops early when a fault latches the runtime.
    ///
    /// # Errors
    ///
    /// Returns the error of the first tick that could not run; the ticks
    /// before it were executed.
    pub fn step_n(
        &mut self,
        ticks: u32,
        first_tick_inputs: &[InputAction],
    ) -> Result<StepOutcome, SimulationError> {
        let mut outcome = StepOutcome {
            tick: self.state.tick.saturating_sub(1),
            changed: false,
            faulted: self.state.faulted,
        };
        for index in 0..ticks {
            let inputs = if index == 0 { first_tick_inputs } else { &[] };
            let step = self.step(inputs)?;
            outcome = StepOutcome {
                tick: step.tick,
                changed: outcome.changed || step.changed,
                faulted: step.faulted,
            };
            if step.faulted {
                break;
            }
        }
        Ok(outcome)
    }

    /// The state solved in the last executed tick, or `None` before the first
    /// step: nothing has been solved yet, so there is nothing to show.
    #[must_use]
    pub fn snapshot(&self) -> Option<SimSnapshot> {
        if !self.view.solved {
            return None;
        }
        let solution = self.solver.solution();
        let devices = self.circuit.devices();

        let mut energized_devices: Vec<DeviceId> = self
            .circuit
            .loads()
            .iter()
            .zip(self.view.loads.iter())
            .filter(|(_, state)| state.is_energized())
            .filter_map(|(load, _)| self.circuit.device(load.device).map(|device| device.id))
            .collect();
        energized_devices.sort_unstable();
        energized_devices.dedup();

        let with_signal = |wanted: &[SignalKind]| -> Vec<DeviceId> {
            self.circuit
                .signals()
                .iter()
                .enumerate()
                .filter(|(index, signal)| {
                    wanted.contains(&signal.kind) && self.view.signals.get(*index)
                })
                .map(|(_, signal)| signal.owner)
                .collect()
        };

        // A changeover contact owns a switch closed while its signal is high
        // and one closed while it is low; it counts as closed only while its
        // `no` side conducts.
        let owners_by_level = |level: Level| -> BTreeSet<SymbolId> {
            self.circuit
                .switches()
                .iter()
                .filter(|switch| {
                    matches!(switch.drive, Drive::Signal { closed_when, .. } if closed_when == level)
                })
                .map(|switch| switch.owner)
                .collect()
        };
        let opening_at_rest = owners_by_level(Level::High);
        let closed_at_rest = owners_by_level(Level::Low);
        let mut closed_symbols: Vec<SymbolId> = self
            .circuit
            .switches()
            .iter()
            .enumerate()
            .filter(|(index, switch)| match switch.drive {
                Drive::Always => false,
                Drive::Signal { closed_when, .. } => {
                    let changeover_rest_side = closed_when == Level::Low
                        && opening_at_rest.contains(&switch.owner)
                        && closed_at_rest.contains(&switch.owner);
                    !changeover_rest_side && self.view.mask.closed_index(*index)
                }
            })
            .map(|(_, switch)| switch.owner)
            .collect();
        closed_symbols.sort_unstable();
        closed_symbols.dedup();

        let live_wires = self
            .circuit
            .wires()
            .iter()
            .map(|(wire, net)| Live {
                id: *wire,
                conductors: solution.conductors(*net),
            })
            .filter(|live| !live.conductors.is_empty())
            .collect();
        let live_junctions = self
            .circuit
            .junctions()
            .iter()
            .map(|(junction, net)| Live {
                id: *junction,
                conductors: solution.conductors(*net),
            })
            .filter(|live| !live.conductors.is_empty())
            .collect();

        let mut motors: Vec<MotorView> = self
            .circuit
            .loads()
            .iter()
            .zip(self.view.loads.iter())
            .filter(|(load, _)| matches!(load.rule, LoadRule::ThreePhaseMotor { .. }))
            .filter_map(|(load, state)| {
                self.circuit.device(load.device).map(|device| MotorView {
                    device: device.id,
                    state: match state {
                        LoadState::Rotating(Direction::Forward) => MotorState::Forward,
                        LoadState::Rotating(Direction::Reverse) => MotorState::Reverse,
                        LoadState::Off | LoadState::On => MotorState::Stopped,
                    },
                })
            })
            .collect();
        motors.sort_by_key(|motor| motor.device);

        let timers = devices
            .iter()
            .enumerate()
            .filter(|(_, device)| matches!(device.class, DeviceClass::Timer(_)))
            .map(|(index, device)| TimerView {
                device: device.id,
                elapsed_ticks: self
                    .view
                    .timers
                    .get(index)
                    .map_or(0, |timer| timer.elapsed_ticks),
                preset_ticks: self.presets.get(index).copied().unwrap_or(0),
                out: device
                    .signal
                    .is_some_and(|signal| self.view.signals.get(signal.index())),
            })
            .collect();

        Some(SimSnapshot {
            tick: self.view.tick,
            t_us: self
                .view
                .tick
                .saturating_mul(u64::from(self.config.tick_us)),
            energized_devices,
            active_devices: with_signal(&[SignalKind::CoilActive, SignalKind::TimerOut]),
            actuated_devices: with_signal(&[SignalKind::Actuated]),
            tripped_devices: with_signal(&[SignalKind::Tripped]),
            closed_symbols,
            live_wires,
            live_junctions,
            motors,
            timers,
            faulted: self.state.faulted,
            faults: self.view.faults.clone(),
        })
    }

    /// Take the changes recorded since the last call.
    pub fn take_trace(&mut self) -> Vec<TraceEvent> {
        std::mem::take(&mut self.trace)
    }

    /// Save the state for a later [`Simulator::restore`].
    #[must_use]
    pub fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            fingerprint: self.circuit.fingerprint(),
            config: self.config,
            state: self.state.clone(),
            view: self.view.clone(),
        }
    }

    /// Return to a checkpoint. Pending trace events belong to the timeline
    /// being abandoned and are discarded.
    ///
    /// # Errors
    ///
    /// Returns [`RestoreError`] when the checkpoint was taken from another
    /// circuit or with another configuration; the state is unchanged.
    pub fn restore(&mut self, checkpoint: &Checkpoint) -> Result<(), RestoreError> {
        if checkpoint.fingerprint != self.circuit.fingerprint() {
            return Err(RestoreError::CircuitMismatch);
        }
        if checkpoint.config != self.config {
            return Err(RestoreError::ConfigMismatch);
        }
        self.state.clone_from(&checkpoint.state);
        self.view.clone_from(&checkpoint.view);
        self.trace.clear();
        if self.view.solved {
            // Snapshots read group conductors from the solver, so it must
            // hold the solution of the restored tick.
            self.solver.solve(&self.circuit, &self.view.mask);
        }
        Ok(())
    }

    /// Return to tick 0 with the initial state. Clears the fault latch and
    /// the pending trace.
    pub fn reset(&mut self) {
        self.state = initial_state(&self.circuit);
        self.view = initial_view(&self.circuit, &self.state);
        self.trace.clear();
    }

    /// Clear the fault latch only. The circuit and its inputs keep their
    /// state, so a fault whose cause persists is detected again.
    pub fn reset_fault(&mut self) {
        if !self.state.faulted {
            return;
        }
        self.state.faulted = false;
        if self.config.record_trace {
            self.trace.push(TraceEvent {
                tick: self.state.tick,
                kind: TraceKind::LatchReset,
            });
        }
    }

    /// Check every action, then turn the actions into signal writes.
    fn resolve_inputs(&self, inputs: &[InputAction]) -> Result<Vec<(SigIx, bool)>, InvalidInput> {
        let mut writes = Vec::with_capacity(inputs.len());
        let mut targets = Vec::with_capacity(inputs.len());
        for action in inputs {
            let id = action.device();
            let device = self
                .circuit
                .device_by_id(id)
                .ok_or(InvalidInput::UnknownDevice(id))?;
            let value = match (*action, device.class) {
                (InputAction::Press(_), DeviceClass::Pushbutton)
                | (InputAction::Trip(_), DeviceClass::ThermalOverload) => true,
                (InputAction::Release(_), DeviceClass::Pushbutton)
                | (InputAction::ResetTrip(_), DeviceClass::ThermalOverload) => false,
                (InputAction::SetClosed(_, closed), DeviceClass::LatchingSwitch) => closed,
                _ => return Err(InvalidInput::ClassMismatch(id)),
            };
            let signal = device.signal.ok_or(InvalidInput::ClassMismatch(id))?;
            writes.push((signal, value));
            targets.push(id);
        }
        targets.sort_unstable();
        for pair in targets.windows(2) {
            if let [first, second] = pair {
                if first == second {
                    return Err(InvalidInput::DuplicateDevice(*first));
                }
            }
        }
        Ok(writes)
    }
}

fn initial_state(circuit: &CompiledCircuit) -> RuntimeState {
    let mut signals = BitSet::new(circuit.signals().len());
    for device in circuit.devices() {
        if let (true, Some(signal)) = (device.initial_closed, device.signal) {
            signals.set(signal.index(), true);
        }
    }
    RuntimeState {
        tick: 0,
        signals,
        timers: vec![TimerState::default(); circuit.devices().len()],
        actuators: vec![ActuatorState::default(); circuit.devices().len()],
        faulted: false,
    }
}

/// The view before any tick: the initial signals, every load off, no fault.
/// It is what the first tick is compared against.
fn initial_view(circuit: &CompiledCircuit, state: &RuntimeState) -> TickView {
    TickView {
        solved: false,
        tick: 0,
        signals: state.signals.clone(),
        loads: vec![LoadState::Off; circuit.loads().len()],
        faults: Vec::new(),
        timers: state.timers.clone(),
        mask: SwitchMask::for_circuit(circuit),
    }
}

const fn activity(state: LoadState) -> LoadActivity {
    match state {
        LoadState::Off => LoadActivity::Off,
        LoadState::On => LoadActivity::On,
        LoadState::Rotating(Direction::Forward) => LoadActivity::Forward,
        LoadState::Rotating(Direction::Reverse) => LoadActivity::Reverse,
    }
}

/// Mechanical interlocks, break before make: a contactor of a pair whose
/// signal would rise at this commit rises only if its partner is low in this
/// tick and stays low at this commit. When both would rise together neither
/// does, and the contention is returned as a warning of this tick.
/// `effective` holds the signals of the tick, `next` those computed for the
/// next one, which this function corrects.
fn apply_interlocks(
    circuit: &CompiledCircuit,
    effective: &BitSet,
    next: &mut BitSet,
) -> Vec<Diagnostic> {
    let mut contentions = Vec::new();
    for (first, second) in circuit.interlocks() {
        let (Some(a), Some(b)) = (circuit.device(*first), circuit.device(*second)) else {
            continue;
        };
        let (Some(sig_a), Some(sig_b)) = (a.signal, b.signal) else {
            continue;
        };
        let (now_a, now_b) = (effective.get(sig_a.index()), effective.get(sig_b.index()));
        let (next_a, next_b) = (next.get(sig_a.index()), next.get(sig_b.index()));
        let rises_a = !now_a && next_a;
        let rises_b = !now_b && next_b;
        if rises_a && rises_b {
            next.set(sig_a.index(), false);
            next.set(sig_b.index(), false);
            contentions.push(
                Diagnostic::new(
                    DiagnosticCode::InterlockContention,
                    Subject::Device(a.id),
                    "both contactors of a mechanical interlock tried to pick up together; neither did",
                )
                .with_related(vec![Subject::Device(b.id)]),
            );
        } else if rises_a && (now_b || next_b) {
            next.set(sig_a.index(), false);
        } else if rises_b && (now_a || next_a) {
            next.set(sig_b.index(), false);
        }
    }
    contentions
}

/// Compare two canonically sorted fault lists and record which faults
/// cleared and which started. Returns whether anything changed.
fn record_fault_changes(
    before: &[Diagnostic],
    now: &[Diagnostic],
    tick: u64,
    mut trace: Option<&mut Vec<TraceEvent>>,
) -> bool {
    let mut changed = false;
    let mut old = before.iter().peekable();
    let mut new = now.iter().peekable();
    loop {
        let order = match (old.peek(), new.peek()) {
            (None, None) => break,
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (Some(a), Some(b)) => a.canonical_cmp(b),
        };
        let (fault, started) = match order {
            Ordering::Equal => {
                old.next();
                new.next();
                continue;
            }
            Ordering::Less => (old.next(), false),
            Ordering::Greater => (new.next(), true),
        };
        let Some(fault) = fault else {
            break;
        };
        changed = true;
        if let Some(trace) = trace.as_deref_mut() {
            let (code, subject, related) = (
                fault.code(),
                fault.subject().clone(),
                fault.related().to_vec(),
            );
            trace.push(TraceEvent {
                tick,
                kind: if started {
                    TraceKind::FaultStarted {
                        code,
                        subject,
                        related,
                    }
                } else {
                    TraceKind::FaultCleared {
                        code,
                        subject,
                        related,
                    }
                },
            });
        }
    }
    changed
}

#[cfg(test)]
impl Simulator {
    /// Lower the trace bound so a test can reach it.
    pub(crate) fn set_trace_limit(&mut self, limit: usize) {
        self.trace_limit = limit;
    }

    /// Jump the tick counter so a test can reach its end.
    pub(crate) fn set_tick(&mut self, tick: u64) {
        self.state.tick = tick;
    }
}

#[cfg(test)]
#[path = "runtime_tests.rs"]
mod tests;
