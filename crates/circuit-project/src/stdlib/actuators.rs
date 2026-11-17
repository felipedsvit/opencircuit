//! Tab 7: mechanical actuators.

use circuit_model::catalog::{Category, ComponentModel, SymbolKind};
use circuit_model::library::ArtPrimitive;

use super::{block_art, line, polyline, rect, symbolic};

/// A cylinder: a barrel with a piston and a rod. `spring` draws the return
/// spring of a single-acting one.
fn cylinder(spring: bool) -> Vec<ArtPrimitive> {
    let mut list = vec![
        line(0, -20, 0, -12),
        line(0, 12, 0, 20),
        rect(-10, -12, 10, 12),
        line(-10, -4, 10, -4),
        line(0, -4, 0, 12),
    ];
    if spring {
        list.push(polyline(&[(-6, 2), (6, 5), (-6, 8), (6, 11)]));
    }
    list
}

/// A directional valve: two squares side by side, with a lead to each end
/// of every column of terminals.
fn valve(columns: &[i32], half_width: i32) -> Vec<ArtPrimitive> {
    let mut list = block_art(columns, half_width, 10, "");
    list.push(line(0, -10, 0, 10));
    list.push(line(-10, 6, -4, -6));
    list.push(line(10, -6, 4, 6));
    list
}

pub(super) fn models() -> Vec<ComponentModel> {
    vec![
        symbolic(
            "cylinder_single",
            Category::Actuators,
            "Cylinder, single-acting",
            SymbolKind::Block2,
            cylinder(true),
        ),
        symbolic(
            "cylinder_double",
            Category::Actuators,
            "Cylinder, double-acting",
            SymbolKind::Block2,
            cylinder(false),
        ),
        symbolic(
            "valve_4_ports",
            Category::Actuators,
            "Directional valve, up to 4 ports",
            SymbolKind::Block4,
            valve(&[-20, 20], 30),
        ),
        symbolic(
            "valve_8_ports",
            Category::Actuators,
            "Directional valve, up to 8 ports",
            SymbolKind::Block8,
            valve(&[-60, -20, 20, 60], 70),
        ),
    ]
}
