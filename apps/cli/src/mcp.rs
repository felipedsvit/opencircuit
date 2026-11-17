//! `opencircuit mcp`: a Model Context Protocol server on standard input and
//! output (issue #104).
//!
//! Messages are JSON-RPC 2.0, one per line. The server answers `initialize`,
//! `ping`, `tools/list` and `tools/call` and ignores notifications. Every tool
//! runs the command of the same name and returns its JSON document
//! (`format: 1`, see `EXTENDING.md`) as text, so an agent reads what a
//! program would.
//!
//! Safety: every path an agent gives is relative to the root directory and
//! must stay inside it, also through symbolic links; a message is at most
//! [`MAX_MESSAGE_BYTES`] and a result at most [`MAX_RESULT_BYTES`]; nothing
//! is executed and nothing is written outside the root. A message is hostile
//! input: nothing in this module panics.

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Component, Path, PathBuf};

use serde_json::{Map, Value, json};

use crate::{Env, run};

/// Largest message accepted, in bytes.
pub(crate) const MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;
/// Largest result returned, in bytes.
pub(crate) const MAX_RESULT_BYTES: usize = 4 * 1024 * 1024;
/// Protocol versions this server speaks, newest first.
const PROTOCOLS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// What reading one line gave.
enum Line {
    End,
    TooLong,
    Text(Vec<u8>),
}

