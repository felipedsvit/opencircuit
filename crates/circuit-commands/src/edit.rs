//! Primitive edits. Each one changes one entity and returns the edit that
//! undoes it.
//!
//! An edit checks its local preconditions (the entity exists, or does not;
//! nothing still uses what is removed) and refuses instead of half applying.
//! Whole-document rules are checked once per transaction.

use circuit_model::{
    Device, DeviceId, DeviceInfo, Endpoint, GridPoint, Junction, JunctionId, ModelRef, ParamValue,
    ProjectMeta, QuarterTurn, ReferenceDesignator, Schematic, Sheet, SheetId, Subject, SupplyId,
    SupplyProfile, SymbolId, SymbolInstance, Waiver, Wire, WireId,
};

use crate::error::CommandError;
use crate::sorted;

/// One change to one entity of a document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Edit {
    /// Replace the project information: name, grid unit and title-block
    /// fields.
    SetProject(ProjectMeta),
    /// Add a drawing sheet.
    InsertSheet(Sheet),
    /// Remove a drawing sheet.
    RemoveSheet(SheetId),
    /// Replace a sheet with one of the same ID.
    SetSheet(Sheet),
    /// Add a supply profile.
    InsertSupply(SupplyProfile),
    /// Remove a supply profile.
    RemoveSupply(SupplyId),
    /// Replace a supply profile with one of the same ID.
    SetSupply(SupplyProfile),
    /// Add a device.
    InsertDevice(Device),
    /// Remove a device that has no symbol.
    RemoveDevice(DeviceId),
    /// Change the designator of a device.
    SetReference {
        /// The device.
        device: DeviceId,
        /// The new designator.
        reference: ReferenceDesignator,
    },
    /// Set (`Some`) or clear (`None`) one parameter of a device.
    SetParam {
        /// The device.
        device: DeviceId,
        /// The parameter key.
        key: String,
        /// The new value, or `None` to fall back to the model default.
        value: Option<ParamValue>,
    },
    /// Change the model reference of a device.
    SetModel {
        /// The device.
        device: DeviceId,
        /// The new model reference.
        model: ModelRef,
    },
    /// Add a symbol instance of an existing device.
    InsertSymbol(SymbolInstance),
    /// Remove a symbol instance no wire ends at.
    RemoveSymbol(SymbolId),
    /// Move and turn a symbol instance.
    Place {
        /// The symbol.
        symbol: SymbolId,
        /// The new position.
        position: GridPoint,
        /// The new rotation.
        rotation: QuarterTurn,
    },
    /// Add a junction.
    InsertJunction(Junction),
    /// Remove a junction no wire ends at.
    RemoveJunction(JunctionId),
    /// Move a junction.
    MoveJunction {
        /// The junction.
        junction: JunctionId,
        /// The new position.
        position: GridPoint,
    },
    /// Add a wire between existing endpoints.
    InsertWire(Wire),
    /// Remove a wire.
    RemoveWire(WireId),
    /// Replace the intermediate waypoints of a wire.
    SetRoute {
        /// The wire.
        wire: WireId,
        /// The new waypoints.
        route: Vec<GridPoint>,
    },
    /// Set (`Some`) or clear (`None`) the override of the generated number of
    /// a wire.
    SetWireNumber {
        /// The wire.
        wire: WireId,
        /// The new override, or `None` to use the generated number.
        number: Option<String>,
    },
    /// Set (`Some`) or clear (`None`) the catalog data of a device.
    SetDeviceInfo {
        /// The device.
        device: DeviceId,
        /// The new catalog data, or `None` for none.
        info: Option<DeviceInfo>,
    },
    /// Add a waiver; there is at most one for a code and a subject.
    InsertWaiver(Waiver),
    /// Remove the waiver of a code and a subject.
    RemoveWaiver {
        /// The diagnostic code.
        code: String,
        /// The element the diagnostic is about.
        subject: Subject,
    },
}

