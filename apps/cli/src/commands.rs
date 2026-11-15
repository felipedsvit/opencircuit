//! The four commands: `validate`, `simulate`, `test` and `export`.

use std::io::{self, Write};
use std::path::Path;

use circuit_model::{ComponentCatalog, Diagnostic, InputAction, Schematic, Severity, SimSnapshot};
use circuit_project::MAX_PROJECT_BYTES;
use circuit_sim::{
    CompiledCircuit, DelayConfig, FaultPolicy, SimConfig, SimulationError, Simulator, TraceEvent,
    compile, format_trace,
};
use serde::Serialize;

use crate::args::{Command, ExportFormat, OutputFormat};
use crate::fsio::{find_scenarios, load_catalog, read_text, write_atomic};
use crate::scenario::{ActionName, MAX_SCENARIO_BYTES, PolicyName, Scenario};
use crate::{CliError, EXIT_FAILED, EXIT_OK, Env};

/// Most ticks requested from the simulator in one call, so that the trace is
/// drained regularly.
const STEP_CHUNK: u32 = 100_000;
/// Most trace events one scenario may produce.
const MAX_TRACE_EVENTS: usize = 2_000_000;

pub(crate) fn execute(
    command: &Command,
    env: &Env,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<u8, CliError> {
    match command {
        // `run` answers `help` before dispatching here.
        Command::Help => Ok(EXIT_OK),
        Command::Validate {
            project,
            format,
            deny_warnings,
            library,
        } => {
            let catalog = load_catalog(library.as_slice())?;
            validate(project, *format, *deny_warnings, &catalog, out)
        }
        Command::Simulate {
            project,
            scenario,
            library,
            tick_us,
            fault_policy,
            trace,
            snapshot_json,
        } => {
            let overrides = Overrides {
                tick_us: *tick_us,
                fault_policy: *fault_policy,
            };
            simulate(
                &SimulateRequest {
                    project,
                    scenario_path: scenario,
                    overrides,
                    library: library.as_deref(),
                    trace: trace.as_deref(),
                    snapshot_json: snapshot_json.as_deref(),
                },
                out,
                err,
            )
        }
        Command::Test {
            paths,
            update_golden,
        } => {
            if *update_golden && env.ci {
                return Err(CliError::Usage(
                    "`--update-golden` is refused while the CI environment variable is set"
                        .to_owned(),
                ));
            }
            test(paths, *update_golden, out)
        }
        Command::Export {
            project,
            output,
            format,
            theme,
            library,
        } => {
            let catalog = load_catalog(library.as_slice())?;
            export(project, output, *format, *theme, &catalog, out, err)
        }
    }
}

/// Draw a project at rest and write it. A project that loads is drawn even
/// when it does not compile: the drawing is how its errors are found.
fn export(
    project: &Path,
    output: &Path,
    format: ExportFormat,
    theme: circuit_export::Theme,
    catalog: &ComponentCatalog,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<u8, CliError> {
    let text = read_text(project, MAX_PROJECT_BYTES)?;
    let doc = match circuit_project::from_str(&text) {
        Ok(doc) => doc,
        Err(error) => {
            writeln!(err, "error: {}: {error}", project.display())?;
            return Ok(EXIT_FAILED);
        }
    };
    let scene = circuit_renderer::build_scene(
        &doc,
        catalog,
        None,
        &circuit_renderer::SceneOptions::default(),
    );
    let drawing = match format {
        ExportFormat::Svg => circuit_export::svg(&scene, theme).into_bytes(),
        ExportFormat::Pdf => circuit_export::pdf(&scene, theme),
    };
    write_atomic(output, &drawing)?;
    writeln!(out, "wrote  {}", output.display())?;
    Ok(EXIT_OK)
}

#[derive(Serialize)]
struct ValidationReport<'a> {
    ok: bool,
    /// Why the project could not be loaded at all.
    error: Option<&'a str>,
    errors: usize,
    warnings: usize,
    diagnostics: &'a [Diagnostic],
}

fn count(diagnostics: &[Diagnostic], severity: Severity) -> usize {
    diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.severity() == severity)
        .count()
}

