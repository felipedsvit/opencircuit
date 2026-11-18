//! Programmatic construction of documents.
//!
//! The builder allocates IDs from the document counter and appends entities.
//! It performs no validation: run [`Schematic::validate`] or the compiler on
//! the result. It exists for fixtures, examples and tests; interactive editing
//! with undo belongs to the editor.

use crate::geometry::{GridPoint, QuarterTurn};
use crate::ids::{DeviceId, IdAllocator, IdExhausted, JunctionId, SupplyId, SymbolId, WireId};
use crate::schematic::{
    Device, Endpoint, Junction, ModelRef, ParamValue, PinRef, ReferenceDesignator, Schematic,
    SymbolInstance, SymbolKey, TerminalKey, Wire,
};
use crate::supply::{PhaseSequence, SupplyKind, SupplyProfile};

/// Builds a [`Schematic`] entity by entity.
#[derive(Clone, Debug)]
pub struct SchematicBuilder {
    doc: Schematic,
    ids: IdAllocator,
}

impl SchematicBuilder {
    /// Start an empty document.
    #[must_use]
    pub fn new(name: &str) -> Self {
        Self {
            doc: Schematic::new(name),
            ids: IdAllocator::new(),
        }
    }

    /// Add a DC supply profile.
    ///
    /// # Errors
    ///
    /// Returns [`IdExhausted`] when the document has no identifier left.
    pub fn dc_supply(&mut self, voltage_mv: u32) -> Result<SupplyId, IdExhausted> {
        self.supply(SupplyKind::Dc, voltage_mv, None, None, None)
    }

    /// Add a three-phase supply profile.
    ///
    /// # Errors
    ///
    /// Returns [`IdExhausted`] when the document has no identifier left.
    pub fn three_phase_supply(
        &mut self,
        line_voltage_mv: u32,
        phase_voltage_mv: Option<u32>,
        frequency_millihz: u32,
        sequence: PhaseSequence,
    ) -> Result<SupplyId, IdExhausted> {
        self.supply(
            SupplyKind::ThreePhase,
            line_voltage_mv,
            phase_voltage_mv,
            Some(frequency_millihz),
            Some(sequence),
        )
    }

    /// Add a supply profile with every field given explicitly.
    ///
    /// # Errors
    ///
    /// Returns [`IdExhausted`] when the document has no identifier left.
    pub fn supply(
        &mut self,
        kind: SupplyKind,
        nominal_voltage_mv: u32,
        phase_voltage_mv: Option<u32>,
        frequency_millihz: Option<u32>,
        phase_sequence: Option<PhaseSequence>,
    ) -> Result<SupplyId, IdExhausted> {
        let id = SupplyId(self.ids.alloc()?);
        self.doc.supplies.push(SupplyProfile {
            id,
            kind,
            nominal_voltage_mv,
            phase_voltage_mv,
            frequency_millihz,
            phase_sequence,
        });
        Ok(id)
    }

    /// Add a device that uses version 1 of a model.
    ///
    /// # Errors
    ///
    /// Returns [`IdExhausted`] when the document has no identifier left.
    pub fn device(
        &mut self,
        reference: &str,
        model_id: &str,
        params: &[(&str, ParamValue)],
    ) -> Result<DeviceId, IdExhausted> {
        self.device_version(reference, model_id, 1, params)
    }

    /// Add a device that uses a given version of a model.
    ///
    /// # Errors
    ///
    /// Returns [`IdExhausted`] when the document has no identifier left.
    pub fn device_version(
        &mut self,
        reference: &str,
        model_id: &str,
        version: u32,
        params: &[(&str, ParamValue)],
    ) -> Result<DeviceId, IdExhausted> {
        let id = DeviceId(self.ids.alloc()?);
        self.doc.devices.push(Device {
            id,
            reference: ReferenceDesignator(reference.to_owned()),
            model: ModelRef {
                id: model_id.to_owned(),
                version,
            },
            params: params
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect(),
            info: None,
        });
        Ok(id)
    }