fn missing(subject: Subject) -> CommandError {
    CommandError::Missing(subject)
}

/// The first wire that ends at `endpoint`.
fn wire_at(doc: &Schematic, at: impl Fn(&Endpoint) -> bool) -> Option<WireId> {
    doc.wires
        .iter()
        .find(|wire| at(&wire.from) || at(&wire.to))
        .map(|wire| wire.id)
}

fn endpoint_exists(doc: &Schematic, endpoint: &Endpoint) -> bool {
    match endpoint {
        Endpoint::Terminal(pin) => sorted::position(&doc.symbols, &pin.symbol_id, |s| s.id).is_ok(),
        Endpoint::Junction(id) => sorted::position(&doc.junctions, id, |j| j.id).is_ok(),
    }
}

fn endpoint_subject(endpoint: &Endpoint) -> Subject {
    match endpoint {
        Endpoint::Terminal(pin) => Subject::Symbol(pin.symbol_id),
        Endpoint::Junction(id) => Subject::Junction(*id),
    }
}

impl Edit {
    /// Apply the edit to a document whose collections are sorted by ID, and
    /// return the edit that undoes it.
    ///
    /// # Errors
    ///
    /// Returns [`CommandError`] when a precondition does not hold; the
    /// document is then unchanged.
    pub(crate) fn apply(self, doc: &mut Schematic) -> Result<Self, CommandError> {
        match self {
            Self::SetProject(project) => Ok(Self::SetProject(std::mem::replace(
                &mut doc.project,
                project,
            ))),
            Self::InsertSheet(sheet) => {
                let id = sheet.id;
                if sorted::insert(&mut doc.sheets, sheet, |s| s.id) {
                    Ok(Self::RemoveSheet(id))
                } else {
                    Err(CommandError::AlreadyExists(Subject::Sheet(id)))
                }
            }
            Self::RemoveSheet(id) => sorted::remove(&mut doc.sheets, &id, |s| s.id)
                .map(Self::InsertSheet)
                .ok_or_else(|| missing(Subject::Sheet(id))),
            Self::SetSheet(sheet) => {
                let id = sheet.id;
                let slot = sorted::find_mut(&mut doc.sheets, &id, |s| s.id)
                    .ok_or_else(|| missing(Subject::Sheet(id)))?;
                Ok(Self::SetSheet(std::mem::replace(slot, sheet)))
            }
            Self::InsertSupply(supply) => {
                let id = supply.id;
                if sorted::insert(&mut doc.supplies, supply, |s| s.id) {
                    Ok(Self::RemoveSupply(id))
                } else {
                    Err(CommandError::AlreadyExists(Subject::Supply(id)))
                }
            }
            Self::RemoveSupply(id) => sorted::remove(&mut doc.supplies, &id, |s| s.id)
                .map(Self::InsertSupply)
                .ok_or_else(|| missing(Subject::Supply(id))),
            Self::SetSupply(supply) => {
                let id = supply.id;
                let slot = sorted::find_mut(&mut doc.supplies, &id, |s| s.id)
                    .ok_or_else(|| missing(Subject::Supply(id)))?;
                Ok(Self::SetSupply(std::mem::replace(slot, supply)))
            }
            Self::InsertDevice(device) => {
                let id = device.id;
                if sorted::insert(&mut doc.devices, device, |d| d.id) {
                    Ok(Self::RemoveDevice(id))
                } else {
                    Err(CommandError::AlreadyExists(Subject::Device(id)))
                }
            }
            Self::RemoveDevice(id) => {
                if let Some(symbol) = doc.symbols.iter().find(|s| s.device_id == id) {
                    return Err(CommandError::InUse {
                        subject: Subject::Device(id),
                        by: Subject::Symbol(symbol.id),
                    });
                }
                sorted::remove(&mut doc.devices, &id, |d| d.id)
                    .map(Self::InsertDevice)
                    .ok_or_else(|| missing(Subject::Device(id)))
            }
            Self::SetReference { device, reference } => {
                let found = sorted::find_mut(&mut doc.devices, &device, |d| d.id)
                    .ok_or_else(|| missing(Subject::Device(device)))?;
                let previous = std::mem::replace(&mut found.reference, reference);
                Ok(Self::SetReference {
                    device,
                    reference: previous,
                })
            }
            Self::SetParam { device, key, value } => {
                let found = sorted::find_mut(&mut doc.devices, &device, |d| d.id)
                    .ok_or_else(|| missing(Subject::Device(device)))?;
                let previous = match value {
                    Some(value) => found.params.insert(key.clone(), value),
                    None => found.params.remove(&key),
                };
                Ok(Self::SetParam {
                    device,
                    key,
                    value: previous,
                })
            }
            Self::SetModel { device, model } => {
                let found = sorted::find_mut(&mut doc.devices, &device, |d| d.id)
                    .ok_or_else(|| missing(Subject::Device(device)))?;
                Ok(Self::SetModel {
                    device,
                    model: std::mem::replace(&mut found.model, model),
                })
            }
            Self::InsertSymbol(symbol) => {
                let id = symbol.id;
                if sorted::position(&doc.devices, &symbol.device_id, |d| d.id).is_err() {
                    return Err(missing(Subject::Device(symbol.device_id)));
                }
                if sorted::insert(&mut doc.symbols, symbol, |s| s.id) {
                    Ok(Self::RemoveSymbol(id))
                } else {
                    Err(CommandError::AlreadyExists(Subject::Symbol(id)))
                }
            }
            Self::RemoveSymbol(id) => {
                let uses = |endpoint: &Endpoint| matches!(endpoint, Endpoint::Terminal(pin) if pin.symbol_id == id);
                if let Some(wire) = wire_at(doc, uses) {
                    return Err(CommandError::InUse {
                        subject: Subject::Symbol(id),
                        by: Subject::Wire(wire),
                    });
                }
                sorted::remove(&mut doc.symbols, &id, |s| s.id)
                    .map(Self::InsertSymbol)
                    .ok_or_else(|| missing(Subject::Symbol(id)))
            }
            Self::Place {
                symbol,
                position,
                rotation,
            } => {
                let found = sorted::find_mut(&mut doc.symbols, &symbol, |s| s.id)
                    .ok_or_else(|| missing(Subject::Symbol(symbol)))?;
                let previous = Self::Place {
                    symbol,
                    position: found.position,
                    rotation: found.rotation,
                };
                found.position = position;
                found.rotation = rotation;
                Ok(previous)
            }
            Self::InsertJunction(junction) => {
                let id = junction.id;
                if sorted::insert(&mut doc.junctions, junction, |j| j.id) {
                    Ok(Self::RemoveJunction(id))
                } else {
                    Err(CommandError::AlreadyExists(Subject::Junction(id)))
                }
            }
            Self::RemoveJunction(id) => {
                let uses = |endpoint: &Endpoint| *endpoint == Endpoint::Junction(id);
                if let Some(wire) = wire_at(doc, uses) {
                    return Err(CommandError::InUse {
                        subject: Subject::Junction(id),
                        by: Subject::Wire(wire),
                    });
                }
                sorted::remove(&mut doc.junctions, &id, |j| j.id)
                    .map(Self::InsertJunction)
                    .ok_or_else(|| missing(Subject::Junction(id)))
            }
            Self::MoveJunction { junction, position } => {
                let found = sorted::find_mut(&mut doc.junctions, &junction, |j| j.id)
                    .ok_or_else(|| missing(Subject::Junction(junction)))?;
                let previous = std::mem::replace(&mut found.position, position);
                Ok(Self::MoveJunction {
                    junction,
                    position: previous,
                })
            }
            Self::InsertWire(wire) => {
                let id = wire.id;
                if wire.from == wire.to {
                    return Err(CommandError::SelfLoop);
                }
                for endpoint in [&wire.from, &wire.to] {
                    if !endpoint_exists(doc, endpoint) {
                        return Err(missing(endpoint_subject(endpoint)));
                    }
                }
                if sorted::insert(&mut doc.wires, wire, |w| w.id) {
                    Ok(Self::RemoveWire(id))
                } else {
                    Err(CommandError::AlreadyExists(Subject::Wire(id)))
                }
            }
            Self::RemoveWire(id) => sorted::remove(&mut doc.wires, &id, |w| w.id)
                .map(Self::InsertWire)
                .ok_or_else(|| missing(Subject::Wire(id))),
            Self::SetRoute { wire, route } => {
                let found = sorted::find_mut(&mut doc.wires, &wire, |w| w.id)
                    .ok_or_else(|| missing(Subject::Wire(wire)))?;
                let previous = std::mem::replace(&mut found.route, route);
                Ok(Self::SetRoute {
                    wire,
                    route: previous,
                })
            }
            Self::SetWireNumber { wire, number } => {
                let found = sorted::find_mut(&mut doc.wires, &wire, |w| w.id)
                    .ok_or_else(|| missing(Subject::Wire(wire)))?;
                Ok(Self::SetWireNumber {
                    wire,
                    number: std::mem::replace(&mut found.number, number),
                })
            }
            Self::SetDeviceInfo { device, info } => {
                let found = sorted::find_mut(&mut doc.devices, &device, |d| d.id)
                    .ok_or_else(|| missing(Subject::Device(device)))?;
                Ok(Self::SetDeviceInfo {
                    device,
                    info: std::mem::replace(&mut found.info, info),
                })
            }
            Self::InsertWaiver(waiver) => {
                let at = waiver_position(doc, &waiver.code, &waiver.subject);
                match at {
                    Ok(_) => Err(CommandError::AlreadyExists(waiver.subject)),
                    Err(at) => {
                        let inverse = Self::RemoveWaiver {
                            code: waiver.code.clone(),
                            subject: waiver.subject.clone(),
                        };
                        doc.waivers.insert(at, waiver);
                        Ok(inverse)
                    }
                }
            }
            Self::RemoveWaiver { code, subject } => match waiver_position(doc, &code, &subject) {
                Ok(at) => Ok(Self::InsertWaiver(doc.waivers.remove(at))),
                Err(_) => Err(missing(subject)),
            },
        }
    }
}

