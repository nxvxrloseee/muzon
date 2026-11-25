//! The prototype binary: picks the demo or the real player from the command
//! line and hands the chosen mode to Iced.

use muzon_native::cli::{self, Mode};

fn main() -> iced::Result {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if cli::wants_help(&args) {
        println!("{}", cli::USAGE);
        return Ok(());
    }

    match cli::parse(args) {
        Ok(Mode::Demo) => muzon_native::demo::run(),
        Ok(Mode::Real(profile)) => match muzon_native::real::RealApp::open(&profile) {
            Ok(Some(app)) => muzon_native::real::run(app),
            Ok(None) => Ok(()),
            Err(error) => {
                // A failure to open the profile (no GStreamer, an unreadable
                // database) must be visible in a terminal, not a window that
                // never appears.
                eprintln!(
                    "muzon: cannot open profile {}: {error:#}",
                    profile.display()
                );
                std::process::exit(1);
            }
        },
        Err(error) => {
            eprintln!("muzon: {error}\n\n{}", cli::USAGE);
            std::process::exit(2);
        }
    }
}
