//! The copper nets of a drawing.
//!
//! A net is a set of terminals and junctions joined by wires. Two wires that
//! merely cross are not connected, and a switch never joins two nets here:
//! this is the static graph that a person sees on the paper, the same one
//! that the simulator compiles before it ever closes a contact.
//!
//! The renderer, the editor and the reports use it to trace a net, number its
//! wires and list where it goes, none of which may depend on the simulator.
//! It is a second implementation of the copper phase of `circuit-sim`'s
//! compiler, and a test in that crate checks that both number every pin,
//! junction and wire the same way.
//!
//! Nothing here trusts the document. A symbol whose model or key does not
//! resolve has no pins, and a wire with an end that does not resolve joins
//! nothing; structural problems are reported by validation, not here.

use std::collections::BTreeMap;

use crate::catalog::{ComponentCatalog, DeviceClass, PARAM_NAME};
use crate::ids::{JunctionId, WireId};
use crate::schematic::{Endpoint, PinRef, Schematic};

/// The index of a net, from 0, in the order of [`StaticNets::nets`].
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NetId(pub u32);

/// One net: everything a wire path connects.
///
/// A net of one pin and no wire is a terminal that nothing is connected to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Net {
    /// The index of the net.
    pub id: NetId,
    /// The terminals, by symbol ID and then by their order in the symbol.
    pub pins: Vec<PinRef>,
    /// The junctions, by ID.
    pub junctions: Vec<JunctionId>,
    /// The wires, by ID.
    pub wires: Vec<WireId>,
}

/// Every net of a document.
///
/// Nets are numbered by their smallest member, where every terminal comes
/// before every junction, so the numbering depends on the content of the
/// document and never on the order of its collections.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StaticNets {
    nets: Vec<Net>,
    of_pin: BTreeMap<PinRef, NetId>,
    of_junction: BTreeMap<JunctionId, NetId>,
    of_wire: BTreeMap<WireId, NetId>,
}

/// Disjoint sets over dense indices; the representative of a set is its
/// smallest member.
struct Sets {
    parent: Vec<u32>,
}

impl Sets {
    fn new(len: u32) -> Self {
        Self {
            parent: (0..len).collect(),
        }
    }

    fn find(&mut self, element: u32) -> u32 {
        let mut current = element;
        loop {
            let Some(&parent) = self.parent.get(current as usize) else {
                return current;
            };
            if parent == current {
                return current;
            }
            let grandparent = self.parent.get(parent as usize).copied().unwrap_or(parent);
            if let Some(slot) = self.parent.get_mut(current as usize) {
                *slot = grandparent;
            }
            current = grandparent;
        }
    }

    fn union(&mut self, a: u32, b: u32) {
        let (a, b) = (self.find(a), self.find(b));
        let (keep, merge) = if a < b { (a, b) } else { (b, a) };
        if keep == merge {
            return;
        }
        if let Some(slot) = self.parent.get_mut(merge as usize) {
            *slot = keep;
        }
    }
}

impl StaticNets {
    /// The nets of a document against a catalog.
    ///
    /// A document too large to index (more than `u32::MAX` terminals and
    /// junctions) has no nets.
    #[must_use]
    pub fn build(doc: &Schematic, catalog: &ComponentCatalog) -> Self {
        Self::try_build(doc, catalog).unwrap_or_default()
    }