fn validate(
    project: &Path,
    format: OutputFormat,
    deny_warnings: bool,
    catalog: &ComponentCatalog,
    out: &mut dyn Write,
) -> Result<u8, CliError> {
    let text = read_text(project, MAX_PROJECT_BYTES)?;
    let (diagnostics, load_error) = match circuit_project::from_str(&text) {
        Ok(doc) => (compile(&doc, catalog).diagnostics, None),
        Err(error) => (Vec::new(), Some(error.to_string())),
    };
    let errors = count(&diagnostics, Severity::Error);
    let warnings = count(&diagnostics, Severity::Warning);
    let ok = load_error.is_none() && errors == 0 && !(deny_warnings && warnings > 0);

    match format {
        OutputFormat::Text => {
            if let Some(error) = &load_error {
                writeln!(out, "error: {error}")?;
            }
            for diagnostic in &diagnostics {
                writeln!(out, "{diagnostic}")?;
            }
            let verdict = if ok { "ok" } else { "failed" };
            writeln!(out, "{verdict}: {errors} error(s), {warnings} warning(s)")?;
        }
        OutputFormat::Json => {
            let report = ValidationReport {
                ok,
                error: load_error.as_deref(),
                errors,
                warnings,
                diagnostics: &diagnostics,
            };
            let text = serde_json::to_string_pretty(&report).map_err(io::Error::from)?;
            writeln!(out, "{text}")?;
        }
    }
    Ok(if ok { EXIT_OK } else { EXIT_FAILED })
}

/// Command-line settings that take precedence over the scenario file.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Overrides {
    /// Tick duration in microseconds.
    pub tick_us: Option<u32>,
    /// Fault policy.
    pub fault_policy: Option<FaultPolicy>,
}

/// What running a scenario produced.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScenarioRun {
    /// The trace in its normative text form.
    pub trace: String,
    /// The last solved tick.
    pub snapshot: Option<SimSnapshot>,
    /// Warnings reported before the first tick, by compilation and by the
    /// simulator configuration.
    pub diagnostics: Vec<Diagnostic>,
}

/// Why a scenario did not run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ScenarioFailure {
    /// The circuit does not compile. Carries every diagnostic.
    Invalid(Vec<Diagnostic>),
    /// The scenario cannot be applied to this circuit, or asks for too much.
    Scenario(String),
}

