//! Change-log trace.
//!
//! The trace records what changed, not one snapshot per tick: signal
//! transitions, load transitions and the start and end of faults. Its text
//! form is what golden tests compare byte for byte, so the format is part of
//! the simulation contract.

use std::fmt::Write as _;

use circuit_model::{DeviceClass, DeviceId, DiagnosticCode, Subject, SymbolId};

use crate::compiled::{CompiledCircuit, SignalKind};
use crate::fingerprint::Fingerprint;

/// What a load started doing.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LoadActivity {
    /// De-energized or stopped.
    Off,
    /// A two-terminal load energized.
    On,
    /// A motor running with the phase sequence.
    Forward,
    /// A motor running against the phase sequence.
    Reverse,
    /// A six-terminal motor running with the phase sequence in star.
    ForwardStar,
    /// A six-terminal motor running with the phase sequence in delta.
    ForwardDelta,
    /// A six-terminal motor running against the phase sequence in star.
    ReverseStar,
    /// A six-terminal motor running against the phase sequence in delta.
    ReverseDelta,
}

impl LoadActivity {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::On => "on",
            Self::Forward => "forward",
            Self::Reverse => "reverse",
            Self::ForwardStar => "forward-star",
            Self::ForwardDelta => "forward-delta",
            Self::ReverseStar => "reverse-star",
            Self::ReverseDelta => "reverse-delta",
        }
    }
}

/// One recorded change.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TraceKind {
    /// A signal changed its effective value.
    Signal {
        /// Owning device.
        device: DeviceId,
        /// Meaning of the signal.
        kind: SignalKind,
        /// Value in the previous tick.
        from: bool,
        /// Value in this tick.
        to: bool,
    },
    /// A load changed what it is doing.
    Load {
        /// Owning device.
        device: DeviceId,
        /// The load symbol.
        symbol: SymbolId,
        /// What the load does from this tick on.
        activity: LoadActivity,
    },
    /// A fault or warning became active.
    FaultStarted {
        /// Stable code.
        code: DiagnosticCode,
        /// Primary element.
        subject: Subject,
        /// Other elements involved.
        related: Vec<Subject>,
    },
    /// A fault or warning is no longer active.
    FaultCleared {
        /// Stable code.
        code: DiagnosticCode,
        /// Primary element.
        subject: Subject,
        /// Other elements involved.
        related: Vec<Subject>,
    },
    /// The runtime latched on a fault at the end of this tick.
    Latched,
    /// The fault latch was reset before this tick.
    LatchReset,
}

/// A change and the tick it happened in.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceEvent {
    /// Tick of the change.
    pub tick: u64,
    /// The change.
    pub kind: TraceKind,
}

/// What a trace must state to be reproducible.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TraceHeader {
    /// Fingerprint of the compiled circuit.
    pub fingerprint: Fingerprint,
    /// Tick duration in microseconds.
    pub tick_us: u32,
    /// Pickup delay in ticks.
    pub pickup_ticks: u32,
    /// Dropout delay in ticks.
    pub dropout_ticks: u32,
    /// `halt` or `report`.
    pub fault_policy: &'static str,
}

/// Render a trace in the normative text format.
///
/// Three header lines, then one line per event, each terminated by a line
/// feed:
///
/// ```text
/// # opencircuit trace v1
/// # fingerprint 0123456789abcdef
/// # tick_us=1000 pickup_ticks=1 dropout_ticks=1 fault_policy=halt
/// t=2 input S1.pressed 0->1
/// t=2 load K1.coil on
/// t=3 signal K1.active 0->1
/// ```
#[must_use]
pub fn format_trace(
    circuit: &CompiledCircuit,
    header: &TraceHeader,
    events: &[TraceEvent],
) -> String {
    let mut out = String::new();
    out.push_str("# opencircuit trace v1\n");
    // Writing to a `String` cannot fail.
    let _ = writeln!(out, "# fingerprint {}", header.fingerprint);
    let _ = writeln!(
        out,
        "# tick_us={} pickup_ticks={} dropout_ticks={} fault_policy={}",
        header.tick_us, header.pickup_ticks, header.dropout_ticks, header.fault_policy
    );
    for event in events {
        write_event(&mut out, circuit, event);
        out.push('\n');
    }
    out
}

