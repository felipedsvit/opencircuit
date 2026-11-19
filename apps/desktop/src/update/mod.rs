//! The update notice (ADR-0020, phase 1): the program looks for a newer
//! published release, tells the person in the status bar and in a dialog, and
//! downloads the installer for the system when asked, checking it against the
//! published SHA-256. Installing is left to the person.
//!
//! The network work runs in a thread of its own. The window reads its results
//! once a frame, so a slow connection never holds up drawing.

mod net;
mod release;
mod settings;
mod ui;
mod verify;
mod version;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui;

use crate::i18n::tr;
use release::{Installer, Release};
use settings::Settings;
use version::Version;

/// The version of this program, as the package states it.
const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// Why an update could not be checked or downloaded. The messages are the
/// English text; the window translates them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateError {
    /// The request did not reach GitHub, or the connection broke.
    Connection,
    /// GitHub refused the request because of its rate limit.
    RateLimited,
    /// GitHub answered with an error.
    Unavailable,
    /// The answer is not a release this program can read.
    BadResponse,
    /// A response or a file is larger than the program accepts.
    TooLarge,
    /// The release has no file for this system.
    NoInstaller,
    /// The release has no SHA-256 for its file.
    NoChecksum,
    /// The download ended before the size the release announces.
    Incomplete,
    /// The download does not have the SHA-256 the release publishes.
    ChecksumMismatch,
    /// A file could not be written, or a folder could not be made.
    Disk,
    /// The folder of the download could not be opened in the file manager.
    Open,
    /// The release has no signature for its checksums.
    NotSigned,
    /// The signature of the checksums does not verify, or names another version.
    SignatureInvalid,
}

impl UpdateError {
    /// What the person is told, translated to the window's language. The
    /// literals are written here, with `tr`, so that the catalog check finds them.
    #[must_use]
    pub fn message(self) -> &'static str {
        match self {
            Self::Connection => tr("Could not reach GitHub. Check the connection and try again."),
            Self::RateLimited => tr("GitHub is limiting checks for now. Try again later."),
            Self::Unavailable => tr("GitHub did not answer. Try again later."),
            Self::BadResponse => tr("The release information is not valid."),
            Self::TooLarge => tr("The download is larger than expected. It was discarded."),
            Self::NoInstaller => tr("The latest release has no installer for this system."),
            Self::NoChecksum => tr("The release has no published SHA-256 for this file."),
            Self::Incomplete => tr("The download is incomplete. It was discarded."),
            Self::ChecksumMismatch => {
                tr("The download does not match the published SHA-256. It was discarded.")
            }
            Self::Disk => tr("The file could not be saved. Check the disk space and try again."),
            Self::Open => tr("The folder could not be opened. Open it by hand."),
            Self::NotSigned => tr(
                "This release is not signed. Download it from the release page and check it by hand.",
            ),
            Self::SignatureInvalid => {
                tr("The signature of this release does not match. The download was not started.")
            }
        }
    }
}

/// What a worker thread sends back.
enum Done {
    /// A release lookup finished. `manual` is true for "Check now".
    Checked {
        manual: bool,
        result: Result<Release, UpdateError>,
    },
    /// An installer download finished.
    Downloaded(Result<PathBuf, UpdateError>),
}

/// The state of the installer download, as the dialog shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum Download {
    /// Nothing asked yet.
    #[default]
    Idle,
    /// Downloading and checking.
    Running,
    /// Downloaded and checked, at this path.
    Ready(PathBuf),
    /// Failed; the message says why.
    Failed(UpdateError),
}

/// The answer of the last "Check now", shown in the Updates window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Notice {
    /// This is the latest version.
    UpToDate,
    /// A newer version exists (named by the settings).
    Found,
    /// The check failed.
    Failed(UpdateError),
}

