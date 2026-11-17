//! Command-line parsing.
//!
//! Five commands with a handful of options do not justify a parser
//! dependency. Options may be written `--name value` or `--name=value`.

use std::path::PathBuf;

use circuit_export::Theme;
use circuit_sim::FaultPolicy;

/// How a command reports its result: for a person, or as one JSON document
/// for another program (`"format": 1`, see EXTENDING.md).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OutputFormat {
    Text,
    Json,
}

/// A parsed command line.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum Command {
    Help,
    Validate {
        project: PathBuf,
        format: OutputFormat,
        deny_warnings: bool,
        library: Option<PathBuf>,
    },
    Simulate {
        project: PathBuf,
        scenario: PathBuf,
        library: Option<PathBuf>,
        tick_us: Option<u32>,
        fault_policy: Option<FaultPolicy>,
        trace: Option<PathBuf>,
        snapshot_json: Option<PathBuf>,
        report: OutputFormat,
    },
    Test {
        paths: Vec<PathBuf>,
        update_golden: bool,
        report: OutputFormat,
    },
    Export {
        project: PathBuf,
        output: PathBuf,
        format: ExportFormat,
        theme: Theme,
        sheet: SheetChoice,
        library: Option<PathBuf>,
        report: OutputFormat,
    },
    New {
        file: PathBuf,
        name: Option<String>,
        force: bool,
        report: OutputFormat,
    },
    Edit {
        project: PathBuf,
        apply: PathBuf,
        output: Option<PathBuf>,
        dry_run: bool,
        library: Option<PathBuf>,
        report: OutputFormat,
    },
    Serve {
        project: PathBuf,
        listen: std::net::SocketAddr,
        library: Option<PathBuf>,
        print_map: bool,
        duration_ms: Option<u64>,
        report: OutputFormat,
    },
    Mcp {
        root: Option<PathBuf>,
    },
    Report {
        project: PathBuf,
        which: ReportChoice,
        /// What is written on standard output: the tables, or, with a file,
        /// the confirmation.
        format: TableFormat,
        output: Option<(PathBuf, FileFormat)>,
        library: Option<PathBuf>,
    },
}

/// Which reports `report` makes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReportChoice {
    One(circuit_report::Kind),
    All,
}

/// How `report` writes tables on standard output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TableFormat {
    Text,
    Json,
    Csv,
}

/// The file format of `report --output`, chosen by the extension.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FileFormat {
    Text,
    Json,
    Csv,
    Pdf,
}

/// Address `serve` listens on unless `--listen` says otherwise: loopback
/// only (ADR-0009).
pub(crate) const DEFAULT_LISTEN: &str = "127.0.0.1:5020";

/// The file format of `export`, chosen by the extension of the output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExportFormat {
    Svg,
    Pdf,
}

/// Which sheets `export` writes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SheetChoice {
    /// A PDF has one page per sheet, and a drawing without sheets one page; an
    /// SVG is the whole canvas.
    Default,
    /// Every sheet, one PDF page each.
    All,
    /// The sheet with this number.
    Number(u32),
}

/// Positional arguments and options of one command line.
#[derive(Debug, Default)]
struct Words<'a> {
    positional: Vec<&'a str>,
    options: Vec<(&'a str, Option<&'a str>)>,
}

impl<'a> Words<'a> {
    /// Split arguments into positionals and options. `valued` lists the
    /// options that take a value and `flags` those that do not.
    fn split(args: &'a [String], valued: &[&str], flags: &[&str]) -> Result<Self, String> {
        let mut words = Self::default();
        let mut rest = args.iter().map(String::as_str);
        let mut options_ended = false;
        while let Some(word) = rest.next() {
            if options_ended || !word.starts_with("--") {
                words.positional.push(word);
                continue;
            }
            if word == "--" {
                options_ended = true;
                continue;
            }
            let (name, inline) = match word.split_once('=') {
                Some((name, value)) => (name, Some(value)),
                None => (word, None),
            };
            if words.options.iter().any(|(seen, _)| *seen == name) {
                return Err(format!("option `{name}` is given more than once"));
            }
            if valued.contains(&name) {
                let value = match inline {
                    Some(value) => value,
                    None => rest
                        .next()
                        .ok_or_else(|| format!("option `{name}` needs a value"))?,
                };
                words.options.push((name, Some(value)));
            } else if flags.contains(&name) {
                if inline.is_some() {
                    return Err(format!("option `{name}` does not take a value"));
                }
                words.options.push((name, None));
            } else {
                return Err(format!("unknown option `{name}`"));
            }
        }
        Ok(words)
    }

