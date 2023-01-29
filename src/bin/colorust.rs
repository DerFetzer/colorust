use color_eyre::Result;
use colorust::gui::ColorustApp;
use eframe::NativeOptions;

fn main() -> Result<()> {
    colorust::init_logging()?;

    let (request_tx, request_rx) = flume::unbounded();
    let (response_tx, response_rx) = flume::unbounded();

    std::thread::spawn(move || colorust::ffmpeg::Thread::new(request_rx, response_tx).run());

    let native_options = NativeOptions::default();
    eframe::run_native(
        "Colorust",
        native_options,
        Box::new(|cc| Box::new(ColorustApp::new(cc, request_tx, response_rx))),
    );

    Ok(())
}
