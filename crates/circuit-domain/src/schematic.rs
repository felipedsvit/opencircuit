//! Schematic document, explicit junctions and wires.
//!
//! A visual crossing between two wires is not a connection. Connectivity is
//! expressed by wire endpoints and explicit [`Junction`] entities.

use std::collections::BTreeSet;

use crate::device::Device;
use crate::ids::{DeviceId, JunctionId, SupplyId, SymbolId, WireId};
use crate::supply::SupplyProfile;
use crate::symbol::{GridPoint, SymbolInstance};
use crate::terminal::PinRef;

pub const CURRENT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Junction {
    pub id: JunctionId,
    pub position: GridPoint,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ConnectionEndpoint {
    Terminal(PinRef),
    Junction(JunctionId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Wire {
    pub id: WireId,
    pub from: ConnectionEndpoint,
    pub to: ConnectionEndpoint,
    pub route: Vec<GridPoint>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Schematic {
    pub schema_version: u32,
    pub devices: Vec<Device>,
    pub symbols: Vec<SymbolInstance>,
    pub junctions: Vec<Junction>,
    pub wires: Vec<Wire>,
    pub supplies: Vec<SupplyProfile>,
}

/// Structural problem found without a component catalog or electrical solver.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ValidationIssue {
    DuplicateDeviceId(DeviceId),
    DuplicateSymbolId(SymbolId),
    DuplicateJunctionId(JunctionId),
    DuplicateWireId(WireId),
    DuplicateSupplyId(SupplyId),
    UnknownDeviceForSymbol { symbol: SymbolId, device: DeviceId },
    UnknownSymbolForWire { wire: WireId, symbol: SymbolId },
    UnknownJunctionForWire { wire: WireId, junction: JunctionId },
    SelfReferencingWire(WireId),
}

impl Schematic {
    /// Check only structural invariants that do not require a component
    /// catalog. Terminal and parameter validation belongs to topology
    /// compilation.
    pub fn validate(&self) -> Result<(), Vec<ValidationIssue>> {
        let mut issues = Vec::new();

        collect_duplicate_ids(
            self.devices.iter().map(|device| device.id),
            ValidationIssue::DuplicateDeviceId,
            &mut issues,
        );
        collect_duplicate_ids(
            self.symbols.iter().map(|symbol| symbol.id),
            ValidationIssue::DuplicateSymbolId,
            &mut issues,
        );
        collect_duplicate_ids(
            self.junctions.iter().map(|junction| junction.id),
            ValidationIssue::DuplicateJunctionId,
            &mut issues,
        );
        collect_duplicate_ids(
            self.wires.iter().map(|wire| wire.id),
            ValidationIssue::DuplicateWireId,
            &mut issues,
        );
        collect_duplicate_ids(
            self.supplies.iter().map(|supply| supply.id),
            ValidationIssue::DuplicateSupplyId,
            &mut issues,
        );

        let device_ids: BTreeSet<DeviceId> = self.devices.iter().map(|device| device.id).collect();
        let symbol_ids: BTreeSet<SymbolId> = self.symbols.iter().map(|symbol| symbol.id).collect();
        let junction_ids: BTreeSet<JunctionId> =
            self.junctions.iter().map(|junction| junction.id).collect();

        for symbol in &self.symbols {
            if !device_ids.contains(&symbol.device_id) {
                issues.push(ValidationIssue::UnknownDeviceForSymbol {
                    symbol: symbol.id,
                    device: symbol.device_id,
                });
            }
        }

        for wire in &self.wires {
            if wire.from == wire.to {
                issues.push(ValidationIssue::SelfReferencingWire(wire.id));
            }

            validate_wire_endpoint(wire.id, &wire.from, &symbol_ids, &junction_ids, &mut issues);
            validate_wire_endpoint(wire.id, &wire.to, &symbol_ids, &junction_ids, &mut issues);
        }

        if issues.is_empty() {
            Ok(())
        } else {
            issues.sort();
            issues.dedup();
            Err(issues)
        }
    }
}

fn collect_duplicate_ids<T, F>(
    ids: impl IntoIterator<Item = T>,
    issue: F,
    issues: &mut Vec<ValidationIssue>,
) where
    T: Copy + Ord,
    F: Fn(T) -> ValidationIssue,
{
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id) {
            issues.push(issue(id));
        }
    }
}

fn validate_wire_endpoint(
    wire: WireId,
    endpoint: &ConnectionEndpoint,
    symbols: &BTreeSet<SymbolId>,
    junctions: &BTreeSet<JunctionId>,
    issues: &mut Vec<ValidationIssue>,
) {
    match endpoint {
        ConnectionEndpoint::Terminal(pin) => {
            if !symbols.contains(&pin.symbol_id) {
                issues.push(ValidationIssue::UnknownSymbolForWire {
                    wire,
                    symbol: pin.symbol_id,
                });
            }
        }
        ConnectionEndpoint::Junction(junction) => {
            if !junctions.contains(junction) {
                issues.push(ValidationIssue::UnknownJunctionForWire {
                    wire,
                    junction: *junction,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{DeviceKind, ReferenceDesignator};
    use crate::supply::SupplyKind;
    use crate::symbol::{QuarterTurn, SymbolKind};
    use crate::terminal::TerminalKey;

    fn valid_schematic() -> Schematic {
        Schematic {
            schema_version: CURRENT_SCHEMA_VERSION,
            devices: vec![Device {
                id: DeviceId(1),
                reference: ReferenceDesignator("K1".to_owned()),
                kind: DeviceKind::Contactor,
            }],
            symbols: vec![SymbolInstance {
                id: SymbolId(10),
                device_id: DeviceId(1),
                kind: SymbolKind::Coil,
                position: GridPoint { x: 0, y: 0 },
                rotation: QuarterTurn::Deg0,
            }],
            junctions: vec![Junction {
                id: JunctionId(20),
                position: GridPoint { x: 2, y: 0 },
            }],
            wires: vec![Wire {
                id: WireId(30),
                from: ConnectionEndpoint::Terminal(PinRef {
                    symbol_id: SymbolId(10),
                    terminal: TerminalKey("A1".to_owned()),
                }),
                to: ConnectionEndpoint::Junction(JunctionId(20)),
                route: Vec::new(),
            }],
            supplies: vec![SupplyProfile {
                id: SupplyId(40),
                kind: SupplyKind::Dc,
                nominal_voltage_mv: 24_000,
                frequency_millihz: None,
                phase_sequence: None,
            }],
        }
    }

    #[test]
    fn accepts_a_consistent_document() {
        assert_eq!(valid_schematic().validate(), Ok(()));
    }

    #[test]
    fn reports_duplicate_device_ids() {
        let mut schematic = valid_schematic();
        schematic.devices.push(Device {
            id: DeviceId(1),
            reference: ReferenceDesignator("K2".to_owned()),
            kind: DeviceKind::Relay,
        });

        assert_eq!(
            schematic.validate(),
            Err(vec![ValidationIssue::DuplicateDeviceId(DeviceId(1))])
        );
    }

    #[test]
    fn reports_symbol_with_unknown_device() {
        let mut schematic = valid_schematic();
        schematic.symbols[0].device_id = DeviceId(99);

        assert_eq!(
            schematic.validate(),
            Err(vec![ValidationIssue::UnknownDeviceForSymbol {
                symbol: SymbolId(10),
                device: DeviceId(99),
            }])
        );
    }

    #[test]
    fn reports_wire_endpoint_to_unknown_junction() {
        let mut schematic = valid_schematic();
        schematic.wires[0].to = ConnectionEndpoint::Junction(JunctionId(99));

        assert_eq!(
            schematic.validate(),
            Err(vec![ValidationIssue::UnknownJunctionForWire {
                wire: WireId(30),
                junction: JunctionId(99),
            }])
        );
    }

    #[test]
    fn reports_self_referencing_wire() {
        let mut schematic = valid_schematic();
        schematic.wires[0].to = schematic.wires[0].from.clone();

        assert_eq!(
            schematic.validate(),
            Err(vec![ValidationIssue::SelfReferencingWire(WireId(30))])
        );
    }

    #[test]
    fn validation_issues_are_sorted() {
        let mut schematic = valid_schematic();
        schematic.devices.push(Device {
            id: DeviceId(1),
            reference: ReferenceDesignator("K2".to_owned()),
            kind: DeviceKind::Relay,
        });
        schematic.wires[0].to = ConnectionEndpoint::Junction(JunctionId(99));

        assert_eq!(
            schematic.validate(),
            Err(vec![
                ValidationIssue::DuplicateDeviceId(DeviceId(1)),
                ValidationIssue::UnknownJunctionForWire {
                    wire: WireId(30),
                    junction: JunctionId(99),
                },
            ])
        );
    }
}
