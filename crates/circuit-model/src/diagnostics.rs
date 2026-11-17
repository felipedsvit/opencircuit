//! Structured diagnostics shared by validation, compilation, simulation and
//! every front end.
//!
//! The textual code (`OC-<CATEGORY>-NNN`) is the stable identifier. Codes are
//! never renumbered or reused. Messages are derived text for humans; no logic
//! may depend on them.

use std::cmp::{Ordering, Reverse};
use std::fmt;

use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

use crate::ids::{DeviceId, JunctionId, SheetId, SupplyId, SymbolId, WireId};
use crate::schematic::PinRef;

/// How serious a diagnostic is.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Informational; never blocks anything.
    Info,
    /// Suspicious but allowed; never blocks anything.
    Warning,
    /// Prevents compilation, or marks a runtime fault.
    Error,
}

impl Severity {
    /// Lowercase name used in command-line output.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Family a diagnostic code belongs to; it is the middle part of the code.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticCategory {
    /// `OC-STR-`: the document is malformed.
    Structural,
    /// `OC-SEM-`: the document is well formed but does not make sense.
    Semantic,
    /// `OC-ELEC-`: an electrical condition, static or at run time.
    Electrical,
    /// `OC-MOD-`: the circuit is outside what the discrete model represents.
    ModelLimitation,
    /// `OC-CFG-`: the simulator configuration.
    Configuration,
}

