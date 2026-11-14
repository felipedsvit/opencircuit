//! One open project: the document, its compilation and its simulator.
//!
//! Nothing here knows about egui. The window shows what a session holds and
//! forwards operator actions to it; every electrical decision is made by the
//! core crates. Per frame the window calls [`Session::advance`] once: it runs
//! the ticks due, collects their trace events and takes one snapshot.

#![cfg_attr(not(test), deny(clippy::arithmetic_side_effects))]

use std::collections::{BTreeMap, VecDeque};
use std::fmt;
use std::path::{Path, PathBuf};

use circuit_model::catalog::PARAM_INITIAL_CLOSED;
use circuit_model::{
    ComponentCatalog, Control, DeviceClass, DeviceId, Diagnostic, InputAction, LayoutIndex,
    Schematic, SimSnapshot, operator_controls,
};
use circuit_project::{MAX_PROJECT_BYTES, ProjectError};
use circuit_renderer::{Scene, SceneOptions, build_scene};
use circuit_sim::{SimConfig, SimulationError, Simulator, compile, format_event};

use crate::clock::SimClock;
use crate::files::{FileError, read_text};

/// Most trace lines kept for the events panel; older ones are dropped.
pub const MAX_EVENTS: usize = 1_000;

/// Why a project could not be opened.
#[derive(Debug)]
pub enum LoadError {
    /// The file could not be read.
    File(FileError),
    /// The text is not a valid project.
    Project {
        /// Where the text came from, for the message.
        origin: String,
        /// What is wrong with it.
        error: ProjectError,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(error) => error.fmt(f),
            Self::Project { origin, error } => write!(f, "{origin}: {error}"),
        }
    }
}

impl std::error::Error for LoadError {}

/// A device the operator can act on.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Operable {
    /// The device.
    pub device: DeviceId,
    /// Its designator.
    pub reference: String,
    /// What the operator can do to it.
    pub control: Control,
}

/// An open project.
#[derive(Debug)]
pub struct Session {
    path: Option<PathBuf>,
    doc: Schematic,
    catalog: ComponentCatalog,
    /// Compilation diagnostics, then configuration diagnostics.
    diagnostics: Vec<Diagnostic>,
    /// `None` when the project does not compile.
    simulator: Option<Simulator>,
    snapshot: Option<SimSnapshot>,
    running: bool,
    clock: SimClock,
    /// Actions for the next tick, one per device; the last one wins.
    pending: BTreeMap<DeviceId, InputAction>,
    /// The latest trace lines, oldest first.
    events: VecDeque<String>,
    /// Why the simulation stopped or refused an action, until the next
    /// successful command.
    message: Option<String>,
}

impl Session {
    /// Open a project file.
    ///
    /// # Errors
    ///
    /// Returns [`LoadError`] when the file cannot be read or is not a valid
    /// project. A project that loads but does not compile is not an error:
    /// it opens without a simulator and with its diagnostics.
    pub fn open(path: &Path) -> Result<Self, LoadError> {
        let text = read_text(path, MAX_PROJECT_BYTES).map_err(LoadError::File)?;
        Self::load(&text, Some(path.to_path_buf()))
    }

    /// Load a project from its text.
    ///
    /// # Errors
    ///
    /// Returns [`LoadError::Project`] when the text is not a valid project.
    pub fn load(text: &str, path: Option<PathBuf>) -> Result<Self, LoadError> {
        let doc = circuit_project::from_str(text).map_err(|error| LoadError::Project {
            origin: path
                .as_deref()
                .map_or_else(|| "project".to_owned(), |p| p.display().to_string()),
            error,
        })?;
        let catalog = ComponentCatalog::builtin();
        let output = compile(&doc, &catalog);
        let mut diagnostics = output.diagnostics;
        let simulator = match output
            .circuit
            .map(|c| Simulator::new(c, SimConfig::default()))
        {
            Some(Ok(simulator)) => {
                diagnostics.extend_from_slice(simulator.diagnostics());
                Some(simulator)
            }
            Some(Err(error)) => {
                diagnostics.push(error.diagnostic());
                None
            }
            None => None,
        };
        Ok(Self {
            path,
            doc,
            catalog,
            diagnostics,
            simulator,
            snapshot: None,
            running: false,
            clock: SimClock::default(),
            pending: BTreeMap::new(),
            events: VecDeque::new(),
            message: None,
        })
    }

