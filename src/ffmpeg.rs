use egui::{CollapsingHeader, ComboBox, DragValue, Slider};
use flume::{Receiver, Sender};
use image::io::Reader as ImageReader;
use image::RgbaImage;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, process::Command};

use crate::gui::GuiElement;

#[derive(Debug)]
pub(crate) enum Request {
    ExtractFrame { args: String, output: PathBuf },
    Convert { args: String },
}

#[derive(Debug)]
pub(crate) enum Response {
    Image(RgbaImage),
    Error(String),
}

#[typetag::serde(tag = "type")]
pub(crate) trait CliOption: GuiElement {
    fn to_option_string(&self) -> String;
}

#[typetag::serde(tag = "type")]
pub(crate) trait Filter: GuiElement {
    fn to_filter_string(&self) -> String;
}

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct FilterOption {
    pub filters: Vec<Box<dyn Filter>>,
}

#[typetag::serde]
impl CliOption for FilterOption {
    fn to_option_string(&self) -> String {
        if self.filters.is_empty() || self.filters.iter().all(|f| !f.is_active()) {
            return String::new();
        }
        let s = "-vf ".to_string();
        let filter_string = self
            .filters
            .iter()
            .filter_map(|f| {
                if f.is_active() {
                    Some(f.to_filter_string())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join(",");

        s + &filter_string
    }
}

#[typetag::serde]
impl GuiElement for FilterOption {
    fn name(&self) -> &'static str {
        "Filters"
    }

    fn draw(&mut self, ui: &mut egui::Ui) {
        for filter in self.filters.iter_mut() {
            CollapsingHeader::new(filter.name()).show(ui, |ui| {
                filter.draw(ui);
            });
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct SkipOption {
    pub seconds: u64,
}

#[typetag::serde]
impl CliOption for SkipOption {
    fn to_option_string(&self) -> String {
        format!("-ss {}", self.seconds)
    }
}

#[typetag::serde]
impl GuiElement for SkipOption {
    fn name(&self) -> &'static str {
        "Skip seconds"
    }

    fn draw(&mut self, ui: &mut egui::Ui) {
        ui.add(DragValue::new(&mut self.seconds));
    }
}

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct NumberOfFramesOption {
    pub frames: u64,
}

#[typetag::serde]
impl CliOption for NumberOfFramesOption {
    fn to_option_string(&self) -> String {
        format!("-frames:v {}", self.frames)
    }
}

#[typetag::serde]
impl GuiElement for NumberOfFramesOption {
    fn name(&self) -> &'static str {
        "Namber of frames"
    }

    fn draw(&mut self, ui: &mut egui::Ui) {
        ui.add(DragValue::new(&mut self.frames));
    }
}

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct InputFile {
    pub path: String,
}

#[typetag::serde]
impl CliOption for InputFile {
    fn to_option_string(&self) -> String {
        format!("-i {}", self.path)
    }
}

#[typetag::serde]
impl GuiElement for InputFile {
    fn name(&self) -> &'static str {
        "Input file"
    }

    fn draw(&mut self, ui: &mut egui::Ui) {
        ui.text_edit_singleline(&mut self.path);
    }
}

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct OutputFile {
    pub path: String,
}

#[typetag::serde]
impl CliOption for OutputFile {
    fn to_option_string(&self) -> String {
        self.path.to_string()
    }
}

#[typetag::serde]
impl GuiElement for OutputFile {
    fn name(&self) -> &'static str {
        "Output file"
    }

    fn draw(&mut self, ui: &mut egui::Ui) {
        ui.text_edit_singleline(&mut self.path);
    }
}

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct Encoder {
    pub expression: String,
}

#[typetag::serde]
impl CliOption for Encoder {
    fn to_option_string(&self) -> String {
        format!("-c:v {}", self.expression)
    }
}

#[typetag::serde]
impl GuiElement for Encoder {
    fn draw(&mut self, ui: &mut egui::Ui) {
        ui.text_edit_singleline(&mut self.expression);
    }

    fn name(&self) -> &'static str {
        "Encoder"
    }
}

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct FilterExposure {
    pub is_active: bool,
    pub exposure: f32,
    pub black: f32,
}

#[typetag::serde]
impl Filter for FilterExposure {
    fn to_filter_string(&self) -> String {
        format!("exposure=exposure={}:black={}", self.exposure, self.black)
    }
}

#[typetag::serde]
impl GuiElement for FilterExposure {
    fn draw(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.is_active, "Active");
        ui.add(
            Slider::new(&mut self.exposure, -3.0..=3.0)
                .clamp_to_range(true)
                .text("Exposure"),
        );
        ui.add(
            Slider::new(&mut self.black, -1.0..=1.0)
                .clamp_to_range(true)
                .text("Black level"),
        );
    }

    fn name(&self) -> &'static str {
        "Exposure"
    }

    fn is_active(&self) -> bool {
        self.is_active
    }
}