/// Read up to the next line feed, keeping at most [`MAX_MESSAGE_BYTES`]; a
/// longer line is read to its end and reported as too long.
fn read_line(input: &mut dyn BufRead) -> io::Result<Line> {
    let mut line: Vec<u8> = Vec::new();
    let mut too_long = false;
    let mut seen = false;
    loop {
        let available = input.fill_buf()?;
        if available.is_empty() {
            return Ok(match (seen, too_long) {
                (false, _) => Line::End,
                (true, true) => Line::TooLong,
                (true, false) => Line::Text(line),
            });
        }
        seen = true;
        let (chunk, found) = match available.iter().position(|byte| *byte == b'\n') {
            Some(at) => (available.get(..at).unwrap_or_default(), true),
            None => (available, false),
        };
        let consumed = chunk.len().saturating_add(usize::from(found));
        if !too_long {
            if line.len().saturating_add(chunk.len()) > MAX_MESSAGE_BYTES {
                too_long = true;
                line.clear();
            } else {
                line.extend_from_slice(chunk);
            }
        }
        input.consume(consumed);
        if found {
            return Ok(if too_long {
                Line::TooLong
            } else {
                Line::Text(line)
            });
        }
    }
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn result(id: Value, value: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": value })
}

/// Serve requests from `input` until it ends, writing answers to `output`.
///
/// # Errors
///
/// Returns the error of reading `input` or writing `output`.
pub(crate) fn serve(
    root: &Path,
    env: &Env,
    input: &mut dyn BufRead,
    output: &mut dyn Write,
) -> io::Result<()> {
    loop {
        let answer = match read_line(input)? {
            Line::End => return Ok(()),
            Line::TooLong => Some(error(
                Value::Null,
                INVALID_REQUEST,
                "the message is larger than the limit",
            )),
            Line::Text(bytes) if bytes.iter().all(u8::is_ascii_whitespace) => None,
            Line::Text(bytes) => handle(root, env, &bytes),
        };
        if let Some(answer) = answer {
            writeln!(output, "{answer}")?;
            output.flush()?;
        }
    }
}

/// The answer to one message, or `None` for a notification.
fn handle(root: &Path, env: &Env, bytes: &[u8]) -> Option<Value> {
    let Ok(message) = serde_json::from_slice::<Value>(bytes) else {
        return Some(error(Value::Null, PARSE_ERROR, "the message is not JSON"));
    };
    let Some(object) = message.as_object() else {
        let what = if message.is_array() {
            "batches are not supported"
        } else {
            "a message is a JSON object"
        };
        return Some(error(Value::Null, INVALID_REQUEST, what));
    };
    let id = match object.get("id") {
        None => return None,
        Some(id) if id.is_string() || id.is_number() => id.clone(),
        Some(_) => {
            return Some(error(
                Value::Null,
                INVALID_REQUEST,
                "an id is a string or a number",
            ));
        }
    };
    let Some(method) = object.get("method").and_then(Value::as_str) else {
        return Some(error(id, INVALID_REQUEST, "a request has a method"));
    };
    let params = object.get("params").and_then(Value::as_object);
    Some(match method {
        "initialize" => result(id, initialize(params)),
        "ping" => result(id, json!({})),
        "tools/list" => result(id, json!({ "tools": tools() })),
        "tools/call" => match call(root, env, params) {
            Ok(value) => result(id, value),
            Err(message) => error(id, INVALID_PARAMS, &message),
        },
        _ => error(id, METHOD_NOT_FOUND, "unknown method"),
    })
}

fn initialize(params: Option<&Map<String, Value>>) -> Value {
    let asked = params
        .and_then(|params| params.get("protocolVersion"))
        .and_then(Value::as_str);
    let version = PROTOCOLS
        .iter()
        .find(|known| Some(**known) == asked)
        .or(PROTOCOLS.first())
        .copied()
        .unwrap_or("2025-06-18");
    json!({
        "protocolVersion": version,
        "capabilities": { "tools": {} },
        "serverInfo": { "name": "opencircuit", "version": env!("CARGO_PKG_VERSION") },
        "instructions": "Paths are relative to the root directory the server was started with. Every tool returns the JSON document of the command of the same name."
    })
}

fn string_property(description: &str) -> Value {
    json!({ "type": "string", "description": description })
}

fn tools() -> Value {
    let project = string_property("The .ocircuit file, relative to the root.");
    let library = string_property("An extension library (.oclib), relative to the root.");
    json!([
        {
            "name": "new",
            "description": "Create an empty project.",
            "inputSchema": { "type": "object", "required": ["project"], "properties": {
                "project": project,
                "name": string_property("The project name."),
                "force": { "type": "boolean", "description": "Replace a file that exists." }
            } }
        },
        {
            "name": "validate",
            "description": "Compile a project and list its diagnostics.",
            "inputSchema": { "type": "object", "required": ["project"], "properties": {
                "project": project,
                "library": library,
                "deny_warnings": { "type": "boolean", "description": "Fail on a warning that the project does not waive." }
            } }
        },
        {
            "name": "simulate",
            "description": "Run a scenario file on a project and return the trace and the last snapshot.",
            "inputSchema": { "type": "object", "required": ["project", "scenario"], "properties": {
                "project": project,
                "scenario": string_property("The scenario file, relative to the root."),
                "library": library,
                "tick_us": { "type": "integer", "description": "Tick duration in microseconds." },
                "fault_policy": { "type": "string", "enum": ["halt", "report"] }
            } }
        },
        {
            "name": "edit",
            "description": "Apply edit-file operations to a project, all or nothing (see EXTENDING.md for the operations).",
            "inputSchema": { "type": "object", "required": ["project", "operations"], "properties": {
                "project": project,
                "operations": { "type": "array", "items": { "type": "object" }, "description": "The operations of an edit file." },
                "dry_run": { "type": "boolean", "description": "Check and write nothing." },
                "library": library
            } }
        },
        {
            "name": "report",
            "description": "Derive the reports of a project: bom, wires, terminals, xref, io, problems or all.",
            "inputSchema": { "type": "object", "required": ["project", "kind"], "properties": {
                "project": project,
                "kind": string_property("bom, wires, terminals, xref, io, problems or all."),
                "library": library
            } }
        },
        {
            "name": "export",
            "description": "Draw a project to an .svg or .pdf file inside the root.",
            "inputSchema": { "type": "object", "required": ["project", "output"], "properties": {
                "project": project,
                "output": string_property("The .svg or .pdf file to write, relative to the root."),
                "theme": { "type": "string", "enum": ["print", "live"] },
                "sheet": string_property("A sheet number, or all."),
                "library": library
            } }
        }
    ])
}

/// The path an agent gave, joined to the root, when it stays inside it.
fn confine(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if relative.is_empty() || relative.contains('\0') || path.is_absolute() {
        return Err(format!(
            "`{relative}` is not a path relative to the root directory"
        ));
    }
    let plain = path
        .components()
        .all(|part| matches!(part, Component::Normal(_) | Component::CurDir));
    if !plain {
        return Err(format!("`{relative}` leaves the root directory"));
    }
    let joined = root.join(path);
    // A link may point out of the root: look at where the longest part that
    // exists really is.
    let mut probe = joined.clone();
    while fs::symlink_metadata(&probe).is_err() {
        if !probe.pop() {
            break;
        }
    }
    let real = fs::canonicalize(&probe)
        .map_err(|error| format!("`{relative}` cannot be resolved: {error}"))?;
    if !real.starts_with(root) {
        return Err(format!("`{relative}` leaves the root directory"));
    }
    Ok(joined)
}

/// What a tool needs to run: the arguments of the command and a file to remove
/// afterwards.
struct Prepared {
    args: Vec<String>,
    cleanup: Option<PathBuf>,
}

fn text<'a>(arguments: &'a Map<String, Value>, key: &str) -> Result<Option<&'a str>, String> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(text)) => Ok(Some(text)),
        Some(_) => Err(format!("`{key}` is a string")),
    }
}

