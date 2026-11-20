//! The window side of the update: the chip in the status bar, the dialog that
//! offers the new version, and the Updates window with the settings.

use std::path::Path;
use std::process::Command;

use eframe::egui::{self, RichText};

use super::settings::{DAY, Frequency};
use opencircuit_update::UpdateError;

use super::{CURRENT, Download, Notice, Updates, installer_name_for_this_system, localized, now};
use crate::dialog::{Choice, Dialog, Kind, Role};
use crate::i18n::{tr, trf, trn};

/// What a button of the dialog does.
#[derive(Clone, Debug)]
enum Action {
    /// Download and check the installer.
    Download,
    /// Ask again after the reminder interval.
    Remind,
    /// Do not offer this version again.
    Skip,
    /// Show the downloaded file in its folder.
    ShowFile(std::path::PathBuf),
    /// Close the dialog.
    Close,
    /// Try the download again.
    Retry,
}

impl Updates {
    /// The chip in the status bar, when a newer version is known.
    pub(crate) fn chip(&mut self, ui: &mut egui::Ui) {
        let Some(version) = self.chip_version().map(str::to_owned) else {
            return;
        };
        ui.separator();
        let text = trf("Version {0} available", &[&version]);
        if ui
            .link(text)
            .on_hover_text(tr("Open the update dialog"))
            .clicked()
        {
            self.open_dialog();
        }
    }