#[derive(Serialize, Deserialize)]
pub(crate) struct FilterLut {
    pub is_active: bool,
    pub file: String,
    pub interpolation: String,
}

impl Default for FilterLut {
    fn default() -> Self {
        Self {
            is_active: false,
            file: String::new(),
            interpolation: "tetrahedral".to_string(),
        }
    }
}

#[typetag::serde]
impl Filter for FilterLut {
    fn to_filter_string(&self) -> String {
        format!("lut3d=file={}:interp={}", self.file, self.interpolation)
    }
}

#[typetag::serde]
impl GuiElement for FilterLut {
    fn draw(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.is_active, "Active");
        ui.text_edit_singleline(&mut self.file);
        ComboBox::from_label("Interpolation")
            .selected_text(&self.interpolation)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut self.interpolation, "nearest".to_string(), "nearest");
                ui.selectable_value(
                    &mut self.interpolation,
                    "trilinear".to_string(),
                    "trilinear",
                );
                ui.selectable_value(
                    &mut self.interpolation,
                    "tetrahedral".to_string(),
                    "tetrahedral",
                );
            });
    }

    fn name(&self) -> &'static str {
        "LUT"
    }

    fn is_active(&self) -> bool {
        self.is_active
    }
}

#[derive(Default, Serialize, Deserialize)]
pub(crate) struct FilterScale {
    pub is_active: bool,
    pub width: u64,
    pub height: u64,
}

#[typetag::serde]
impl Filter for FilterScale {
    fn to_filter_string(&self) -> String {
        format!("scale={}:{}", self.width, self.height)
    }
}

#[typetag::serde]
impl GuiElement for FilterScale {
    fn draw(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.is_active, "Active");
        ui.horizontal(|ui| {
            ui.label("Width");
            ui.add(DragValue::new(&mut self.width));
        });
        ui.horizontal(|ui| {
            ui.label("Heigth");
            ui.add(DragValue::new(&mut self.height));
        });
    }

    fn name(&self) -> &'static str {
        "Scale"
    }

    fn is_active(&self) -> bool {
        self.is_active
    }
}

#[derive(Serialize, Deserialize)]
pub(crate) struct FilterEq {
    pub is_active: bool,
    pub contrast: f32,
    pub brightness: f32,
    pub saturation: f32,
    pub gamma: f32,
}

impl Default for FilterEq {
    fn default() -> Self {
        Self {
            is_active: false,
            contrast: 1.,
            brightness: 0.,
            saturation: 1.,
            gamma: 1.,
        }
    }
}

#[typetag::serde]
impl Filter for FilterEq {
    fn to_filter_string(&self) -> String {
        format!(
            "eq=contrast={}:brightness={}:saturation={}:gamma={}",
            self.contrast, self.brightness, self.saturation, self.gamma
        )
    }
}

#[typetag::serde]
impl GuiElement for FilterEq {
    fn draw(&mut self, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.is_active, "Active");
        ui.add(
            Slider::new(&mut self.contrast, 0.0..=3.0)
                .clamp_to_range(true)
                .logarithmic(true)
                .text("Contrast"),
        );
        ui.add(
            Slider::new(&mut self.brightness, -1.0..=1.0)
                .clamp_to_range(true)
                .logarithmic(true)
                .text("Brightness"),
        );
        ui.add(
            Slider::new(&mut self.saturation, 0.0..=3.0)
                .clamp_to_range(true)
                .text("Saturation"),
        );
        ui.add(
            Slider::new(&mut self.gamma, 0.1..=10.0)
                .clamp_to_range(true)
                .logarithmic(true)
                .text("Gamma"),
        );
    }

    fn name(&self) -> &'static str {
        "Eq"
    }

    fn is_active(&self) -> bool {
        self.is_active
    }
}

pub(crate) struct Thread {
    pub request_rx: Receiver<Request>,
    pub response_tx: Sender<Response>,
}

impl Thread {
    pub fn new(request_rx: Receiver<Request>, response_tx: Sender<Response>) -> Self {
        Self {
            request_rx,
            response_tx,
        }
    }

    pub fn run(&mut self) -> ! {
        loop {
            if let Ok(request) = self.request_rx.recv() {
                println!("{:?}", request);
                match request {
                    Request::ExtractFrame { args, output } => {
                        let ffmpeg_output = Command::new("ffmpeg")
                            .args(args.split(' ').filter(|a| !a.is_empty()))
                            .output()
                            .unwrap();
                        println!(
                            "code: {}, \n{}\n{}",
                            ffmpeg_output.status.code().unwrap(),
                            String::from_utf8(ffmpeg_output.stdout).unwrap(),
                            String::from_utf8(ffmpeg_output.stderr).unwrap(),
                        );
                        let img = ImageReader::open(output).unwrap().decode().unwrap();
                        self.response_tx
                            .send(Response::Image(img.into_rgba8()))
                            .unwrap();
                    }
                    Request::Convert { args } => todo!(),
                }
            }
        }
    }
}