/// The update state of the window.
#[derive(Debug)]
pub struct Updates {
    /// What is remembered between runs.
    settings: Settings,
    /// Where the settings are saved. `None` keeps them in memory only.
    path: Option<PathBuf>,
    /// The folder the installers are downloaded to.
    folder: Option<PathBuf>,
    /// The latest release lookup (the API) and the origin its links must have.
    api_url: String,
    origin: String,
    /// False for a window that never goes to the network (tests, the browser).
    enabled: bool,
    /// The first frame has run its startup check.
    started: bool,
    /// A lookup is running; `Some(manual)` tells who asked for it.
    checking: Option<bool>,
    /// The installer download.
    download: Download,
    /// The answer of the last "Check now".
    notice: Option<Notice>,
    /// The newer release found in this run, for its notes.
    latest: Option<Release>,
    /// The dialog is open.
    dialog_open: bool,
    /// The Updates window is open.
    window_open: bool,
    /// Where the workers send their results.
    sender: Sender<Done>,
    /// Where the results arrive.
    inbox: Receiver<Done>,
}

impl Default for Updates {
    fn default() -> Self {
        Self::disabled()
    }
}

impl Updates {
    /// Updates that never go to the network: the state of a window that did
    /// not ask for them, as in the tests and in the browser build.
    #[must_use]
    pub fn disabled() -> Self {
        Self::new(Settings::default(), None, None, false)
    }

    /// Updates of the program: the settings in `update.json` and the installers
    /// in `updates/`, both under `data_dir`.
    #[must_use]
    pub fn enabled_in(data_dir: &Path) -> Self {
        let path = data_dir.join("update.json");
        let settings = settings::load(&path);
        Self::new(settings, Some(path), Some(data_dir.join("updates")), true)
    }

    fn new(
        settings: Settings,
        path: Option<PathBuf>,
        folder: Option<PathBuf>,
        enabled: bool,
    ) -> Self {
        let (sender, inbox) = mpsc::channel();
        Self {
            settings,
            path,
            folder,
            api_url: net::API_URL.to_owned(),
            origin: net::ORIGIN.to_owned(),
            enabled,
            started: false,
            checking: None,
            download: Download::Idle,
            notice: None,
            latest: None,
            dialog_open: false,
            window_open: false,
            sender,
            inbox,
        }
    }

    /// Run what a frame needs: the results of the workers, the startup check
    /// on the first frame, and the dialog when it is due.
    pub fn tick(&mut self, ctx: &egui::Context) {
        if !self.enabled {
            return;
        }
        while let Ok(done) = self.inbox.try_recv() {
            self.finish(done);
        }
        if self.started {
            return;
        }
        self.started = true;
        let now = now();
        if self.settings.check_due(now) {
            self.start_check(ctx, false);
        }
        if self.settings.dialog_due(now) {
            self.announce(now);
        }
    }

    /// The version to show in the status bar, when the chip is due.
    #[must_use]
    pub fn chip_version(&self) -> Option<&str> {
        self.settings.offers_update()
    }

    /// Open the dialog about the version found.
    pub fn open_dialog(&mut self) {
        self.dialog_open = true;
    }

    /// Open the Updates window.
    pub fn open_window(&mut self) {
        self.window_open = true;
    }

    /// Record a newer version as found. Used by the tests of the window.
    #[doc(hidden)]
    pub fn offer(&mut self, version: &str) {
        self.settings.available = Some(version.to_owned());
    }

    /// Record the answer of a finished worker.
    fn finish(&mut self, done: Done) {
        let now = now();
        match done {
            Done::Checked { manual, result } => {
                self.checking = None;
                self.record_check(manual, result, now);
            }
            Done::Downloaded(result) => {
                self.download = match result {
                    Ok(path) => Download::Ready(path),
                    Err(error) => Download::Failed(error),
                };
            }
        }
    }