    /// The file the project came from, if any.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// The document.
    #[must_use]
    pub const fn document(&self) -> &Schematic {
        &self.doc
    }

    /// The component catalog the document was compiled against.
    #[must_use]
    pub const fn catalog(&self) -> &ComponentCatalog {
        &self.catalog
    }

    /// Compilation and configuration diagnostics, in canonical order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// Whether the project compiled and can be simulated.
    #[must_use]
    pub const fn can_simulate(&self) -> bool {
        self.simulator.is_some()
    }

    /// The state of the last solved tick, or `None` before the first one.
    #[must_use]
    pub const fn snapshot(&self) -> Option<&SimSnapshot> {
        self.snapshot.as_ref()
    }

    /// The scene of the document in its current state.
    #[must_use]
    pub fn scene(&self, options: &SceneOptions) -> Scene {
        build_scene(&self.doc, &self.catalog, self.snapshot.as_ref(), options)
    }

    /// Whether the simulation advances with real time.
    #[must_use]
    pub const fn is_running(&self) -> bool {
        self.running
    }

    /// Whether the runtime is latched by a fault.
    #[must_use]
    pub fn is_faulted(&self) -> bool {
        self.snapshot.as_ref().is_some_and(|s| s.faulted)
    }

    /// The next tick the simulator executes.
    #[must_use]
    pub fn next_tick(&self) -> u64 {
        self.simulator.as_ref().map_or(0, Simulator::tick)
    }

    /// The duration of one tick in microseconds.
    #[must_use]
    pub fn tick_us(&self) -> u32 {
        self.simulator
            .as_ref()
            .map_or(0, |sim| sim.config().tick_us)
    }

    /// The real-time clock and its speed.
    #[must_use]
    pub const fn clock(&self) -> &SimClock {
        &self.clock
    }

    /// The real-time clock, to change its speed.
    pub const fn clock_mut(&mut self) -> &mut SimClock {
        &mut self.clock
    }

    /// The latest trace lines, oldest first.
    pub fn events(&self) -> impl Iterator<Item = &str> {
        self.events.iter().map(String::as_str)
    }

    /// Why the simulation stopped or refused an action, if it did.
    #[must_use]
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// Run with real time. Does nothing while latched by a fault.
    pub fn play(&mut self) {
        if self.simulator.is_some() && !self.is_faulted() {
            self.running = true;
            self.clock.reset();
            self.message = None;
        }
    }

    /// Stop advancing; pending actions wait for the next tick.
    pub fn pause(&mut self) {
        self.running = false;
    }

    /// Pause and run exactly one tick with the pending actions.
    pub fn step(&mut self) {
        self.running = false;
        self.run_ticks(1);
    }

    /// Back to tick 0 with the initial state, paused.
    pub fn reset(&mut self) {
        if let Some(sim) = &mut self.simulator {
            sim.reset();
        }
        self.running = false;
        self.snapshot = None;
        self.pending.clear();
        self.events.clear();
        self.clock.reset();
        self.message = None;
    }

    /// Clear the fault latch. A fault whose cause persists is detected again
    /// at the next tick.
    pub fn reset_fault(&mut self) {
        if let Some(sim) = &mut self.simulator {
            sim.reset_fault();
        }
        self.collect_events();
        if let Some(snapshot) = &mut self.snapshot {
            snapshot.faulted = false;
        }
        self.message = None;
    }

