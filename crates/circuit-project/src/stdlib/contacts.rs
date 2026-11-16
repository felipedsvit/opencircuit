//! Tab 5: contacts and terminals.

use circuit_model::catalog::{Category, ComponentModel, DeviceClass, SymbolKind};
use circuit_model::library::ArtPrimitive;

use super::{circle, dot, model, rect, symbol, with_vertical_leads};

const ALWAYS_CLOSED: &str =
    "Terminals conduct all the time: it joins what is wired to its two sides.";

/// A terminal or a connection point: two terminals, always conducting.
fn through(name: &str, title: &str, levels: u8, art: Vec<ArtPrimitive>) -> ComponentModel {
    model(
        name,
        DeviceClass::Fuse,
        Category::Contacts,
        title,
        Vec::new(),
        vec![symbol("link", SymbolKind::FuseLink, 1, levels, false, art)],
    )
    .with_note(ALWAYS_CLOSED)
}

pub(super) fn models() -> Vec<ComponentModel> {
    vec![
        through(
            "terminal_square",
            "Terminal, square",
            1,
            with_vertical_leads(10, vec![rect(-8, -10, 8, 10)]),
        ),
        through(
            "terminal_round",
            "Terminal, round",
            1,
            with_vertical_leads(8, vec![circle(0, 0, 8)]),
        ),
        through(
            "terminal_stacked",
            "Terminal block, stacked levels (2 to 4)",
            4,
            with_vertical_leads(8, vec![circle(0, 0, 8), dot(0, 0, 2)]),
        ),
        through(
            "connection_point",
            "Connection point (In/Out)",
            1,
            with_vertical_leads(4, vec![circle(0, 0, 4)]),
        ),
    ]
}
