//! Scenario files: a circuit, a configuration, timed inputs and what to
//! expect.
//!
//! Devices are named by reference designator; the tool resolves them against
//! the compiled circuit. Unknown fields are rejected.

use std::path::{Component, Path};

use serde::Deserialize;

/// Newest scenario schema version this build reads. Version 1 is still
/// read; version 2 adds `libraries`.
pub const SCENARIO_SCHEMA_VERSION: u32 = 2;
/// Most libraries one scenario may load.
pub const MAX_SCENARIO_LIBRARIES: usize = 64;
/// Largest scenario text, in bytes.
pub const MAX_SCENARIO_BYTES: usize = 4 * 1024 * 1024;
/// Last tick a scenario may ask for. Bounds the run time of one scenario.
pub const MAX_UNTIL_TICK: u64 = 10_000_000;
/// Most events in one scenario.
pub const MAX_EVENTS: usize = 100_000;

/// A scenario file.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Scenario {
    /// 1 or [`SCENARIO_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Project file, relative to the scenario file.
    pub circuit: String,
    /// Extension library files the project needs, relative to the scenario
    /// file, added to the catalog in this order (schema version 2).
    #[serde(default)]
    pub libraries: Vec<String>,
    /// Simulator configuration; every field is optional.
    #[serde(default)]
    pub config: ScenarioConfig,
    /// Inputs, each applied at its tick.
    #[serde(default)]
    pub events: Vec<ScenarioEvent>,
    /// The last tick to execute, inclusive.
    pub until_tick: u64,
    /// What the run must produce.
    #[serde(default)]
    pub expect: Expectation,
}

/// Simulator configuration of a scenario. Absent fields use the defaults.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioConfig {
    /// Tick duration in microseconds.
    pub tick_us: Option<u32>,
    /// Pickup delay in ticks.
    pub pickup_ticks: Option<u32>,
    /// Dropout delay in ticks.
    pub dropout_ticks: Option<u32>,
    /// Fault policy.
    pub fault_policy: Option<PolicyName>,
}

/// Fault policy as written in a scenario.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PolicyName {
    /// Latch on the first latching fault.
    Halt,
    /// Report faults and keep running.
    Report,
}

/// One timed input.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ScenarioEvent {
    /// Tick in which the action takes effect.
    pub tick: u64,
    /// What to do.
    pub action: ActionName,
    /// Reference designator of the device.
    pub device: String,
}

/// Action as written in a scenario.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ActionName {
    /// Press a pushbutton.
    Press,
    /// Release a pushbutton.
    Release,
    /// Close a latching switch.
    Close,
    /// Open a latching switch.
    Open,
    /// Trip a thermal overload relay.
    Trip,
    /// Reset a thermal overload relay.
    ResetTrip,
}

/// What a scenario expects.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Expectation {
    /// Golden trace file, relative to the scenario file.
    pub trace: Option<String>,
    /// Codes of the diagnostics compilation must report, in any order.
    #[serde(default)]
    pub diagnostics: Vec<String>,
}

impl Scenario {
    /// Parse and check a scenario.
    ///
    /// # Errors
    ///
    /// Returns a message when the text is too large, is not a scenario of
    /// the supported version, or exceeds the tick or event limits.
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > MAX_SCENARIO_BYTES {
            return Err(format!(
                "the scenario is larger than {MAX_SCENARIO_BYTES} bytes"
            ));
        }
        let scenario: Self = serde_json::from_str(text).map_err(|error| error.to_string())?;
        if !(1..=SCENARIO_SCHEMA_VERSION).contains(&scenario.schema_version) {
            return Err(format!(
                "scenario schema version {} is not supported; this build reads versions 1 to {SCENARIO_SCHEMA_VERSION}",
                scenario.schema_version
            ));
        }
        if scenario.schema_version < 2 && !scenario.libraries.is_empty() {
            return Err("`libraries` needs scenario schema version 2".to_owned());
        }
        if scenario.libraries.len() > MAX_SCENARIO_LIBRARIES {
            return Err(format!("more than {MAX_SCENARIO_LIBRARIES} libraries"));
        }
        if scenario.until_tick > MAX_UNTIL_TICK {
            return Err(format!(
                "`until_tick` {} exceeds the limit of {MAX_UNTIL_TICK}",
                scenario.until_tick
            ));
        }
        if scenario.events.len() > MAX_EVENTS {
            return Err(format!("more than {MAX_EVENTS} events"));
        }
        // A scenario names the files it reads and, when goldens are updated,
        // the file it writes. Keep both inside the scenario's own directory.
        let libraries = scenario
            .libraries
            .iter()
            .map(|path| ("libraries", Some(path.as_str())));
        for (field, path) in [
            ("circuit", Some(scenario.circuit.as_str())),
            ("expect.trace", scenario.expect.trace.as_deref()),
        ]
        .into_iter()
        .chain(libraries)
        {
            if path.is_some_and(|path| !is_plain_relative(path)) {
                return Err(format!(
                    "`{field}` must be a relative path without `..` components"
                ));
            }
        }
        if let Some(late) = scenario
            .events
            .iter()
            .find(|event| event.tick > scenario.until_tick)
        {
            return Err(format!(
                "event at tick {} is after `until_tick` {}",
                late.tick, scenario.until_tick
            ));
        }
        Ok(scenario)
    }
}