/// Where the waiver of a code and a subject is, or would be, in the sorted
/// list of waivers.
fn waiver_position(doc: &Schematic, code: &str, subject: &Subject) -> Result<usize, usize> {
    doc.waivers
        .binary_search_by(|waiver| waiver.sort_key().cmp(&(code, subject)))
}

#[cfg(test)]
mod tests {
    use circuit_model::builder::pin;
    use circuit_model::{SchematicBuilder, SupplyKind, SymbolKey};

    use super::*;

    /// A device with one symbol, a junction and a wire between them.
    fn doc() -> Schematic {
        let mut b = SchematicBuilder::new("edits");
        b.dc_supply(24_000).unwrap();
        let device = b.device("S1", "pushbutton_no", &[]).unwrap();
        let contact = b
            .symbol(device, "contact", None, GridPoint::new(0, 0))
            .unwrap();
        let junction = b.junction(GridPoint::new(0, 4)).unwrap();
        b.wire(pin(contact, "p2"), junction).unwrap();
        b.finish()
    }

    fn doc_project() -> ProjectMeta {
        doc().project
    }

    /// Applying an edit then its inverse gives back the same document.
    fn round_trip(edit: Edit) {
        let original = doc();
        let mut edited = original.clone();

        let inverse = edit.clone().apply(&mut edited).unwrap();
        assert_ne!(edited, original, "{edit:?} changed nothing");
        inverse.apply(&mut edited).unwrap();

        assert_eq!(edited, original, "{edit:?}");
    }