    /// Add a symbol instance of a device.
    ///
    /// # Errors
    ///
    /// Returns [`IdExhausted`] when the document has no identifier left.
    pub fn symbol(
        &mut self,
        device: DeviceId,
        symbol_key: &str,
        slot: Option<u8>,
        position: GridPoint,
    ) -> Result<SymbolId, IdExhausted> {
        self.symbol_turned(device, symbol_key, slot, position, QuarterTurn::Deg0)
    }

    /// Add a symbol instance of a device, turned by `rotation`.
    ///
    /// # Errors
    ///
    /// Returns [`IdExhausted`] when the document has no identifier left.
    pub fn symbol_turned(
        &mut self,
        device: DeviceId,
        symbol_key: &str,
        slot: Option<u8>,
        position: GridPoint,
        rotation: QuarterTurn,
    ) -> Result<SymbolId, IdExhausted> {
        let id = SymbolId(self.ids.alloc()?);
        self.doc.symbols.push(SymbolInstance {
            id,
            device_id: device,
            symbol_key: SymbolKey(symbol_key.to_owned()),
            slot,
            position,
            rotation,
            layer: None,
            mirrored: false,
        });
        Ok(id)
    }

    /// Add a junction.
    ///
    /// # Errors
    ///
    /// Returns [`IdExhausted`] when the document has no identifier left.
    pub fn junction(&mut self, position: GridPoint) -> Result<JunctionId, IdExhausted> {
        let id = JunctionId(self.ids.alloc()?);
        self.doc.junctions.push(Junction { id, position });
        Ok(id)
    }

    /// Add a wire with no intermediate waypoint.
    ///
    /// # Errors
    ///
    /// Returns [`IdExhausted`] when the document has no identifier left.
    pub fn wire(
        &mut self,
        from: impl Into<Endpoint>,
        to: impl Into<Endpoint>,
    ) -> Result<WireId, IdExhausted> {
        self.wire_routed(from, to, &[])
    }

    /// Add a wire through intermediate waypoints. The two ends are derived
    /// from the endpoints and are not part of `route`.
    ///
    /// # Errors
    ///
    /// Returns [`IdExhausted`] when the document has no identifier left.
    pub fn wire_routed(
        &mut self,
        from: impl Into<Endpoint>,
        to: impl Into<Endpoint>,
        route: &[GridPoint],
    ) -> Result<WireId, IdExhausted> {
        let id = WireId(self.ids.alloc()?);
        self.doc.wires.push(Wire {
            id,
            from: from.into(),
            to: to.into(),
            route: route.to_vec(),
            number: None,
            layer: None,
            spec: None,
        });
        Ok(id)
    }

    /// Finish the document: persist the counter and sort every collection.
    #[must_use]
    pub fn finish(mut self) -> Schematic {
        self.doc.next_id = self.ids.next_id();
        self.doc.sort_canonical();
        self.doc
    }
}

/// Endpoint at a terminal of a symbol.
#[must_use]
pub fn pin(symbol: SymbolId, terminal: &str) -> Endpoint {
    Endpoint::Terminal(PinRef {
        symbol_id: symbol,
        terminal: TerminalKey(terminal.to_owned()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_allocates_ids_from_one_counter_and_persists_next_id() {
        let mut builder = SchematicBuilder::new("demo");
        let supply = builder.dc_supply(24_000).unwrap();
        let device = builder.device("K1", "contactor.generic", &[]).unwrap();
        let coil = builder
            .symbol(device, "coil", None, GridPoint::new(0, 4))
            .unwrap();
        let junction = builder.junction(GridPoint::new(0, 2)).unwrap();
        let first = builder.wire(pin(coil, "a1"), junction).unwrap();
        let second = builder.wire(junction, pin(coil, "a2")).unwrap();

        let doc = builder.finish();

        assert_eq!(
            (supply.0, device.0, coil.0, junction.0, first.0, second.0),
            (1, 2, 3, 4, 5, 6)
        );
        assert_eq!(doc.next_id, 7);
        assert_eq!(doc.validate(), []);
    }
}
