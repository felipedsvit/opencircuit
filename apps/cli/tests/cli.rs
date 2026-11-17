//! End-to-end tests of the `opencircuit` binary: output and exit codes.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root exists")
}

fn opencircuit(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_opencircuit"))
        .args(args)
        .current_dir(root())
        .env_remove("CI")
        .output()
        .expect("the binary runs")
}

fn code(output: &Output) -> i32 {
    output.status.code().expect("the process exited normally")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A fresh directory for one test.
fn scratch(name: &str) -> PathBuf {
    let directory = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).expect("scratch directory");
    directory
}

/// Copy the files of an example directory into a scratch directory.
fn copy_example(example: &str, name: &str) -> PathBuf {
    let target = scratch(name);
    for entry in fs::read_dir(root().join(example)).expect("example directory") {
        let path = entry.expect("entry").path();
        if path.is_file() {
            fs::copy(&path, target.join(path.file_name().expect("file name"))).expect("copy");
        }
    }
    target
}

const COMMAND: &str = "examples/direct-starter/command.ocircuit";
const COMMAND_SCENARIO: &str = "examples/direct-starter/command.scenario.json";
const STATIC_SHORT: &str = "tests/fixtures/static-short/static-short.ocircuit";

#[test]
fn help_succeeds() {
    let output = opencircuit(&["--help"]);

    assert_eq!(code(&output), 0);
    assert!(stdout(&output).contains("opencircuit validate"));
}

#[test]
fn unknown_command_is_a_usage_error() {
    let output = opencircuit(&["frobnicate"]);

    assert_eq!(code(&output), 2);
    assert!(stderr(&output).contains("unknown command `frobnicate`"));
}

#[test]
fn valid_project_validates() {
    let output = opencircuit(&["validate", COMMAND]);

    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), "ok: 0 error(s), 0 warning(s)\n");
}

#[test]
fn project_with_an_error_fails_validation() {
    let output = opencircuit(&["validate", STATIC_SHORT]);

    assert_eq!(code(&output), 1);
    assert!(stdout(&output).contains("error[OC-ELEC-001]"));
}

#[test]
fn json_report_is_structured() {
    let output = opencircuit(&["validate", STATIC_SHORT, "--format", "json"]);
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).expect("JSON report");

    assert_eq!(code(&output), 1);
    assert_eq!(report["ok"], false);
    assert_eq!(report["errors"], 1);
    assert_eq!(report["diagnostics"][0]["code"], "OC-ELEC-001");
    assert_eq!(report["diagnostics"][0]["severity"], "error");
    assert_eq!(report["diagnostics"][0]["subject"]["device"], 2);
}

#[test]
fn warnings_fail_validation_only_when_denied() {
    let directory = scratch("warnings");
    let project = directory.join("warning.ocircuit");
    // The command circuit plus a thermal overload relay without any symbol:
    // allowed by its model, so only a warning.
    let text = fs::read_to_string(root().join(COMMAND)).expect("example");
    let mut doc: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    let next_id = doc["next_id"].as_u64().expect("next_id");
    doc["devices"]
        .as_array_mut()
        .expect("devices")
        .push(serde_json::json!({
            "id": next_id,
            "reference": "F9",
            "model": { "id": "thermal_overload.generic", "version": 1 },
            "params": {}
        }));
    doc["next_id"] = serde_json::json!(next_id + 1);
    fs::write(&project, doc.to_string()).expect("write project");
    let path = project.to_str().expect("UTF-8 path");

    let lenient = opencircuit(&["validate", path]);
    let strict = opencircuit(&["validate", path, "--deny-warnings"]);

    assert_eq!(code(&lenient), 0);
    assert!(stdout(&lenient).contains("warning[OC-SEM-005]"));
    assert_eq!(code(&strict), 1);
}

#[test]
fn project_that_does_not_load_fails_validation() {
    let directory = scratch("broken");
    let project = directory.join("broken.ocircuit");
    fs::write(&project, r#"{ "schema_version": 9 }"#).expect("write");

    let output = opencircuit(&["validate", project.to_str().expect("UTF-8 path")]);

    assert_eq!(code(&output), 1);
    assert!(stdout(&output).contains("schema version 9"));
}

#[test]
fn missing_file_is_an_io_error() {
    let output = opencircuit(&["validate", "does/not/exist.ocircuit"]);

    assert_eq!(code(&output), 2);
    assert!(stderr(&output).contains("does/not/exist.ocircuit"));
}

#[test]
fn simulate_prints_the_golden_trace() {
    let output = opencircuit(&["simulate", COMMAND, "--scenario", COMMAND_SCENARIO]);
    let golden =
        fs::read_to_string(root().join("examples/direct-starter/command.trace")).expect("golden");

    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), golden);
}

#[test]
fn simulate_writes_trace_and_snapshot_files() {
    let directory = scratch("simulate-files");
    let trace = directory.join("out.trace");
    let snapshot = directory.join("snapshot.json");

    let output = opencircuit(&[
        "simulate",
        COMMAND,
        "--scenario",
        COMMAND_SCENARIO,
        "--trace",
        trace.to_str().expect("UTF-8 path"),
        "--snapshot-json",
        snapshot.to_str().expect("UTF-8 path"),
    ]);
    let snapshot: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&snapshot).expect("snapshot")).expect("JSON");

    assert_eq!(code(&output), 0);
    assert_eq!(stdout(&output), "");
    assert!(
        fs::read_to_string(&trace)
            .expect("trace")
            .starts_with("# opencircuit trace v1\n")
    );
    assert_eq!(snapshot["tick"], 20);
    assert_eq!(snapshot["t_us"], 20_000);
    assert_eq!(snapshot["faulted"], false);
    assert_eq!(snapshot["active_devices"], serde_json::json!([]));
}

#[test]
fn command_line_overrides_the_scenario_fault_policy() {
    let output = opencircuit(&[
        "simulate",
        "examples/forward-reverse/reversing.ocircuit",
        "--scenario",
        "examples/forward-reverse/simultaneous.scenario.json",
        "--fault-policy",
        "report",
    ]);
    let trace = stdout(&output);

    assert_eq!(code(&output), 0);
    assert!(trace.contains("fault_policy=report"));
    assert!(trace.contains("t=4 fault OC-ELEC-001 cleared"));
    assert!(!trace.contains("runtime latched"));
}

#[test]
fn simulate_rejects_an_unknown_designator_and_lists_the_known_ones() {
    let directory = scratch("unknown-device");
    let scenario = directory.join("bad.scenario.json");
    fs::write(
        &scenario,
        r#"{ "schema_version": 1, "circuit": "x", "until_tick": 3,
             "events": [{ "tick": 1, "action": "press", "device": "S9" }] }"#,
    )
    .expect("write");

    let output = opencircuit(&[
        "simulate",
        COMMAND,
        "--scenario",
        scenario.to_str().expect("UTF-8 path"),
    ]);

    assert_eq!(code(&output), 2);
    assert!(stderr(&output).contains("no device `S9`; it has: P24, P0, S2, S1, K1"));
}

#[test]
fn simulate_refuses_a_project_that_does_not_compile() {
    let output = opencircuit(&[
        "simulate",
        STATIC_SHORT,
        "--scenario",
        "tests/fixtures/static-short/static-short.scenario.json",
    ]);

    assert_eq!(code(&output), 1);
    assert!(stderr(&output).contains("OC-ELEC-001"));
}

#[test]
fn every_committed_scenario_passes() {
    let output = opencircuit(&["test", "examples", "tests/fixtures"]);

    assert_eq!(code(&output), 0, "{}", stdout(&output));
    assert!(stdout(&output).ends_with("24 scenario(s), 0 failed\n"));
}

#[test]
fn changed_golden_fails_with_the_first_difference() {
    let directory = copy_example("examples/direct-starter", "tampered");
    let golden = directory.join("command.trace");
    let text = fs::read_to_string(&golden).expect("golden");
    fs::write(&golden, text.replace("t=11 signal", "t=12 signal")).expect("tamper");

    let output = opencircuit(&["test", directory.to_str().expect("UTF-8 path")]);
    let report = stdout(&output);

    assert_eq!(code(&output), 1);
    assert!(report.contains("FAIL"));
    assert!(report.contains("expected: t=12 signal K1.active 1->0"));
    assert!(report.contains("actual:   t=11 signal K1.active 1->0"));
}

