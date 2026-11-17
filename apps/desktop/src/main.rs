//! `opencircuit-desktop [PROJECT]`: open the window, optionally with a
//! project.

// A release build for Windows is a window program: no console behind it.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

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
    use std::path::PathBuf;
    use std::process::ExitCode;

    use eframe::egui;
    use opencircuit_desktop::{DesktopApp, Language};

    const USAGE: &str = "usage: opencircuit-desktop [PROJECT.ocircuit]";

    pub(crate) fn main() -> ExitCode {
        let mut args = std::env::args_os().skip(1);
        let path = args.next().map(PathBuf::from);
        if args.next().is_some() {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
        if path
            .as_deref()
            .is_some_and(|p| p.as_os_str() == "--help" || p.as_os_str() == "-h")
        {
            println!("{USAGE}");
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
                Ok(Box::new(app))
            }),
        );
        match result {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("opencircuit-desktop: {error}");
                ExitCode::FAILURE
            }
        }
    }
}