    #[test]
    fn every_kind_of_edit_is_undone_by_its_inverse() {
        let doc = doc();
        let device = doc.devices[0].clone();
        let symbol = doc.symbols[0].clone();
        let junction = doc.junctions[0].clone();
        let wire = doc.wires[0].clone();
        let supply = doc.supplies[0].clone();
        let new = 100;
        let sheet = Sheet {
            id: SheetId(new),
            number: 1,
            title: "one".to_owned(),
            origin: GridPoint::new(0, 0),
            size: circuit_model::SheetSize::A4,
            orientation: circuit_model::Orientation::Landscape,
            zones: circuit_model::Zones {
                columns: 8,
                rows: 6,
            },
        };
        for edit in [
            Edit::SetProject(ProjectMeta {
                author: Some("A".to_owned()),
                ..doc_project()
            }),
            Edit::InsertSheet(sheet.clone()),
            Edit::InsertSupply(SupplyProfile {
                id: SupplyId(new),
                ..supply.clone()
            }),
            Edit::SetSupply(SupplyProfile {
                nominal_voltage_mv: 12_000,
                ..supply.clone()
            }),
            Edit::InsertDevice(Device {
                id: DeviceId(new),
                reference: ReferenceDesignator("S9".to_owned()),
                ..device.clone()
            }),
            Edit::SetReference {
                device: device.id,
                reference: ReferenceDesignator("S7".to_owned()),
            },
            Edit::SetParam {
                device: device.id,
                key: "anything".to_owned(),
                value: Some(ParamValue::Int(3)),
            },
            Edit::InsertSymbol(SymbolInstance {
                id: SymbolId(new),
                ..symbol.clone()
            }),
            Edit::Place {
                symbol: symbol.id,
                position: GridPoint::new(5, 5),
                rotation: QuarterTurn::Deg90,
            },
            Edit::InsertJunction(Junction {
                id: JunctionId(new),
                position: GridPoint::new(9, 9),
            }),
            Edit::MoveJunction {
                junction: junction.id,
                position: GridPoint::new(1, 4),
            },
            Edit::InsertWire(Wire {
                id: WireId(new),
                from: pin(symbol.id, "p1"),
                to: Endpoint::Junction(junction.id),
                route: Vec::new(),
                number: None,
            }),
            Edit::RemoveWire(wire.id),
            Edit::SetRoute {
                wire: wire.id,
                route: vec![GridPoint::new(0, 3)],
            },
            Edit::SetWireNumber {
                wire: wire.id,
                number: Some("17".to_owned()),
            },
            Edit::SetDeviceInfo {
                device: device.id,
                info: Some(DeviceInfo {
                    part_no: Some("A-1".to_owned()),
                    ..DeviceInfo::default()
                }),
            },
            Edit::InsertWaiver(waiver("OC-STR-018", Subject::Wire(wire.id))),
        ] {
            round_trip(edit);
        }
    }

