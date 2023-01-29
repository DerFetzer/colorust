use color_eyre::Result;
use log::LevelFilter;
use simple_logger::SimpleLogger;

pub mod ffmpeg;
pub mod gui;
pub mod mlt;

pub fn init_logging() -> Result<()> {
    SimpleLogger::new()
        .with_level(LevelFilter::Info)
        .env()
        .init()?;
    color_eyre::install()?;

    Ok(())
}