macro_rules! diagnostic_codes {
    ($(
        $(#[$doc:meta])*
        $variant:ident = $code:literal, $category:ident, $severity:ident, latches = $latches:literal;
    )*) => {
        /// Stable identifier of a diagnostic.
        ///
        /// The declaration order is the canonical order used when sorting.
        #[non_exhaustive]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub enum DiagnosticCode {
            $( $(#[$doc])* $variant, )*
        }

        impl DiagnosticCode {
            /// Every code, in canonical order.
            pub const ALL: &'static [Self] = &[ $( Self::$variant, )* ];

            /// The stable textual code, for example `OC-ELEC-001`.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self { $( Self::$variant => $code, )* }
            }

            /// The family this code belongs to.
            #[must_use]
            pub const fn category(self) -> DiagnosticCategory {
                match self { $( Self::$variant => DiagnosticCategory::$category, )* }
            }

            /// The severity every diagnostic with this code carries.
            #[must_use]
            pub const fn default_severity(self) -> Severity {
                match self { $( Self::$variant => Severity::$severity, )* }
            }

            /// Whether this fault latches the runtime under the fail-closed
            /// fault policy.
            #[must_use]
            pub const fn latches_runtime(self) -> bool {
                match self { $( Self::$variant => $latches, )* }
            }
        }
    };
}

diagnostic_codes! {
    /// The same ID is used by more than one entity.
    DuplicateId = "OC-STR-001", Structural, Error, latches = false;
    /// A reference points at a device, symbol, junction or supply that does not exist.
    DanglingReference = "OC-STR-002", Structural, Error, latches = false;
    /// The component model or its version is not in the catalog.
    UnknownModel = "OC-STR-003", Structural, Error, latches = false;
    /// The symbol key does not exist in the component model.
    UnknownSymbolKey = "OC-STR-004", Structural, Error, latches = false;
    /// The terminal key does not exist in the symbol.
    UnknownTerminal = "OC-STR-005", Structural, Error, latches = false;
    /// Too few or too many instances of a symbol, or a missing or repeated slot.
    SymbolInstanceLimit = "OC-STR-006", Structural, Error, latches = false;
    /// A junction with no wire.
    JunctionUnconnected = "OC-STR-007", Structural, Error, latches = false;
    /// A junction with a single wire.
    JunctionDangling = "OC-STR-008", Structural, Warning, latches = false;
    /// A wire segment that is not horizontal or vertical. Geometry has no
    /// electrical meaning, so the compiler reports it without blocking.
    NonOrthogonalWire = "OC-STR-009", Structural, Warning, latches = false;
    /// A parameter is unknown, missing, of the wrong type or out of range.
    InvalidParameter = "OC-STR-010", Structural, Error, latches = false;
    /// An ID is zero, above the maximum, or not below `next_id`.
    IdOutOfRange = "OC-STR-011", Structural, Error, latches = false;
    /// A supply profile whose fields contradict its kind.
    InvalidSupplyProfile = "OC-STR-012", Structural, Error, latches = false;
    /// A wire whose two endpoints are the same point.
    SelfLoopWire = "OC-STR-013", Structural, Error, latches = false;
    /// The in-memory document does not carry the current schema version.
    UnsupportedSchemaVersion = "OC-STR-014", Structural, Error, latches = false;
    /// A sheet whose number another sheet has, or that is zero.
    DuplicateSheetNumber = "OC-STR-015", Structural, Error, latches = false;
    /// A sheet whose frame overlaps another, leaves the coordinate range,
    /// has no zones, or has a size of zero or above the limit.
    InvalidSheet = "OC-STR-016", Structural, Error, latches = false;
    /// A wire that crosses the edge of a sheet: the printed sheets cut it.
    WireCrossesSheet = "OC-STR-017", Structural, Warning, latches = false;
    /// A terminal or a junction lies on another terminal, on another junction
    /// or on the middle of a wire that is not in its net: it looks connected
    /// and is not (ADR-0014).
    LooseConnection = "OC-STR-018", Structural, Warning, latches = false;
    /// Two wires of different nets run over the same stretch of a line, so
    /// they look like one (ADR-0014).
    OverlappingWires = "OC-STR-019", Structural, Warning, latches = false;
    /// A waiver that names something that is not a diagnostic code, a code
    /// that cannot be waived (an error, or this one), a subject that is not in
    /// the document, or that repeats another waiver (ADR-0014).
    InvalidWaiver = "OC-STR-020", Structural, Warning, latches = false;
    /// The same reference designator is used by distinct devices.
    DuplicateReference = "OC-SEM-001", Semantic, Error, latches = false;
    /// A reference designator that does not match the allowed pattern.
    InvalidReference = "OC-SEM-002", Semantic, Error, latches = false;
    /// A terminal that must be connected has no wire.
    MissingRequiredTerminal = "OC-SEM-003", Semantic, Error, latches = false;
    /// The same conductor is provided by more than one supply. Retired: no
    /// longer emitted since every supply is its own section (ADR-0005); the
    /// code is kept so that it is never reused.
    ConflictingSupplies = "OC-SEM-004", Semantic, Error, latches = false;
    /// A device without any symbol.
    DeviceWithoutSymbols = "OC-SEM-005", Semantic, Warning, latches = false;
    /// A net that no supply can reach, even with every switch closed.
    FloatingNet = "OC-SEM-006", Semantic, Warning, latches = false;
    /// The document has loads and no supply.
    NoSupply = "OC-SEM-007", Semantic, Warning, latches = false;
    /// A supply that reaches no load.
    UnusedSupply = "OC-SEM-008", Semantic, Info, latches = false;
    /// A mechanical interlock that does not name two distinct contactors, or
    /// a contactor named by more than one interlock.
    InvalidInterlock = "OC-SEM-009", Semantic, Error, latches = false;
    /// A transformer whose secondary supply profile is not single-phase, is
    /// named by another transformer, or is also tapped by a supply terminal.
    InvalidTransformer = "OC-SEM-010", Semantic, Error, latches = false;
    /// A device of the symbol-only class: it is drawn and wired, but it has
    /// no behaviour in the simulation (ADR-0013).
    NotSimulated = "OC-SEM-011", Semantic, Info, latches = false;
    /// Two wires of one net carry different wire numbers (ADR-0014); the one
    /// with the smaller ID wins.
    WireNumberConflict = "OC-SEM-012", Semantic, Warning, latches = false;
    /// Two terminal blocks with the same strip and number (ADR-0016).
    DuplicateTerminal = "OC-SEM-013", Semantic, Warning, latches = false;
    /// Incompatible conductors meet in one group, including any AC/DC mix.
    ShortCircuit = "OC-ELEC-001", Electrical, Error, latches = true;
    /// Protective earth meets a live conductor.
    ProtectiveEarthFault = "OC-ELEC-002", Electrical, Error, latches = true;
    /// Neutral and protective earth share a group.
    NeutralEarthBond = "OC-ELEC-003", Electrical, Warning, latches = false;
    /// The supply kind or voltage label does not match the load; the load stays off.
    LoadSupplyMismatch = "OC-ELEC-004", Electrical, Warning, latches = false;
    /// Both terminals of a load are in the same group; the load stays off.
    LoadTerminalsShorted = "OC-ELEC-005", Electrical, Warning, latches = false;
    /// A motor is fed by only part of its phases.
    PhaseLoss = "OC-ELEC-006", Electrical, Warning, latches = false;
    /// Both contactors of a mechanical interlock tried to pick up at the same
    /// commit; neither did.
    InterlockContention = "OC-ELEC-007", Electrical, Warning, latches = false;
    /// Loads permanently wired in series; rejected at compile time.
    StaticSeriesLoads = "OC-MOD-001", ModelLimitation, Error, latches = false;
    /// Loads end up in series at run time; the affected loads stay off.
    DynamicUnsupportedTopology = "OC-MOD-002", ModelLimitation, Error, latches = false;
    /// The document is larger than the compiler supports.
    CircuitTooLarge = "OC-MOD-003", ModelLimitation, Error, latches = false;
    /// The windings of a six-terminal motor are energized in a way that is
    /// neither star nor delta; the motor stays off.
    UnsupportedWindingConnection = "OC-MOD-004", ModelLimitation, Error, latches = false;
    /// The tick duration or an actuation delay is invalid.
    InvalidConfig = "OC-CFG-001", Configuration, Error, latches = false;
    /// A timer preset is not a multiple of the tick and was rounded up.
    PresetQuantized = "OC-CFG-002", Configuration, Warning, latches = false;
}

impl DiagnosticCode {
    /// Parse a textual code produced by [`DiagnosticCode::as_str`].
    #[must_use]
    pub fn parse(code: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|candidate| candidate.as_str() == code)
    }
}