    fn waiver(code: &str, subject: Subject) -> Waiver {
        Waiver {
            code: code.to_owned(),
            subject,
            reason: "accepted".to_owned(),
        }
    }

    #[test]
    fn waivers_stay_sorted_are_unique_and_are_removed_by_code_and_subject() {
        let mut edited = doc();
        let original = edited.clone();
        let wire = |id| Subject::Wire(WireId(id));
        let mut inverses = Vec::new();
        for (code, subject) in [
            ("OC-STR-019", wire(1)),
            ("OC-STR-018", wire(9)),
            ("OC-STR-018", wire(2)),
        ] {
            inverses.push(
                Edit::InsertWaiver(waiver(code, subject))
                    .apply(&mut edited)
                    .unwrap(),
            );
        }
        let order: Vec<_> = edited
            .waivers
            .iter()
            .map(|w| (w.code.as_str(), w.subject.clone()))
            .collect();
        assert_eq!(
            order,
            [
                ("OC-STR-018", wire(2)),
                ("OC-STR-018", wire(9)),
                ("OC-STR-019", wire(1)),
            ]
        );
        assert!(edited.is_canonical());

        assert!(matches!(
            Edit::InsertWaiver(waiver("OC-STR-018", wire(9))).apply(&mut edited),
            Err(CommandError::AlreadyExists(_))
        ));
        assert!(matches!(
            Edit::RemoveWaiver {
                code: "OC-STR-018".to_owned(),
                subject: wire(5)
            }
            .apply(&mut edited),
            Err(CommandError::Missing(_))
        ));
        assert_eq!(edited.waivers.len(), 3);

        for inverse in inverses.into_iter().rev() {
            inverse.apply(&mut edited).unwrap();
        }
        assert_eq!(edited, original);
    }

