//! The I/O map of a project (ADR-0009): which Modbus address reads or writes
//! which device, in device-ID order.

use std::fmt::Write as _;

use circuit_model::input::operator_controls;
use circuit_model::{
    ComponentCatalog, Control, DeviceClass, DeviceId, InputAction, MotorState, Schematic,
    SimSnapshot,
};

use crate::modbus::{Exception, Request, Response, function};

/// What a discrete input reports.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Observable {
    /// A contactor or relay picked up, a transformer powered, a timer's
    /// output high.
    Active,
    /// A lamp lit or a buzzer sounding.
    Energized,
}

/// One mapped device.
#[derive(Clone, Debug, Eq, PartialEq)]
struct Entry<T> {
    device: DeviceId,
    reference: String,
    kind: T,
}

/// The Modbus table an address belongs to.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Table {
    /// Coils: inputs of the circuit, which a client reads and writes.
    Coil,
    /// Discrete inputs: states of the circuit, which a client reads.
    DiscreteInput,
    /// Input registers: the tick, the fault latch and the motors.
    InputRegister,
}

/// One line of the I/O map: what a client finds at an address.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MapEntry {
    /// The table.
    pub table: Table,
    /// The first address.
    pub address: u16,
    /// How many consecutive addresses it takes: 4 for the tick, 1 for the
    /// rest.
    pub words: u16,
    /// What it is: a reference designator, `tick` or `fault latch`.
    pub name: String,
    /// What a value means.
    pub meaning: &'static str,
}

/// Modbus addresses of the inputs and outputs of a project.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IoMap {
    coils: Vec<Entry<Control>>,
    discrete: Vec<Entry<Observable>>,
    motors: Vec<Entry<()>>,
}

/// Input registers before the motors: four words of the tick, then the
/// fault latch.
const FIXED_REGISTERS: usize = 5;

impl IoMap {
    /// The map of a document, in device-ID order. Devices whose model is
    /// not in `catalog` are left out.
    #[must_use]
    pub fn new(doc: &Schematic, catalog: &ComponentCatalog) -> Self {
        let mut devices: Vec<_> = doc.devices.iter().collect();
        devices.sort_by_key(|device| device.id);
        let mut map = Self::default();
        for device in devices {
            let Some(model) = catalog.model(&device.model) else {
                continue;
            };
            let reference = device.reference.0.clone();
            let class = model.class();
            if let Some(control) = operator_controls(class).first() {
                map.coils.push(Entry {
                    device: device.id,
                    reference: reference.clone(),
                    kind: *control,
                });
            }
            let observable = match class {
                DeviceClass::Contactor
                | DeviceClass::Relay
                | DeviceClass::Transformer
                | DeviceClass::Timer(_) => Some(Observable::Active),
                DeviceClass::Lamp | DeviceClass::Buzzer => Some(Observable::Energized),
                _ => None,
            };
            if let Some(kind) = observable {
                map.discrete.push(Entry {
                    device: device.id,
                    reference: reference.clone(),
                    kind,
                });
            }
            if class == DeviceClass::Motor {
                map.motors.push(Entry {
                    device: device.id,
                    reference,
                    kind: (),
                });
            }
        }
        map
    }

    /// The map as text, one address per line, for `--print-map`.
    #[must_use]
    pub fn describe(&self) -> String {
        let mut text = String::new();
        for entry in self.entries() {
            let range = if entry.words > 1 {
                format!(
                    "{}-{}",
                    entry.address,
                    entry.address.saturating_add(entry.words).saturating_sub(1)
                )
            } else {
                entry.address.to_string()
            };
            let table = match (entry.table, entry.words > 1) {
                (Table::Coil, _) => "coil",
                (Table::DiscreteInput, _) => "discrete input",
                (Table::InputRegister, true) => "input registers",
                (Table::InputRegister, false) => "input register",
            };
            let _ = writeln!(text, "{table} {range} {} ({})", entry.name, entry.meaning);
        }
        text
    }

