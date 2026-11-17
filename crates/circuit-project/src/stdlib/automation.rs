//! Tab 6: automation, logic and control.

use circuit_model::catalog::{Category, ComponentModel, DeviceClass, Operator, SymbolKind};
use circuit_model::library::ArtPrimitive;

use super::{
    block_art, circle, line, load_params, model, polyline, rect, symbol, symbol_with_states,
    symbolic, text, with_vertical_leads,
};

/// A solenoid or valve coil: a box with a plunger that moves when it is
/// picked up.
fn solenoid() -> ComponentModel {
    let at_rest = with_vertical_leads(10, vec![rect(-10, -10, 10, 10), rect(-4, -4, 4, 2)]);
    let picked_up = with_vertical_leads(10, vec![rect(-10, -10, 10, 10), rect(-4, 2, 4, 8)]);
    model(
        "solenoid",
        DeviceClass::Relay,
        Category::Automation,
        "Solenoid or valve coil",
        load_params(),
        vec![symbol_with_states(
            "coil",
            SymbolKind::Coil,
            1,
            1,
            at_rest,
            picked_up,
        )],
    )
    .with_note("Only the coil is simulated. The valve or the plunger it drives is not.")
}

/// A proximity sensor with a two-wire contact output, operated from the
/// operator panel like a limit switch.
fn sensor(name: &str, title: &str, mark: &str) -> ComponentModel {
    let head = |normally_closed: bool| {
        let mut list = with_vertical_leads(
            10,
            vec![
                polyline(&[(0, -10), (10, 0), (0, 10), (-10, 0), (0, -10)]),
                text(0, 0, mark),
            ],
        );
        if normally_closed {
            list.push(line(-5, -17, 5, -13));
        }
        list
    };
    model(
        name,
        DeviceClass::Pushbutton,
        Category::Automation,
        title,
        Vec::new(),
        vec![
            symbol("no", SymbolKind::ContactNo, 0, 1, false, head(false)),
            symbol("nc", SymbolKind::ContactNc, 0, 1, false, head(true)),
        ],
    )
    .with_operator(Operator::Position)
    .with_note("Two-wire contact output. The sensing itself is not simulated: operate it from the simulation panel.")
}

/// A logic or sequence block.
fn block2(name: &str, title: &str, art: Vec<ArtPrimitive>) -> ComponentModel {
    symbolic(name, Category::Automation, title, SymbolKind::Block2, art)
}

pub(super) fn models() -> Vec<ComponentModel> {
    // A negation bubble on the bottom lead of the NOT block.
    let mut not = vec![line(0, -20, 0, -10), line(0, 14, 0, 20)];
    not.push(rect(-10, -10, 10, 10));
    not.push(circle(0, 12, 2));
    not.push(text(0, 0, "1"));
    let mut initial_step = with_vertical_leads(10, vec![rect(-10, -10, 10, 10)]);
    initial_step.push(rect(-7, -7, 7, 7));
    let transition = with_vertical_leads(0, vec![line(-8, 0, 8, 0)]);
    vec![
        solenoid(),
        sensor("sensor_inductive", "Inductive proximity sensor", "I"),
        sensor("sensor_capacitive", "Capacitive proximity sensor", "C"),
        sensor("sensor_optical", "Optical sensor", "O"),
        sensor("sensor_ultrasonic", "Ultrasonic sensor", "U"),
        symbolic(
            "plc",
            Category::Automation,
            "PLC, 8 terminals",
            SymbolKind::Block8,
            block_art(&[-60, -20, 20, 60], 70, 12, "PLC"),
        ),
        symbolic(
            "logic_and",
            Category::Automation,
            "Logic block AND",
            SymbolKind::Block4,
            block_art(&[-20, 20], 30, 10, "&"),
        ),
        symbolic(
            "logic_or",
            Category::Automation,
            "Logic block OR",
            SymbolKind::Block4,
            block_art(&[-20, 20], 30, 10, ">=1"),
        ),
        block2("logic_not", "Logic block NOT", not),
        block2(
            "grafcet_step",
            "Sequence step (GRAFCET)",
            with_vertical_leads(10, vec![rect(-10, -10, 10, 10)]),
        ),
        block2(
            "grafcet_initial_step",
            "Sequence initial step (GRAFCET)",
            initial_step,
        ),
        block2(
            "grafcet_transition",
            "Sequence transition (GRAFCET)",
            transition,
        ),
    ]
}