    /// Store the answer of a release lookup and decide what it shows.
    fn record_check(&mut self, manual: bool, result: Result<Release, UpdateError>, now: u64) {
        match result {
            Ok(release) => {
                self.settings.last_check = now;
                let newer =
                    Version::parse(CURRENT).is_some_and(|current| release.version > current);
                if newer {
                    let version = release.version.to_string();
                    self.settings.available = Some(version.clone());
                    self.latest = Some(release);
                    if manual {
                        self.notice = Some(Notice::Found);
                    }
                    let skipped = self.settings.skipped.as_deref() == Some(version.as_str());
                    if !skipped && (manual || self.settings.dialog_due(now)) {
                        self.announce(now);
                    }
                } else {
                    self.settings.available = None;
                    self.settings.announced = None;
                    self.latest = None;
                    if manual {
                        self.notice = Some(Notice::UpToDate);
                    }
                }
                self.save();
            }
            Err(UpdateError::RateLimited) => {
                // A refused check still counts: the next one waits for the interval.
                self.settings.last_check = now;
                if manual {
                    self.notice = Some(Notice::Failed(UpdateError::RateLimited));
                }
                self.save();
            }
            Err(error) => {
                if manual {
                    self.notice = Some(Notice::Failed(error));
                }
            }
        }
    }

    /// The dialog opens for the version found, and the reminder starts now.
    fn announce(&mut self, now: u64) {
        self.settings.announced = self.settings.available.clone();
        self.settings.last_reminder = now;
        self.dialog_open = true;
        self.save();
    }

    /// Start a release lookup on a worker thread. A lookup already running is
    /// not doubled.
    fn start_check(&mut self, ctx: &egui::Context, manual: bool) {
        if !self.enabled || self.checking.is_some() {
            return;
        }
        self.checking = Some(manual);
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        let api = self.api_url.clone();
        let origin = self.origin.clone();
        let spawned = std::thread::Builder::new()
            .name("opencircuit-update-check".to_owned())
            .spawn(move || {
                let result = net::check(&api, &origin);
                let _ = sender.send(Done::Checked { manual, result });
                ctx.request_repaint();
            });
        if spawned.is_err() {
            self.checking = None;
            if manual {
                self.notice = Some(Notice::Failed(UpdateError::Connection));
            }
        }
    }

    /// Download the installer of this system on a worker thread: the release
    /// is read again, so the link and the checksum are the current ones.
    fn start_download(&mut self, ctx: &egui::Context) {
        let Some(folder) = self.folder.clone() else {
            self.download = Download::Failed(UpdateError::Disk);
            return;
        };
        let Some(installer) = release::current_installer() else {
            self.download = Download::Failed(UpdateError::NoInstaller);
            return;
        };
        self.download = Download::Running;
        let sender = self.sender.clone();
        let ctx = ctx.clone();
        let api = self.api_url.clone();
        let origin = self.origin.clone();
        let spawned = std::thread::Builder::new()
            .name("opencircuit-update-download".to_owned())
            .spawn(move || {
                let result = net::download_latest(
                    &api,
                    &origin,
                    verify::PUBLIC_KEY_FILE,
                    installer,
                    &folder,
                );
                let _ = sender.send(Done::Downloaded(result));
                ctx.request_repaint();
            });
        if spawned.is_err() {
            self.download = Download::Failed(UpdateError::Connection);
        }
    }

    /// Save the settings. A file that cannot be written only means the schedule
    /// is not remembered for the next run, so the failure is not shown.
    fn save(&self) {
        if let Some(path) = &self.path {
            let _ = settings::save(path, &self.settings);
        }
    }

    /// The page of a version, on the project's site.
    fn page_of(version: &str) -> String {
        format!("{}releases/tag/v{version}", net::ORIGIN)
    }
}

/// Seconds since the Unix epoch, or `0` when the clock is before it.
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// The installer this system uses, as far as the window needs to say.
#[must_use]
pub fn installer_name_for_this_system() -> Option<&'static str> {
    release::current_installer().map(|installer| match installer {
        Installer::AppImage => "AppImage",
        Installer::Deb => tr("Debian package"),
        Installer::Rpm => tr("RPM package"),
        Installer::Dmg => tr("disk image"),
        Installer::Msi => tr("Windows installer"),
    })
}

#[cfg(test)]
mod tests {
    use super::{Done, Notice, Release, Settings, UpdateError, Updates, now};

    fn release(tag: &str) -> Release {
        let origin = "https://github.com/felipedsvit/opencircuit/";
        let json = format!(
            r#"{{"tag_name": "{tag}", "html_url": "{origin}releases/tag/{tag}", "body": "Notes"}}"#
        );
        Release::from_json(json.as_bytes(), origin).expect("a release")
    }

