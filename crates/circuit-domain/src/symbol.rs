//! Graphical symbol instances placed on the schematic grid.
//!
//! Coordinates are logical document units, never screen pixels.

use crate::ids::{DeviceId, SymbolId};

#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct GridPoint {
    pub x: i32,
    pub y: i32,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum QuarterTurn {
    Deg0,
    Deg90,
    Deg180,
    Deg270,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SymbolKind {
    Coil,
    ContactNormallyOpen,
    ContactNormallyClosed,
    ChangeoverContact,
    Pushbutton,
    Motor,
    Lamp,
    Buzzer,
    SupplyTerminal,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SymbolInstance {
    pub id: SymbolId,
    pub device_id: DeviceId,
    pub kind: SymbolKind,
    pub position: GridPoint,
    pub rotation: QuarterTurn,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn symbol_instance_references_its_device() {
        let symbol = SymbolInstance {
            id: SymbolId(12),
            device_id: DeviceId(7),
            kind: SymbolKind::Coil,
            position: GridPoint { x: 4, y: -2 },
            rotation: QuarterTurn::Deg90,
        };

        assert_eq!(symbol.device_id, DeviceId(7));
        assert_ne!(symbol.id.0, symbol.device_id.0);
    }

    #[test]
    fn quarter_turn_has_only_named_right_angle_rotations() {
        let rotations = [
            QuarterTurn::Deg0,
            QuarterTurn::Deg90,
            QuarterTurn::Deg180,
            QuarterTurn::Deg270,
        ];

        assert_eq!(rotations.len(), 4);
    }
}