/// Compile a document and run a scenario on it.
///
/// # Errors
///
/// Returns [`ScenarioFailure::Invalid`] when the document has an error, and
/// [`ScenarioFailure::Scenario`] when the configuration is invalid, a device
/// is unknown, an action does not fit its device, or the trace grows past
/// its limit.
pub fn run_scenario(
    doc: &Schematic,
    scenario: &Scenario,
    overrides: Overrides,
    catalog: &ComponentCatalog,
) -> Result<ScenarioRun, ScenarioFailure> {
    let output = compile(doc, catalog);
    let Some(circuit) = output.circuit else {
        return Err(ScenarioFailure::Invalid(output.diagnostics));
    };

    let defaults = SimConfig::default();
    let config = SimConfig {
        tick_us: overrides
            .tick_us
            .or(scenario.config.tick_us)
            .unwrap_or(defaults.tick_us),
        delays: DelayConfig {
            pickup_ticks: scenario
                .config
                .pickup_ticks
                .unwrap_or(defaults.delays.pickup_ticks),
            dropout_ticks: scenario
                .config
                .dropout_ticks
                .unwrap_or(defaults.delays.dropout_ticks),
        },
        fault_policy: overrides
            .fault_policy
            .or(scenario.config.fault_policy.map(|policy| match policy {
                PolicyName::Halt => FaultPolicy::Halt,
                PolicyName::Report => FaultPolicy::Report,
            }))
            .unwrap_or(defaults.fault_policy),
        record_trace: true,
    };

    let mut events = resolve_events(&circuit, scenario).map_err(ScenarioFailure::Scenario)?;
    events.sort_by_key(|(tick, _)| *tick);

    let mut sim = Simulator::new(circuit, config)
        .map_err(|error| ScenarioFailure::Scenario(error.to_string()))?;
    let mut diagnostics = output.diagnostics;
    diagnostics.extend_from_slice(sim.diagnostics());

    let after_last = scenario.until_tick.saturating_add(1);
    let mut recorded: Vec<TraceEvent> = Vec::new();
    let mut cursor = 0_usize;
    let mut actions: Vec<InputAction> = Vec::new();
    while sim.tick() <= scenario.until_tick && !sim.is_faulted() {
        let tick = sim.tick();
        actions.clear();
        while let Some((at, action)) = events.get(cursor) {
            if *at != tick {
                break;
            }
            actions.push(*action);
            cursor = cursor.saturating_add(1);
        }
        // Run up to the next scripted tick, or to the end.
        let next = events
            .get(cursor)
            .map_or(after_last, |(at, _)| (*at).min(after_last));
        let span = u32::try_from(next.saturating_sub(tick))
            .unwrap_or(u32::MAX)
            .clamp(1, STEP_CHUNK);
        loop {
            match sim.step_n(span, &actions) {
                Ok(_) | Err(SimulationError::AlreadyFaulted) => break,
                Err(SimulationError::TraceBufferFull) => {
                    drain(&mut sim, &mut recorded)?;
                    if sim.tick() != tick {
                        // Some ticks ran, so the inputs were applied.
                        break;
                    }
                }
                Err(SimulationError::InvalidInput(error)) => {
                    return Err(ScenarioFailure::Scenario(format!("tick {tick}: {error}")));
                }
                Err(error @ SimulationError::TickOverflow) => {
                    return Err(ScenarioFailure::Scenario(error.to_string()));
                }
            }
        }
        drain(&mut sim, &mut recorded)?;
    }

    Ok(ScenarioRun {
        trace: format_trace(sim.circuit(), &sim.trace_header(), &recorded),
        snapshot: sim.snapshot(),
        diagnostics,
    })
}

fn drain(sim: &mut Simulator, recorded: &mut Vec<TraceEvent>) -> Result<(), ScenarioFailure> {
    recorded.append(&mut sim.take_trace());
    if recorded.len() > MAX_TRACE_EVENTS {
        return Err(ScenarioFailure::Scenario(format!(
            "the trace has more than {MAX_TRACE_EVENTS} events"
        )));
    }
    Ok(())
}

/// Turn scenario events into simulator inputs, resolving designators.
fn resolve_events(
    circuit: &CompiledCircuit,
    scenario: &Scenario,
) -> Result<Vec<(u64, InputAction)>, String> {
    let mut resolved = Vec::with_capacity(scenario.events.len());
    for event in &scenario.events {
        let Some(device) = circuit.device_by_reference(&event.device) else {
            let known: Vec<&str> = circuit
                .devices()
                .iter()
                .map(|device| device.reference.0.as_str())
                .collect();
            return Err(format!(
                "tick {}: the circuit has no device `{}`; it has: {}",
                event.tick,
                event.device,
                known.join(", ")
            ));
        };
        let action = match event.action {
            ActionName::Press => InputAction::Press(device.id),
            ActionName::Release => InputAction::Release(device.id),
            ActionName::Close => InputAction::SetClosed(device.id, true),
            ActionName::Open => InputAction::SetClosed(device.id, false),
            ActionName::Trip => InputAction::Trip(device.id),
            ActionName::ResetTrip => InputAction::ResetTrip(device.id),
        };
        resolved.push((event.tick, action));
    }
    Ok(resolved)
}

fn read_scenario(path: &Path) -> Result<Scenario, CliError> {
    let text = read_text(path, MAX_SCENARIO_BYTES)?;
    Scenario::parse(&text)
        .map_err(|message| CliError::Usage(format!("{}: {message}", path.display())))
}