    fn try_build(doc: &Schematic, catalog: &ComponentCatalog) -> Option<Self> {
        // When an ID appears twice, the first entity in document order wins,
        // as in `LayoutIndex`.
        let mut devices = BTreeMap::new();
        for device in &doc.devices {
            devices.entry(device.id).or_insert(device);
        }
        let mut symbols = BTreeMap::new();
        for symbol in &doc.symbols {
            symbols.entry(symbol.id).or_insert(symbol);
        }
        let mut junctions: Vec<JunctionId> = doc.junctions.iter().map(|j| j.id).collect();
        junctions.sort_unstable();
        junctions.dedup();
        let mut wires = BTreeMap::new();
        for wire in &doc.wires {
            wires.entry(wire.id).or_insert(wire);
        }

        // Nodes: the terminals of every symbol that resolves, then the
        // junctions.
        let mut pins: Vec<PinRef> = Vec::new();
        let mut pin_node: BTreeMap<PinRef, u32> = BTreeMap::new();
        for (id, symbol) in &symbols {
            let definition = devices
                .get(&symbol.device_id)
                .and_then(|device| catalog.model(&device.model))
                .and_then(|model| model.symbol(&symbol.symbol_key));
            let Some(definition) = definition else {
                continue;
            };
            for terminal in definition.terminals() {
                let pin = PinRef {
                    symbol_id: *id,
                    terminal: terminal.key.clone(),
                };
                pin_node.insert(pin.clone(), u32::try_from(pins.len()).ok()?);
                pins.push(pin);
            }
        }
        let pin_count = u32::try_from(pins.len()).ok()?;
        let junction_node: BTreeMap<JunctionId, u32> = junctions
            .iter()
            .copied()
            .zip(0_u32..)
            .map(|(id, index)| pin_count.checked_add(index).map(|node| (id, node)))
            .collect::<Option<_>>()?;
        let node_count = pin_count.checked_add(u32::try_from(junctions.len()).ok()?)?;

        let node_of = |endpoint: &Endpoint| match endpoint {
            Endpoint::Terminal(pin) => pin_node.get(pin).copied(),
            Endpoint::Junction(id) => junction_node.get(id).copied(),
        };

        let mut sets = Sets::new(node_count);
        let mut wire_node: Vec<(WireId, u32)> = Vec::new();
        for (id, wire) in &wires {
            let (Some(from), Some(to)) = (node_of(&wire.from), node_of(&wire.to)) else {
                continue;
            };
            sets.union(from, to);
            wire_node.push((*id, from));
        }

        // Signal arrows of one name are one net (ADR-0016).
        let mut named: BTreeMap<String, u32> = BTreeMap::new();
        for (id, symbol) in &symbols {
            let Some(device) = devices.get(&symbol.device_id) else {
                continue;
            };
            let Some(model) = catalog.model(&device.model) else {
                continue;
            };
            if model.class() != DeviceClass::SignalArrow {
                continue;
            }
            let terminal = model
                .symbol(&symbol.symbol_key)
                .and_then(|definition| definition.terminals().first());
            let (Some(name), Some(terminal)) =
                (model.text_param(&device.params, PARAM_NAME), terminal)
            else {
                continue;
            };
            let pin = PinRef {
                symbol_id: *id,
                terminal: terminal.key.clone(),
            };
            let Some(&node) = pin_node.get(&pin) else {
                continue;
            };
            match named.get(name) {
                Some(&other) => sets.union(node, other),
                None => {
                    named.insert(name.to_owned(), node);
                }
            }
        }

        // A net is numbered by its smallest node, and nodes are visited in
        // ascending order.
        let mut net_of_node: Vec<u32> = Vec::new();
        let mut net_count = 0_u32;
        for node in 0..node_count {
            let root = sets.find(node);
            let net = if root == node {
                let net = net_count;
                net_count = net_count.checked_add(1)?;
                net
            } else {
                net_of_node.get(root as usize).copied()?
            };
            net_of_node.push(net);
        }

        let mut nets: Vec<Net> = (0..net_count)
            .map(|index| Net {
                id: NetId(index),
                pins: Vec::new(),
                junctions: Vec::new(),
                wires: Vec::new(),
            })
            .collect();
        let mut of_pin = BTreeMap::new();
        let mut of_junction = BTreeMap::new();
        let mut of_wire = BTreeMap::new();
        for (pin, node) in pins.iter().zip(0_u32..) {
            let net = NetId(*net_of_node.get(node as usize)?);
            nets.get_mut(net.0 as usize)?.pins.push(pin.clone());
            of_pin.insert(pin.clone(), net);
        }
        for id in &junctions {
            let node = *junction_node.get(id)?;
            let net = NetId(*net_of_node.get(node as usize)?);
            nets.get_mut(net.0 as usize)?.junctions.push(*id);
            of_junction.insert(*id, net);
        }
        for (id, node) in wire_node {
            let net = NetId(*net_of_node.get(node as usize)?);
            nets.get_mut(net.0 as usize)?.wires.push(id);
            of_wire.insert(id, net);
        }

        Some(Self {
            nets,
            of_pin,
            of_junction,
            of_wire,
        })
    }

    /// Every terminal, junction and wire of the nets that touch `selection`,
    /// for tracing a net (ADR-0014): a wire, a junction, a terminal or a
    /// symbol (all its terminals) each bring the nets they are in. A net with
    /// no wire is left out, since there is nothing to follow.
    #[must_use]
    pub fn traced(
        &self,
        selection: &std::collections::BTreeSet<crate::diagnostics::Subject>,
    ) -> std::collections::BTreeSet<crate::diagnostics::Subject> {
        use crate::diagnostics::Subject;
        let mut wanted: std::collections::BTreeSet<NetId> = std::collections::BTreeSet::new();
        for subject in selection {
            match subject {
                Subject::Wire(id) => wanted.extend(self.net_of_wire(*id)),
                Subject::Junction(id) => wanted.extend(self.net_of_junction(*id)),
                Subject::Pin(pin) => wanted.extend(self.net_of_pin(pin)),
                Subject::Symbol(symbol) => wanted.extend(
                    self.of_pin
                        .iter()
                        .filter(|(pin, _)| pin.symbol_id == *symbol)
                        .map(|(_, net)| *net),
                ),
                _ => {}
            }
        }
        let mut traced = std::collections::BTreeSet::new();
        for id in wanted {
            let Some(net) = self.net(id).filter(|net| !net.wires.is_empty()) else {
                continue;
            };
            traced.extend(net.pins.iter().cloned().map(Subject::Pin));
            traced.extend(net.junctions.iter().copied().map(Subject::Junction));
            traced.extend(net.wires.iter().copied().map(Subject::Wire));
        }
        traced
    }