    fn value(&self, name: &str) -> Option<&'a str> {
        self.options
            .iter()
            .find(|(option, _)| *option == name)
            .and_then(|(_, value)| *value)
    }

    fn flag(&self, name: &str) -> bool {
        self.options.iter().any(|(option, _)| *option == name)
    }

    fn single_positional(&self, what: &str) -> Result<PathBuf, String> {
        match self.positional.as_slice() {
            [only] => Ok(PathBuf::from(only)),
            [] => Err(format!("missing {what}")),
            _ => Err(format!("expected one {what}")),
        }
    }
}

/// The `--format` of a command: `text`, the default, or `json`.
fn report_format(words: &Words<'_>) -> Result<OutputFormat, String> {
    match words.value("--format") {
        None | Some("text") => Ok(OutputFormat::Text),
        Some("json") => Ok(OutputFormat::Json),
        Some(other) => Err(format!("unknown format `{other}`; use `text` or `json`")),
    }
}

pub(crate) fn parse(args: &[String]) -> Result<Command, String> {
    let Some((command, rest)) = args.split_first() else {
        return Err("missing command".to_owned());
    };
    match command.as_str() {
        "help" | "--help" | "-h" => Ok(Command::Help),
        "validate" => {
            let words = Words::split(rest, &["--format", "--library"], &["--deny-warnings"])?;
            let format = report_format(&words)?;
            Ok(Command::Validate {
                project: words.single_positional("project file")?,
                format,
                deny_warnings: words.flag("--deny-warnings"),
                library: words.value("--library").map(PathBuf::from),
            })
        }
        "simulate" => {
            let words = Words::split(
                rest,
                &[
                    "--scenario",
                    "--tick-us",
                    "--fault-policy",
                    "--trace",
                    "--snapshot-json",
                    "--format",
                    "--library",
                ],
                &[],
            )?;
            let tick_us = match words.value("--tick-us") {
                None => None,
                Some(text) => Some(
                    text.parse::<u32>()
                        .ok()
                        .filter(|tick_us| *tick_us > 0)
                        .ok_or_else(|| {
                            format!("`--tick-us` must be a positive integer, found `{text}`")
                        })?,
                ),
            };
            let fault_policy = match words.value("--fault-policy") {
                None => None,
                Some("halt") => Some(FaultPolicy::Halt),
                Some("report") => Some(FaultPolicy::Report),
                Some(other) => {
                    return Err(format!(
                        "unknown fault policy `{other}`; use `halt` or `report`"
                    ));
                }
            };
            Ok(Command::Simulate {
                project: words.single_positional("project file")?,
                scenario: words
                    .value("--scenario")
                    .map(PathBuf::from)
                    .ok_or("`simulate` needs `--scenario <file>`")?,
                library: words.value("--library").map(PathBuf::from),
                tick_us,
                fault_policy,
                trace: words.value("--trace").map(PathBuf::from),
                snapshot_json: words.value("--snapshot-json").map(PathBuf::from),
                report: report_format(&words)?,
            })
        }
        "test" => {
            let words = Words::split(rest, &["--format"], &["--update-golden"])?;
            if words.positional.is_empty() {
                return Err("`test` needs at least one path".to_owned());
            }
            Ok(Command::Test {
                paths: words.positional.iter().map(PathBuf::from).collect(),
                update_golden: words.flag("--update-golden"),
                report: report_format(&words)?,
            })
        }
        "export" => {
            let words = Words::split(
                rest,
                &["--output", "--theme", "--sheet", "--format", "--library"],
                &[],
            )?;
            let output = words
                .value("--output")
                .map(PathBuf::from)
                .ok_or("`export` needs `--output <file>`")?;
            let format = match output
                .extension()
                .and_then(|extension| extension.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref()
            {
                Some("svg") => ExportFormat::Svg,
                Some("pdf") => ExportFormat::Pdf,
                _ => {
                    return Err(format!(
                        "cannot tell the format of `{}`; the output must end in `.svg` or `.pdf`",
                        output.display()
                    ));
                }
            };
            let theme = match words.value("--theme") {
                None | Some("print") => Theme::Print,
                Some("live") => Theme::Live,
                Some(other) => {
                    return Err(format!("unknown theme `{other}`; use `print` or `live`"));
                }
            };
            let sheet = match words.value("--sheet") {
                None => SheetChoice::Default,
                Some("all") => SheetChoice::All,
                Some(number) => number
                    .parse::<u32>()
                    .ok()
                    .filter(|number| *number > 0)
                    .map(SheetChoice::Number)
                    .ok_or_else(|| {
                        format!("`--sheet` takes `all` or a sheet number from 1, not `{number}`")
                    })?,
            };
            if format == ExportFormat::Svg && sheet == SheetChoice::All {
                return Err(
                    "an SVG holds one page; give `--sheet <number>` or write a `.pdf` for every sheet"
                        .to_owned(),
                );
            }
            Ok(Command::Export {
                project: words.single_positional("project file")?,
                output,
                format,
                theme,
                sheet,
                library: words.value("--library").map(PathBuf::from),
                report: report_format(&words)?,
            })
        }
        "new" => {
            let words = Words::split(rest, &["--name", "--format"], &["--force"])?;
            Ok(Command::New {
                file: words.single_positional("project file")?,
                name: words.value("--name").map(str::to_owned),
                force: words.flag("--force"),
                report: report_format(&words)?,
            })
        }
        "edit" => {
            let words = Words::split(
                rest,
                &["--apply", "--output", "--library", "--format"],
                &["--dry-run"],
            )?;
            Ok(Command::Edit {
                project: words.single_positional("project file")?,
                apply: words
                    .value("--apply")
                    .map(PathBuf::from)
                    .ok_or("`edit` needs `--apply <edit file>`")?,
                output: words.value("--output").map(PathBuf::from),
                dry_run: words.flag("--dry-run"),
                library: words.value("--library").map(PathBuf::from),
                report: report_format(&words)?,
            })
        }
        "serve" => {
            let words = Words::split(
                rest,
                &["--listen", "--library", "--duration-ms", "--format"],
                &["--print-map"],
            )?;
            let report = report_format(&words)?;
            if report == OutputFormat::Json && !words.flag("--print-map") {
                return Err(
                    "`--format json` of `serve` goes with `--print-map`; the server itself only prints a log"
                        .to_owned(),
                );
            }
            let listen_text = words.value("--listen").unwrap_or(DEFAULT_LISTEN);
            let listen = listen_text.parse().map_err(|_| {
                format!("`--listen` must be an address and a port, found `{listen_text}`")
            })?;
            let duration_ms = match words.value("--duration-ms") {
                None => None,
                Some(text) => Some(text.parse::<u64>().map_err(|_| {
                    format!("`--duration-ms` must be a whole number, found `{text}`")
                })?),
            };
            Ok(Command::Serve {
                project: words.single_positional("project file")?,
                listen,
                library: words.value("--library").map(PathBuf::from),
                print_map: words.flag("--print-map"),
                duration_ms,
                report,
            })
        }
        "report" => parse_report(rest),
        "mcp" => {
            let words = Words::split(rest, &["--root"], &[])?;
            if !words.positional.is_empty() {
                return Err("`mcp` takes no file; give the directory with `--root`".to_owned());
            }
            Ok(Command::Mcp {
                root: words.value("--root").map(PathBuf::from),
            })
        }
        other => Err(format!("unknown command `{other}`")),
    }
}

/// `report <project> --kind <name>|all [--format text|json|csv]
/// [--output <file.csv|json|txt|pdf>] [--library ...]`.
fn parse_report(rest: &[String]) -> Result<Command, String> {
    let words = Words::split(rest, &["--kind", "--format", "--output", "--library"], &[])?;
    let names = circuit_report::Kind::ALL
        .map(circuit_report::Kind::name)
        .join("|");
    let which = match words.value("--kind") {
        None => return Err(format!("`report` needs `--kind {names}|all`")),
        Some("all") => ReportChoice::All,
        Some(name) => circuit_report::Kind::parse(name)
            .map(ReportChoice::One)
            .ok_or_else(|| format!("unknown report `{name}`; use {names} or all"))?,
    };
    let format = match words.value("--format") {
        None | Some("text") => TableFormat::Text,
        Some("json") => TableFormat::Json,
        Some("csv") => TableFormat::Csv,
        Some(other) => {
            return Err(format!(
                "unknown format `{other}`; use `text`, `json` or `csv`"
            ));
        }
    };
    let output = match words.value("--output").map(PathBuf::from) {
        None => None,
        Some(path) => {
            let file = match path
                .extension()
                .and_then(|extension| extension.to_str())
                .map(str::to_ascii_lowercase)
                .as_deref()
            {
                Some("csv") => FileFormat::Csv,
                Some("json") => FileFormat::Json,
                Some("txt") => FileFormat::Text,
                Some("pdf") => FileFormat::Pdf,
                _ => {
                    return Err(format!(
                        "cannot tell the format of `{}`; the output must end in `.csv`, `.json`, `.txt` or `.pdf`",
                        path.display()
                    ));
                }
            };
            Some((path, file))
        }
    };
    if output.is_some() && format == TableFormat::Csv {
        return Err(
            "`--format csv` writes the table on standard output; with `--output` the extension of the file picks the format and `--format` is `text` or `json`"
                .to_owned(),
        );
    }
    let csv = format == TableFormat::Csv
        || output
            .as_ref()
            .is_some_and(|(_, file)| *file == FileFormat::Csv);
    if csv && which == ReportChoice::All {
        return Err("a CSV holds one table; give `--kind <name>` instead of `all`".to_owned());
    }
    Ok(Command::Report {
        project: words.single_positional("project file")?,
        which,
        format,
        output,
        library: words.value("--library").map(PathBuf::from),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_words(words: &[&str]) -> Result<Command, String> {
        let args: Vec<String> = words.iter().map(|word| (*word).to_owned()).collect();
        parse(&args)
    }

    #[test]
    fn validate_defaults_to_text_without_denying_warnings() {
        assert_eq!(
            parse_words(&["validate", "a.ocircuit"]),
            Ok(Command::Validate {
                project: PathBuf::from("a.ocircuit"),
                format: OutputFormat::Text,
                deny_warnings: false,
                library: None,
            })
        );
    }

    #[test]
    fn option_value_may_be_separate_or_inline() {
        let separate = parse_words(&["validate", "--format", "json", "a.ocircuit"]);
        let inline = parse_words(&["validate", "a.ocircuit", "--format=json"]);

        assert_eq!(separate, inline);
        assert!(matches!(
            inline,
            Ok(Command::Validate {
                format: OutputFormat::Json,
                ..
            })
        ));
    }

    #[test]
    fn simulate_reads_every_option() {
        assert_eq!(
            parse_words(&[
                "simulate",
                "a.ocircuit",
                "--scenario",
                "s.json",
                "--tick-us=500",
                "--fault-policy",
                "report",
                "--trace",
                "out.trace",
                "--snapshot-json",
                "snap.json",
            ]),
            Ok(Command::Simulate {
                project: PathBuf::from("a.ocircuit"),
                scenario: PathBuf::from("s.json"),
                library: None,
                tick_us: Some(500),
                fault_policy: Some(FaultPolicy::Report),
                trace: Some(PathBuf::from("out.trace")),
                snapshot_json: Some(PathBuf::from("snap.json")),
                report: OutputFormat::Text,
            })
        );
    }

    #[test]
    fn new_and_edit_read_their_options() {
        assert_eq!(
            parse_words(&["new", "a.ocircuit", "--name", "Pump", "--force"]),
            Ok(Command::New {
                file: PathBuf::from("a.ocircuit"),
                name: Some("Pump".to_owned()),
                force: true,
                report: OutputFormat::Text,
            })
        );
        assert_eq!(
            parse_words(&["edit", "a.ocircuit", "--apply", "tx.json", "--dry-run"]),
            Ok(Command::Edit {
                project: PathBuf::from("a.ocircuit"),
                apply: PathBuf::from("tx.json"),
                output: None,
                dry_run: true,
                library: None,
                report: OutputFormat::Text,
            })
        );
        assert!(parse_words(&["edit", "a.ocircuit"]).is_err());
        assert!(parse_words(&["new"]).is_err());
        assert!(parse_words(&["new", "a", "b"]).is_err());
    }

    #[test]
    fn mcp_takes_only_a_root() {
        assert_eq!(parse_words(&["mcp"]), Ok(Command::Mcp { root: None }));
        assert_eq!(
            parse_words(&["mcp", "--root", "work"]),
            Ok(Command::Mcp {
                root: Some(PathBuf::from("work"))
            })
        );
        assert!(parse_words(&["mcp", "a.ocircuit"]).is_err());
    }

    #[test]
    fn report_needs_a_known_kind_and_one_table_for_csv() {
        assert!(parse_words(&["report", "a.ocircuit"]).is_err());
        assert!(parse_words(&["report", "a.ocircuit", "--kind", "nope"]).is_err());
        assert!(
            parse_words(&["report", "a.ocircuit", "--kind", "all", "--format", "csv"]).is_err()
        );
        assert!(
            parse_words(&["report", "a.ocircuit", "--kind", "all", "--output", "a.csv"]).is_err()
        );
        assert!(
            parse_words(&[
                "report",
                "a.ocircuit",
                "--kind",
                "bom",
                "--output",
                "a.csv",
                "--format",
                "csv"
            ])
            .is_err()
        );
        assert!(
            parse_words(&[
                "report",
                "a.ocircuit",
                "--kind",
                "bom",
                "--output",
                "a.xlsx"
            ])
            .is_err()
        );
    }

    #[test]
    fn report_reads_the_kind_the_format_and_the_file() {
        assert_eq!(
            parse_words(&["report", "a.ocircuit", "--kind", "wires", "--format", "csv"]),
            Ok(Command::Report {
                project: PathBuf::from("a.ocircuit"),
                which: ReportChoice::One(circuit_report::Kind::Wires),
                format: TableFormat::Csv,
                output: None,
                library: None,
            })
        );
        assert_eq!(
            parse_words(&[
                "report",
                "a.ocircuit",
                "--kind",
                "all",
                "--output",
                "out.json"
            ]),
            Ok(Command::Report {
                project: PathBuf::from("a.ocircuit"),
                which: ReportChoice::All,
                format: TableFormat::Text,
                output: Some((PathBuf::from("out.json"), FileFormat::Json)),
                library: None,
            })
        );
    }

    #[test]
    fn every_command_with_a_report_takes_format_json() {
        for words in [
            &[
                "simulate",
                "a.ocircuit",
                "--scenario",
                "s.json",
                "--format",
                "json",
            ][..],
            &["test", "examples", "--format", "json"],
            &[
                "export",
                "a.ocircuit",
                "--output",
                "a.pdf",
                "--format",
                "json",
            ],
            &["serve", "a.ocircuit", "--print-map", "--format", "json"],
        ] {
            let json = match parse_words(words).unwrap() {
                Command::Simulate { report, .. }
                | Command::Test { report, .. }
                | Command::Export { report, .. }
                | Command::Serve { report, .. } => report,
                other => panic!("{other:?}"),
            };
            assert_eq!(json, OutputFormat::Json, "{words:?}");
        }
    }

    #[test]
    fn a_format_other_than_text_or_json_is_refused_and_serve_needs_print_map_for_json() {
        assert!(parse_words(&["test", "examples", "--format", "xml"]).is_err());
        assert!(parse_words(&["serve", "a.ocircuit", "--format", "json"]).is_err());
        assert!(parse_words(&["serve", "a.ocircuit", "--format", "text"]).is_ok());
    }

    #[test]
    fn test_collects_paths_and_the_update_flag() {
        assert_eq!(
            parse_words(&["test", "examples", "--update-golden", "more"]),
            Ok(Command::Test {
                paths: vec![PathBuf::from("examples"), PathBuf::from("more")],
                update_golden: true,
                report: OutputFormat::Text,
            })
        );
    }

    #[test]
    fn double_dash_ends_the_options() {
        assert_eq!(
            parse_words(&["test", "--", "--update-golden"]),
            Ok(Command::Test {
                paths: vec![PathBuf::from("--update-golden")],
                update_golden: false,
                report: OutputFormat::Text,
            })
        );
    }

    #[test]
    fn help_is_recognised_in_every_spelling() {
        for word in ["help", "--help", "-h"] {
            assert_eq!(parse_words(&[word]), Ok(Command::Help));
        }
    }

    #[test]
    fn malformed_command_lines_are_usage_errors() {
        for words in [
            &[][..],
            &["frobnicate"],
            &["validate"],
            &["validate", "a", "b"],
            &["validate", "a", "--format", "yaml"],
            &["validate", "a", "--format"],
            &["validate", "a", "--deny-warnings=yes"],
            &["validate", "a", "--verbose"],
            &["validate", "a", "--format", "json", "--format", "text"],
            &["simulate", "a"],
            &["simulate", "a", "--scenario", "s", "--tick-us", "0"],
            &["simulate", "a", "--scenario", "s", "--tick-us", "-5"],
            &[
                "simulate",
                "a",
                "--scenario",
                "s",
                "--tick-us",
                "4294967296",
            ],
            &[
                "simulate",
                "a",
                "--scenario",
                "s",
                "--fault-policy",
                "explode",
            ],
            &["test"],
            &["test", "--update-golden"],
        ] {
            assert!(parse_words(words).is_err(), "{words:?} should be rejected");
        }
    }

    #[test]
    fn export_reads_the_format_from_the_extension_and_defaults_to_print() {
        assert_eq!(
            parse_words(&["export", "a.ocircuit", "--output", "out/a.SVG"]),
            Ok(Command::Export {
                project: PathBuf::from("a.ocircuit"),
                output: PathBuf::from("out/a.SVG"),
                format: ExportFormat::Svg,
                theme: Theme::Print,
                sheet: SheetChoice::Default,
                library: None,
                report: OutputFormat::Text,
            })
        );
        assert!(matches!(
            parse_words(&["export", "a.ocircuit", "--output", "a.pdf"]),
            Ok(Command::Export {
                format: ExportFormat::Pdf,
                ..
            })
        ));
        assert!(matches!(
            parse_words(&["export", "a.ocircuit", "--output=a.svg", "--theme", "live"]),
            Ok(Command::Export {
                theme: Theme::Live,
                ..
            })
        ));
    }

    #[test]
    fn export_refuses_an_unknown_format_or_theme_or_a_missing_output() {
        for words in [
            &["export", "a.ocircuit", "--output", "a.png"][..],
            &["export", "a.ocircuit", "--output", "a"],
            &[
                "export",
                "a.ocircuit",
                "--output",
                "a.svg",
                "--theme",
                "dark",
            ],
            &["export", "a.ocircuit"],
            &["export", "--output", "a.svg"],
        ] {
            assert!(parse_words(words).is_err(), "{words:?}");
        }
    }

    #[test]
    fn serve_listens_on_loopback_unless_told_otherwise() {
        assert_eq!(
            parse_words(&["serve", "a.ocircuit"]),
            Ok(Command::Serve {
                project: PathBuf::from("a.ocircuit"),
                listen: DEFAULT_LISTEN.parse().unwrap(),
                library: None,
                print_map: false,
                duration_ms: None,
                report: OutputFormat::Text,
            })
        );
        assert!(matches!(
            parse_words(&[
                "serve",
                "a.ocircuit",
                "--listen",
                "0.0.0.0:502",
                "--print-map"
            ]),
            Ok(Command::Serve {
                print_map: true,
                ..
            })
        ));
        for words in [
            &["serve", "a.ocircuit", "--listen", "localhost"][..],
            &["serve", "a.ocircuit", "--duration-ms", "-1"],
            &["serve"],
        ] {
            assert!(parse_words(words).is_err(), "{words:?}");
        }
    }
}
