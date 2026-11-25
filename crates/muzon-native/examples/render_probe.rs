//! Diagnostic launcher for the exact native UI, on a disposable profile.
//! Logs only the compositor's adapter selection, never network/service records.
use muzon_native::cli::{self, Mode};

struct RendererLog;
impl log::Log for RendererLog {
    fn enabled(&self, record: &log::Metadata<'_>) -> bool {
        record.level() <= log::Level::Info
            && record.target().starts_with("iced_wgpu::window::compositor")
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            eprintln!("[muzon render probe] {}", record.args());
        }
    }
    fn flush(&self) {}
}
static LOGGER: RendererLog = RendererLog;

fn main() -> iced::Result {
    log::set_logger(&LOGGER).expect("diagnostic logger");
    log::set_max_level(log::LevelFilter::Info);
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = cli::parse(args).expect("render_probe --profile /path/to/disposable/profile");
    match mode {
        Mode::Demo => muzon_native::demo::run(),
        Mode::Real(profile) => {
            match muzon_native::real::RealApp::open(&profile).expect("open diagnostic profile") {
                Some(app) => muzon_native::real::run(app),
                None => Ok(()),
            }
        }
    }
}