fn required<'a>(arguments: &'a Map<String, Value>, key: &str) -> Result<&'a str, String> {
    text(arguments, key)?.ok_or_else(|| format!("`{key}` is required"))
}

fn flag(arguments: &Map<String, Value>, key: &str) -> Result<bool, String> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(value)) => Ok(*value),
        Some(_) => Err(format!("`{key}` is true or false")),
    }
}

fn number(arguments: &Map<String, Value>, key: &str) -> Result<Option<u64>, String> {
    match arguments.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .map(Some)
            .ok_or_else(|| format!("`{key}` is a whole number")),
    }
}

fn path_of(root: &Path, arguments: &Map<String, Value>, key: &str) -> Result<String, String> {
    Ok(confine(root, required(arguments, key)?)?
        .to_string_lossy()
        .into_owned())
}

fn prepare(root: &Path, name: &str, arguments: &Map<String, Value>) -> Result<Prepared, String> {
    let mut args: Vec<String> = vec![name.to_owned()];
    let mut cleanup = None;
    let library = |args: &mut Vec<String>| -> Result<(), String> {
        if let Some(library) = text(arguments, "library")? {
            args.push("--library".to_owned());
            args.push(confine(root, library)?.to_string_lossy().into_owned());
        }
        Ok(())
    };
    match name {
        "new" => {
            args.push(path_of(root, arguments, "project")?);
            if let Some(project_name) = text(arguments, "name")? {
                args.push("--name".to_owned());
                args.push(project_name.to_owned());
            }
            if flag(arguments, "force")? {
                args.push("--force".to_owned());
            }
        }
        "validate" => {
            args.push(path_of(root, arguments, "project")?);
            if flag(arguments, "deny_warnings")? {
                args.push("--deny-warnings".to_owned());
            }
            library(&mut args)?;
        }
        "simulate" => {
            args.push(path_of(root, arguments, "project")?);
            args.push("--scenario".to_owned());
            args.push(path_of(root, arguments, "scenario")?);
            if let Some(tick) = number(arguments, "tick_us")? {
                args.push("--tick-us".to_owned());
                args.push(tick.to_string());
            }
            if let Some(policy) = text(arguments, "fault_policy")? {
                args.push("--fault-policy".to_owned());
                args.push(policy.to_owned());
            }
            library(&mut args)?;
        }
        "edit" => {
            args.push(path_of(root, arguments, "project")?);
            let operations = arguments
                .get("operations")
                .and_then(Value::as_array)
                .ok_or("`operations` is a list of edit-file operations")?;
            let file = json!({ "schema_version": 1, "operations": operations });
            let bytes = serde_json::to_vec(&file).map_err(|error| error.to_string())?;
            // The operations go to a file of their own, next to the root's
            // files and removed afterwards.
            let temporary = root.join(format!(".mcp-edit-{}.json", std::process::id()));
            fs::write(&temporary, bytes)
                .map_err(|error| format!("cannot write the edit file: {error}"))?;
            args.push("--apply".to_owned());
            args.push(temporary.to_string_lossy().into_owned());
            cleanup = Some(temporary);
            if flag(arguments, "dry_run")? {
                args.push("--dry-run".to_owned());
            }
            library(&mut args)?;
        }
        "report" => {
            args.push(path_of(root, arguments, "project")?);
            args.push("--kind".to_owned());
            args.push(required(arguments, "kind")?.to_owned());
            library(&mut args)?;
        }
        "export" => {
            args.push(path_of(root, arguments, "project")?);
            args.push("--output".to_owned());
            args.push(path_of(root, arguments, "output")?);
            if let Some(theme) = text(arguments, "theme")? {
                args.push("--theme".to_owned());
                args.push(theme.to_owned());
            }
            if let Some(sheet) = text(arguments, "sheet")? {
                args.push("--sheet".to_owned());
                args.push(sheet.to_owned());
            }
            library(&mut args)?;
        }
        other => return Err(format!("unknown tool `{other}`")),
    }
    args.push("--format".to_owned());
    args.push("json".to_owned());
    Ok(Prepared { args, cleanup })
}

