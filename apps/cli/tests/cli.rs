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
    assert!(stdout(&output).ends_with("23 scenario(s), 0 failed\n"));
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

    use super::{COMMAND, opencircuit, root, stdout};

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
        let mut child = Command::new(env!("CARGO_BIN_EXE_opencircuit"))
            .args([
                "serve",
                project,
                "--listen",
                "127.0.0.1:0",
                "--duration-ms",
                "20000",
            ])
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
        let answer = ask(stream, 9, &[2, 0, 0, 0, 1]);
        assert_eq!(&answer[..2], &[2, 1]);
        answer[2] & 1 == 1
    }

    fn eventually(stream: &mut TcpStream, expected: bool) {
        for _ in 0..200 {
            if k1_active(stream) == expected {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("K1 never became {expected}");
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
}