    /// The dialog about the version found, when it is open.
    pub(crate) fn dialog(&mut self, ctx: &egui::Context) {
        if !self.dialog_open {
            return;
        }
        let Some(version) = self.settings.available.clone() else {
            self.dialog_open = false;
            return;
        };
        let remind = self
            .settings
            .frequency
            .days()
            .map(|days| trf("Remind me in {0} days", &[&days]));
        let page = Self::page_of(&version);
        let title = trf("Version {0} available", &[&version]);
        let (kind, message, options) = self.dialog_options(&version, remind.as_deref());
        let choices: Vec<Choice<'_>> = options
            .iter()
            .map(|(label, role, _)| Choice {
                label,
                role: *role,
                name: None,
            })
            .collect();
        let notes = self
            .latest
            .as_ref()
            .filter(|release| release.version.to_string() == version)
            .map(|release| release.notes.clone())
            .filter(|notes| !notes.trim().is_empty());
        let installer = installer_name_for_this_system();
        let download = self.download.clone();

        let outcome = Dialog {
            id: egui::Id::unique("Update dialog"),
            kind,
            title: &title,
            message: &message,
            choices: &choices,
        }
        .show(ctx, |ui| {
            if let Some(notes) = &notes {
                egui::ScrollArea::vertical()
                    .max_height(160.0)
                    .show(ui, |ui| {
                        ui.label(notes.as_str());
                    });
            }
            match &download {
                Download::Idle => {
                    if let Some(name) = installer {
                        ui.label(
                            RichText::new(trf("Installer for this system: {0}", &[&name])).weak(),
                        );
                    }
                    ui.hyperlink_to(tr("Release page"), page.as_str());
                }
                Download::Running => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(tr("Downloading and checking..."));
                    });
                }
                Download::Ready(path) => {
                    ui.label(
                        RichText::new(tr(
                            "Downloaded and checked against the signed SHA-256 list.",
                        ))
                        .weak(),
                    );
                    ui.add(
                        egui::Label::new(RichText::new(path.display().to_string()).monospace())
                            .wrap(),
                    );
                }
                Download::Failed(error) => {
                    ui.colored_label(ui.visuals().error_fg_color, localized(*error));
                }
            }
        });

        if outcome.dismissed {
            self.dialog_open = false;
            return;
        }
        let action = outcome
            .chosen
            .and_then(|index| options.get(index))
            .map(|(_, _, action)| action.clone());
        if let Some(action) = action {
            self.act(ctx, action);
        }
    }

    /// The kind, the message and the buttons of the dialog for the download
    /// state. The buttons are labelled text with a role and the action each
    /// does.
    fn dialog_options(
        &self,
        version: &str,
        remind: Option<&str>,
    ) -> (Kind, String, Vec<(String, Role, Action)>) {
        let skip = tr("Skip this version");
        let close = tr("Close");
        match &self.download {
            Download::Idle => {
                let mut options = vec![(
                    trf("Download {0}", &[&version]),
                    Role::Primary,
                    Action::Download,
                )];
                if let Some(remind) = remind {
                    options.push((remind.to_owned(), Role::Secondary, Action::Remind));
                }
                options.push((skip.to_owned(), Role::Secondary, Action::Skip));
                let message = trf("You have version {0}.", &[&CURRENT]);
                (Kind::Question, message, options)
            }
            Download::Running => {
                let options = vec![(close.to_owned(), Role::Secondary, Action::Close)];
                (
                    Kind::Question,
                    tr("Downloading and checking...").to_owned(),
                    options,
                )
            }
            Download::Ready(path) => {
                let options = vec![
                    (
                        tr("Show file").to_owned(),
                        Role::Primary,
                        Action::ShowFile(path.clone()),
                    ),
                    (close.to_owned(), Role::Secondary, Action::Close),
                ];
                (Kind::Question, tr("The file is ready.").to_owned(), options)
            }
            Download::Failed(error) => {
                let options = vec![
                    (tr("Try again").to_owned(), Role::Primary, Action::Retry),
                    (close.to_owned(), Role::Secondary, Action::Close),
                ];
                (Kind::Error, localized(*error).to_owned(), options)
            }
        }
    }

    /// Do what a button of the dialog asks.
    fn act(&mut self, ctx: &egui::Context, action: Action) {
        match action {
            Action::Download | Action::Retry => self.start_download(ctx),
            Action::Remind => {
                self.settings.last_reminder = now();
                self.dialog_open = false;
                self.save();
            }
            Action::Skip => {
                self.settings.skipped = self.settings.available.clone();
                self.dialog_open = false;
                self.save();
            }
            Action::ShowFile(path) => {
                if !open_folder_of(&path) {
                    self.download = Download::Failed(UpdateError::Open);
                }
            }
            Action::Close => self.dialog_open = false,
        }
    }

    /// The Updates window: how often to check, how to install, and "Check now".
    pub(crate) fn window(&mut self, ctx: &egui::Context) {
        if !self.window_open {
            return;
        }
        let mut open = self.window_open;
        egui::Window::new(tr("Updates"))
            .id(egui::Id::unique("Updates window"))
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| self.window_body(ui, ctx));
        self.window_open = open;
    }

    fn window_body(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.strong(trf("Current version: {0}", &[&CURRENT]));
        ui.label(last_check_text(self.settings.last_check, now()));
        ui.add_space(8.0);

        ui.strong(tr("Check for updates"));
        let mut frequency = self.settings.frequency;
        let mut changed = false;
        changed |= ui
            .radio_value(&mut frequency, Frequency::Every3Days, tr("Every 3 days"))
            .changed();
        changed |= ui
            .radio_value(&mut frequency, Frequency::Every5Days, tr("Every 5 days"))
            .changed();
        changed |= ui
            .radio_value(&mut frequency, Frequency::Never, tr("Never"))
            .changed();
        if changed {
            self.settings.frequency = frequency;
            self.save();
        }
        if frequency == Frequency::Never {
            ui.label(RichText::new(tr("Automatic checks are off. Check now still works.")).weak());
        }
        ui.add_space(8.0);

        ui.strong(tr("Install updates"));
        ui.add(egui::RadioButton::new(true, tr("Download and check")));
        ui.add_enabled(
            false,
            egui::RadioButton::new(false, tr("Install automatically")),
        )
        .on_disabled_hover_text(tr("Not available yet."));
        ui.label(
            RichText::new(tr("Downloads are checked against the signed SHA-256 list.")).weak(),
        );
        ui.separator();

        let busy = self.checking.is_some();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!busy, egui::Button::new(tr("Check now")))
                .clicked()
            {
                self.start_check(ctx, true);
            }
            if busy {
                ui.spinner();
                ui.label(tr("Checking..."));
            }
        });
        match self.notice {
            Some(Notice::UpToDate) => {
                ui.label(tr("You have the latest version."));
            }
            Some(Notice::Found) => {
                if let Some(version) = self.settings.available.clone() {
                    ui.label(trf("Version {0} is available.", &[&version]));
                }
            }
            Some(Notice::Failed(error)) => {
                ui.colored_label(ui.visuals().error_fg_color, localized(error));
            }
            None => {}
        }
    }
}

/// When the last check was, in words.
fn last_check_text(last: u64, now: u64) -> String {
    if last == 0 {
        return tr("Never checked").to_owned();
    }
    let days = now.saturating_sub(last) / DAY;
    match usize::try_from(days) {
        Ok(0) => tr("Checked today").to_owned(),
        Ok(count) => trn(
            "Last checked {0} day ago",
            "Last checked {0} days ago",
            count,
        ),
        Err(_) => tr("Checked a long time ago").to_owned(),
    }
}

/// Open the folder that holds `path` in the file manager, with the file
/// selected where the system allows it. `false` when it could not be started.
fn open_folder_of(path: &Path) -> bool {
    let started = if cfg!(target_os = "windows") {
        Command::new("explorer")
            .arg(format!("/select,{}", path.display()))
            .spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg("-R").arg(path).spawn()
    } else {
        match path.parent() {
            Some(folder) => Command::new("xdg-open").arg(folder).spawn(),
            None => return false,
        }
    };
    started.is_ok()
}