/// Run a tool and wrap what the command wrote as the result of the call.
fn call(root: &Path, env: &Env, params: Option<&Map<String, Value>>) -> Result<Value, String> {
    let params = params.ok_or("`tools/call` needs params")?;
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or("`name` is the name of a tool")?;
    let empty = Map::new();
    let arguments = match params.get("arguments") {
        None | Some(Value::Null) => &empty,
        Some(Value::Object(arguments)) => arguments,
        Some(_) => return Err("`arguments` is an object".to_owned()),
    };
    let prepared = match prepare(root, name, arguments) {
        Ok(prepared) => prepared,
        // A tool that cannot run says why as its own result, so that the
        // agent can read it and try again; an unknown tool is a protocol error.
        Err(message) if message.starts_with("unknown tool") => return Err(message),
        Err(message) => return Ok(tool_result(&message, true)),
    };
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let code = run(&prepared.args, env, &mut out, &mut err);
    if let Some(file) = &prepared.cleanup {
        let _ = fs::remove_file(file);
    }
    let mut written = String::from_utf8_lossy(&out).into_owned();
    if written.trim().is_empty() {
        written = String::from_utf8_lossy(&err).into_owned();
    }
    if written.len() > MAX_RESULT_BYTES {
        return Ok(tool_result(
            "the result is larger than the limit; ask for less (one report, one sheet)",
            true,
        ));
    }
    Ok(tool_result(written.trim_end(), code != 0))
}

