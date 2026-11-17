//! Tab 8: lines, cables and connections.

use circuit_model::catalog::{
    Category, ComponentModel, DeviceClass, ParamDefinition, ParamType, SymbolKind,
};
use circuit_model::schematic::ParamValue;

use super::{circle, line, model, symbol};

/// A terminal of a terminal strip (ADR-0016): always conducting, numbered by
/// its strip and its number.
fn terminal_block() -> ComponentModel {
    model(
        "terminal_block",
        DeviceClass::TerminalBlock,
        Category::Lines,
        "Terminal block",
        vec![
            ParamDefinition {
                key: "strip".to_owned(),
                ty: ParamType::Name,
                default: Some(ParamValue::Text("X1".to_owned())),
            },
            ParamDefinition {
                key: "number".to_owned(),
                ty: ParamType::Int { min: 1, max: 9_999 },
                default: Some(ParamValue::Int(1)),
            },
        ],
        vec![symbol(
            "terminal",
            SymbolKind::TerminalPoint,
            1,
            1,
            false,
            vec![
                line(0, -20, 0, -5),
                line(0, 5, 0, 20),
                circle(0, 0, 5),
            ],
        )],
    )
    .with_note("A terminal of a strip: it always conducts. Give it the `strip` and the `number` it has on the panel; the terminal plan lists it.")
}

pub(super) fn models() -> Vec<ComponentModel> {
    vec![terminal_block(), model(
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