    #[test]
    fn a_number_and_device_info_of_something_missing_are_refused() {
        let mut edited = doc();

        assert!(matches!(
            Edit::SetWireNumber {
                wire: WireId(900),
                number: None
            }
            .apply(&mut edited),
            Err(CommandError::Missing(Subject::Wire(_)))
        ));
        assert!(matches!(
            Edit::SetDeviceInfo {
                device: DeviceId(900),
                info: None
            }
            .apply(&mut edited),
            Err(CommandError::Missing(Subject::Device(_)))
        ));
    }

    #[test]
    fn a_sheet_is_replaced_and_removed_by_edits_that_undo_themselves() {
        let mut edited = doc();
        let sheet = Sheet {
            id: SheetId(100),
            number: 1,
            title: "one".to_owned(),
            origin: GridPoint::new(0, 0),
            size: circuit_model::SheetSize::A4,
            orientation: circuit_model::Orientation::Landscape,
            zones: circuit_model::Zones {
                columns: 8,
                rows: 6,
            },
        };
        let original = edited.clone();
        let mut inverses = vec![Edit::InsertSheet(sheet.clone()).apply(&mut edited).unwrap()];
        inverses.push(
            Edit::SetSheet(Sheet {
                title: "other".to_owned(),
                ..sheet.clone()
            })
            .apply(&mut edited)
            .unwrap(),
        );
        assert_eq!(edited.sheets[0].title, "other");

        assert!(matches!(
            Edit::InsertSheet(sheet.clone()).apply(&mut edited),
            Err(CommandError::AlreadyExists(Subject::Sheet(_)))
        ));
        assert!(matches!(
            Edit::RemoveSheet(SheetId(7)).apply(&mut edited),
            Err(CommandError::Missing(Subject::Sheet(_)))
        ));
        assert!(matches!(
            Edit::SetSheet(Sheet {
                id: SheetId(7),
                ..sheet
            })
            .apply(&mut edited),
            Err(CommandError::Missing(Subject::Sheet(_)))
        ));
        for inverse in inverses.into_iter().rev() {
            inverse.apply(&mut edited).unwrap();
        }

        assert_eq!(edited, original);
    }

    #[test]
    fn removals_are_undone_once_nothing_uses_the_entity() {
        let mut edited = doc();
        let original = edited.clone();
        let (wire, junction, symbol, device, supply) = (
            original.wires[0].id,
            original.junctions[0].id,
            original.symbols[0].id,
            original.devices[0].id,
            original.supplies[0].id,
        );

        let mut inverses = Vec::new();
        for edit in [
            Edit::RemoveWire(wire),
            Edit::RemoveJunction(junction),
            Edit::RemoveSymbol(symbol),
            Edit::RemoveDevice(device),
            Edit::RemoveSupply(supply),
        ] {
            inverses.push(edit.apply(&mut edited).unwrap());
        }
        for inverse in inverses.into_iter().rev() {
            inverse.apply(&mut edited).unwrap();
        }

        assert_eq!(edited, original);
    }