/// Render one event as a line of the trace, without the line feed: the
/// same text as in [`format_trace`].
#[must_use]
pub fn format_event(circuit: &CompiledCircuit, event: &TraceEvent) -> String {
    let mut out = String::new();
    write_event(&mut out, circuit, event);
    out
}

fn write_event(out: &mut String, circuit: &CompiledCircuit, event: &TraceEvent) {
    let _ = write!(out, "t={} ", event.tick);
    match &event.kind {
        TraceKind::Signal {
            device,
            kind,
            from,
            to,
        } => {
            let word = if kind.is_input() { "input" } else { "signal" };
            let _ = write!(
                out,
                "{word} {}.{} {}->{}",
                device_name(circuit, *device),
                signal_name(circuit, *device, *kind),
                u8::from(*from),
                u8::from(*to)
            );
        }
        TraceKind::Load {
            device,
            symbol,
            activity,
        } => {
            let _ = write!(
                out,
                "load {}.{} {}",
                device_name(circuit, *device),
                load_name(circuit, *symbol),
                activity.as_str()
            );
        }
        TraceKind::FaultStarted {
            code,
            subject,
            related,
        } => write_fault(out, circuit, *code, "started", subject, related),
        TraceKind::FaultCleared {
            code,
            subject,
            related,
        } => write_fault(out, circuit, *code, "cleared", subject, related),
        TraceKind::Latched => out.push_str("runtime latched"),
        TraceKind::LatchReset => out.push_str("runtime latch-reset"),
    }
}

fn write_fault(
    out: &mut String,
    circuit: &CompiledCircuit,
    code: DiagnosticCode,
    change: &str,
    subject: &Subject,
    related: &[Subject],
) {
    let _ = write!(
        out,
        "fault {code} {change} {}",
        subject_name(circuit, subject)
    );
    for other in related {
        let _ = write!(out, ",{}", subject_name(circuit, other));
    }
}

fn device_name(circuit: &CompiledCircuit, device: DeviceId) -> String {
    circuit.device_by_id(device).map_or_else(
        || format!("device#{device}"),
        |found| found.reference.0.clone(),
    )
}

fn signal_name(circuit: &CompiledCircuit, device: DeviceId, kind: SignalKind) -> &'static str {
    match kind {
        SignalKind::Actuated => match circuit.device_by_id(device).map(|found| found.class) {
            Some(DeviceClass::LatchingSwitch) => "closed",
            _ => "pressed",
        },
        SignalKind::Tripped => "tripped",
        SignalKind::CoilActive => "active",
        SignalKind::TimerOut => "out",
        SignalKind::Powered => "powered",
    }
}

fn load_name(circuit: &CompiledCircuit, symbol: SymbolId) -> String {
    circuit
        .loads()
        .iter()
        .find(|load| load.symbol == symbol)
        .map_or_else(
            || format!("symbol#{symbol}"),
            |load| load.symbol_key.0.clone(),
        )
}

fn subject_name(circuit: &CompiledCircuit, subject: &Subject) -> String {
    match subject {
        Subject::Document => "document".to_owned(),
        Subject::Device(device) => device_name(circuit, *device),
        Subject::Supply(supply) => format!("supply#{supply}"),
        Subject::Symbol(symbol) => format!("symbol#{symbol}"),
        Subject::Pin(pin) => format!("pin#{}:{}", pin.symbol_id, pin.terminal.0),
        Subject::Wire(wire) => format!("wire#{wire}"),
        Subject::Junction(junction) => format!("junction#{junction}"),
        Subject::Sheet(sheet) => format!("sheet#{sheet}"),
    }
}