#[test]
fn golden_with_crlf_line_endings_is_reported_as_such() {
    let directory = copy_example("examples/direct-starter", "crlf");
    let golden = directory.join("command.trace");
    let text = fs::read_to_string(&golden).expect("golden");
    fs::write(&golden, text.replace('\n', "\r\n")).expect("rewrite");

    let output = opencircuit(&["test", directory.to_str().expect("UTF-8 path")]);

    assert_eq!(code(&output), 1);
    assert!(stdout(&output).contains("check line endings"));
}

#[test]
fn update_golden_rewrites_a_stale_golden() {
    let directory = copy_example("examples/direct-starter", "update");
    let golden = directory.join("command.trace");
    let original = fs::read_to_string(&golden).expect("golden");
    fs::write(&golden, "stale\n").expect("tamper");
    let path = directory.to_str().expect("UTF-8 path");

    let update = opencircuit(&["test", path, "--update-golden"]);
    let rerun = opencircuit(&["test", path]);

    assert_eq!(code(&update), 0);
    assert_eq!(fs::read_to_string(&golden).expect("golden"), original);
    assert_eq!(code(&rerun), 0);
}

#[test]
fn update_golden_is_refused_in_ci() {
    let output = Command::new(env!("CARGO_BIN_EXE_opencircuit"))
        .args(["test", "examples", "--update-golden"])
        .current_dir(root())
        .env("CI", "true")
        .output()
        .expect("the binary runs");

    assert_eq!(code(&output), 2);
    assert!(stderr(&output).contains("refused while the CI environment variable is set"));
}

#[test]
fn directory_without_scenarios_is_a_usage_error() {
    let directory = scratch("empty");

    let output = opencircuit(&["test", directory.to_str().expect("UTF-8 path")]);

    assert_eq!(code(&output), 2);
    assert!(stderr(&output).contains("no `*.scenario.json` file found"));
}

