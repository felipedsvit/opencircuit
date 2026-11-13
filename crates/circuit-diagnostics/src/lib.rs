//! Structured diagnostics shared by compilation, simulation and the UI.
//!
//! Codes are stable; messages may be localized later and must not drive
//! program logic.

use circuit_domain::{DeviceId, JunctionId, PinRef, SupplyId, SymbolId, WireId};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticCategory {
    Structural,
    Semantic,
    Electrical,
    ModelLimitation,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticCode {
    DuplicateDeviceReference,
    UnknownDeviceReference,
    MissingRequiredTerminal,
    InvalidConnection,
    ShortCircuit,
    ProtectiveEarthFault,
    FloatingNet,
    UnsupportedCircuitModel,
}

impl DiagnosticCode {
    pub const fn category(self) -> DiagnosticCategory {
        match self {
            Self::DuplicateDeviceReference
            | Self::MissingRequiredTerminal
            | Self::InvalidConnection => DiagnosticCategory::Structural,
            Self::UnknownDeviceReference => DiagnosticCategory::Semantic,
            Self::ShortCircuit | Self::ProtectiveEarthFault | Self::FloatingNet => {
                DiagnosticCategory::Electrical
            }
            Self::UnsupportedCircuitModel => DiagnosticCategory::ModelLimitation,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Subject {
    Device(DeviceId),
    Symbol(SymbolId),
    Terminal(PinRef),
    Wire(WireId),
    Junction(JunctionId),
    Supply(SupplyId),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub severity: Severity,
    pub subject: Subject,
    pub related: Vec<Subject>,
    pub message: String,
}

impl Diagnostic {
    pub const fn category(&self) -> DiagnosticCategory {
        self.code.category()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use circuit_domain::TerminalKey;

    #[test]
    fn diagnostic_keeps_primary_and_related_subjects() {
        let diagnostic = Diagnostic {
            code: DiagnosticCode::DuplicateDeviceReference,
            severity: Severity::Error,
            subject: Subject::Device(DeviceId(1)),
            related: vec![Subject::Device(DeviceId(2))],
            message: "device reference K1 is already in use".to_owned(),
        };

        assert_eq!(diagnostic.related, vec![Subject::Device(DeviceId(2))]);
        assert_eq!(diagnostic.severity, Severity::Error);
        assert_eq!(diagnostic.category(), DiagnosticCategory::Structural);
    }

    #[test]
    fn terminal_is_a_diagnostic_subject() {
        let subject = Subject::Terminal(PinRef {
            symbol_id: SymbolId(5),
            terminal: TerminalKey("A2".to_owned()),
        });

        assert!(matches!(subject, Subject::Terminal(_)));
    }

    #[test]
    fn wire_and_junction_are_diagnostic_subjects() {
        assert!(matches!(Subject::Wire(WireId(3)), Subject::Wire(_)));
        assert!(matches!(
            Subject::Junction(JunctionId(4)),
            Subject::Junction(_)
        ));
        assert!(matches!(Subject::Supply(SupplyId(5)), Subject::Supply(_)));
    }

    #[test]
    fn runtime_faults_are_electrical_or_model_limitations() {
        assert_eq!(
            DiagnosticCode::ShortCircuit.category(),
            DiagnosticCategory::Electrical
        );
        assert_eq!(
            DiagnosticCode::UnsupportedCircuitModel.category(),
            DiagnosticCategory::ModelLimitation
        );
        assert_eq!(
            DiagnosticCode::UnknownDeviceReference.category(),
            DiagnosticCategory::Semantic
        );
    }
}