    /// Account for `real_us` microseconds of real time: while running, run
    /// the ticks that are due at the current speed.
    pub fn advance(&mut self, real_us: u64) {
        if !self.running {
            return;
        }
        let ticks = self.clock.advance(real_us, self.tick_us());
        if ticks > 0 {
            self.run_ticks(ticks);
        }
    }

    /// The devices the operator can act on, by ID.
    #[must_use]
    pub fn operable(&self) -> Vec<Operable> {
        if self.simulator.is_none() {
            return Vec::new();
        }
        let mut devices: Vec<Operable> = self
            .doc
            .devices
            .iter()
            .filter_map(|device| {
                let model = self.catalog.model(&device.model)?;
                let control = operator_controls(model.class()).first().copied()?;
                Some(Operable {
                    device: device.id,
                    reference: device.reference.0.clone(),
                    control,
                })
            })
            .collect();
        devices.sort_by_key(|operable| operable.device);
        devices.dedup_by_key(|operable| operable.device);
        devices
    }

    /// The control of a device, if the operator can act on it.
    #[must_use]
    pub fn control_of(&self, device: DeviceId) -> Option<Control> {
        let layout = LayoutIndex::new(&self.doc, &self.catalog);
        let model = self.catalog.model(&layout.device(device)?.model)?;
        operator_controls(model.class()).first().copied()
    }

    /// Whether a control is engaged: the button held, the switch closed, the
    /// thermal relay tripped. An action waiting for the next tick counts.
    #[must_use]
    pub fn engaged(&self, device: DeviceId, control: Control) -> bool {
        if let Some(action) = self.pending.get(&device) {
            return matches!(
                action,
                InputAction::Press(_) | InputAction::SetClosed(_, true) | InputAction::Trip(_)
            );
        }
        match (&self.snapshot, control) {
            (Some(snapshot), Control::Hold | Control::Toggle) => {
                snapshot.actuated_devices.contains(&device)
            }
            (Some(snapshot), Control::Trip) => snapshot.tripped_devices.contains(&device),
            (None, Control::Toggle) => self.initially_closed(device),
            (None, Control::Hold | Control::Trip) => false,
        }
    }

    /// Whether an action for this device waits for the next tick.
    #[must_use]
    pub fn is_pending(&self, device: DeviceId) -> bool {
        self.pending.contains_key(&device)
    }

    /// Engage a control that is released, or release one that is engaged.
    /// The action applies at the next tick.
    pub fn operate(&mut self, device: DeviceId, control: Control) {
        let [engage, release] = control.actions(device);
        let action = if self.engaged(device, control) {
            release
        } else {
            engage
        };
        self.queue(action);
    }

    /// Queue an action for the next tick. A later action on the same device
    /// replaces it, since the simulator accepts one action per device and
    /// tick.
    pub fn queue(&mut self, action: InputAction) {
        if self.simulator.is_some() {
            self.pending.insert(action.device(), action);
        }
    }

    fn initially_closed(&self, device: DeviceId) -> bool {
        let layout = LayoutIndex::new(&self.doc, &self.catalog);
        layout.device(device).is_some_and(|device| {
            self.catalog.model(&device.model).is_some_and(|model| {
                model.class() == DeviceClass::LatchingSwitch
                    && model.bool_param(&device.params, PARAM_INITIAL_CLOSED) == Some(true)
            })
        })
    }

    /// Run up to `ticks` ticks, the pending actions applying to the first,
    /// then collect the trace and take one snapshot.
    fn run_ticks(&mut self, ticks: u32) {
        let Some(sim) = &mut self.simulator else {
            return;
        };
        let inputs: Vec<InputAction> = std::mem::take(&mut self.pending).into_values().collect();
        match sim.step_n(ticks, &inputs) {
            Ok(outcome) if outcome.faulted => {
                self.running = false;
                self.message = Some(format!(
                    "Latched by a fault at tick {}. Reset the fault to continue.",
                    outcome.tick
                ));
            }
            Ok(_) => {}
            Err(SimulationError::AlreadyFaulted) => {
                self.running = false;
                self.message =
                    Some("The runtime is latched by a fault. Reset the fault first.".to_owned());
            }
            Err(error) => {
                self.running = false;
                self.message = Some(error.to_string());
            }
        }
        self.collect_events();
        if let Some(sim) = &self.simulator {
            self.snapshot = sim.snapshot();
        }
    }