    /// Every address of the map, in table and address order: the coils, the
    /// discrete inputs, then the input registers.
    #[must_use]
    pub fn entries(&self) -> Vec<MapEntry> {
        let mut entries = Vec::new();
        for (address, entry) in (0_u16..).zip(&self.coils) {
            let meaning = match entry.kind {
                Control::Hold => "1 = pressed",
                Control::Toggle => "1 = closed",
                Control::Trip => "1 = tripped",
            };
            entries.push(MapEntry {
                table: Table::Coil,
                address,
                words: 1,
                name: entry.reference.clone(),
                meaning,
            });
        }
        for (address, entry) in (0_u16..).zip(&self.discrete) {
            let meaning = match entry.kind {
                Observable::Active => "1 = active",
                Observable::Energized => "1 = energized",
            };
            entries.push(MapEntry {
                table: Table::DiscreteInput,
                address,
                words: 1,
                name: entry.reference.clone(),
                meaning,
            });
        }
        entries.push(MapEntry {
            table: Table::InputRegister,
            address: 0,
            words: 4,
            name: "tick".to_owned(),
            meaning: "highest word first",
        });
        entries.push(MapEntry {
            table: Table::InputRegister,
            address: 4,
            words: 1,
            name: "fault latch".to_owned(),
            meaning: "1 = latched",
        });
        let first_motor = u16::try_from(FIXED_REGISTERS).unwrap_or(u16::MAX);
        for (index, entry) in (0_u16..).zip(&self.motors) {
            entries.push(MapEntry {
                table: Table::InputRegister,
                address: first_motor.saturating_add(index),
                words: 1,
                name: entry.reference.clone(),
                meaning: "0 stopped, 1 forward, 2 reverse",
            });
        }
        entries
    }

    /// Answer a request against the last solved snapshot (`None` before the
    /// first tick), and return the input actions its writes ask for, to be
    /// applied at the next tick boundary.
    #[must_use]
    pub fn handle(
        &self,
        request: &Request,
        snapshot: Option<&SimSnapshot>,
    ) -> (Response, Vec<InputAction>) {
        match request {
            Request::ReadCoils { start, count } => (
                bits(
                    function::READ_COILS,
                    &self.coil_states(snapshot),
                    *start,
                    *count,
                ),
                Vec::new(),
            ),
            Request::ReadDiscreteInputs { start, count } => (
                bits(
                    function::READ_DISCRETE_INPUTS,
                    &self.discrete_states(snapshot),
                    *start,
                    *count,
                ),
                Vec::new(),
            ),
            Request::ReadInputRegisters { start, count } => {
                let registers = self.registers(snapshot);
                match window(registers.len(), *start, *count) {
                    Some(range) => (
                        Response::Registers(registers.get(range).unwrap_or_default().to_vec()),
                        Vec::new(),
                    ),
                    None => (
                        exception(
                            function::READ_INPUT_REGISTERS,
                            Exception::IllegalDataAddress,
                        ),
                        Vec::new(),
                    ),
                }
            }
            Request::WriteSingleCoil { address, value } => {
                match self.coils.get(usize::from(*address)) {
                    Some(entry) => (
                        Response::CoilWritten {
                            address: *address,
                            value: *value,
                        },
                        vec![action(entry, *value)],
                    ),
                    None => (
                        exception(function::WRITE_SINGLE_COIL, Exception::IllegalDataAddress),
                        Vec::new(),
                    ),
                }
            }
            Request::WriteMultipleCoils { start, values } => {
                let count = u16::try_from(values.len()).unwrap_or(u16::MAX);
                match window(self.coils.len(), *start, count) {
                    Some(range) => {
                        let actions = self
                            .coils
                            .get(range)
                            .unwrap_or_default()
                            .iter()
                            .zip(values)
                            .map(|(entry, value)| action(entry, *value))
                            .collect();
                        (
                            Response::CoilsWritten {
                                start: *start,
                                count,
                            },
                            actions,
                        )
                    }
                    None => (
                        exception(
                            function::WRITE_MULTIPLE_COILS,
                            Exception::IllegalDataAddress,
                        ),
                        Vec::new(),
                    ),
                }
            }
            Request::Refused {
                function,
                exception: why,
            } => (exception(*function, *why), Vec::new()),
        }
    }

