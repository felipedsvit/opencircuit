//! The numbers printed next to terminals, such as A1/A2 on a coil or 13/14
//! on a normally open contact.
//!
//! A label is derived, never stored: the document keeps structural terminal
//! keys (`a1`, `p1`, `pole1.in`), and the label follows from the kind of the
//! symbol, the class of its device and the slot of the symbol. The rules are
//! the usual terminal markings of low-voltage control gear:
//!
//! | Symbol | Labels |
//! | --- | --- |
//! | coil, transformer primary | A1, A2 |
//! | normally open contact, slot n | n3, n4 (timed: n7, n8) |
//! | normally closed contact, slot n | n1, n2 (timed: n5, n6) |
//! | changeover, slot n | n1 common, n2 normally closed, n4 normally open |
//! | contact of a thermal overload relay | 95, 96 closed; 97, 98 open |
//! | power contacts and thermal element, pole k | 2k-1 in, 2k out |
//! | fuse link | 1, 2 |
//! | motor | U1, V1, W1, U2, V2, W2 |
//! | lamp, buzzer, transformer secondary | X1, X2 |
//!
//! A symbol without a slot counts as slot 1, so a pushbutton reads 13/14 or
//! 11/12. Slots above 9 have no single-digit marking and get no label, and
//! neither does a supply terminal: its designator names it.

use crate::catalog::{ComponentCatalog, DeviceClass, SymbolKind};
use crate::schematic::{PinRef, Schematic, TerminalKey};

/// A designator, a strip or any name split for natural order: its letters, its
/// first number and what follows it, so that `K2` sorts before `K10` and `X2`
/// before `X10`.
#[must_use]
pub fn natural_key(text: &str) -> (String, u64, String) {
    let split = text
        .find(|c: char| c.is_ascii_digit())
        .unwrap_or(text.len());
    let prefix = text.get(..split).unwrap_or_default();
    let rest = text.get(split..).unwrap_or_default();
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let number = if digits.is_empty() {
        0
    } else {
        digits.parse::<u64>().unwrap_or(u64::MAX)
    };
    let tail = rest.get(digits.len()..).unwrap_or_default();
    (prefix.to_owned(), number, tail.to_owned())
}

/// A terminal named for a person: `K1:A1`, the designator of its device and the
/// printed number of the terminal, or its key when it has none (`P24:out`).
/// A terminal whose symbol or device is not in the document is `?:key`.
#[must_use]
pub fn pin_name(doc: &Schematic, catalog: &ComponentCatalog, pin: &PinRef) -> String {
    let symbol = doc.symbols.iter().find(|symbol| symbol.id == pin.symbol_id);
    let device = symbol.and_then(|symbol| {
        doc.devices
            .iter()
            .find(|device| device.id == symbol.device_id)
    });
    let label = symbol.zip(device).and_then(|(symbol, device)| {
        let model = catalog.model(&device.model)?;
        let definition = model.symbol(&symbol.symbol_key)?;
        terminal_label(model.class(), definition.kind(), symbol.slot, &pin.terminal)
    });
    format!(
        "{}:{}",
        device.map_or("?", |device| device.reference.0.as_str()),
        label.unwrap_or_else(|| pin.terminal.0.clone())
    )
}

