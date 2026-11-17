//! Tab 2: protection.

use circuit_model::catalog::{Category, ComponentModel, DeviceClass, SymbolKind};
use circuit_model::library::ArtPrimitive;

use super::{
    block_art, circle, initial_closed, line, model, plain, polyline, rect, symbol,
    symbol_with_states, symbolic, with_vertical_leads,
};

/// A fuse: two terminals, always conducting (melting is not simulated).
fn fuse(name: &str, title: &str, art: Vec<ArtPrimitive>) -> ComponentModel {
    model(
        name,
        DeviceClass::Fuse,
        Category::Protection,
        title,
        Vec::new(),
        vec![symbol("link", SymbolKind::FuseLink, 1, 1, false, art)],
    )
    .with_note("A fuse never melts in the simulation: it always conducts.")
}

/// Horizontal offsets of the poles of a symbol with `poles` poles, in
/// subunits: one pitch of 40 apart and centred on the origin.
fn pole_columns(poles: u8) -> &'static [i32] {
    match poles {
        1 => &[0],
        2 => &[-20, 20],
        3 => &[-40, 0, 40],
        _ => &[-60, -20, 20, 60],
    }
}

/// The contacts of a circuit breaker: each pole with a cross on its upper
/// lead and, open, a tilted blade or, closed, an upright one.
fn breaker_art(poles: u8, closed: bool) -> Vec<ArtPrimitive> {
    let mut list = Vec::new();
    let columns = pole_columns(poles);
    for &x in columns {
        list.push(line(x, -20, x, -10));
        list.push(line(x, 10, x, 20));
        let tip = if closed { x } else { x.saturating_sub(7) };
        list.push(line(x, 10, tip, -10));
        list.push(line(x.saturating_sub(3), -18, x.saturating_add(3), -14));
        list.push(line(x.saturating_sub(3), -14, x.saturating_add(3), -18));
    }
    if let (Some(&first), Some(&last)) = (columns.first(), columns.last()) {
        if first != last {
            list.push(line(first, 0, last, 0));
        }
    }
    list
}

fn breaker(poles: u8, title: &str, name: &str) -> ComponentModel {
    model(
        name,
        DeviceClass::LatchingSwitch,
        Category::Protection,
        title,
        vec![initial_closed()],
        vec![symbol_with_states(
            "main",
            SymbolKind::PowerContacts { poles },
            1,
            1,
            breaker_art(poles, false),
            breaker_art(poles, true),
        )],
    )
    .with_note("It is switched by hand; there is no current in the simulation, so it never trips by overload.")
}

/// The current path of a motor protection breaker: a box with a cross on
/// each of its three poles.
fn motor_breaker_art() -> Vec<ArtPrimitive> {
    let mut list = Vec::new();
    for x in [-40, 0, 40] {
        list.push(line(x, -20, x, -12));
        list.push(line(x, 12, x, 20));
        list.push(rect(x.saturating_sub(6), -12, x.saturating_add(6), 12));
        list.push(line(x.saturating_sub(4), -8, x.saturating_add(4), 8));
        list.push(line(x.saturating_sub(4), 8, x.saturating_add(4), -8));
    }
    list
}

pub(super) fn models() -> Vec<ComponentModel> {
    let cartridge = with_vertical_leads(12, vec![rect(-4, -12, 4, 12), line(0, -12, 0, 12)]);
    let diazed = with_vertical_leads(
        10,
        vec![circle(0, 0, 10), line(0, -10, 0, 10), line(-4, 0, 4, 0)],
    );
    let blade = with_vertical_leads(
        14,
        vec![
            rect(-6, -14, 6, 14),
            line(0, -14, 0, 14),
            line(-6, -8, 6, -8),
            line(-6, 8, 6, 8),
        ],
    );
    let glass = with_vertical_leads(
        12,
        vec![
            rect(-4, -12, 4, 12),
            polyline(&[(0, -12), (3, -6), (-3, 0), (3, 6), (0, 12)]),
        ],
    );
    let mut surge = with_vertical_leads(10, vec![rect(-8, -10, 8, 10)]);
    surge.push(line(-8, 8, 8, -8));
    let mut models = vec![
        fuse("fuse_cartridge", "Fuse, cartridge", cartridge),
        fuse("fuse_diazed", "Fuse, Diazed (screw type)", diazed),
        fuse("fuse_nh", "Fuse, NH blade", blade),
        fuse("fuse_glass", "Fuse, glass tube", glass),
        breaker(1, "Circuit breaker, 1-pole (MCB)", "mcb_1p"),
        breaker(2, "Circuit breaker, 2-pole (MCB)", "mcb_2p"),
        breaker(3, "Circuit breaker, 3-pole (MCB)", "mcb_3p"),
        breaker(4, "Circuit breaker, 4-pole (MCB)", "mcb_4p"),
        model(
            "motor_breaker_3p",
            DeviceClass::ThermalOverload,
            Category::Protection,
            "Motor protection breaker, 3-pole",
            Vec::new(),
            vec![
                symbol(
                    "element",
                    SymbolKind::ThermalElement { poles: 3 },
                    0,
                    1,
                    false,
                    motor_breaker_art(),
                ),
                plain("aux_nc", SymbolKind::ContactNc, 0, 1, false),
                plain("aux_no", SymbolKind::ContactNo, 0, 1, false),
            ],
        )
        .with_note("It is tripped by hand from the operator panel; its poles stay closed when tripped. Use the auxiliary contacts to open the control circuit."),
    ];
    models.extend([
        symbolic(
            "spd",
            Category::Protection,
            "Surge protective device (SPD)",
            SymbolKind::Block2,
            surge,
        ),
        symbolic(
            "rcd_2p",
            Category::Protection,
            "Residual-current device (RCD), 2-pole",
            SymbolKind::Block4,
            block_art(&[-20, 20], 30, 10, "RCD"),
        ),
        symbolic(
            "rcd_4p",
            Category::Protection,
            "Residual-current device (RCD), 4-pole",
            SymbolKind::Block8,
            block_art(&[-60, -20, 20, 60], 70, 10, "RCD"),
        ),
    ]);
    models
}