impl fmt::Display for DiagnosticCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for DiagnosticCode {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

/// The document element a diagnostic is about.
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Subject {
    /// The document as a whole.
    Document,
    /// A supply profile.
    Supply(SupplyId),
    /// A device.
    Device(DeviceId),
    /// A symbol instance.
    Symbol(SymbolId),
    /// One terminal of a symbol instance.
    Pin(PinRef),
    /// A wire.
    Wire(WireId),
    /// A junction.
    Junction(JunctionId),
    /// A drawing sheet.
    Sheet(SheetId),
}

impl fmt::Display for Subject {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Document => f.write_str("document"),
            Self::Supply(id) => write!(f, "supply {id}"),
            Self::Device(id) => write!(f, "device {id}"),
            Self::Symbol(id) => write!(f, "symbol {id}"),
            Self::Pin(pin) => write!(f, "pin {}:{}", pin.symbol_id, pin.terminal.0),
            Self::Wire(id) => write!(f, "wire {id}"),
            Self::Junction(id) => write!(f, "junction {id}"),
            Self::Sheet(id) => write!(f, "sheet {id}"),
        }
    }
}

/// One structured finding.
///
/// The severity is fixed by the code, so a diagnostic cannot claim a severity
/// that contradicts its code.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Diagnostic {
    code: DiagnosticCode,
    severity: Severity,
    subject: Subject,
    related: Vec<Subject>,
    message: String,
}

impl Diagnostic {
    /// Create a diagnostic; the severity comes from the code.
    #[must_use]
    pub fn new(code: DiagnosticCode, subject: Subject, message: impl Into<String>) -> Self {
        Self {
            code,
            severity: code.default_severity(),
            subject,
            related: Vec::new(),
            message: message.into(),
        }
    }

    /// Attach the other elements involved. They are stored sorted and without
    /// repetition so that equal findings compare equal.
    #[must_use]
    pub fn with_related(mut self, mut related: Vec<Subject>) -> Self {
        related.sort();
        related.dedup();
        self.related = related;
        self
    }

    /// The stable code.
    #[must_use]
    pub const fn code(&self) -> DiagnosticCode {
        self.code
    }

    /// The severity, derived from the code.
    #[must_use]
    pub const fn severity(&self) -> Severity {
        self.severity
    }

    /// The family of the code.
    #[must_use]
    pub const fn category(&self) -> DiagnosticCategory {
        self.code.category()
    }

    /// The primary element.
    #[must_use]
    pub const fn subject(&self) -> &Subject {
        &self.subject
    }

    /// The other elements involved, sorted.
    #[must_use]
    pub fn related(&self) -> &[Subject] {
        &self.related
    }

    /// Human-readable text. Never drive logic from it.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Whether the severity is [`Severity::Error`].
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }

    /// Canonical order: severity descending, then code, subject, related
    /// elements and finally the message as a tie breaker.
    #[must_use]
    pub fn canonical_cmp(&self, other: &Self) -> Ordering {
        (
            Reverse(self.severity),
            self.code,
            &self.subject,
            &self.related,
            &self.message,
        )
            .cmp(&(
                Reverse(other.severity),
                other.code,
                &other.subject,
                &other.related,
                &other.message,
            ))
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}[{}] {}: {}",
            self.severity, self.code, self.subject, self.message
        )
    }
}