    /// Move the pending trace of the simulator to the events, keeping the
    /// latest [`MAX_EVENTS`].
    fn collect_events(&mut self) {
        let Some(sim) = &mut self.simulator else {
            return;
        };
        for event in sim.take_trace() {
            self.events.push_back(format_event(sim.circuit(), &event));
        }
        while self.events.len() > MAX_EVENTS {
            self.events.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMAND: &str = include_str!("../../../examples/direct-starter/command.ocircuit");
    const REVERSING: &str = include_str!("../../../examples/forward-reverse/reversing.ocircuit");
    const SERIES_LOADS: &str =
        include_str!("../../../tests/fixtures/series-loads/series-loads.ocircuit");

    fn codes(session: &Session) -> Vec<&'static str> {
        session
            .diagnostics()
            .iter()
            .map(|d| d.code().as_str())
            .collect()
    }

    fn device(session: &Session, reference: &str) -> DeviceId {
        session
            .document()
            .devices
            .iter()
            .find(|d| d.reference.0 == reference)
            .map(|d| d.id)
            .unwrap()
    }

    fn active(session: &Session, reference: &str) -> bool {
        let id = device(session, reference);
        session
            .snapshot()
            .is_some_and(|s| s.active_devices.contains(&id))
    }

    #[test]
    fn example_that_compiles_can_be_simulated() {
        let session = Session::load(COMMAND, None).unwrap();

        assert!(session.can_simulate());
    }

    #[test]
    fn example_that_compiles_has_no_diagnostic() {
        let session = Session::load(COMMAND, None).unwrap();

        assert_eq!(codes(&session), Vec::<&str>::new());
    }

    #[test]
    fn project_that_does_not_compile_opens_with_its_diagnostics() {
        let session = Session::load(SERIES_LOADS, None).unwrap();

        assert!(!session.can_simulate());
        assert_eq!(codes(&session), ["OC-MOD-001", "OC-SEM-006"]);
    }

    #[test]
    fn project_that_does_not_compile_is_still_drawn() {
        let session = Session::load(SERIES_LOADS, None).unwrap();

        assert!(!session.scene(&SceneOptions::default()).items.is_empty());
    }

    #[test]
    fn project_that_does_not_compile_cannot_be_played() {
        let mut session = Session::load(SERIES_LOADS, None).unwrap();

        session.play();

        assert!(!session.is_running());
    }

    #[test]
    fn text_that_is_not_a_project_is_refused_with_its_origin() {
        let error = Session::load("{", Some(PathBuf::from("broken.ocircuit"))).unwrap_err();

        assert!(
            error.to_string().starts_with("broken.ocircuit: "),
            "{error}"
        );
    }

    #[test]
    fn nothing_is_solved_before_the_first_tick() {
        let session = Session::load(COMMAND, None).unwrap();

        assert_eq!(session.snapshot(), None);
    }

    #[test]
    fn pressing_start_picks_up_the_contactor_and_the_seal_keeps_it() {
        let mut session = Session::load(COMMAND, None).unwrap();
        let s1 = device(&session, "S1");

        session.operate(s1, Control::Hold);
        session.step();
        session.step();
        session.operate(s1, Control::Hold);
        session.step();
        session.step();

        assert!(active(&session, "K1"));
        assert!(!session.engaged(s1, Control::Hold));
    }

    #[test]
    fn paused_session_does_not_advance_with_time() {
        let mut session = Session::load(COMMAND, None).unwrap();

        session.advance(1_000_000);

        assert_eq!(session.next_tick(), 0);
    }

    #[test]
    fn running_session_advances_with_real_time() {
        let mut session = Session::load(COMMAND, None).unwrap();

        session.play();
        session.advance(16_000);

        assert_eq!(session.next_tick(), 16);
        assert!(session.snapshot().is_some());
    }

    #[test]
    fn pending_action_counts_as_engaged_before_the_tick() {
        let mut session = Session::load(COMMAND, None).unwrap();
        let s1 = device(&session, "S1");

        session.operate(s1, Control::Hold);

        assert!(session.engaged(s1, Control::Hold));
        assert!(session.is_pending(s1));
    }

    #[test]
    fn later_action_on_a_device_replaces_the_pending_one() {
        let mut session = Session::load(COMMAND, None).unwrap();
        let s1 = device(&session, "S1");

        session.queue(InputAction::Press(s1));
        session.queue(InputAction::Release(s1));
        session.step();

        assert!(!active(&session, "K1"));
        assert!(session.message().is_none());
    }

    #[test]
    fn steps_record_trace_events_in_the_normative_format() {
        let mut session = Session::load(COMMAND, None).unwrap();
        let s1 = device(&session, "S1");

        session.operate(s1, Control::Hold);
        session.step();

        assert_eq!(
            session.events().collect::<Vec<_>>(),
            ["t=0 input S1.pressed 0->1", "t=0 load K1.coil on"]
        );
    }

    #[test]
    fn simultaneous_start_latches_pauses_and_explains() {
        let mut session = Session::load(REVERSING, None).unwrap();
        let (s1, s3) = (device(&session, "S1"), device(&session, "S3"));
        session.play();

        session.operate(s1, Control::Hold);
        session.operate(s3, Control::Hold);
        session.advance(10_000);

        assert!(session.is_faulted());
        assert!(!session.is_running());
        assert!(session.message().is_some_and(|m| m.contains("Latched")));
    }

    #[test]
    fn reset_fault_clears_the_latch_and_allows_playing_again() {
        let mut session = Session::load(REVERSING, None).unwrap();
        let (s1, s3) = (device(&session, "S1"), device(&session, "S3"));
        session.operate(s1, Control::Hold);
        session.operate(s3, Control::Hold);
        session.step();
        session.step();

        session.reset_fault();
        session.play();

        assert!(!session.is_faulted());
        assert!(session.is_running());
    }

    #[test]
    fn reset_returns_to_tick_zero_with_nothing_pending() {
        let mut session = Session::load(COMMAND, None).unwrap();
        let s1 = device(&session, "S1");
        session.operate(s1, Control::Hold);
        session.step();
        session.operate(s1, Control::Hold);

        session.reset();

        assert_eq!(session.next_tick(), 0);
        assert_eq!(session.snapshot(), None);
        assert!(!session.is_pending(s1));
        assert_eq!(session.events().count(), 0);
    }

    #[test]
    fn operable_devices_are_the_pushbuttons_of_the_control_circuit() {
        let session = Session::load(COMMAND, None).unwrap();

        let references: Vec<String> = session
            .operable()
            .into_iter()
            .map(|operable| operable.reference)
            .collect();

        assert_eq!(references, ["S2", "S1"]);
    }

    #[test]
    fn project_that_does_not_compile_offers_no_controls() {
        let session = Session::load(SERIES_LOADS, None).unwrap();

        assert_eq!(session.operable(), []);
    }

    #[test]
    fn events_keep_only_the_latest_lines() {
        let mut session = Session::load(COMMAND, None).unwrap();
        let s1 = device(&session, "S1");

        for _ in 0..MAX_EVENTS {
            session.operate(s1, Control::Hold);
            session.step();
        }

        assert_eq!(session.events().count(), MAX_EVENTS);
    }
}