    fn coil_states(&self, snapshot: Option<&SimSnapshot>) -> Vec<bool> {
        self.coils
            .iter()
            .map(|entry| {
                snapshot.is_some_and(|snapshot| match entry.kind {
                    Control::Hold | Control::Toggle => {
                        snapshot.actuated_devices.contains(&entry.device)
                    }
                    Control::Trip => snapshot.tripped_devices.contains(&entry.device),
                })
            })
            .collect()
    }

    fn discrete_states(&self, snapshot: Option<&SimSnapshot>) -> Vec<bool> {
        self.discrete
            .iter()
            .map(|entry| {
                snapshot.is_some_and(|snapshot| match entry.kind {
                    Observable::Active => snapshot.active_devices.contains(&entry.device),
                    Observable::Energized => snapshot.energized_devices.contains(&entry.device),
                })
            })
            .collect()
    }

    fn registers(&self, snapshot: Option<&SimSnapshot>) -> Vec<u16> {
        let tick = snapshot.map_or(0, |snapshot| snapshot.tick);
        let words = tick.to_be_bytes();
        let mut registers: Vec<u16> = words
            .chunks_exact(2)
            .map(|pair| match pair {
                [high, low] => u16::from_be_bytes([*high, *low]),
                _ => 0,
            })
            .collect();
        registers.push(u16::from(snapshot.is_some_and(|snapshot| snapshot.faulted)));
        for entry in &self.motors {
            let state = snapshot
                .and_then(|snapshot| {
                    snapshot
                        .motors
                        .iter()
                        .find(|motor| motor.device == entry.device)
                })
                .map_or(MotorState::Stopped, |motor| motor.state);
            registers.push(match state {
                MotorState::Stopped => 0,
                MotorState::Forward => 1,
                MotorState::Reverse => 2,
            });
        }
        registers
    }
}

/// The addresses `start .. start + count` when they lie inside `len`.
fn window(len: usize, start: u16, count: u16) -> Option<std::ops::Range<usize>> {
    let start = usize::from(start);
    let end = start.checked_add(usize::from(count))?;
    (count > 0 && end <= len).then_some(start..end)
}

fn bits(function: u8, states: &[bool], start: u16, count: u16) -> Response {
    match window(states.len(), start, count) {
        Some(range) => Response::Bits {
            function,
            bits: states.get(range).unwrap_or_default().to_vec(),
        },
        None => exception(function, Exception::IllegalDataAddress),
    }
}

const fn exception(function: u8, exception: Exception) -> Response {
    Response::Exception {
        function,
        exception,
    }
}

/// The action a coil write asks for: engage on 1, disengage on 0.
fn action(entry: &Entry<Control>, value: bool) -> InputAction {
    let [engage, disengage] = entry.kind.actions(entry.device);
    if value { engage } else { disengage }
}

#[cfg(test)]
mod tests {
    use circuit_model::builder::pin;
    use circuit_model::{MotorView, ParamValue, SchematicBuilder};

    use super::*;

    /// S1 (pushbutton), Q1 (main switch), F1 (thermal relay), K1, H1, M1.
    fn doc() -> (Schematic, [DeviceId; 6]) {
        let mut b = SchematicBuilder::new("io");
        let at = circuit_model::GridPoint::new(0, 0);
        let s1 = b.device("S1", "pushbutton_no", &[]).unwrap();
        let s1_contact = b.symbol(s1, "contact", None, at).unwrap();
        let q1 = b.device("Q1", "main_switch.3p", &[]).unwrap();
        let f1 = b.device("F1", "thermal_overload.generic", &[]).unwrap();
        let load = [
            ("rated_voltage_mv", ParamValue::Int(24_000)),
            ("connection", ParamValue::Text("dc".to_owned())),
        ];
        let k1 = b.device("K1", "contactor.generic", &load).unwrap();
        let h1 = b.device("H1", "lamp.generic", &load).unwrap();
        let m1 = b
            .device(
                "M1",
                "motor.3ph",
                &[("rated_voltage_mv", ParamValue::Int(400_000))],
            )
            .unwrap();
        let junction = b.junction(at).unwrap();
        b.wire(pin(s1_contact, "p1"), junction).unwrap();
        (b.finish(), [s1, q1, f1, k1, h1, m1])
    }