/// Every file below `directory`, in a stable order.
fn files_below(directory: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![directory.to_path_buf()];
    while let Some(current) = pending.pop() {
        for entry in fs::read_dir(&current).expect("directory is readable") {
            let path = entry.expect("entry").path();
            if path.is_dir() {
                pending.push(path);
            } else {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

#[test]
fn examples_and_fixtures_contain_no_carriage_return() {
    // Goldens are compared byte for byte. A checkout that rewrites line
    // endings (for example `core.autocrlf` without `.gitattributes`) would
    // turn every golden into an unreadable diff; this names the file instead.
    let mut offenders = Vec::new();
    for directory in ["examples", "tests/fixtures"] {
        for path in files_below(&root().join(directory)) {
            // PDF goldens are binary (`.gitattributes`): their cross-reference
            // table ends lines with CR LF by design.
            if path.extension().is_some_and(|extension| extension == "pdf") {
                continue;
            }
            let bytes = fs::read(&path).expect("file is readable");
            if bytes.contains(&b'\r') {
                offenders.push(path);
            }
        }
    }

    assert_eq!(offenders, Vec::<PathBuf>::new());
}

#[test]
fn export_writes_the_same_svg_as_the_committed_drawing() {
    let directory = scratch("export-svg");
    let output = directory.join("star-delta.svg");
    let output_text = output.to_str().expect("UTF-8 path");

    let run = opencircuit(&[
        "export",
        "examples/star-delta/star-delta.ocircuit",
        "--output",
        output_text,
    ]);

    assert_eq!(code(&run), 0, "{}", stderr(&run));
    assert_eq!(
        fs::read_to_string(&output).expect("written"),
        fs::read_to_string(root().join("examples/star-delta/star-delta.svg")).expect("golden")
    );
}

#[test]
fn export_is_deterministic_in_the_live_theme() {
    let directory = scratch("export-live");
    let mut texts = Vec::new();
    for name in ["first.svg", "second.svg"] {
        let output = directory.join(name);
        let run = opencircuit(&[
            "export",
            COMMAND,
            "--output",
            output.to_str().expect("UTF-8 path"),
            "--theme",
            "live",
        ]);
        assert_eq!(code(&run), 0, "{}", stderr(&run));
        texts.push(fs::read(&output).expect("written"));
    }

    assert_eq!(texts[0], texts[1]);
}

#[test]
fn export_draws_a_project_that_does_not_compile() {
    let directory = scratch("export-broken");
    let output = directory.join("short.svg");

    let run = opencircuit(&[
        "export",
        STATIC_SHORT,
        "--output",
        output.to_str().expect("UTF-8 path"),
    ]);

    assert_eq!(code(&run), 0, "{}", stderr(&run));
    assert!(
        fs::read_to_string(&output)
            .expect("written")
            .starts_with("<?xml")
    );
}

#[test]
fn export_of_an_unreadable_project_fails_and_writes_nothing() {
    let directory = scratch("export-unreadable");
    let project = directory.join("broken.ocircuit");
    fs::write(&project, "{ not json").expect("project");
    let output = directory.join("broken.svg");

    let run = opencircuit(&[
        "export",
        project.to_str().expect("UTF-8 path"),
        "--output",
        output.to_str().expect("UTF-8 path"),
    ]);

    assert_eq!(code(&run), 1);
    assert!(!output.exists());
}

#[test]
fn export_with_an_unknown_extension_is_a_usage_error() {
    let run = opencircuit(&["export", COMMAND, "--output", "drawing.png"]);

    assert_eq!(code(&run), 2);
    assert!(stderr(&run).contains("`.svg` or `.pdf`"));
}

#[test]
fn export_writes_the_same_pdf_as_the_committed_drawing() {
    let directory = scratch("export-pdf");
    let output = directory.join("star-delta.pdf");

    let run = opencircuit(&[
        "export",
        "examples/star-delta/star-delta.ocircuit",
        "--output",
        output.to_str().expect("UTF-8 path"),
    ]);

    assert_eq!(code(&run), 0, "{}", stderr(&run));
    assert_eq!(
        fs::read(&output).expect("written"),
        fs::read(root().join("examples/star-delta/star-delta.pdf")).expect("golden")
    );
}

const SHEETS: &str = "examples/sheets/starter-sheets.ocircuit";

fn export_to(directory: &Path, name: &str, extra: &[&str]) -> (Output, PathBuf) {
    let output = directory.join(name);
    let mut args = vec![
        "export",
        SHEETS,
        "--output",
        output.to_str().expect("UTF-8 path"),
    ];
    args.extend_from_slice(extra);
    (opencircuit(&args), output)
}

fn pdf_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn a_pdf_of_a_project_with_sheets_has_one_page_per_sheet_and_matches_the_committed_one() {
    let directory = scratch("export-sheets-pdf");

    let (run, output) = export_to(&directory, "all.pdf", &[]);

    assert_eq!(code(&run), 0, "{}", stderr(&run));
    let bytes = fs::read(&output).expect("written");
    let text = pdf_text(&bytes);
    assert!(text.contains("/Count 2"));
    assert_eq!(text.matches("/MediaBox [0 0 840 590]").count(), 2);
    assert_eq!(
        bytes,
        fs::read(root().join("examples/sheets/starter-sheets.pdf")).expect("golden")
    );
    // `all` says the same.
    let (run, again) = export_to(&directory, "again.pdf", &["--sheet", "all"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    assert_eq!(fs::read(&again).expect("written"), bytes);
}

#[test]
fn one_sheet_is_exported_as_a_page_of_paper_in_pdf_or_svg() {
    let directory = scratch("export-one-sheet");

    let (run, pdf) = export_to(&directory, "two.pdf", &["--sheet", "2"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    let text = pdf_text(&fs::read(&pdf).expect("written"));
    assert!(text.contains("/Count 1") && text.contains("/MediaBox [0 0 840 590]"));

    let (run, svg) = export_to(&directory, "two.svg", &["--sheet", "2"]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    let svg = fs::read_to_string(&svg).expect("written");
    assert!(
        svg.contains("viewBox=\"1060 -80 840 590\""),
        "{}",
        &svg[..300]
    );
    assert!(svg.contains("Sheet 2/2") && !svg.contains("Sheet 1/2"));
    assert!(svg.contains("for\u{e7}a"));
}

#[test]
fn export_sheet_refuses_what_it_cannot_do() {
    let directory = scratch("export-sheet-errors");

    let (missing, written) = export_to(&directory, "x.pdf", &["--sheet", "9"]);
    assert_eq!(code(&missing), 1);
    assert!(
        stderr(&missing).contains("no sheet number 9"),
        "{}",
        stderr(&missing)
    );
    assert!(!written.exists());

    let (svg_all, _) = export_to(&directory, "x.svg", &["--sheet", "all"]);
    assert_eq!(code(&svg_all), 2);
    assert!(stderr(&svg_all).contains("an SVG holds one page"));

    for bad in ["0", "-1", "two"] {
        let (run, _) = export_to(&directory, "y.pdf", &["--sheet", bad]);
        assert_eq!(code(&run), 2, "{bad}");
    }
    // A project without sheets has none to choose.
    let none = opencircuit(&[
        "export",
        "examples/direct-starter/starter.ocircuit",
        "--output",
        directory.join("z.pdf").to_str().expect("UTF-8 path"),
        "--sheet",
        "1",
    ]);
    assert_eq!(code(&none), 1);
}

const ACME_LAMP: &str = "examples/library/acme-lamp.ocircuit";

#[test]
fn a_project_of_a_library_model_validates_only_with_the_library() {
    let without = opencircuit(&["validate", ACME_LAMP]);
    let file = opencircuit(&[
        "validate",
        ACME_LAMP,
        "--library",
        "examples/library/acme.oclib",
    ]);
    let directory = opencircuit(&["validate", ACME_LAMP, "--library", "examples/library"]);

    assert_eq!(code(&without), 1);
    assert!(stdout(&without).contains("OC-STR-003"));
    assert_eq!(code(&file), 0, "{}", stdout(&file));
    assert_eq!(code(&directory), 0, "{}", stdout(&directory));
}

#[test]
fn a_broken_library_is_a_usage_error_that_names_the_file() {
    let directory = scratch("broken-library");
    let library = directory.join("broken.oclib");
    fs::write(&library, "{ \"schema_version\": 1 }").expect("library");

    let run = opencircuit(&[
        "validate",
        ACME_LAMP,
        "--library",
        library.to_str().expect("UTF-8 path"),
    ]);

    assert_eq!(code(&run), 2);
    assert!(stderr(&run).contains("broken.oclib"), "{}", stderr(&run));
}

#[test]
fn export_draws_library_artwork_with_the_library() {
    let directory = scratch("export-library");
    let output = directory.join("acme.svg");

    let run = opencircuit(&[
        "export",
        ACME_LAMP,
        "--output",
        output.to_str().expect("UTF-8 path"),
        "--library",
        "examples/library",
    ]);

    assert_eq!(code(&run), 0, "{}", stderr(&run));
    assert_eq!(
        fs::read_to_string(&output).expect("written"),
        fs::read_to_string(root().join("examples/library/acme-lamp.svg")).expect("golden")
    );
}

mod serve {
    use std::io::{BufRead as _, BufReader, Read as _, Write as _};
    use std::net::TcpStream;
    use std::process::{Child, Command, Stdio};
    use std::time::Duration;

    use super::{ACME_LAMP, COMMAND, code, opencircuit, root, stdout};

    /// A running server, killed when dropped.
    struct Server {
        child: Child,
        address: String,
    }

    impl Drop for Server {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    fn start(project: &str) -> Server {
        start_with(project, &[])
    }

    fn start_with(project: &str, options: &[&str]) -> Server {
        let mut child = Command::new(env!("CARGO_BIN_EXE_opencircuit"))
            .args([
                "serve",
                project,
                "--listen",
                "127.0.0.1:0",
                "--duration-ms",
                "20000",
            ])
            .args(options)
            .current_dir(root())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("the server starts");
        let mut line = String::new();
        BufReader::new(child.stdout.take().expect("stdout"))
            .read_line(&mut line)
            .expect("the server announces its address");
        let address = line
            .trim()
            .strip_prefix("listening on ")
            .unwrap_or_else(|| panic!("unexpected first line `{line}`"))
            .to_owned();
        Server { child, address }
    }

    fn connect(server: &Server) -> TcpStream {
        let stream = TcpStream::connect(&server.address).expect("connects");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .expect("timeout");
        stream
    }

    /// Send one request PDU and return the answer PDU.
    fn ask(stream: &mut TcpStream, transaction: u16, pdu: &[u8]) -> Vec<u8> {
        let mut frame = transaction.to_be_bytes().to_vec();
        frame.extend([0, 0]);
        frame.extend(u16::try_from(pdu.len() + 1).unwrap().to_be_bytes());
        frame.push(1);
        frame.extend(pdu);
        stream.write_all(&frame).expect("sent");
        let mut header = [0_u8; 7];
        stream.read_exact(&mut header).expect("header");
        assert_eq!(&header[..2], &transaction.to_be_bytes());
        let length = usize::from(u16::from_be_bytes([header[4], header[5]])) - 1;
        let mut answer = vec![0_u8; length];
        stream.read_exact(&mut answer).expect("answer");
        answer
    }

    fn k1_active(stream: &mut TcpStream) -> bool {
        discrete_input(stream, 0)
    }

    fn discrete_input(stream: &mut TcpStream, address: u8) -> bool {
        let answer = ask(stream, 9, &[2, 0, address, 0, 1]);
        assert_eq!(&answer[..2], &[2, 1]);
        answer[2] & 1 == 1
    }

    fn eventually(stream: &mut TcpStream, expected: bool) {
        eventually_at(stream, 0, expected);
    }

    fn eventually_at(stream: &mut TcpStream, address: u8, expected: bool) {
        for _ in 0..200 {
            if discrete_input(stream, address) == expected {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("discrete input {address} never became {expected}");
    }

    #[test]
    fn print_map_lists_the_addresses_without_listening() {
        let run = opencircuit(&["serve", COMMAND, "--print-map"]);

        assert_eq!(
            stdout(&run),
            "coil 0 S2 (1 = pressed)\n\
             coil 1 S1 (1 = pressed)\n\
             discrete input 0 K1 (1 = active)\n\
             input registers 0-3 tick (highest word first)\n\
             input register 4 fault latch (1 = latched)\n"
        );
    }

    #[test]
    fn a_client_starts_and_stops_the_direct_online_starter() {
        let server = start(COMMAND);
        let mut client = connect(&server);

        // Press S1 (coil 1): K1 picks up and seals in.
        assert_eq!(
            ask(&mut client, 1, &[5, 0, 1, 0xFF, 0x00]),
            [5, 0, 1, 0xFF, 0x00]
        );
        eventually(&mut client, true);
        ask(&mut client, 2, &[5, 0, 1, 0x00, 0x00]);
        std::thread::sleep(Duration::from_millis(50));
        assert!(k1_active(&mut client));

        // Press S2 (coil 0), the stop button: K1 drops out.
        ask(&mut client, 3, &[5, 0, 0, 0xFF, 0x00]);
        eventually(&mut client, false);

        // The tick advances with real time.
        let registers = ask(&mut client, 4, &[4, 0, 0, 0, 5]);
        assert_eq!(registers[..2], [4, 10]);
        let tick = registers[2..10]
            .iter()
            .fold(0_u64, |tick, byte| (tick << 8) | u64::from(*byte));
        assert!(tick > 0);
    }

    #[test]
    fn requests_outside_the_map_or_the_supported_functions_get_exceptions() {
        let server = start(COMMAND);
        let mut client = connect(&server);

        assert_eq!(ask(&mut client, 1, &[1, 0, 2, 0, 1]), [0x81, 2]);
        assert_eq!(ask(&mut client, 2, &[3, 0, 0, 0, 1]), [0x83, 1]);
        assert_eq!(ask(&mut client, 3, &[6, 0, 0, 0, 1]), [0x86, 1]);
    }

    #[test]
    fn a_frame_that_is_not_modbus_closes_the_connection() {
        let server = start(COMMAND);
        let mut client = connect(&server);

        client
            .write_all(&[0, 1, 0x12, 0x34, 0, 6, 1, 1, 0, 0, 0, 1])
            .expect("sent");
        let mut byte = [0_u8; 1];

        // Closed: end of stream, or a reset on some platforms; never a
        // timeout with the connection still open.
        match client.read(&mut byte) {
            Ok(read) => assert_eq!(read, 0),
            Err(error) => assert!(
                !matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ),
                "{error}"
            ),
        }
    }

    #[test]
    fn the_fifth_client_is_refused() {
        let server = start(COMMAND);
        let mut kept: Vec<TcpStream> = (0..4).map(|_| connect(&server)).collect();
        for (index, client) in kept.iter_mut().enumerate() {
            let transaction = u16::try_from(index).unwrap();
            assert_eq!(ask(client, transaction, &[2, 0, 0, 0, 1])[0], 2);
        }

        let mut fifth = connect(&server);
        let mut byte = [0_u8; 1];

        assert_eq!(fifth.read(&mut byte).unwrap_or(0), 0);
    }

    /// The exit test of M4 (ROADMAP.md): a third-party library is loaded,
    /// the example that uses it simulates against its golden trace, and the
    /// Modbus server operates it.
    #[test]
    fn m4_exit_a_library_component_is_simulated_and_served() {
        let library = "examples/library/acme.oclib";

        // Loaded: the project validates with the library only.
        let validate = opencircuit(&["validate", ACME_LAMP, "--library", library]);
        assert_eq!(code(&validate), 0, "{}", stdout(&validate));

        // Simulated: the scenario declares the library and matches its golden.
        let test = opencircuit(&["test", "examples/library"]);
        assert_eq!(code(&test), 0, "{}", stdout(&test));

        // Served: the map lists the library relay, and a client holding S1
        // picks it up and lights H1.
        let map = opencircuit(&["serve", ACME_LAMP, "--library", library, "--print-map"]);
        assert!(
            stdout(&map).contains("coil 0 S1 (1 = pressed)\ndiscrete input 0 K1 (1 = active)\ndiscrete input 1 H1 (1 = energized)\n"),
            "{}",
            stdout(&map)
        );
        let server = start_with(ACME_LAMP, &["--library", library]);
        let mut client = connect(&server);
        assert!(!discrete_input(&mut client, 1));
        ask(&mut client, 1, &[5, 0, 0, 0xFF, 0x00]);
        eventually_at(&mut client, 0, true);
        eventually_at(&mut client, 1, true);
        ask(&mut client, 2, &[5, 0, 0, 0x00, 0x00]);
        eventually_at(&mut client, 1, false);
    }
}

// `--format json`: one document on stdout, `"format": 1`.

fn json(output: &Output) -> serde_json::Value {
    serde_json::from_str(&stdout(output)).unwrap_or_else(|error| {
        panic!(
            "stdout is not one JSON document ({error}): {}",
            stdout(output)
        )
    })
}

#[test]
fn every_json_document_names_its_format_and_command() {
    let validate = opencircuit(&["validate", COMMAND, "--format", "json"]);
    let value = json(&validate);

    assert_eq!(value["format"], 1);
    assert_eq!(value["command"], "validate");
    assert_eq!(value["ok"], true);
}

#[test]
fn simulate_in_json_carries_the_trace_the_snapshot_and_the_diagnostics() {
    let run = opencircuit(&[
        "simulate",
        "examples/direct-starter/starter.ocircuit",
        "--scenario",
        "examples/direct-starter/starter.scenario.json",
        "--format",
        "json",
    ]);

    assert_eq!(code(&run), 0, "{}", stderr(&run));
    let value = json(&run);
    assert_eq!(
        (value["format"].clone(), value["command"].clone()),
        (1.into(), "simulate".into())
    );
    assert_eq!(value["ok"], true);
    assert_eq!(value["faulted"], false);
    assert_eq!(value["tick"], 24);
    assert_eq!(value["diagnostics"], serde_json::json!([]));
    assert_eq!(
        value["trace"].as_str().expect("the trace text"),
        fs::read_to_string(root().join("examples/direct-starter/starter.trace")).expect("golden")
    );
    assert_eq!(value["snapshot"]["tick"], 24);
    // Nothing goes to stderr: the diagnostics are in the document.
    assert_eq!(stderr(&run), "");
}

#[test]
fn simulate_in_json_leaves_the_trace_to_its_file_when_one_is_asked_for() {
    let directory = scratch("simulate-json-trace");
    let trace = directory.join("out.trace");

    let run = opencircuit(&[
        "simulate",
        "examples/direct-starter/starter.ocircuit",
        "--scenario",
        "examples/direct-starter/starter.scenario.json",
        "--trace",
        trace.to_str().expect("UTF-8 path"),
        "--format",
        "json",
    ]);

    assert_eq!(code(&run), 0, "{}", stderr(&run));
    assert!(json(&run)["trace"].is_null());
    assert!(
        fs::read_to_string(&trace)
            .expect("written")
            .starts_with("# opencircuit trace v1")
    );
}

#[test]
fn a_circuit_that_does_not_compile_is_a_failure_document_with_its_diagnostics() {
    let run = opencircuit(&[
        "simulate",
        "tests/fixtures/static-short/static-short.ocircuit",
        "--scenario",
        "tests/fixtures/static-short/static-short.scenario.json",
        "--format",
        "json",
    ]);

    assert_eq!(code(&run), 1);
    let value = json(&run);
    assert_eq!(value["ok"], false);
    assert_eq!(value["command"], "simulate");
    assert!(
        value["error"]
            .as_str()
            .expect("a message")
            .contains("does not compile")
    );
    assert!(
        value["diagnostics"]
            .as_array()
            .expect("a list")
            .iter()
            .any(|diagnostic| diagnostic["code"] == "OC-ELEC-001")
    );
}

#[test]
fn test_in_json_lists_every_scenario_and_why_one_failed() {
    let all = opencircuit(&["test", "examples", "tests/fixtures", "--format", "json"]);
    assert_eq!(code(&all), 0, "{}", stdout(&all));
    let value = json(&all);
    assert_eq!(
        (
            value["ok"].clone(),
            value["total"].clone(),
            value["failed"].clone()
        ),
        (true.into(), 24.into(), 0.into())
    );
    let scenarios = value["scenarios"].as_array().expect("a list");
    assert_eq!(scenarios.len(), 24);
    assert!(
        scenarios
            .iter()
            .all(|s| s["status"] == "pass" && s["reasons"] == serde_json::json!([]))
    );

    // A golden trace that no longer matches.
    let directory = copy_example("examples/direct-starter", "test-json-fail");
    fs::write(directory.join("starter.trace"), "# opencircuit trace v1\n").expect("changed");
    let failing = opencircuit(&[
        "test",
        directory.to_str().expect("UTF-8 path"),
        "--format",
        "json",
    ]);
    assert_eq!(code(&failing), 1);
    let value = json(&failing);
    assert_eq!(value["ok"], false);
    let failed: Vec<&serde_json::Value> = value["scenarios"]
        .as_array()
        .expect("a list")
        .iter()
        .filter(|s| s["status"] == "fail")
        .collect();
    assert_eq!(failed.len(), 1);
    assert!(
        failed[0]["reasons"][0]
            .as_str()
            .expect("a reason")
            .contains("differs")
    );
}

#[test]
fn export_in_json_names_the_file_its_kind_and_its_pages() {
    let directory = scratch("export-json");
    let pdf = directory.join("s.pdf");
    let svg = directory.join("s.svg");

    let run = opencircuit(&[
        "export",
        SHEETS,
        "--output",
        pdf.to_str().expect("UTF-8 path"),
        "--format",
        "json",
    ]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    let value = json(&run);
    assert_eq!(
        (value["kind"].clone(), value["pages"].clone()),
        ("pdf".into(), 2.into())
    );
    assert!(value["file"].as_str().expect("a path").ends_with("s.pdf"));

    let run = opencircuit(&[
        "export",
        SHEETS,
        "--output",
        svg.to_str().expect("UTF-8 path"),
        "--sheet",
        "1",
        "--format",
        "json",
    ]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    assert_eq!(
        (json(&run)["kind"].clone(), json(&run)["pages"].clone()),
        ("svg".into(), 1.into())
    );

    // Asking for a sheet that is not there is a failure document, and writes
    // nothing.
    let missing = directory.join("none.pdf");
    let run = opencircuit(&[
        "export",
        SHEETS,
        "--output",
        missing.to_str().expect("UTF-8 path"),
        "--sheet",
        "9",
        "--format",
        "json",
    ]);
    assert_eq!(code(&run), 1);
    assert_eq!(json(&run)["ok"], false);
    assert!(!missing.exists());
}

#[test]
fn the_modbus_map_in_json_is_the_map_of_the_text() {
    let text = opencircuit(&["serve", COMMAND, "--print-map"]);
    let run = opencircuit(&["serve", COMMAND, "--print-map", "--format", "json"]);

    assert_eq!(code(&run), 0, "{}", stderr(&run));
    let value = json(&run);
    assert_eq!(value["command"], "serve");
    let lines: Vec<String> = value["map"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|entry| {
            let table = match entry["table"].as_str().expect("a table") {
                "discrete_input" => "discrete input",
                "input_register" if entry["words"] == 1 => "input register",
                "input_register" => "input registers",
                other => other,
            };
            let address = entry["address"].as_u64().expect("an address");
            let words = entry["words"].as_u64().expect("a size");
            let range = if words > 1 {
                format!("{address}-{}", address + words - 1)
            } else {
                address.to_string()
            };
            format!(
                "{table} {range} {} ({})",
                entry["name"].as_str().expect("a name"),
                entry["meaning"].as_str().expect("a meaning")
            )
        })
        .collect();
    assert_eq!(lines.join("\n") + "\n", stdout(&text));
}

#[test]
fn a_bad_format_and_serve_without_print_map_are_usage_errors() {
    for args in [
        vec!["test", "examples", "--format", "xml"],
        vec!["serve", COMMAND, "--format", "json"],
    ] {
        let run = opencircuit(&args);
        assert_eq!(code(&run), 2, "{args:?}");
        assert_eq!(stdout(&run), "");
    }
}

// `new` and `edit`: a project made through the command line.

const EDIT_FILE: &str = "examples/edit/starter-sheets.edit.json";

/// A trace without its fingerprint line, which depends on the order in which
/// the IDs were handed out and not on the circuit.
fn without_fingerprint(trace: &str) -> String {
    trace
        .lines()
        .filter(|line| !line.starts_with("# fingerprint"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn m5_exit_a_project_made_only_through_the_command_line_validates_simulates_and_exports() {
    let directory = scratch("m5-exit");
    let project = directory.join("starter-sheets.ocircuit");
    let path = project.to_str().expect("UTF-8 path");

    let made = opencircuit(&["new", path, "--name", "Partida direta"]);
    assert_eq!(code(&made), 0, "{}", stderr(&made));

    let edited = opencircuit(&["edit", path, "--apply", EDIT_FILE, "--format", "json"]);
    assert_eq!(code(&edited), 0, "{}{}", stdout(&edited), stderr(&edited));
    let report = json(&edited);
    assert_eq!(report["operations"], 55);
    assert_eq!(
        (report["errors"].clone(), report["warnings"].clone()),
        (0.into(), 0.into())
    );
    assert_eq!(report["created"]["coil"]["kind"], "symbol");
    assert_eq!(report["created"]["control"]["kind"], "supply");

    // Valid, with no warning at all.
    let validate = opencircuit(&["validate", path, "--deny-warnings"]);
    assert_eq!(code(&validate), 0, "{}", stdout(&validate));

    // Two sheets with the title block filled in.
    let text = fs::read_to_string(&project).expect("written");
    let doc: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    assert_eq!(doc["schema_version"], 3);
    assert_eq!(doc["sheets"].as_array().expect("sheets").len(), 2);
    assert_eq!(doc["project"]["author"], "OpenCircuit");

    // Simulated: the same events as the example drawn by the generator.
    fs::copy(
        root().join("examples/sheets/starter-sheets.scenario.json"),
        directory.join("starter-sheets.scenario.json"),
    )
    .expect("copy");
    let simulate = opencircuit(&[
        "simulate",
        path,
        "--scenario",
        directory
            .join("starter-sheets.scenario.json")
            .to_str()
            .expect("UTF-8 path"),
    ]);
    assert_eq!(code(&simulate), 0, "{}", stderr(&simulate));
    let golden =
        fs::read_to_string(root().join("examples/sheets/starter-sheets.trace")).expect("golden");
    assert_eq!(
        without_fingerprint(&stdout(&simulate)),
        without_fingerprint(&golden)
    );

    // Exported: a two-page PDF that is, byte for byte, the committed one, so
    // it has the frame, the terminal numbers and the cross-references of the
    // example.
    let pdf = directory.join("out.pdf");
    let export = opencircuit(&[
        "export",
        path,
        "--output",
        pdf.to_str().expect("UTF-8 path"),
    ]);
    assert_eq!(code(&export), 0, "{}", stderr(&export));
    let bytes = fs::read(&pdf).expect("written");
    assert!(String::from_utf8_lossy(&bytes).contains("/Count 2"));
    assert_eq!(
        bytes,
        fs::read(root().join("examples/sheets/starter-sheets.pdf")).expect("committed")
    );

    // And the text of the first sheet, readable in its SVG.
    let svg = directory.join("one.svg");
    let export = opencircuit(&[
        "export",
        path,
        "--output",
        svg.to_str().expect("UTF-8 path"),
        "--sheet",
        "1",
    ]);
    assert_eq!(code(&export), 0, "{}", stderr(&export));
    let svg = fs::read_to_string(&svg).expect("written");
    for text in [
        ">A1<",
        ">13-14 1/2C<",
        ">1-6 2/2B<",
        ">Sheet 1/2<",
        ">Circuito de comando<",
    ] {
        assert!(svg.contains(text), "{text}");
    }
}

#[test]
fn new_writes_an_empty_project_and_refuses_to_replace_one_without_force() {
    let directory = scratch("new");
    let file = directory.join("pump.ocircuit");
    let path = file.to_str().expect("UTF-8 path");

    let run = opencircuit(&["new", path]);
    assert_eq!(code(&run), 0, "{}", stderr(&run));
    let doc: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&file).expect("written")).expect("JSON");
    // The name is the file name without its extension.
    assert_eq!(doc["project"]["name"], "pump");
    assert_eq!(doc["sheets"], serde_json::json!([]));
    let valid = opencircuit(&["validate", path]);
    assert_eq!(code(&valid), 0, "{}", stdout(&valid));

    fs::write(&file, "keep me").expect("written");
    let again = opencircuit(&["new", path]);
    assert_eq!(code(&again), 2);
    assert_eq!(fs::read_to_string(&file).expect("kept"), "keep me");

    let forced = opencircuit(&[
        "new", path, "--name", "Pump 2", "--force", "--format", "json",
    ]);
    assert_eq!(code(&forced), 0, "{}", stderr(&forced));
    assert_eq!(json(&forced)["command"], "new");
    assert!(
        fs::read_to_string(&file)
            .expect("written")
            .contains("Pump 2")
    );
}

#[test]
fn edit_dry_run_and_output_leave_the_project_alone() {
    let directory = scratch("edit-copies");
    let project = directory.join("a.ocircuit");
    let path = project.to_str().expect("UTF-8 path");
    assert_eq!(code(&opencircuit(&["new", path])), 0);
    let before = fs::read(&project).expect("written");

    let dry = opencircuit(&[
        "edit",
        path,
        "--apply",
        EDIT_FILE,
        "--dry-run",
        "--format",
        "json",
    ]);
    assert_eq!(code(&dry), 0, "{}", stderr(&dry));
    assert!(json(&dry)["wrote"].is_null());
    assert_eq!(fs::read(&project).expect("kept"), before);

    let elsewhere = directory.join("b.ocircuit");
    let out = opencircuit(&[
        "edit",
        path,
        "--apply",
        EDIT_FILE,
        "--output",
        elsewhere.to_str().expect("UTF-8 path"),
    ]);
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    assert_eq!(fs::read(&project).expect("kept"), before);
    assert!(
        fs::read_to_string(&elsewhere)
            .expect("written")
            .contains("\"sheets\"")
    );
}

#[test]
fn an_operation_that_is_refused_writes_nothing_and_says_which() {
    let directory = scratch("edit-refused");
    let project = directory.join("a.ocircuit");
    let path = project.to_str().expect("UTF-8 path");
    assert_eq!(code(&opencircuit(&["new", path])), 0);
    let before = fs::read(&project).expect("written");
    let file = directory.join("bad.edit.json");
    fs::write(
        &file,
        r#"{ "schema_version": 1, "operations": [
             { "op": "junction", "at": [0, 0] },
             { "op": "set_param", "device": "K9", "key": "x", "value": 1 } ] }"#,
    )
    .expect("written");

    let run = opencircuit(&["edit", path, "--apply", file.to_str().expect("UTF-8 path")]);
    assert_eq!(code(&run), 1);
    assert!(
        stderr(&run).contains("operation 2 (set_param)"),
        "{}",
        stderr(&run)
    );
    assert_eq!(fs::read(&project).expect("kept"), before);

    let json_run = opencircuit(&[
        "edit",
        path,
        "--apply",
        file.to_str().expect("UTF-8 path"),
        "--format",
        "json",
    ]);
    assert_eq!(code(&json_run), 1);
    let value = json(&json_run);
    assert_eq!(
        (value["ok"].clone(), value["command"].clone()),
        (false.into(), "edit".into())
    );
    assert!(
        value["error"]
            .as_str()
            .expect("a message")
            .contains("no device `K9`")
    );
}

#[test]
fn a_file_that_is_not_an_edit_file_is_a_usage_error() {
    let directory = scratch("edit-invalid");
    let project = directory.join("a.ocircuit");
    let path = project.to_str().expect("UTF-8 path");
    assert_eq!(code(&opencircuit(&["new", path])), 0);
    for (name, text) in [
        ("garbage.json", "not json"),
        (
            "unknown.json",
            r#"{ "schema_version": 1, "operations": [ { "op": "rm_rf" } ] }"#,
        ),
        (
            "future.json",
            r#"{ "schema_version": 9, "operations": [] }"#,
        ),
    ] {
        let file = directory.join(name);
        fs::write(&file, text).expect("written");
        let run = opencircuit(&["edit", path, "--apply", file.to_str().expect("UTF-8 path")]);
        assert_eq!(code(&run), 2, "{name}: {}", stderr(&run));
        assert_eq!(stdout(&run), "");
    }
    // A missing project or edit file is an I/O error too.
    let missing = opencircuit(&["edit", path, "--apply", "nowhere.json"]);
    assert_eq!(code(&missing), 2);
}

/// An edit file that uses the standard parts: a solenoid coil between a DC
/// positive and a DC negative terminal, and a diode that is drawn only.
const STANDARD_PARTS_EDIT: &str = r#"{
  "schema_version": 1,
  "operations": [
    { "op": "add_supply", "preset": "dc_24v", "as": "control" },
    { "op": "place", "model": "std.supply_dc_pos", "symbol": "terminal", "at": [0, 0],
      "reference": "P24", "as": "p24" },
    { "op": "set_param", "device": "P24", "key": "supply", "value": { "supply": "control" } },
    { "op": "place", "model": "std.supply_dc_neg", "symbol": "terminal", "at": [20, 0],
      "reference": "P0", "as": "p0" },
    { "op": "set_param", "device": "P0", "key": "supply", "value": { "supply": "control" } },
    { "op": "place", "model": "std.solenoid", "symbol": "coil", "at": [10, 10],
      "reference": "Y1", "as": "coil" },
    { "op": "set_param", "device": "Y1", "key": "rated_voltage_mv", "value": 24000 },
    { "op": "set_param", "device": "Y1", "key": "connection", "value": "dc" },
    { "op": "connect", "from": { "symbol": "p24", "terminal": "out" },
      "to": { "symbol": "coil", "terminal": "a1" } },
    { "op": "connect", "from": { "symbol": "coil", "terminal": "a2" },
      "to": { "symbol": "p0", "terminal": "out" } },
    { "op": "place", "model": "std.diode", "symbol": "body", "at": [30, 10],
      "reference": "A1", "as": "diode" }
  ]
}"#;

#[test]
fn the_standard_parts_are_available_to_every_command_without_a_library_file() {
    let directory = scratch("standard-parts");
    let project = directory.join("parts.ocircuit");
    let path = project.to_str().expect("UTF-8 path");
    let edit = directory.join("parts.edit.json");
    fs::write(&edit, STANDARD_PARTS_EDIT).expect("written");

    let made = opencircuit(&["new", path]);
    assert_eq!(code(&made), 0, "{}", stderr(&made));
    let edited = opencircuit(&[
        "edit",
        path,
        "--apply",
        edit.to_str().expect("UTF-8 path"),
        "--format",
        "json",
    ]);
    assert_eq!(code(&edited), 0, "{}{}", stdout(&edited), stderr(&edited));

    // It validates, and says that the diode is not simulated.
    let validate = opencircuit(&["validate", path, "--format", "json"]);
    assert_eq!(code(&validate), 0, "{}", stdout(&validate));
    let report = json(&validate);
    assert_eq!(report["ok"], true);
    assert!(
        stdout(&validate).contains("OC-SEM-011"),
        "{}",
        stdout(&validate)
    );
    assert_eq!(report["errors"], 0);

    // A model of the library that does not exist is still refused.
    let unknown = STANDARD_PARTS_EDIT.replacen("std.diode", "std.nothing", 1);
    fs::write(&edit, unknown).expect("written");
    let refused = opencircuit(&["edit", path, "--apply", edit.to_str().expect("UTF-8 path")]);
    assert_ne!(code(&refused), 0);
}

const STARTER: &str = "examples/direct-starter/starter.ocircuit";

#[test]
fn report_prints_a_table_as_text_by_default() {
    let output = opencircuit(&["report", STARTER, "--kind", "bom"]);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.starts_with("Bill of materials\n"), "{text}");
    assert!(text.contains("contactor.generic v1"), "{text}");
    // A supply tap is a point of the drawing, not a part.
    assert!(!text.contains("supply.terminal"), "{text}");
}

#[test]
fn report_writes_one_table_as_csv_on_standard_output() {
    let output = opencircuit(&["report", STARTER, "--kind", "io", "--format", "csv"]);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let text = stdout(&output);
    assert!(
        text.starts_with("Table,Address,Words,Name,Meaning\n"),
        "{text}"
    );
    assert!(text.contains("coil,0,1,Q1,1 = closed\n"), "{text}");
}

#[test]
fn report_all_in_json_has_every_table_with_format_1() {
    let output = opencircuit(&["report", STARTER, "--kind", "all", "--format", "json"]);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let document: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("JSON");
    assert_eq!(document["format"], 1);
    assert_eq!(document["command"], "report");
    assert_eq!(document["ok"], true);
    let kinds: Vec<&str> = document["reports"]
        .as_array()
        .expect("reports")
        .iter()
        .map(|report| report["kind"].as_str().expect("kind"))
        .collect();
    assert_eq!(
        kinds,
        ["bom", "wires", "terminals", "xref", "io", "problems"]
    );
    assert_eq!(document["reports"][0]["columns"][1], "Qty");
}

#[test]
fn report_to_a_file_picks_the_format_from_the_extension() {
    let directory = scratch("report-file");
    let csv = directory.join("terminals.csv");

    let output = opencircuit(&[
        "report",
        STARTER,
        "--kind",
        "terminals",
        "--output",
        csv.to_str().expect("utf-8 path"),
    ]);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(
        stdout(&output).starts_with("wrote  "),
        "{}",
        stdout(&output)
    );
    let written = fs::read_to_string(&csv).expect("written");
    assert!(
        written.starts_with("Reference,Symbol,Terminal,Key,Wire,Connected to\n"),
        "{written}"
    );

    let json = directory.join("all.json");
    let output = opencircuit(&[
        "report",
        STARTER,
        "--kind",
        "all",
        "--output",
        json.to_str().expect("utf-8 path"),
        "--format",
        "json",
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let confirmation: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("JSON");
    assert_eq!(confirmation["kind"], "json");
    assert_eq!(confirmation["reports"], 6);
    let written: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&json).expect("written")).expect("JSON");
    assert_eq!(written["reports"].as_array().expect("reports").len(), 6);
}

#[test]
fn report_problems_lists_the_diagnostics_of_a_project_that_does_not_compile() {
    let output = opencircuit(&[
        "report",
        STATIC_SHORT,
        "--kind",
        "problems",
        "--format",
        "csv",
    ]);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert!(
        stdout(&output).contains("error,OC-ELEC-001,"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn report_usage_errors_exit_with_2_and_a_broken_project_with_1() {
    for words in [
        &["report", STARTER][..],
        &["report", STARTER, "--kind", "nope"],
        &["report", STARTER, "--kind", "all", "--format", "csv"],
        &[
            "report",
            STARTER,
            "--kind",
            "bom",
            "--output",
            "report.xlsx",
        ],
    ] {
        let output = opencircuit(words);
        assert_eq!(code(&output), 2, "{words:?}");
    }

    let directory = scratch("report-broken");
    let broken = directory.join("broken.ocircuit");
    fs::write(&broken, "{ not json").expect("write");
    let output = opencircuit(&[
        "report",
        broken.to_str().expect("utf-8 path"),
        "--kind",
        "bom",
        "--format",
        "json",
    ]);
    assert_eq!(code(&output), 1);
    let document: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("JSON");
    assert_eq!(document["ok"], false);
    assert_eq!(document["command"], "report");
}

#[test]
fn report_to_a_pdf_sets_every_table_on_its_own_pages() {
    let directory = scratch("report-pdf");
    let pdf = directory.join("all.pdf");

    let output = opencircuit(&[
        "report",
        STARTER,
        "--kind",
        "all",
        "--output",
        pdf.to_str().expect("utf-8 path"),
        "--format",
        "json",
    ]);

    assert_eq!(code(&output), 0, "{}", stderr(&output));
    let confirmation: serde_json::Value = serde_json::from_str(&stdout(&output)).expect("JSON");
    assert_eq!(confirmation["kind"], "pdf");
    let bytes = fs::read(&pdf).expect("written");
    assert!(bytes.starts_with(b"%PDF-"));
    // At least one page for each of the six tables; a long one takes more.
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let pages: usize = text
        .split("/Count ")
        .nth(1)
        .and_then(|rest| rest.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|digits| digits.parse().ok())
        .expect("a page count");
    assert!(pages >= 6, "{pages} pages");

    let again = directory.join("again.pdf");
    let output = opencircuit(&[
        "report",
        STARTER,
        "--kind",
        "all",
        "--output",
        again.to_str().expect("utf-8 path"),
    ]);
    assert_eq!(code(&output), 0, "{}", stderr(&output));
    assert_eq!(fs::read(&again).expect("written"), bytes);
}

/// Two pushbuttons whose terminals touch at (0, 2): drawn as one line, and
/// not connected unless a wire says so.
fn touching_buttons(wired: bool) -> String {
    let connect = if wired {
        r#",{ "op": "connect", "from": { "symbol": "first", "terminal": "p2" },
              "to": { "symbol": "second", "terminal": "p1" } }"#
    } else {
        ""
    };
    format!(
        r#"{{ "schema_version": 1, "operations": [
            {{ "op": "place", "model": "pushbutton_no", "symbol": "contact", "at": [0, 0],
               "reference": "S1", "as": "first" }},
            {{ "op": "place", "model": "pushbutton_no", "symbol": "contact", "at": [0, 4],
               "reference": "S2", "as": "second" }}{connect}
        ] }}"#
    )
}

#[test]
fn terminals_that_touch_without_a_wire_are_a_warning_and_a_wire_removes_it() {
    let directory = scratch("loose-connection");
    let project = directory.join("loose.ocircuit");
    let path = project.to_str().expect("utf-8 path");
    assert_eq!(code(&opencircuit(&["new", path])), 0);

    for (wired, warned) in [(false, true), (true, false)] {
        let edit = directory.join("edit.json");
        fs::write(&edit, touching_buttons(wired)).expect("write");
        assert_eq!(code(&opencircuit(&["new", path, "--force"])), 0);
        let applied = opencircuit(&["edit", path, "--apply", edit.to_str().expect("utf-8 path")]);
        assert_eq!(
            code(&applied),
            0,
            "{}{}",
            stdout(&applied),
            stderr(&applied)
        );

        let validate = opencircuit(&["validate", path, "--deny-warnings"]);
        let text = stdout(&validate);
        assert_eq!(text.contains("OC-STR-018"), warned, "{text}");
        assert_eq!(code(&validate), i32::from(warned), "{text}");
    }
}

/// Add a waiver to a project file, as a person editing the JSON would.
fn add_waiver(project: &Path, code: &str, subject: &serde_json::Value, reason: &str) {
    let text = fs::read_to_string(project).expect("read");
    let mut doc: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    let waivers = doc["waivers"].take();
    let mut list = waivers.as_array().cloned().unwrap_or_default();
    list.push(serde_json::json!({ "code": code, "subject": subject, "reason": reason }));
    doc["waivers"] = serde_json::Value::Array(list);
    fs::write(project, serde_json::to_string_pretty(&doc).expect("JSON")).expect("write");
}

#[test]
fn a_waived_warning_is_marked_with_its_reason_and_no_longer_fails_deny_warnings() {
    let directory = scratch("waived");
    let project = directory.join("waived.ocircuit");
    let path = project.to_str().expect("utf-8 path");
    assert_eq!(code(&opencircuit(&["new", path])), 0);
    let edit = directory.join("edit.json");
    fs::write(&edit, touching_buttons(false)).expect("write");
    let applied = opencircuit(&["edit", path, "--apply", edit.to_str().expect("utf-8 path")]);
    assert_eq!(
        code(&applied),
        0,
        "{}{}",
        stdout(&applied),
        stderr(&applied)
    );

    let before = opencircuit(&["validate", path, "--deny-warnings", "--format", "json"]);
    assert_eq!(code(&before), 1);
    let before = json(&before);
    let finding = before["diagnostics"][0].clone();
    assert_eq!(finding["code"], "OC-STR-018");
    assert_eq!(finding["waived"], serde_json::Value::Null);
    assert_eq!(before["waived"], 0);

    add_waiver(
        &project,
        "OC-STR-018",
        &finding["subject"],
        "intended: test jig",
    );

    let text = opencircuit(&["validate", path, "--deny-warnings"]);
    assert_eq!(code(&text), 0, "{}", stdout(&text));
    let shown = stdout(&text);
    assert!(shown.contains("waived[OC-STR-018]"), "{shown}");
    assert!(shown.contains("(reason: intended: test jig)"), "{shown}");
    assert!(
        shown.ends_with("ok: 0 error(s), 0 warning(s), 1 waived\n"),
        "{shown}"
    );

    let after = json(&opencircuit(&["validate", path, "--format", "json"]));
    assert_eq!(
        (after["warnings"].clone(), after["waived"].clone()),
        (0.into(), 1.into())
    );
    assert_eq!(
        after["diagnostics"][0]["waived"]["reason"],
        "intended: test jig"
    );

    // A waiver of an error does nothing and is itself reported.
    add_waiver(&project, "OC-STR-013", &finding["subject"], "no");
    let report = opencircuit(&["validate", path, "--deny-warnings"]);
    assert_eq!(code(&report), 1);
    assert!(
        stdout(&report).contains("OC-STR-020"),
        "{}",
        stdout(&report)
    );
}

const DOCUMENTATION_EDIT: &str = "examples/edit/documentation.edit.json";

#[test]
fn m7_exit_a_project_documented_through_the_command_line_reports_numbers_and_exports() {
    let directory = scratch("m7-exit");
    let project = directory.join("documentation.ocircuit");
    let path = project.to_str().expect("UTF-8 path");

    // Made only through the command line: the starter on two sheets, then the
    // numbering rule and the catalog data of its parts.
    assert_eq!(
        code(&opencircuit(&["new", path, "--name", "Partida direta"])),
        0
    );
    let edited = opencircuit(&[
        "edit",
        path,
        "--apply",
        DOCUMENTATION_EDIT,
        "--format",
        "json",
    ]);
    assert_eq!(code(&edited), 0, "{}{}", stdout(&edited), stderr(&edited));
    let report = json(&edited);
    assert_eq!(report["operations"], 62);
    assert_eq!(
        (report["errors"].clone(), report["warnings"].clone()),
        (0.into(), 0.into())
    );
    let validate = opencircuit(&["validate", path, "--deny-warnings"]);
    assert_eq!(code(&validate), 0, "{}", stdout(&validate));

    // The reports are the ones committed next to the example that the
    // generator draws: the same bill of materials, wires, terminals,
    // cross-reference exceptions and I/O map.
    for kind in ["bom", "wires", "terminals", "xref", "io"] {
        let made = opencircuit(&["report", path, "--kind", kind, "--format", "csv"]);
        assert_eq!(code(&made), 0, "{kind}: {}", stderr(&made));
        let golden = fs::read_to_string(root().join(format!("examples/documentation/{kind}.csv")))
            .expect("a committed report");
        assert_eq!(stdout(&made), golden, "{kind}");
    }
    // The wires are numbered on each sheet and say where they go.
    let wires = stdout(&opencircuit(&[
        "report", path, "--kind", "wires", "--format", "csv",
    ]));
    assert!(wires.contains("W1.1,1,F1:95,P24:out"), "{wires}");
    assert!(wires.contains("W2.1,2,L1:out,Q1:1"), "{wires}");
    // The bill of materials carries the catalog data.
    let bom = stdout(&opencircuit(&[
        "report", path, "--kind", "bom", "--format", "csv",
    ]));
    assert!(bom.contains("CTR-9"), "{bom}");

    // All the reports as one PDF, and the drawing with its wire numbers.
    let reports = directory.join("reports.pdf");
    let made = opencircuit(&[
        "report",
        path,
        "--kind",
        "all",
        "--output",
        reports.to_str().expect("UTF-8 path"),
    ]);
    assert_eq!(code(&made), 0, "{}", stderr(&made));
    assert!(fs::read(&reports).expect("written").starts_with(b"%PDF-"));
    let drawing = directory.join("documentation.pdf");
    let exported = opencircuit(&[
        "export",
        path,
        "--output",
        drawing.to_str().expect("UTF-8 path"),
    ]);
    assert_eq!(code(&exported), 0, "{}", stderr(&exported));

    // Renumbering the designators keeps them unique, and the second time
    // changes nothing.
    let renumber = directory.join("renumber.json");
    fs::write(
        &renumber,
        r#"{ "schema_version": 1, "operations": [ { "op": "renumber" } ] }"#,
    )
    .expect("write");
    let apply = |file: &Path| {
        let done = opencircuit(&["edit", path, "--apply", file.to_str().expect("UTF-8 path")]);
        assert_eq!(code(&done), 0, "{}{}", stdout(&done), stderr(&done));
        fs::read_to_string(&project).expect("written")
    };
    let once = apply(&renumber);
    assert_eq!(apply(&renumber), once);
    assert_eq!(
        code(&opencircuit(&["validate", path, "--deny-warnings"])),
        0
    );

    // A drawing that looks connected and is not fails --deny-warnings, until
    // the warning is waived with a reason; the waiver is listed in the
    // problems report.
    let touching = directory.join("touching.json");
    fs::write(
        &touching,
        r#"{ "schema_version": 1, "operations": [
            { "op": "place", "model": "pushbutton_no", "symbol": "contact", "at": [300, 300],
              "reference": "S50", "as": "jig_a" },
            { "op": "place", "model": "pushbutton_no", "symbol": "contact", "at": [300, 304],
              "reference": "S51", "as": "jig_b" }
        ] }"#,
    )
    .expect("write");
    apply(&touching);
    let failing = opencircuit(&["validate", path, "--deny-warnings"]);
    assert_eq!(code(&failing), 1);
    assert!(
        stdout(&failing).contains("OC-STR-018"),
        "{}",
        stdout(&failing)
    );

    let waive = directory.join("waive.json");
    fs::write(
        &waive,
        r#"{ "schema_version": 1, "operations": [
            { "op": "waive", "code": "OC-STR-018", "reason": "test jig, wired on site",
              "subject": { "symbol": { "device": "S50", "symbol": "contact" }, "terminal": "p2" } }
        ] }"#,
    )
    .expect("write");
    apply(&waive);
    let accepted = opencircuit(&["validate", path, "--deny-warnings"]);
    assert_eq!(code(&accepted), 0, "{}", stdout(&accepted));
    assert!(
        stdout(&accepted).ends_with("1 waived\n"),
        "{}",
        stdout(&accepted)
    );
    let problems = stdout(&opencircuit(&[
        "report", path, "--kind", "problems", "--format", "csv",
    ]));
    assert!(
        problems.contains(",yes,test jig, wired on site")
            || problems.contains(",yes,\"test jig, wired on site\""),
        "{problems}"
    );
}

#[test]
fn the_mcp_server_answers_over_a_pipe_and_stays_inside_its_root() {
    use std::io::{BufRead as _, BufReader, Write as _};
    use std::process::Stdio;

    let directory = scratch("mcp-pipe");
    let mut child = Command::new(env!("CARGO_BIN_EXE_opencircuit"))
        .args(["mcp", "--root", directory.to_str().expect("UTF-8 path")])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the server starts");
    let mut stdin = child.stdin.take().expect("stdin");
    let mut answers = BufReader::new(child.stdout.take().expect("stdout"));
    let mut ask = |request: serde_json::Value| -> serde_json::Value {
        writeln!(stdin, "{request}").expect("write");
        stdin.flush().expect("flush");
        let mut line = String::new();
        answers.read_line(&mut line).expect("an answer");
        serde_json::from_str(&line).expect("JSON")
    };

    let init = ask(
        serde_json::json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": { "protocolVersion": "2025-06-18" } }),
    );
    assert_eq!(init["result"]["serverInfo"]["name"], "opencircuit");

    let made = ask(
        serde_json::json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/call",
        "params": { "name": "new", "arguments": { "project": "p.ocircuit", "name": "Pipe" } } }),
    );
    assert_eq!(made["result"]["isError"], false);
    assert!(directory.join("p.ocircuit").is_file());

    let escaped = ask(
        serde_json::json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call",
        "params": { "name": "new", "arguments": { "project": "../out.ocircuit" } } }),
    );
    assert_eq!(escaped["result"]["isError"], true);
    assert!(
        !directory
            .parent()
            .expect("parent")
            .join("out.ocircuit")
            .exists()
    );

    // Closing standard input ends the server with success.
    drop(ask);
    drop(stdin);
    let status = child.wait().expect("exit");
    assert!(status.success());
}

#[test]
fn mcp_with_a_root_that_is_not_a_directory_is_a_usage_error() {
    let output = opencircuit(&["mcp", "--root", "no/such/directory"]);

    assert_eq!(code(&output), 2);
}