    /// Every net, in the order of its [`NetId`].
    #[must_use]
    pub fn nets(&self) -> &[Net] {
        &self.nets
    }

    /// The net with this index.
    #[must_use]
    pub fn net(&self, id: NetId) -> Option<&Net> {
        self.nets.get(id.0 as usize)
    }

    /// The net a terminal belongs to.
    #[must_use]
    pub fn net_of_pin(&self, pin: &PinRef) -> Option<NetId> {
        self.of_pin.get(pin).copied()
    }

    /// The net a junction belongs to.
    #[must_use]
    pub fn net_of_junction(&self, junction: JunctionId) -> Option<NetId> {
        self.of_junction.get(&junction).copied()
    }

    /// The net a wire belongs to; a wire with an end that does not resolve
    /// belongs to none.
    #[must_use]
    pub fn net_of_wire(&self, wire: WireId) -> Option<NetId> {
        self.of_wire.get(&wire).copied()
    }

    /// The net a wire end belongs to.
    #[must_use]
    pub fn net_of_endpoint(&self, endpoint: &Endpoint) -> Option<NetId> {
        match endpoint {
            Endpoint::Terminal(pin) => self.net_of_pin(pin),
            Endpoint::Junction(id) => self.net_of_junction(*id),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::{SchematicBuilder, pin};
    use crate::geometry::GridPoint;
    use crate::ids::SymbolId;

    /// Two pushbuttons, S1 and S2, side by side, and nothing else.
    fn two_buttons() -> (SchematicBuilder, SymbolId, SymbolId) {
        let mut b = SchematicBuilder::new("nets");
        let s1 = b.device("S1", "pushbutton_no", &[]).unwrap();
        let s2 = b.device("S2", "pushbutton_no", &[]).unwrap();
        let first = b.symbol(s1, "contact", None, GridPoint::new(0, 0)).unwrap();
        let second = b
            .symbol(s2, "contact", None, GridPoint::new(10, 0))
            .unwrap();
        (b, first, second)
    }

    fn nets_of(b: SchematicBuilder) -> (Schematic, StaticNets) {
        let doc = b.finish();
        let nets = StaticNets::build(&doc, &ComponentCatalog::builtin());
        (doc, nets)
    }

    #[test]
    fn a_wire_joins_its_ends_and_every_other_pin_stays_alone() {
        let (mut b, first, second) = two_buttons();
        b.wire(pin(first, "p2"), pin(second, "p1")).unwrap();

        let (doc, nets) = nets_of(b);

        let id = |symbol, key: &str| nets.net_of_pin(&pin_ref(symbol, key)).unwrap();
        assert_eq!(id(first, "p2"), id(second, "p1"));
        assert_ne!(id(first, "p1"), id(first, "p2"));
        assert_ne!(id(second, "p1"), id(second, "p2"));
        assert_eq!(nets.nets().len(), 3);
        let joined = nets.net(id(first, "p2")).unwrap();
        assert_eq!(joined.wires, [doc.wires[0].id]);
        assert_eq!(joined.pins.len(), 2);
    }

    #[test]
    fn wires_through_a_junction_form_one_net() {
        let (mut b, first, second) = two_buttons();
        let junction = b.junction(GridPoint::new(5, 5)).unwrap();
        b.wire(pin(first, "p2"), junction).unwrap();
        b.wire(junction, pin(second, "p1")).unwrap();

        let (doc, nets) = nets_of(b);

        let net = nets.net_of_junction(doc.junctions[0].id).unwrap();
        assert_eq!(nets.net_of_pin(&pin_ref(first, "p2")), Some(net));
        assert_eq!(nets.net_of_pin(&pin_ref(second, "p1")), Some(net));
        assert_eq!(nets.net(net).unwrap().wires.len(), 2);
        assert_eq!(nets.net(net).unwrap().junctions, [doc.junctions[0].id]);
    }

    #[test]
    fn crossing_wires_that_share_no_endpoint_are_different_nets() {
        let (mut b, first, second) = two_buttons();
        let a = b.junction(GridPoint::new(5, 0)).unwrap();
        let c = b.junction(GridPoint::new(5, 10)).unwrap();
        b.wire(pin(first, "p1"), pin(second, "p2")).unwrap();
        b.wire(a, c).unwrap();

        let (doc, nets) = nets_of(b);

        assert_ne!(
            nets.net_of_wire(doc.wires[0].id),
            nets.net_of_wire(doc.wires[1].id)
        );
    }

    #[test]
    fn nets_are_numbered_by_their_smallest_terminal_before_any_junction() {
        let (mut b, first, second) = two_buttons();
        let junction = b.junction(GridPoint::new(5, 5)).unwrap();
        b.wire(pin(second, "p1"), junction).unwrap();

        let (_, nets) = nets_of(b);

        // Pins in order: first.p1, first.p2, second.p1, second.p2; the
        // junction joins second.p1, so it takes that net's number.
        assert_eq!(nets.net_of_pin(&pin_ref(first, "p1")), Some(NetId(0)));
        assert_eq!(nets.net_of_pin(&pin_ref(first, "p2")), Some(NetId(1)));
        assert_eq!(nets.net_of_pin(&pin_ref(second, "p1")), Some(NetId(2)));
        assert_eq!(nets.net_of_pin(&pin_ref(second, "p2")), Some(NetId(3)));
        assert_eq!(nets.nets().len(), 4);
    }

    #[test]
    fn tracing_a_wire_a_junction_or_a_symbol_brings_the_whole_net() {
        use crate::diagnostics::Subject;
        use std::collections::BTreeSet;

        let (mut b, first, second) = two_buttons();
        let junction = b.junction(GridPoint::new(5, 5)).unwrap();
        b.wire(pin(first, "p2"), junction).unwrap();
        b.wire(junction, pin(second, "p1")).unwrap();
        let (doc, nets) = nets_of(b);
        let whole = nets.traced(&BTreeSet::from([Subject::Wire(doc.wires[0].id)]));

        // The net holds both pins, the junction and both wires.
        assert_eq!(whole.len(), 5);
        assert_eq!(
            nets.traced(&BTreeSet::from([Subject::Junction(doc.junctions[0].id)])),
            whole
        );
        // A symbol brings the nets of all its terminals; the free one has no
        // wire and brings nothing.
        assert_eq!(
            nets.traced(&BTreeSet::from([Subject::Symbol(first)])),
            whole
        );
        assert!(nets.traced(&BTreeSet::new()).is_empty());
        assert!(
            nets.traced(&BTreeSet::from([Subject::Device(doc.devices[0].id)]))
                .is_empty()
        );
    }

    #[test]
    fn the_order_of_the_collections_does_not_change_the_nets() {
        let (mut b, first, second) = two_buttons();
        let junction = b.junction(GridPoint::new(5, 5)).unwrap();
        b.wire(pin(first, "p2"), junction).unwrap();
        b.wire(junction, pin(second, "p1")).unwrap();
        let (mut doc, nets) = nets_of(b);

        doc.devices.reverse();
        doc.symbols.reverse();
        doc.junctions.reverse();
        doc.wires.reverse();

        assert_eq!(StaticNets::build(&doc, &ComponentCatalog::builtin()), nets);
    }

    #[test]
    fn a_wire_to_something_that_does_not_exist_joins_nothing() {
        let (b, first, _) = two_buttons();
        let mut doc = b.finish();
        doc.wires.push(crate::schematic::Wire {
            id: WireId(900),
            from: Endpoint::Terminal(pin_ref(first, "p1")),
            to: Endpoint::Junction(JunctionId(901)),
            route: Vec::new(),
            number: None,
            layer: None,
            spec: None,
        });

        let nets = StaticNets::build(&doc, &ComponentCatalog::builtin());

        assert_eq!(nets.net_of_wire(WireId(900)), None);
        assert_eq!(nets.nets().len(), 4);
    }

    #[test]
    fn a_symbol_of_an_unknown_model_has_no_pins() {
        let (b, first, _) = two_buttons();
        let mut doc = b.finish();
        doc.devices[0].model.id = "no.such.model".to_owned();

        let nets = StaticNets::build(&doc, &ComponentCatalog::builtin());

        assert_eq!(nets.net_of_pin(&pin_ref(first, "p1")), None);
        assert_eq!(nets.nets().len(), 2);
    }

    fn pin_ref(symbol: SymbolId, key: &str) -> PinRef {
        PinRef {
            symbol_id: symbol,
            terminal: crate::schematic::TerminalKey(key.to_owned()),
        }
    }
}
