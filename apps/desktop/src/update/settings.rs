//! The update settings, kept in `update.json` in the data folder, and the
//! schedule they set: when to check, and when the dialog opens again.
//!
//! The schedule is a function of the settings and of the time it is asked at,
//! so it can be tested without a clock or a network.

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::files::read_text;
use crate::files::write_atomic;

/// Seconds in a day.
pub const DAY: u64 = 86_400;
/// Largest settings file read. The file holds a few short fields.
const MAX_SETTINGS_BYTES: usize = 16 * 1024;

/// How often the program checks for a new release, and reminds the person of
/// one. `Never` turns both off; "Check now" still works.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Frequency {
    /// Every three days.
    Every3Days,
    /// Every five days. The default.
    #[default]
    Every5Days,
    /// Never on its own.
    Never,
}

impl Frequency {
    /// The number of days between checks and reminders, `None` for `Never`.
    #[must_use]
    pub const fn days(self) -> Option<u64> {
        match self {
            Self::Every3Days => Some(3),
            Self::Every5Days => Some(5),
            Self::Never => None,
        }
    }
}

/// What the program remembers between runs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// How often to check and remind.
    pub frequency: Frequency,
    /// When the last check that got an answer was made, in seconds since the
    /// Unix epoch. `0` when none was.
    pub last_check: u64,
    /// When the dialog last opened for the version it is about.
    pub last_reminder: u64,
    /// The newer version found, as written in its tag, if any.
    pub available: Option<String>,
    /// The version whose dialog has opened (so that it is not a new offer
    /// every time).
    pub announced: Option<String>,
    /// The version the person chose to skip.
    pub skipped: Option<String>,
}

impl Settings {
    /// Whether a check should start now, at `now` seconds after the epoch.
    #[must_use]
    pub fn check_due(&self, now: u64) -> bool {
        self.frequency
            .days()
            .is_some_and(|days| now.saturating_sub(self.last_check) >= days.saturating_mul(DAY))
    }

    /// Whether the dialog should open now for the version found.
    ///
    /// A version opens the dialog once. After that the dialog comes back every
    /// reminder interval while the version is still neither downloaded nor
    /// skipped. A skipped version never opens it again.
    #[must_use]
    pub fn dialog_due(&self, now: u64) -> bool {
        let Some(version) = self.available.as_deref() else {
            return false;
        };
        if self.skipped.as_deref() == Some(version) {
            return false;
        }
        if self.announced.as_deref() != Some(version) {
            return true;
        }
        self.frequency
            .days()
            .is_some_and(|days| now.saturating_sub(self.last_reminder) >= days.saturating_mul(DAY))
    }

    /// Whether the chip in the status bar shows the version found.
    #[must_use]
    pub fn offers_update(&self) -> Option<&str> {
        let version = self.available.as_deref()?;
        let on = self.frequency != Frequency::Never;
        (on && self.skipped.as_deref() != Some(version)).then_some(version)
    }
}

/// Read the settings at `path`. A missing or damaged file gives the defaults,
/// so the program still starts; the next save replaces the file.
#[must_use]
pub fn load(path: &Path) -> Settings {
    read_text(path, MAX_SETTINGS_BYTES)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Write the settings to `path`, atomically (a crash leaves the old file).
///
/// # Errors
///
/// When the file cannot be written.
pub fn save(path: &Path, settings: &Settings) -> io::Result<()> {
    let text = serde_json::to_string_pretty(settings)
        .map_err(|error| io::Error::other(error.to_string()))?;
    write_atomic(path, text).map_err(|error| io::Error::other(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{DAY, Frequency, Settings, load, save};

    const NOW: u64 = 1_800_000_000;

    fn found(version: &str) -> Settings {
        Settings {
            available: Some(version.to_owned()),
            ..Settings::default()
        }
    }

    #[test]
    fn the_first_run_checks_and_later_runs_wait_for_the_interval() {
        let mut settings = Settings::default();
        assert!(settings.check_due(NOW), "no check yet");

        settings.last_check = NOW;
        assert!(!settings.check_due(NOW + 5 * DAY - 1));
        assert!(settings.check_due(NOW + 5 * DAY));

        settings.frequency = Frequency::Every3Days;
        assert!(!settings.check_due(NOW + 3 * DAY - 1));
        assert!(settings.check_due(NOW + 3 * DAY));
    }

    #[test]
    fn never_means_no_automatic_check_and_no_reminder() {
        let mut settings = found("0.0.3");
        settings.frequency = Frequency::Never;
        settings.last_check = 0;
        assert!(!settings.check_due(NOW));
        assert!(
            settings.dialog_due(NOW),
            "a version found by hand opens once"
        );
        settings.announced = Some("0.0.3".to_owned());
        assert!(!settings.dialog_due(NOW + 100 * DAY));
        assert_eq!(settings.offers_update(), None, "no chip with Never");
    }

    #[test]
    fn a_new_version_opens_the_dialog_once_then_every_interval() {
        let mut settings = found("0.0.3");
        assert!(settings.dialog_due(NOW));

        settings.announced = Some("0.0.3".to_owned());
        settings.last_reminder = NOW;
        assert!(!settings.dialog_due(NOW + 5 * DAY - 1));
        assert!(settings.dialog_due(NOW + 5 * DAY));
    }

    #[test]
    fn a_skipped_version_stays_quiet_and_a_newer_one_is_offered_again() {
        let mut settings = found("0.0.3");
        settings.skipped = Some("0.0.3".to_owned());
        assert!(!settings.dialog_due(NOW + 100 * DAY));
        assert_eq!(settings.offers_update(), None);

        settings.available = Some("0.0.4".to_owned());
        assert!(settings.dialog_due(NOW));
        assert_eq!(settings.offers_update(), Some("0.0.4"));
    }

    #[test]
    fn settings_round_trip_and_a_damaged_file_gives_the_defaults() {
        let dir = std::env::temp_dir().join(format!("oc-update-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("a scratch folder");
        let path = dir.join("update.json");

        let settings = Settings {
            frequency: Frequency::Every3Days,
            last_check: NOW,
            ..found("0.0.3")
        };
        save(&path, &settings).expect("saved");
        assert_eq!(load(&path), settings);

        std::fs::write(&path, "{ not json").expect("a damaged file");
        assert_eq!(load(&path), Settings::default());

        std::fs::write(&path, r#"{"frequency": "every_week"}"#).expect("a wrong value");
        assert_eq!(load(&path), Settings::default());

        assert_eq!(load(&dir.join("missing.json")), Settings::default());
        std::fs::remove_dir_all(&dir).expect("cleaned up");
    }
}