/// Put diagnostics in canonical order (severity descending, then code, then
/// subject, then the related elements) and drop exact repetitions.
pub fn sort_diagnostics(diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.sort_by(Diagnostic::canonical_cmp);
    diagnostics.dedup();
}

/// Whether any diagnostic has [`Severity::Error`].
#[must_use]
pub fn has_errors(diagnostics: &[Diagnostic]) -> bool {
    diagnostics.iter().any(Diagnostic::is_error)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::schematic::TerminalKey;

    /// The published code table. A change here is a change of public contract:
    /// codes may be appended, never renumbered, renamed or removed.
    const PUBLISHED: &[(&str, &str, &str, bool)] = &[
        ("OC-STR-001", "DuplicateId", "error", false),
        ("OC-STR-002", "DanglingReference", "error", false),
        ("OC-STR-003", "UnknownModel", "error", false),
        ("OC-STR-004", "UnknownSymbolKey", "error", false),
        ("OC-STR-005", "UnknownTerminal", "error", false),
        ("OC-STR-006", "SymbolInstanceLimit", "error", false),
        ("OC-STR-007", "JunctionUnconnected", "error", false),
        ("OC-STR-008", "JunctionDangling", "warning", false),
        ("OC-STR-009", "NonOrthogonalWire", "warning", false),
        ("OC-STR-010", "InvalidParameter", "error", false),
        ("OC-STR-011", "IdOutOfRange", "error", false),
        ("OC-STR-012", "InvalidSupplyProfile", "error", false),
        ("OC-STR-013", "SelfLoopWire", "error", false),
        ("OC-STR-014", "UnsupportedSchemaVersion", "error", false),
        ("OC-STR-015", "DuplicateSheetNumber", "error", false),
        ("OC-STR-016", "InvalidSheet", "error", false),
        ("OC-STR-017", "WireCrossesSheet", "warning", false),
        ("OC-STR-018", "LooseConnection", "warning", false),
        ("OC-STR-019", "OverlappingWires", "warning", false),
        ("OC-STR-020", "InvalidWaiver", "warning", false),
        ("OC-SEM-001", "DuplicateReference", "error", false),
        ("OC-SEM-002", "InvalidReference", "error", false),
        ("OC-SEM-003", "MissingRequiredTerminal", "error", false),
        ("OC-SEM-004", "ConflictingSupplies", "error", false),
        ("OC-SEM-005", "DeviceWithoutSymbols", "warning", false),
        ("OC-SEM-006", "FloatingNet", "warning", false),
        ("OC-SEM-007", "NoSupply", "warning", false),
        ("OC-SEM-008", "UnusedSupply", "info", false),
        ("OC-SEM-009", "InvalidInterlock", "error", false),
        ("OC-SEM-010", "InvalidTransformer", "error", false),
        ("OC-SEM-011", "NotSimulated", "info", false),
        ("OC-SEM-012", "WireNumberConflict", "warning", false),
        ("OC-SEM-013", "DuplicateTerminal", "warning", false),
        ("OC-ELEC-001", "ShortCircuit", "error", true),
        ("OC-ELEC-002", "ProtectiveEarthFault", "error", true),
        ("OC-ELEC-003", "NeutralEarthBond", "warning", false),
        ("OC-ELEC-004", "LoadSupplyMismatch", "warning", false),
        ("OC-ELEC-005", "LoadTerminalsShorted", "warning", false),
        ("OC-ELEC-006", "PhaseLoss", "warning", false),
        ("OC-ELEC-007", "InterlockContention", "warning", false),
        ("OC-MOD-001", "StaticSeriesLoads", "error", false),
        ("OC-MOD-002", "DynamicUnsupportedTopology", "error", false),
        ("OC-MOD-003", "CircuitTooLarge", "error", false),
        ("OC-MOD-004", "UnsupportedWindingConnection", "error", false),
        ("OC-CFG-001", "InvalidConfig", "error", false),
        ("OC-CFG-002", "PresetQuantized", "warning", false),
    ];

    #[test]
    fn code_table_matches_the_published_contract() {
        let actual: Vec<(String, String, String, bool)> = DiagnosticCode::ALL
            .iter()
            .map(|code| {
                (
                    code.as_str().to_owned(),
                    format!("{code:?}"),
                    code.default_severity().as_str().to_owned(),
                    code.latches_runtime(),
                )
            })
            .collect();
        let expected: Vec<(String, String, String, bool)> = PUBLISHED
            .iter()
            .map(|(code, name, severity, latches)| {
                (
                    (*code).to_owned(),
                    (*name).to_owned(),
                    (*severity).to_owned(),
                    *latches,
                )
            })
            .collect();

        assert_eq!(actual, expected);
    }

    #[test]
    fn codes_are_unique() {
        let unique: BTreeSet<&str> = DiagnosticCode::ALL.iter().map(|c| c.as_str()).collect();

        assert_eq!(unique.len(), DiagnosticCode::ALL.len());
    }

    #[test]
    fn code_prefix_matches_its_category() {
        for code in DiagnosticCode::ALL {
            let prefix = match code.category() {
                DiagnosticCategory::Structural => "OC-STR-",
                DiagnosticCategory::Semantic => "OC-SEM-",
                DiagnosticCategory::Electrical => "OC-ELEC-",
                DiagnosticCategory::ModelLimitation => "OC-MOD-",
                DiagnosticCategory::Configuration => "OC-CFG-",
            };
            assert!(
                code.as_str().starts_with(prefix),
                "{code:?} has code {code} outside {prefix}"
            );
        }
    }

    #[test]
    fn only_short_circuit_and_protective_earth_fault_latch() {
        let latching: Vec<DiagnosticCode> = DiagnosticCode::ALL
            .iter()
            .copied()
            .filter(|code| code.latches_runtime())
            .collect();

        assert_eq!(
            latching,
            [
                DiagnosticCode::ShortCircuit,
                DiagnosticCode::ProtectiveEarthFault
            ]
        );
    }

    #[test]
    fn parse_round_trips_every_code_and_rejects_unknown_text() {
        for code in DiagnosticCode::ALL {
            assert_eq!(DiagnosticCode::parse(code.as_str()), Some(*code));
        }
        assert_eq!(DiagnosticCode::parse("OC-ELEC-999"), None);
        assert_eq!(DiagnosticCode::parse("oc-elec-001"), None);
    }

    #[test]
    fn severity_is_fixed_by_the_code() {
        let diagnostic = Diagnostic::new(
            DiagnosticCode::ShortCircuit,
            Subject::Device(DeviceId(1)),
            "conductors l1+l2 meet",
        );

        assert_eq!(diagnostic.severity(), Severity::Error);
        assert_eq!(diagnostic.category(), DiagnosticCategory::Electrical);
    }

    #[test]
    fn related_subjects_are_stored_sorted_and_unique() {
        let diagnostic = Diagnostic::new(
            DiagnosticCode::DuplicateReference,
            Subject::Device(DeviceId(1)),
            "K1 is used twice",
        )
        .with_related(vec![
            Subject::Device(DeviceId(9)),
            Subject::Device(DeviceId(2)),
            Subject::Device(DeviceId(9)),
        ]);

        assert_eq!(
            diagnostic.related(),
            [Subject::Device(DeviceId(2)), Subject::Device(DeviceId(9))]
        );
    }

    #[test]
    fn diagnostics_sort_by_severity_desc_then_code_then_subject() {
        let info = Diagnostic::new(
            DiagnosticCode::UnusedSupply,
            Subject::Device(DeviceId(1)),
            "",
        );
        let warning = Diagnostic::new(
            DiagnosticCode::JunctionDangling,
            Subject::Junction(JunctionId(4)),
            "",
        );
        let error_late_code = Diagnostic::new(
            DiagnosticCode::InvalidReference,
            Subject::Device(DeviceId(1)),
            "",
        );
        let error_wire_9 = Diagnostic::new(
            DiagnosticCode::DanglingReference,
            Subject::Wire(WireId(9)),
            "",
        );
        let error_wire_3 = Diagnostic::new(
            DiagnosticCode::DanglingReference,
            Subject::Wire(WireId(3)),
            "",
        );
        let mut diagnostics = vec![
            info.clone(),
            error_wire_9.clone(),
            warning.clone(),
            error_late_code.clone(),
            error_wire_3.clone(),
            warning.clone(),
        ];

        sort_diagnostics(&mut diagnostics);

        assert_eq!(
            diagnostics,
            [error_wire_3, error_wire_9, error_late_code, warning, info]
        );
    }

    #[test]
    fn display_shows_severity_code_subject_and_message() {
        let diagnostic = Diagnostic::new(
            DiagnosticCode::MissingRequiredTerminal,
            Subject::Pin(PinRef {
                symbol_id: SymbolId(11),
                terminal: TerminalKey("a2".to_owned()),
            }),
            "terminal a2 has no wire",
        );

        assert_eq!(
            diagnostic.to_string(),
            "error[OC-SEM-003] pin 11:a2: terminal a2 has no wire"
        );
    }
}
