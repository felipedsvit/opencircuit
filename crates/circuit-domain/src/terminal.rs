//! Stable terminal keys used by wires and diagnostics.
//!
//! Terminals are addressed by strings such as `A1`, `13` or `U1`; positional
//! pin indices only exist in the compiled netlist.

use crate::ids::SymbolId;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TerminalKey(pub String);

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PinRef {
    pub symbol_id: SymbolId,
    pub terminal: TerminalKey,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_reference_uses_a_stable_terminal_key() {
        let pin = PinRef {
            symbol_id: SymbolId(12),
            terminal: TerminalKey("A1".to_owned()),
        };

        assert_eq!(pin.terminal.0, "A1");
    }
}