    fn snapshot() -> SimSnapshot {
        SimSnapshot {
            tick: 0x0001_0002_0003_0004,
            t_us: 0,
            energized_devices: Vec::new(),
            active_devices: Vec::new(),
            actuated_devices: Vec::new(),
            tripped_devices: Vec::new(),
            closed_symbols: Vec::new(),
            live_wires: Vec::new(),
            live_junctions: Vec::new(),
            motors: Vec::new(),
            timers: Vec::new(),
            faulted: false,
            faults: Vec::new(),
        }
    }

    #[test]
    fn map_lists_inputs_outputs_and_motors_in_device_order() {
        let (doc, _) = doc();
        let map = IoMap::new(&doc, &ComponentCatalog::builtin());

        assert_eq!(
            map.describe(),
            "coil 0 S1 (1 = pressed)\n\
             coil 1 Q1 (1 = closed)\n\
             coil 2 F1 (1 = tripped)\n\
             discrete input 0 K1 (1 = active)\n\
             discrete input 1 H1 (1 = energized)\n\
             input registers 0-3 tick (highest word first)\n\
             input register 4 fault latch (1 = latched)\n\
             input register 5 M1 (0 stopped, 1 forward, 2 reverse)\n"
        );
    }

    #[test]
    fn coil_writes_become_the_actions_of_each_control() {
        let (doc, [s1, q1, f1, ..]) = doc();
        let map = IoMap::new(&doc, &ComponentCatalog::builtin());

        let (response, actions) = map.handle(
            &Request::WriteMultipleCoils {
                start: 0,
                values: vec![true, false, true],
            },
            None,
        );

        assert_eq!(response, Response::CoilsWritten { start: 0, count: 3 });
        assert_eq!(
            actions,
            [
                InputAction::Press(s1),
                InputAction::SetClosed(q1, false),
                InputAction::Trip(f1)
            ]
        );
    }

    #[test]
    fn reads_report_the_snapshot() {
        let (doc, [s1, _, f1, k1, h1, m1]) = doc();
        let map = IoMap::new(&doc, &ComponentCatalog::builtin());
        let mut state = snapshot();
        state.actuated_devices = vec![s1];
        state.tripped_devices = vec![f1];
        state.active_devices = vec![k1];
        state.energized_devices = vec![h1];
        state.faulted = true;
        state.motors = vec![MotorView {
            device: m1,
            state: MotorState::Reverse,
            winding: None,
        }];

        let read = |request| map.handle(&request, Some(&state)).0;

        assert_eq!(
            read(Request::ReadCoils { start: 0, count: 3 }),
            Response::Bits {
                function: 1,
                bits: vec![true, false, true]
            }
        );
        assert_eq!(
            read(Request::ReadDiscreteInputs { start: 0, count: 2 }),
            Response::Bits {
                function: 2,
                bits: vec![true, true]
            }
        );
        assert_eq!(
            read(Request::ReadInputRegisters { start: 0, count: 6 }),
            Response::Registers(vec![1, 2, 3, 4, 1, 2])
        );
    }

    #[test]
    fn addresses_outside_the_map_are_exceptions_and_write_nothing() {
        let (doc, _) = doc();
        let map = IoMap::new(&doc, &ComponentCatalog::builtin());

        for request in [
            Request::ReadCoils { start: 2, count: 2 },
            Request::ReadDiscreteInputs { start: 0, count: 3 },
            Request::ReadInputRegisters { start: 6, count: 1 },
            Request::ReadCoils {
                start: u16::MAX,
                count: 2000,
            },
        ] {
            assert!(matches!(
                map.handle(&request, None).0,
                Response::Exception {
                    exception: Exception::IllegalDataAddress,
                    ..
                }
            ));
        }
        let (response, actions) = map.handle(
            &Request::WriteSingleCoil {
                address: 3,
                value: true,
            },
            None,
        );
        assert!(matches!(response, Response::Exception { .. }));
        assert!(actions.is_empty());
    }
}
