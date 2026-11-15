//! Command-line parsing.
//!
//! Four commands with a handful of options do not justify a parser
//! dependency. Options may be written `--name value` or `--name=value`.

use std::path::PathBuf;

use circuit_export::Theme;
use circuit_sim::FaultPolicy;

/// How `validate` prints its findings.
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
    },
    Simulate {
        project: PathBuf,
        scenario: PathBuf,
        tick_us: Option<u32>,
        fault_policy: Option<FaultPolicy>,
        trace: Option<PathBuf>,
        snapshot_json: Option<PathBuf>,
    },
    Test {
        paths: Vec<PathBuf>,
        update_golden: bool,
    },
    Export {
        project: PathBuf,
        output: PathBuf,
        format: ExportFormat,
        theme: Theme,
    },
}

/// The file format of `export`, chosen by the extension of the output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExportFormat {
    Svg,
    Pdf,
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

pub(crate) fn parse(args: &[String]) -> Result<Command, String> {
    let Some((command, rest)) = args.split_first() else {
        return Err("missing command".to_owned());
    };
    match command.as_str() {
        "help" | "--help" | "-h" => Ok(Command::Help),
        "validate" => {
            let words = Words::split(rest, &["--format"], &["--deny-warnings"])?;
            let format = match words.value("--format") {
                None | Some("text") => OutputFormat::Text,
                Some("json") => OutputFormat::Json,
                Some(other) => {
                    return Err(format!("unknown format `{other}`; use `text` or `json`"));
                }
            };
            Ok(Command::Validate {
                project: words.single_positional("project file")?,
                format,
                deny_warnings: words.flag("--deny-warnings"),
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
                tick_us,
                fault_policy,
                trace: words.value("--trace").map(PathBuf::from),
                snapshot_json: words.value("--snapshot-json").map(PathBuf::from),
            })
        }
        "test" => {
            let words = Words::split(rest, &[], &["--update-golden"])?;
            if words.positional.is_empty() {
                return Err("`test` needs at least one path".to_owned());
            }
            Ok(Command::Test {
                paths: words.positional.iter().map(PathBuf::from).collect(),
                update_golden: words.flag("--update-golden"),
            })
        }
        "export" => {
            let words = Words::split(rest, &["--output", "--theme"], &[])?;
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
            Ok(Command::Export {
                project: words.single_positional("project file")?,
                output,
                format,
                theme,
            })
        }
        other => Err(format!("unknown command `{other}`")),
    }
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
                tick_us: Some(500),
                fault_policy: Some(FaultPolicy::Report),
                trace: Some(PathBuf::from("out.trace")),
                snapshot_json: Some(PathBuf::from("snap.json")),
            })
        );
    }

    #[test]
    fn test_collects_paths_and_the_update_flag() {
        assert_eq!(
            parse_words(&["test", "examples", "--update-golden", "more"]),
            Ok(Command::Test {
                paths: vec![PathBuf::from("examples"), PathBuf::from("more")],
                update_golden: true,
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
}