    #[test]
    fn symbol_with_a_wire_cannot_be_removed() {
        let mut edited = doc();
        let symbol = edited.symbols[0].id;

        let refused = Edit::RemoveSymbol(symbol).apply(&mut edited);

        assert_eq!(
            refused,
            Err(CommandError::InUse {
                subject: Subject::Symbol(symbol),
                by: Subject::Wire(edited.wires[0].id),
            })
        );
        assert_eq!(edited, doc());
    }

    #[test]
    fn device_with_a_symbol_cannot_be_removed() {
        let mut edited = doc();
        let device = edited.devices[0].id;

        assert!(matches!(
            Edit::RemoveDevice(device).apply(&mut edited),
            Err(CommandError::InUse { .. })
        ));
    }

    #[test]
    fn junction_with_a_wire_cannot_be_removed() {
        let mut edited = doc();
        let junction = edited.junctions[0].id;

        assert!(matches!(
            Edit::RemoveJunction(junction).apply(&mut edited),
            Err(CommandError::InUse { .. })
        ));
    }

    #[test]
    fn inserting_an_existing_id_is_refused() {
        let mut edited = doc();
        let device = edited.devices[0].clone();

        assert_eq!(
            Edit::InsertDevice(device.clone()).apply(&mut edited),
            Err(CommandError::AlreadyExists(Subject::Device(device.id)))
        );
    }

    #[test]
    fn wire_to_itself_is_refused() {
        let mut edited = doc();
        let junction = edited.junctions[0].id;

        assert_eq!(
            Edit::InsertWire(Wire {
                id: WireId(100),
                from: Endpoint::Junction(junction),
                to: Endpoint::Junction(junction),
                route: Vec::new(),
                number: None,
            })
            .apply(&mut edited),
            Err(CommandError::SelfLoop)
        );
    }

    #[test]
    fn wire_to_a_missing_symbol_is_refused() {
        let mut edited = doc();
        let junction = edited.junctions[0].id;

        assert_eq!(
            Edit::InsertWire(Wire {
                id: WireId(100),
                from: Endpoint::Junction(junction),
                to: pin(SymbolId(77), "p1"),
                route: Vec::new(),
                number: None,
            })
            .apply(&mut edited),
            Err(CommandError::Missing(Subject::Symbol(SymbolId(77))))
        );
    }

    #[test]
    fn symbol_of_a_missing_device_is_refused() {
        let mut edited = doc();

        assert_eq!(
            Edit::InsertSymbol(SymbolInstance {
                id: SymbolId(100),
                device_id: DeviceId(77),
                symbol_key: SymbolKey("contact".to_owned()),
                slot: None,
                position: GridPoint::default(),
                rotation: QuarterTurn::Deg0,
            })
            .apply(&mut edited),
            Err(CommandError::Missing(Subject::Device(DeviceId(77))))
        );
    }

    #[test]
    fn editing_a_missing_entity_is_refused() {
        let mut edited = doc();

        for edit in [
            Edit::RemoveSupply(SupplyId(77)),
            Edit::SetSupply(SupplyProfile {
                id: SupplyId(77),
                kind: SupplyKind::Dc,
                nominal_voltage_mv: 24_000,
                phase_voltage_mv: None,
                frequency_millihz: None,
                phase_sequence: None,
            }),
            Edit::RemoveDevice(DeviceId(77)),
            Edit::SetReference {
                device: DeviceId(77),
                reference: ReferenceDesignator("K1".to_owned()),
            },
            Edit::RemoveWire(WireId(77)),
            Edit::MoveJunction {
                junction: JunctionId(77),
                position: GridPoint::default(),
            },
            Edit::Place {
                symbol: SymbolId(77),
                position: GridPoint::default(),
                rotation: QuarterTurn::Deg0,
            },
        ] {
            assert!(
                matches!(
                    edit.clone().apply(&mut edited),
                    Err(CommandError::Missing(_))
                ),
                "{edit:?}"
            );
        }
        assert_eq!(edited, doc());
    }
}