/// What `simulate` was asked to do.
struct SimulateRequest<'a> {
    project: &'a Path,
    scenario_path: &'a Path,
    overrides: Overrides,
    /// `--library`, added before the libraries the scenario declares.
    library: Option<&'a Path>,
    trace: Option<&'a Path>,
    snapshot_json: Option<&'a Path>,
}

fn simulate(
    request: &SimulateRequest<'_>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<u8, CliError> {
    let SimulateRequest {
        project,
        scenario_path,
        overrides,
        library,
        trace,
        snapshot_json,
    } = *request;
    let text = read_text(project, MAX_PROJECT_BYTES)?;
    let scenario = read_scenario(scenario_path)?;
    let directory = scenario_path.parent().unwrap_or_else(|| Path::new("."));
    let libraries: Vec<std::path::PathBuf> = library
        .map(Path::to_path_buf)
        .into_iter()
        .chain(scenario.libraries.iter().map(|path| directory.join(path)))
        .collect();
    let catalog = load_catalog(&libraries)?;
    let doc = match circuit_project::from_str(&text) {
        Ok(doc) => doc,
        Err(error) => {
            writeln!(err, "error: {}: {error}", project.display())?;
            return Ok(EXIT_FAILED);
        }
    };

    match run_scenario(&doc, &scenario, overrides, &catalog) {
        Ok(run) => {
            for diagnostic in &run.diagnostics {
                writeln!(err, "{diagnostic}")?;
            }
            match trace {
                Some(path) => write_atomic(path, &run.trace)?,
                None => out.write_all(run.trace.as_bytes())?,
            }
            if let Some(path) = snapshot_json {
                let mut json =
                    serde_json::to_string_pretty(&run.snapshot).map_err(io::Error::from)?;
                json.push('\n');
                write_atomic(path, &json)?;
            }
            Ok(EXIT_OK)
        }
        Err(ScenarioFailure::Invalid(diagnostics)) => {
            for diagnostic in &diagnostics {
                writeln!(err, "{diagnostic}")?;
            }
            Ok(EXIT_FAILED)
        }
        Err(ScenarioFailure::Scenario(message)) => Err(CliError::Usage(format!(
            "{}: {message}",
            scenario_path.display()
        ))),
    }
}

/// Outcome of checking one scenario against its expectations.
enum Verdict {
    Pass,
    /// The golden trace was written or rewritten.
    Updated,
    Fail(Vec<String>),
}

fn test(
    paths: &[std::path::PathBuf],
    update_golden: bool,
    out: &mut dyn Write,
) -> Result<u8, CliError> {
    let scenarios = find_scenarios(paths)?;
    if scenarios.is_empty() {
        return Err(CliError::Usage(
            "no `*.scenario.json` file found".to_owned(),
        ));
    }
    let mut failed = 0_usize;
    for path in &scenarios {
        match check_scenario(path, update_golden)? {
            Verdict::Pass => writeln!(out, "ok     {}", path.display())?,
            Verdict::Updated => writeln!(out, "wrote  {}", path.display())?,
            Verdict::Fail(reasons) => {
                failed = failed.saturating_add(1);
                writeln!(out, "FAIL   {}", path.display())?;
                for reason in reasons {
                    writeln!(out, "       {reason}")?;
                }
            }
        }
    }
    writeln!(out, "{} scenario(s), {failed} failed", scenarios.len())?;
    Ok(if failed == 0 { EXIT_OK } else { EXIT_FAILED })
}

fn codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    let mut codes: Vec<String> = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.code().as_str().to_owned())
        .collect();
    codes.sort();
    codes
}

