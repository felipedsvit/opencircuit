//! `opencircuit-desktop [PROJECT]`: open the window, optionally with a
//! project.

use std::path::PathBuf;
use std::process::ExitCode;

use eframe::egui;
use opencircuit_desktop::DesktopApp;

const USAGE: &str = "usage: opencircuit-desktop [PROJECT.ocircuit]";

fn main() -> ExitCode {
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
            .with_inner_size([1200.0, 800.0]),
        ..Default::default()
    };
    let result = eframe::run_native(
        "OpenCircuit",
        options,
        Box::new(move |_creation| Ok(Box::new(DesktopApp::new(path.as_deref())))),
    );
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("opencircuit-desktop: {error}");
            ExitCode::FAILURE
        }
    }
}
