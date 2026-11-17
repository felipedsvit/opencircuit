//! Tab 8: lines, cables and connections.

use circuit_model::catalog::{Category, ComponentModel, DeviceClass, SymbolKind};

use super::{line, model, symbol};

pub(super) fn models() -> Vec<ComponentModel> {
    vec![model(
        "busbar",
        DeviceClass::Fuse,
        Category::Lines,
        "Busbar or through link",
        Vec::new(),
        vec![symbol(
            "link",
            SymbolKind::FuseLink,
            1,
            8,
            false,
            vec![
                line(0, -20, 0, 20),
                line(-6, -20, 6, -20),
                line(-6, 20, 6, 20),
            ],
        )],
    )
    .with_note("It always conducts. Place one per conductor: a three-phase connection is three of them side by side.")]
}
