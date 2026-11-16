//! Tab 1: supplies and sources.

use circuit_model::Conductor;
use circuit_model::catalog::{Category, ComponentModel, DeviceClass, SymbolKind};
use circuit_model::library::ArtPrimitive;

use super::{
    block_art, circle, fixed_conductor, line, model, polyline, supply_ref, symbol, symbolic, text,
};

/// A supply terminal of one conductor. The symbol is 20 subunits tall above
/// its terminal, which is the origin.
fn terminal(
    name: &str,
    title: &str,
    conductor: Conductor,
    art: Vec<ArtPrimitive>,
    note: Option<&str>,
) -> ComponentModel {
    let part = model(
        name,
        DeviceClass::PowerSupply,
        Category::Supplies,
        title,
        vec![fixed_conductor(conductor), supply_ref()],
        vec![symbol(
            "terminal",
            SymbolKind::SupplyTerminal,
            1,
            255,
            false,
            art,
        )],
    );
    match note {
        Some(note) => part.with_note(note),
        None => part,
    }
}

/// A bar across a stem, with the name of the conductor above it.
fn bar(label: &str) -> Vec<ArtPrimitive> {
    vec![
        line(0, -12, 0, 0),
        line(-8, -12, 8, -12),
        text(0, -17, label),
    ]
}

/// The earth symbol: a stem and three bars that shorten upwards.
fn ground() -> Vec<ArtPrimitive> {
    vec![
        line(0, -8, 0, 0),
        line(-8, -8, 8, -8),
        line(-5, -12, 5, -12),
        line(-2, -16, 2, -16),
    ]
}

/// A battery terminal: the stem ends on the long plate of the positive
/// terminal and on the short plate of the negative one.
fn battery(positive: bool) -> Vec<ArtPrimitive> {
    let (stem_plate, other_plate): (i32, i32) = if positive { (8, 4) } else { (4, 8) };
    vec![
        line(0, -8, 0, 0),
        line(stem_plate.saturating_neg(), -8, stem_plate, -8),
        line(other_plate.saturating_neg(), -13, other_plate, -13),
        text(0, -18, if positive { "+" } else { "-" }),
    ]
}

/// A socket outlet: a circle on the stem with the contact named inside.
fn socket(label: &str) -> Vec<ArtPrimitive> {
    vec![line(0, -2, 0, 0), circle(0, -10, 8), text(0, -10, label)]
}

pub(super) fn models() -> Vec<ComponentModel> {
    let mut models = vec![
        terminal("supply_l1", "Line L1", Conductor::L1, bar("L1"), None),
        terminal("supply_l2", "Line L2", Conductor::L2, bar("L2"), None),
        terminal("supply_l3", "Line L3", Conductor::L3, bar("L3"), None),
        terminal("supply_n", "Neutral N", Conductor::N, bar("N"), None),
        terminal(
            "supply_pe",
            "Protective earth PE",
            Conductor::Pe,
            ground(),
            None,
        ),
        terminal(
            "supply_dc_pos",
            "DC positive",
            Conductor::DcPos,
            bar("+"),
            None,
        ),
        terminal(
            "supply_dc_neg",
            "DC negative",
            Conductor::DcNeg,
            bar("-"),
            None,
        ),
        terminal(
            "supply_dc_ground",
            "DC ground or chassis",
            Conductor::Pe,
            ground(),
            Some(
                "It provides the protective-earth conductor: the simulation has no separate chassis conductor.",
            ),
        ),
        terminal(
            "battery_pos",
            "Battery, positive terminal",
            Conductor::DcPos,
            battery(true),
            Some(
                "A battery is two terminals: place the positive and the negative one on the same DC supply.",
            ),
        ),
        terminal(
            "battery_neg",
            "Battery, negative terminal",
            Conductor::DcNeg,
            battery(false),
            Some(
                "A battery is two terminals: place the positive and the negative one on the same DC supply.",
            ),
        ),
        terminal(
            "socket_l1",
            "Socket outlet, line",
            Conductor::L1,
            socket("L"),
            Some(
                "A socket is one terminal per contact: place the line, neutral and earth ones on the same supply.",
            ),
        ),
        terminal(
            "socket_n",
            "Socket outlet, neutral",
            Conductor::N,
            socket("N"),
            Some(
                "A socket is one terminal per contact: place the line, neutral and earth ones on the same supply.",
            ),
        ),
        terminal(
            "socket_pe",
            "Socket outlet, earth",
            Conductor::Pe,
            socket("PE"),
            Some(
                "A socket is one terminal per contact: place the line, neutral and earth ones on the same supply.",
            ),
        ),
    ];
    models.extend(symbol_only());
    models
}

/// Parts of this tab that the discrete model cannot simulate.
fn symbol_only() -> Vec<ComponentModel> {
    let mut diode = vec![line(0, -20, 0, -8), line(0, 8, 0, 20), line(-8, 8, 8, 8)];
    diode.push(polyline(&[(-8, -8), (8, -8), (0, 8), (-8, -8)]));
    let mut bridge = block_art(&[-20, 20], 30, 10, "");
    bridge.push(polyline(&[(-20, 0), (0, -8), (20, 0), (0, 8), (-20, 0)]));
    bridge.push(text(-24, 6, "+"));
    bridge.push(text(24, 6, "-"));
    let autotransformer = block_art(&[-20, 20], 30, 10, "AT");
    vec![
        symbolic(
            "diode",
            Category::Supplies,
            "Diode",
            SymbolKind::Block2,
            diode,
        ),
        symbolic(
            "rectifier_bridge",
            Category::Supplies,
            "Bridge rectifier",
            SymbolKind::Block4,
            bridge,
        ),
        symbolic(
            "transformer_3p",
            Category::Supplies,
            "Three-phase transformer",
            SymbolKind::Block8,
            block_art(&[-60, -20, 20, 60], 70, 12, "3~ TR"),
        ),
        symbolic(
            "autotransformer",
            Category::Supplies,
            "Autotransformer",
            SymbolKind::Block4,
            autotransformer,
        ),
    ]
}