fn tool_result(text: &str, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

/// Serve on the process's standard input and the given output.
///
/// # Errors
///
/// Returns an error when the root is not a directory or when reading or
/// writing fails.
pub(crate) fn serve_stdio(root: &Path, env: &Env, output: &mut dyn Write) -> Result<(), String> {
    let root = fs::canonicalize(root).map_err(|error| format!("{}: {error}", root.display()))?;
    if !root.is_dir() {
        return Err(format!("{} is not a directory", root.display()));
    }
    let stdin = io::stdin();
    let mut input = stdin.lock();
    serve(&root, env, &mut input, output).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("opencircuit-mcp-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("scratch");
        fs::canonicalize(&directory).expect("canonical")
    }

    /// Send `lines` to a server rooted at `root` and read its answers.
    fn exchange(root: &Path, lines: &[&str]) -> Vec<Value> {
        let input = lines.join("\n") + "\n";
        let mut output: Vec<u8> = Vec::new();
        serve(
            root,
            &Env::default(),
            &mut Cursor::new(input.into_bytes()),
            &mut output,
        )
        .expect("served");
        String::from_utf8(output)
            .expect("UTF-8")
            .lines()
            .map(|line| serde_json::from_str(line).expect("each answer is JSON"))
            .collect()
    }

    fn call_tool(root: &Path, name: &str, arguments: &Value) -> Value {
        let request = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call",
            "params": { "name": name, "arguments": arguments } });
        exchange(root, &[&request.to_string()])
            .into_iter()
            .next()
            .expect("an answer")
    }

    fn text_of(answer: &Value) -> String {
        answer["result"]["content"][0]["text"]
            .as_str()
            .expect("text content")
            .to_owned()
    }

    #[test]
    fn initialize_names_the_server_and_agrees_on_a_known_protocol_version() {
        let root = scratch("init");

        let answers = exchange(
            &root,
            &[
                r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#,
                r#"{"jsonrpc":"2.0","id":2,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}"#,
            ],
        );

        assert_eq!(answers[0]["result"]["protocolVersion"], "2024-11-05");
        assert_eq!(answers[0]["result"]["serverInfo"]["name"], "opencircuit");
        assert!(answers[0]["result"]["capabilities"]["tools"].is_object());
        // An unknown version gets the newest one the server speaks.
        assert_eq!(answers[1]["result"]["protocolVersion"], "2025-06-18");
    }

    #[test]
    fn a_notification_is_not_answered_and_ping_is() {
        let root = scratch("ping");

        let answers = exchange(
            &root,
            &[
                r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
                r#"{"jsonrpc":"2.0","id":"a","method":"ping"}"#,
            ],
        );

        assert_eq!(answers.len(), 1);
        assert_eq!(answers[0]["id"], "a");
        assert_eq!(answers[0]["result"], json!({}));
    }

    #[test]
    fn the_tools_are_listed_with_a_schema_each() {
        let root = scratch("list");

        let answers = exchange(
            &root,
            &[r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#],
        );

        let names: Vec<&str> = answers[0]["result"]["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .map(|tool| {
                assert_eq!(tool["inputSchema"]["type"], "object");
                tool["name"].as_str().expect("name")
            })
            .collect();
        assert_eq!(
            names,
            ["new", "validate", "simulate", "edit", "report", "export"]
        );
    }

    #[test]
    fn bad_messages_get_a_protocol_error_and_the_server_goes_on() {
        let root = scratch("bad");

        let answers = exchange(
            &root,
            &[
                "not json",
                "[1, 2]",
                "42",
                r#"{"jsonrpc":"2.0","id":1}"#,
                r#"{"jsonrpc":"2.0","id":{},"method":"ping"}"#,
                r#"{"jsonrpc":"2.0","id":2,"method":"no/such"}"#,
                r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"nope"}}"#,
                r#"{"jsonrpc":"2.0","id":4,"method":"tools/call"}"#,
                "",
                r#"{"jsonrpc":"2.0","id":5,"method":"ping"}"#,
            ],
        );

        let codes: Vec<i64> = answers
            .iter()
            .map(|answer| answer["error"]["code"].as_i64().unwrap_or(0))
            .collect();
        assert_eq!(
            codes,
            [
                -32700, -32600, -32600, -32600, -32600, -32601, -32602, -32602, 0
            ]
        );
        // The last request still got its answer.
        assert_eq!(answers.last().expect("an answer")["id"], 5);
    }

    #[test]
    fn a_message_over_the_limit_is_refused_and_the_next_one_is_read() {
        let root = scratch("big");
        let huge = "x".repeat(MAX_MESSAGE_BYTES + 10);
        let input = format!(
            "{huge}\n{}\n",
            r#"{"jsonrpc":"2.0","id":9,"method":"ping"}"#
        );
        let mut output: Vec<u8> = Vec::new();

        serve(
            &root,
            &Env::default(),
            &mut Cursor::new(input.into_bytes()),
            &mut output,
        )
        .expect("served");

        let answers: Vec<Value> = String::from_utf8(output)
            .expect("UTF-8")
            .lines()
            .map(|line| serde_json::from_str(line).expect("JSON"))
            .collect();
        assert_eq!(answers[0]["error"]["code"], INVALID_REQUEST);
        assert_eq!(answers[1]["id"], 9);
    }

    #[test]
    fn paths_that_leave_the_root_are_refused() {
        let root = scratch("confine");
        let outside = scratch("confine-outside");
        fs::write(outside.join("secret.ocircuit"), "{}").expect("file");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, root.join("link")).expect("symlink");

        let mut refused = vec![
            "/etc/passwd".to_owned(),
            "../secret.ocircuit".to_owned(),
            "a/../../x".to_owned(),
            String::new(),
            "bad\0name".to_owned(),
        ];
        #[cfg(unix)]
        refused.push("link/secret.ocircuit".to_owned());
        for path in refused {
            let answer = call_tool(&root, "validate", &json!({ "project": path }));
            assert_eq!(answer["result"]["isError"], true, "{path:?}");
            assert!(
                text_of(&answer).contains("root directory"),
                "{path:?}: {}",
                text_of(&answer)
            );
        }
        // A path inside the root is not refused for its place.
        assert!(confine(&root, "inside/project.ocircuit").is_ok());
        assert!(confine(&root, "./project.ocircuit").is_ok());
    }

    #[test]
    fn a_tool_that_is_missing_an_argument_says_so_as_its_result() {
        let root = scratch("missing");

        let answer = call_tool(&root, "validate", &json!({}));

        assert_eq!(answer["result"]["isError"], true);
        assert!(text_of(&answer).contains("`project` is required"));
        let wrong = call_tool(&root, "validate", &json!({ "project": 3 }));
        assert!(text_of(&wrong).contains("is a string"));
    }

    #[test]
    fn a_project_is_made_edited_validated_reported_and_exported_through_the_tools() {
        let root = scratch("flow");

        let made = call_tool(
            &root,
            "new",
            &json!({ "project": "demo.ocircuit", "name": "Demo" }),
        );
        assert_eq!(made["result"]["isError"], false, "{}", text_of(&made));

        let edited = call_tool(
            &root,
            "edit",
            &json!({ "project": "demo.ocircuit", "operations": [
                { "op": "place", "model": "pushbutton_no", "symbol": "contact",
                  "at": [0, 0], "reference": "S1" },
                { "op": "place", "model": "contactor.generic", "symbol": "coil",
                  "at": [10, 0], "reference": "K1" }
            ] }),
        );
        assert_eq!(edited["result"]["isError"], false, "{}", text_of(&edited));
        let document: Value = serde_json::from_str(&text_of(&edited)).expect("JSON document");
        assert_eq!(document["format"], 1);
        assert_eq!(document["operations"], 2);
        // The temporary edit file is gone.
        assert!(
            !root
                .join(format!(".mcp-edit-{}.json", std::process::id()))
                .exists()
        );

        let validated = call_tool(&root, "validate", &json!({ "project": "demo.ocircuit" }));
        let document: Value = serde_json::from_str(&text_of(&validated)).expect("JSON");
        assert_eq!(document["command"], "validate");

        let report = call_tool(
            &root,
            "report",
            &json!({ "project": "demo.ocircuit", "kind": "bom" }),
        );
        let document: Value = serde_json::from_str(&text_of(&report)).expect("JSON");
        assert_eq!(document["reports"][0]["kind"], "bom");
        assert_eq!(
            document["reports"][0]["rows"].as_array().map(Vec::len),
            Some(2)
        );

        let exported = call_tool(
            &root,
            "export",
            &json!({ "project": "demo.ocircuit", "output": "demo.svg" }),
        );
        assert_eq!(
            exported["result"]["isError"],
            false,
            "{}",
            text_of(&exported)
        );
        assert!(
            fs::read_to_string(root.join("demo.svg"))
                .expect("drawn")
                .contains("<svg")
        );
    }

    #[test]
    fn a_refused_edit_is_an_error_result_that_leaves_the_project_alone() {
        let root = scratch("refused");
        call_tool(&root, "new", &json!({ "project": "demo.ocircuit" }));
        let before = fs::read_to_string(root.join("demo.ocircuit")).expect("project");

        let answer = call_tool(
            &root,
            "edit",
            &json!({ "project": "demo.ocircuit", "operations": [
                { "op": "set_reference", "device": "NOPE", "to": "K1" }
            ] }),
        );

        assert_eq!(answer["result"]["isError"], true);
        assert!(text_of(&answer).contains("operation 1"));
        assert_eq!(
            fs::read_to_string(root.join("demo.ocircuit")).expect("project"),
            before
        );
    }

    #[test]
    fn nothing_is_written_outside_the_root_by_an_export() {
        let root = scratch("export-out");
        call_tool(&root, "new", &json!({ "project": "demo.ocircuit" }));

        let answer = call_tool(
            &root,
            "export",
            &json!({ "project": "demo.ocircuit", "output": "../escaped.svg" }),
        );

        assert_eq!(answer["result"]["isError"], true);
        assert!(!root.parent().expect("parent").join("escaped.svg").exists());
    }
}