    #[test]
    fn a_newer_version_found_by_hand_is_announced_and_remembered() {
        let mut updates = Updates::disabled();
        updates.record_check(true, Ok(release("v0.0.3")), now());

        assert_eq!(updates.chip_version(), Some("0.0.3"));
        assert!(updates.dialog_open);
        assert_eq!(updates.notice, Some(Notice::Found));
        assert_eq!(updates.settings.announced.as_deref(), Some("0.0.3"));
    }

    #[test]
    fn an_automatic_check_announces_a_new_version_once() {
        let mut updates = Updates::disabled();
        updates.record_check(false, Ok(release("v0.0.3")), now());
        assert!(updates.dialog_open);

        updates.dialog_open = false;
        updates.record_check(false, Ok(release("v0.0.3")), now());
        assert!(
            !updates.dialog_open,
            "the same version does not open the dialog again"
        );
        assert_eq!(updates.chip_version(), Some("0.0.3"));
    }

    #[test]
    fn the_same_version_as_this_program_is_not_an_update() {
        let mut updates = Updates::disabled();
        updates.record_check(true, Ok(release("v0.0.2")), now());

        assert_eq!(updates.chip_version(), None);
        assert_eq!(updates.notice, Some(Notice::UpToDate));
        assert!(!updates.dialog_open);
    }

    #[test]
    fn a_skipped_version_does_not_open_the_dialog_or_show_the_chip() {
        let mut updates = Updates::disabled();
        updates.settings = Settings {
            skipped: Some("0.0.3".to_owned()),
            ..Settings::default()
        };
        updates.record_check(false, Ok(release("v0.0.3")), now());

        assert!(!updates.dialog_open);
        assert_eq!(updates.chip_version(), None);
    }

    #[test]
    fn a_failed_automatic_check_is_quiet_and_a_failed_manual_one_says_why() {
        let mut updates = Updates::disabled();
        updates.record_check(false, Err(UpdateError::Connection), now());
        assert_eq!(updates.notice, None);
        assert_eq!(
            updates.settings.last_check, 0,
            "no answer, no new check time"
        );

        updates.record_check(true, Err(UpdateError::Connection), now());
        assert_eq!(
            updates.notice,
            Some(Notice::Failed(UpdateError::Connection))
        );
    }

    #[test]
    fn a_refused_check_counts_as_checked_so_the_next_one_waits() {
        let mut updates = Updates::disabled();
        updates.record_check(false, Err(UpdateError::RateLimited), 1_000);
        assert_eq!(updates.settings.last_check, 1_000);
    }

    #[test]
    fn a_disabled_window_never_starts_a_lookup() {
        let mut updates = Updates::disabled();
        let ctx = eframe::egui::Context::default();
        updates.start_check(&ctx, true);
        updates.start_download(&ctx);
        assert_eq!(updates.checking, None);
        assert!(updates.inbox.try_recv().is_err());
    }

    #[test]
    fn a_result_from_a_worker_is_stored_when_the_frame_reads_it() {
        let mut updates = Updates::disabled();
        updates.checking = Some(true);
        let _ = updates.sender.send(Done::Checked {
            manual: true,
            result: Ok(release("v0.0.3")),
        });

        while let Ok(done) = updates.inbox.try_recv() {
            updates.finish(done);
        }

        assert_eq!(updates.checking, None);
        assert_eq!(updates.chip_version(), Some("0.0.3"));
    }

    #[test]
    fn every_error_has_a_message_and_the_installer_names_are_known() {
        for error in [
            UpdateError::Connection,
            UpdateError::RateLimited,
            UpdateError::Unavailable,
            UpdateError::BadResponse,
            UpdateError::TooLarge,
            UpdateError::NoInstaller,
            UpdateError::NoChecksum,
            UpdateError::Incomplete,
            UpdateError::ChecksumMismatch,
            UpdateError::Disk,
            UpdateError::Open,
            UpdateError::NotSigned,
            UpdateError::SignatureInvalid,
        ] {
            assert!(!error.message().is_empty());
        }
    }
}
