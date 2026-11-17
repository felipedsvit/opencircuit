//! Tab 4: motors and power.

use circuit_model::catalog::{Category, ComponentModel, DeviceClass, SymbolKind};
use circuit_model::library::ArtPrimitive;

use super::{
    block_art, circle, line, load_params, model, plain, rated_voltage, rect, symbol, symbolic,
    text, with_vertical_leads,
};

/// A two-terminal load drawn as a machine: a circle with two letters.
fn machine(name: &str, title: &str, top: &str, bottom: &str, note: &str) -> ComponentModel {
    let art = with_vertical_leads(
        10,
        vec![circle(0, 0, 10), text(0, -3, top), text(0, 5, bottom)],
    );
    model(
        name,
        DeviceClass::Lamp,
        Category::Motors,
        title,
        load_params(),
        vec![symbol("load", SymbolKind::Lamp, 1, 1, false, art)],
    )
    .with_note(note)
}

/// A drive: main contacts that pass the three phases while its RUN coil is
/// energized.
fn drive(name: &str, title: &str, label: &str, note: &str) -> ComponentModel {
    let mut art: Vec<ArtPrimitive> = Vec::new();
    for x in [-40, 0, 40] {
        art.push(line(x, -20, x, -12));
        art.push(line(x, 12, x, 20));
    }
    art.push(rect(-50, -12, 50, 12));
    art.push(text(0, 0, label));
    model(
        name,
        DeviceClass::Contactor,
        Category::Motors,
        title,
        load_params(),
        vec![
            plain("run", SymbolKind::Coil, 1, 1, false),
            symbol(
                "main",
                SymbolKind::PowerContacts { poles: 3 },
                0,
                1,
                false,
                art,
            ),
            plain("aux_no", SymbolKind::ContactNo, 0, 8, true),
            plain("aux_nc", SymbolKind::ContactNc, 0, 8, true),
        ],
    )
    .with_note(note)
}

pub(super) fn models() -> Vec<ComponentModel> {
    let resistor = with_vertical_leads(10, vec![rect(-5, -10, 5, 10)]);
    // The outer leads sit on the edge of the symbol area: one box under all
    // three, with the three phases entering it.
    let mut heater_3p: Vec<ArtPrimitive> = [-40, 0, 40]
        .into_iter()
        .map(|x| line(x, -20, x, 0))
        .collect();
    heater_3p.push(rect(-40, 0, 40, 30));
    heater_3p.push(text(0, 15, "3~ R"));
    vec![
        machine(
            "motor_1ph",
            "Motor, single-phase",
            "M",
            "1~",
            "Only the energized state is simulated: it has no direction, speed or torque.",
        ),
        machine(
            "motor_dc",
            "Motor, direct current",
            "M",
            "=",
            "Only the energized state is simulated: it has no direction, speed or torque.",
        ),
        model(
            "heater_1p",
            DeviceClass::Lamp,
            Category::Motors,
            "Heater or resistor, single-phase",
            load_params(),
            vec![symbol("load", SymbolKind::Lamp, 1, 1, false, resistor)],
        )
        .with_note("A two-terminal load: it is energized or not; there is no current or power."),
        model(
            "heater_3p",
            DeviceClass::Motor,
            Category::Motors,
            "Heater, three-phase",
            vec![rated_voltage()],
            vec![symbol("load", SymbolKind::Motor, 1, 1, false, heater_3p)],
        )
        .with_note("It is evaluated as a three-phase load, so it may show forward or reverse like a motor; the direction has no meaning for a heater."),
        drive(
            "soft_starter",
            "Soft starter (RUN input)",
            "SS",
            "Approximation: the three phases pass while the RUN coil is energized. There is no ramp.",
        ),
        drive(
            "frequency_inverter",
            "Frequency inverter (RUN input)",
            "VFD",
            "Approximation: the three phases pass while the RUN coil is energized. Frequency and speed are not simulated.",
        ),
        symbolic(
            "servo_motor",
            Category::Motors,
            "Servo motor",
            SymbolKind::Block4,
            block_art(&[-20, 20], 30, 10, "SM"),
        ),
        symbolic(
            "stepper_motor",
            Category::Motors,
            "Stepper motor",
            SymbolKind::Block4,
            block_art(&[-20, 20], 30, 10, "ST"),
        ),
    ]
}