/// The label printed next to `terminal` of a symbol of `kind` that belongs
/// to a device of `class`, in `slot`; `None` when the terminal has no
/// marking.
#[must_use]
pub fn terminal_label(
    class: DeviceClass,
    kind: SymbolKind,
    slot: Option<u8>,
    terminal: &TerminalKey,
) -> Option<String> {
    let key = terminal.0.as_str();
    let n = slot.unwrap_or(1);
    let digit = (1..=9).contains(&n).then_some(n);
    let timed = matches!(class, DeviceClass::Timer(_));
    let thermal = class == DeviceClass::ThermalOverload;
    let pair = |first: &str, second: &str| match key {
        "p1" => Some(first.to_owned()),
        "p2" => Some(second.to_owned()),
        _ => None,
    };
    match kind {
        SymbolKind::Coil | SymbolKind::TransformerPrimary => match key {
            "a1" => Some("A1".to_owned()),
            "a2" => Some("A2".to_owned()),
            _ => None,
        },
        SymbolKind::ContactNo if thermal => pair("97", "98"),
        SymbolKind::ContactNc if thermal => pair("95", "96"),
        SymbolKind::ContactNo => {
            let (first, second) = if timed { (7, 8) } else { (3, 4) };
            digit.and_then(|n| pair(&format!("{n}{first}"), &format!("{n}{second}")))
        }
        SymbolKind::ContactNc => {
            let (first, second) = if timed { (5, 6) } else { (1, 2) };
            digit.and_then(|n| pair(&format!("{n}{first}"), &format!("{n}{second}")))
        }
        SymbolKind::Changeover => digit.and_then(|n| match key {
            "com" => Some(format!("{n}1")),
            "nc" => Some(format!("{n}2")),
            "no" => Some(format!("{n}4")),
            _ => None,
        }),
        SymbolKind::PowerContacts { .. } | SymbolKind::ThermalElement { .. } => {
            let (pole, side) = key.strip_prefix("pole")?.split_once('.')?;
            let pole: u16 = pole.parse().ok()?;
            let out = pole.checked_mul(2)?;
            match side {
                "in" => out.checked_sub(1).map(|number| number.to_string()),
                "out" => Some(out.to_string()),
                _ => None,
            }
        }
        SymbolKind::FuseLink => pair("1", "2"),
        // The number of a terminal is its `number` parameter, drawn with it.
        SymbolKind::TerminalPoint => None,
        SymbolKind::Motor | SymbolKind::MotorSixTerminal => {
            matches!(key, "u1" | "v1" | "w1" | "u2" | "v2" | "w2").then(|| key.to_uppercase())
        }
        SymbolKind::Lamp | SymbolKind::Buzzer | SymbolKind::TransformerSecondary => {
            matches!(key, "x1" | "x2").then(|| key.to_uppercase())
        }
        SymbolKind::Block2 | SymbolKind::Block4 | SymbolKind::Block8 => key
            .strip_prefix('t')
            .filter(|number| !number.is_empty() && number.chars().all(|c| c.is_ascii_digit()))
            .map(str::to_owned),
        SymbolKind::SupplyTerminal | SymbolKind::InterlockLink => None,
    }
}

#[cfg(test)]
mod pin_name_tests {
    use super::*;
    use crate::builder::SchematicBuilder;
    use crate::geometry::GridPoint;

