//! `opencircuit-desktop [PROJECT]`: open the window, optionally with a
//! project.

// A release build for Windows is a window program: no console behind it.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable,
    // `println!` and `eprintln!` panic when the stream cannot be written.
    clippy::print_stdout,
    clippy::print_stderr
)]

#[cfg(not(target_arch = "wasm32"))]
fn main() -> std::process::ExitCode {
    native::main()
}

/// In a browser, start on the canvas of `index.html`.
#[cfg(target_arch = "wasm32")]
fn main() {
    opencircuit_desktop::web::start();
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::io::Write as _;
    use std::path::PathBuf;
    use std::process::ExitCode;

    use eframe::egui;
    use opencircuit_desktop::project_lock::ProjectLocks;
    use opencircuit_desktop::recent::RecentProjects;
    use opencircuit_desktop::recovery::{DEFAULT_INTERVAL_SECONDS, RecoveryStore};
    use opencircuit_desktop::versions::VersionStore;
    use opencircuit_desktop::workspace::Preferences;
    use opencircuit_desktop::{DesktopApp, Language};

    const USAGE: &str = "usage: opencircuit-desktop [PROJECT.ocircuit]";

    pub(crate) fn main() -> ExitCode {
        let mut args = std::env::args_os().skip(1);
        let path = args.next().map(PathBuf::from);
        // Nothing more can be done when a standard stream is closed.
        if args.next().is_some() {
            let _ = writeln!(std::io::stderr(), "{USAGE}");
            return ExitCode::from(2);
        }
        if path
            .as_deref()
            .is_some_and(|p| p.as_os_str() == "--help" || p.as_os_str() == "-h")
        {
            let _ = writeln!(std::io::stdout(), "{USAGE}");
            return ExitCode::SUCCESS;
        }

        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title("OpenCircuit")
                // The name of the desktop file, so that a desktop finds the
                // icon of the window.
                .with_app_id("opencircuit")
                .with_inner_size([1200.0, 800.0]),
            ..Default::default()
        };
        let result = eframe::run_native(
            "OpenCircuit",
            options,
            Box::new(move |_creation| {
                let mut app = DesktopApp::new(path.as_deref());
                // The language of the environment, read once, here and not in
                // the window: a test of the window is always in English.
                app.set_language(Language::from_environment(|name| std::env::var(name).ok()));
                // Autosave into the user's data folder, chosen here and not in
                // the window: a test of the window never touches it.
                if let Some(store) = RecoveryStore::in_user_data() {
                    app.enable_recovery(store, DEFAULT_INTERVAL_SECONDS);
                    // A panic writes the newest changes before the process ends.
                    if let Some(copy) = app.panic_copy() {
                        copy.install_hook();
                    }
                }
                if let Some(locks) = ProjectLocks::in_user_data() {
                    app.enable_project_locks(locks, 5.0);
                }
                if let Some(versions) = VersionStore::in_user_data() {
                    app.enable_version_history(versions);
                }
                if let Some(recent) = RecentProjects::in_user_data() {
                    app.enable_recent_projects(recent);
                }
                // The theme, panels and shortcuts of the last session.
                if let Some(preferences) = Preferences::in_user_data() {
                    app.enable_preferences(preferences);
                }
                Ok(Box::new(app))
            }),
        );
        match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                let _ = writeln!(std::io::stderr(), "opencircuit-desktop: {error}");
                ExitCode::FAILURE
            }
        }
    }
}