/// Whether a path is relative and made of plain names only.
fn is_plain_relative(path: &str) -> bool {
    let mut components = Path::new(path).components().peekable();
    components.peek().is_some()
        && components.all(|component| matches!(component, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = r#"{ "schema_version": 1, "circuit": "a.ocircuit", "until_tick": 5 }"#;

    #[test]
    fn minimal_scenario_uses_defaults() {
        let scenario = Scenario::parse(MINIMAL).unwrap();

        assert_eq!(scenario.config, ScenarioConfig::default());
        assert_eq!(scenario.events, []);
        assert_eq!(scenario.expect, Expectation::default());
    }

    #[test]
    fn full_scenario_is_read() {
        let scenario = Scenario::parse(
            r#"{
              "schema_version": 1,
              "circuit": "command.ocircuit",
              "config": { "tick_us": 1000, "pickup_ticks": 1, "dropout_ticks": 2, "fault_policy": "report" },
              "events": [
                { "tick": 2, "action": "press", "device": "S1" },
                { "tick": 3, "action": "reset_trip", "device": "F1" }
              ],
              "until_tick": 20,
              "expect": { "trace": "command.trace", "diagnostics": ["OC-SEM-008"] }
            }"#,
        )
        .unwrap();

        assert_eq!(scenario.config.fault_policy, Some(PolicyName::Report));
        assert_eq!(scenario.config.dropout_ticks, Some(2));
        assert_eq!(scenario.events[1].action, ActionName::ResetTrip);
        assert_eq!(scenario.expect.trace.as_deref(), Some("command.trace"));
        assert_eq!(scenario.expect.diagnostics, ["OC-SEM-008"]);
    }

    #[test]
    fn unknown_field_or_action_is_rejected() {
        let unknown_field = MINIMAL.replace("\"until_tick\"", "\"speed\": 2, \"until_tick\"");
        let unknown_action = r#"{ "schema_version": 1, "circuit": "a", "until_tick": 5,
            "events": [{ "tick": 1, "action": "smash", "device": "S1" }] }"#;

        assert!(Scenario::parse(&unknown_field).is_err());
        assert!(Scenario::parse(unknown_action).is_err());
    }

    #[test]
    fn other_schema_versions_are_rejected() {
        for version in ["0", "3"] {
            let other = MINIMAL.replace(
                "\"schema_version\": 1",
                &format!("\"schema_version\": {version}"),
            );

            assert_eq!(
                Scenario::parse(&other),
                Err(format!(
                    "scenario schema version {version} is not supported; this build reads versions 1 to 2"
                ))
            );
        }
    }

    #[test]
    fn libraries_need_version_2_and_stay_inside_the_directory() {
        let with = |version: u32, library: &str| {
            format!(
                r#"{{ "schema_version": {version}, "circuit": "a.ocircuit", "libraries": ["{library}"], "until_tick": 5 }}"#
            )
        };

        assert_eq!(
            Scenario::parse(&with(2, "acme.oclib")).map(|s| s.libraries),
            Ok(vec!["acme.oclib".to_owned()])
        );
        assert!(Scenario::parse(&with(1, "acme.oclib")).is_err());
        assert!(Scenario::parse(&with(2, "../acme.oclib")).is_err());
    }

    #[test]
    fn run_length_is_bounded() {
        let endless = MINIMAL.replace("\"until_tick\": 5", "\"until_tick\": 10000001");

        assert_eq!(
            Scenario::parse(&endless),
            Err("`until_tick` 10000001 exceeds the limit of 10000000".to_owned())
        );
    }

    #[test]
    fn event_after_the_last_tick_is_rejected() {
        let late = r#"{ "schema_version": 1, "circuit": "a", "until_tick": 5,
            "events": [{ "tick": 6, "action": "press", "device": "S1" }] }"#;

        assert_eq!(
            Scenario::parse(late),
            Err("event at tick 6 is after `until_tick` 5".to_owned())
        );
    }

    #[test]
    fn paths_that_leave_the_scenario_directory_are_rejected() {
        for circuit in ["../secret.ocircuit", "/etc/passwd", "a/../../b", ""] {
            let text = MINIMAL.replace("a.ocircuit", circuit);

            assert_eq!(
                Scenario::parse(&text),
                Err("`circuit` must be a relative path without `..` components".to_owned()),
                "{circuit:?}"
            );
        }
        let golden = MINIMAL.replace(
            "\"until_tick\": 5",
            "\"until_tick\": 5, \"expect\": { \"trace\": \"../../.profile\" }",
        );

        assert_eq!(
            Scenario::parse(&golden),
            Err("`expect.trace` must be a relative path without `..` components".to_owned())
        );
    }

    #[test]
    fn nested_relative_paths_are_accepted() {
        let text = MINIMAL.replace("a.ocircuit", "circuits/a.ocircuit");

        assert!(Scenario::parse(&text).is_ok());
    }

    #[test]
    fn oversized_or_malformed_text_is_rejected() {
        assert!(Scenario::parse(&" ".repeat(MAX_SCENARIO_BYTES + 1)).is_err());
        assert!(Scenario::parse("{").is_err());
        assert!(Scenario::parse(r#"{ "schema_version": 1.5 }"#).is_err());
    }
}