    #[test]
    fn a_terminal_is_named_with_its_designator_and_its_printed_number() {
        let mut b = SchematicBuilder::new("names");
        let k1 = b.device("K1", "contactor.generic", &[]).unwrap();
        let coil = b.symbol(k1, "coil", None, GridPoint::new(0, 0)).unwrap();
        let doc = b.finish();
        let catalog = ComponentCatalog::builtin();
        let pin = |symbol, key: &str| PinRef {
            symbol_id: symbol,
            terminal: TerminalKey(key.to_owned()),
        };

        assert_eq!(pin_name(&doc, &catalog, &pin(coil, "a1")), "K1:A1");
        // No printed number for a key the symbol does not mark.
        assert_eq!(pin_name(&doc, &catalog, &pin(coil, "zz")), "K1:zz");
        assert_eq!(
            pin_name(&doc, &catalog, &pin(crate::ids::SymbolId(99), "a1")),
            "?:a1"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::TimerMode;

    fn label(class: DeviceClass, kind: SymbolKind, slot: Option<u8>, key: &str) -> Option<String> {
        terminal_label(class, kind, slot, &TerminalKey(key.to_owned()))
    }

    fn labels(class: DeviceClass, kind: SymbolKind, slot: Option<u8>) -> Vec<Option<String>> {
        kind.terminals()
            .iter()
            .map(|terminal| terminal_label(class, kind, slot, &terminal.key))
            .collect()
    }

    fn some(texts: &[&str]) -> Vec<Option<String>> {
        texts.iter().map(|text| Some((*text).to_owned())).collect()
    }

    #[test]
    fn coils_and_auxiliary_contacts_follow_the_usual_markings() {
        let k = DeviceClass::Contactor;
        assert_eq!(labels(k, SymbolKind::Coil, None), some(&["A1", "A2"]));
        assert_eq!(
            labels(k, SymbolKind::ContactNo, Some(1)),
            some(&["13", "14"])
        );
        assert_eq!(
            labels(k, SymbolKind::ContactNc, Some(2)),
            some(&["21", "22"])
        );
        assert_eq!(
            labels(k, SymbolKind::ContactNo, Some(4)),
            some(&["43", "44"])
        );
        assert_eq!(
            labels(k, SymbolKind::Changeover, Some(1)),
            some(&["11", "12", "14"])
        );
        assert_eq!(
            labels(k, SymbolKind::PowerContacts { poles: 3 }, None),
            some(&["1", "2", "3", "4", "5", "6"])
        );
    }

    #[test]
    fn timers_thermal_relays_and_pushbuttons_have_their_own_numbers() {
        let timer = DeviceClass::Timer(TimerMode::OnDelay);
        assert_eq!(
            labels(timer, SymbolKind::ContactNo, Some(1)),
            some(&["17", "18"])
        );
        assert_eq!(
            labels(timer, SymbolKind::ContactNc, Some(1)),
            some(&["15", "16"])
        );
        let thermal = DeviceClass::ThermalOverload;
        assert_eq!(
            labels(thermal, SymbolKind::ContactNc, Some(1)),
            some(&["95", "96"])
        );
        assert_eq!(
            labels(thermal, SymbolKind::ContactNo, Some(1)),
            some(&["97", "98"])
        );
        let button = DeviceClass::Pushbutton;
        assert_eq!(
            labels(button, SymbolKind::ContactNo, None),
            some(&["13", "14"])
        );
        assert_eq!(
            labels(button, SymbolKind::ContactNc, None),
            some(&["11", "12"])
        );
    }

    #[test]
    fn motors_lamps_and_fuses_are_marked_and_supply_terminals_are_not() {
        assert_eq!(
            labels(DeviceClass::Motor, SymbolKind::MotorSixTerminal, None),
            some(&["U1", "V1", "W1", "U2", "V2", "W2"])
        );
        assert_eq!(
            labels(DeviceClass::Lamp, SymbolKind::Lamp, None),
            some(&["X1", "X2"])
        );
        assert_eq!(
            labels(DeviceClass::Fuse, SymbolKind::FuseLink, None),
            some(&["1", "2"])
        );
        assert_eq!(
            labels(DeviceClass::PowerSupply, SymbolKind::SupplyTerminal, None),
            [None]
        );
    }

    #[test]
    fn slots_without_a_single_digit_and_unknown_keys_get_no_label() {
        let k = DeviceClass::Relay;
        assert_eq!(label(k, SymbolKind::ContactNo, Some(10), "p1"), None);
        assert_eq!(label(k, SymbolKind::ContactNo, Some(0), "p1"), None);
        assert_eq!(label(k, SymbolKind::Coil, None, "x9"), None);
        assert_eq!(
            label(k, SymbolKind::PowerContacts { poles: 3 }, None, "pole.in"),
            None
        );
    }

    #[test]
    fn block_terminals_are_marked_with_their_number() {
        for kind in [SymbolKind::Block2, SymbolKind::Block4, SymbolKind::Block8] {
            let numbers: Vec<Option<String>> = (1..=kind.terminals().len())
                .map(|n| Some(n.to_string()))
                .collect();

            assert_eq!(
                labels(DeviceClass::Symbolic, kind, None),
                numbers,
                "{kind:?}"
            );
        }
        assert_eq!(
            label(DeviceClass::Symbolic, SymbolKind::Block2, None, "x"),
            None
        );
        assert_eq!(
            label(DeviceClass::Symbolic, SymbolKind::Block2, None, "t"),
            None
        );
        assert_eq!(
            label(DeviceClass::Symbolic, SymbolKind::Block2, None, "tx"),
            None
        );
    }
}