fn check_scenario(path: &Path, update_golden: bool) -> Result<Verdict, CliError> {
    let scenario = read_scenario(path)?;
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let project_path = directory.join(&scenario.circuit);
    let project_text = read_text(&project_path, MAX_PROJECT_BYTES)?;
    let doc = match circuit_project::from_str(&project_text) {
        Ok(doc) => doc,
        Err(error) => {
            return Ok(Verdict::Fail(vec![format!(
                "{} does not load: {error}",
                scenario.circuit
            )]));
        }
    };

    let mut expected_codes = scenario.expect.diagnostics.clone();
    expected_codes.sort();
    let code_mismatch = |found: &[Diagnostic]| {
        let found = codes(found);
        (found != expected_codes).then(|| {
            format!(
                "diagnostics differ: expected [{}], found [{}]",
                expected_codes.join(", "),
                found.join(", ")
            )
        })
    };

    let libraries: Vec<std::path::PathBuf> = scenario
        .libraries
        .iter()
        .map(|library| directory.join(library))
        .collect();
    let catalog = load_catalog(&libraries)?;
    let run = match run_scenario(&doc, &scenario, Overrides::default(), &catalog) {
        Ok(run) => run,
        Err(ScenarioFailure::Invalid(diagnostics)) => {
            let mut reasons: Vec<String> = code_mismatch(&diagnostics).into_iter().collect();
            if scenario.expect.trace.is_some() {
                reasons.push("the circuit does not compile, so there is no trace".to_owned());
            }
            return Ok(if reasons.is_empty() {
                Verdict::Pass
            } else {
                Verdict::Fail(reasons)
            });
        }
        Err(ScenarioFailure::Scenario(message)) => {
            return Err(CliError::Usage(format!("{}: {message}", path.display())));
        }
    };

    let mut reasons: Vec<String> = code_mismatch(&run.diagnostics).into_iter().collect();
    let mut updated = false;
    if let Some(golden_name) = &scenario.expect.trace {
        let golden_path = directory.join(golden_name);
        let golden = match read_text(&golden_path, MAX_PROJECT_BYTES) {
            Ok(text) => Some(text),
            Err(CliError::File { source, .. }) if source.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        if golden.as_deref() != Some(run.trace.as_str()) {
            if update_golden {
                write_atomic(&golden_path, &run.trace)?;
                updated = true;
            } else {
                match golden {
                    Some(golden) => {
                        reasons.push(format!("the trace differs from {golden_name}"));
                        reasons.extend(describe_difference(&golden, &run.trace));
                    }
                    None => reasons.push(format!(
                        "{golden_name} does not exist; create it with `--update-golden`"
                    )),
                }
            }
        }
    }

    Ok(if !reasons.is_empty() {
        Verdict::Fail(reasons)
    } else if updated {
        Verdict::Updated
    } else {
        Verdict::Pass
    })
}

/// Where two traces first differ, in a form a person can act on.
fn describe_difference(expected: &str, actual: &str) -> Vec<String> {
    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();
    let longest = expected_lines.len().max(actual_lines.len());
    let first = (0..longest).find(|index| expected_lines.get(*index) != actual_lines.get(*index));
    match first {
        Some(index) => vec![
            format!("first difference at line {}:", index.saturating_add(1)),
            format!(
                "  expected: {}",
                expected_lines.get(index).unwrap_or(&"<end of trace>")
            ),
            format!(
                "  actual:   {}",
                actual_lines.get(index).unwrap_or(&"<end of trace>")
            ),
            format!(
                "expected {} line(s), got {}",
                expected_lines.len(),
                actual_lines.len()
            ),
        ],
        None => vec![
            "the lines are equal but the bytes differ: check line endings and the final newline"
                .to_owned(),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn difference_points_at_the_first_line_that_changed() {
        let report = describe_difference("a\nb\nc\n", "a\nx\nc\nd\n");

        assert_eq!(
            report,
            [
                "first difference at line 2:",
                "  expected: b",
                "  actual:   x",
                "expected 3 line(s), got 4",
            ]
        );
    }

    #[test]
    fn missing_tail_is_shown_as_end_of_trace() {
        let report = describe_difference("a\nb\n", "a\n");

        assert_eq!(
            report,
            [
                "first difference at line 2:",
                "  expected: b",
                "  actual:   <end of trace>",
                "expected 2 line(s), got 1",
            ]
        );
    }

    #[test]
    fn carriage_returns_are_named_as_the_cause() {
        let report = describe_difference("a\nb\n", "a\r\nb\r\n");

        assert_eq!(
            report,
            ["the lines are equal but the bytes differ: check line endings and the final newline"]
        );
    }
}
