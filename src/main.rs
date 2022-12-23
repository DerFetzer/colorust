use eframe::NativeOptions;
use gui::ColorustApp;

mod ffmpeg;
mod gui;

fn main() {
    let (request_tx, request_rx) = flume::unbounded();
    let (response_tx, response_rx) = flume::unbounded();

    std::thread::spawn(move || ffmpeg::Thread::new(request_rx, response_tx).run());

    let native_options = NativeOptions::default();
    eframe::run_native(
        "Colorust",
        native_options,
        Box::new(|cc| Box::new(ColorustApp::new(cc, request_tx, response_rx))),
    );
}
