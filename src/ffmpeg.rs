use egui::{CollapsingHeader, ComboBox, DragValue, Slider};
use egui_file::FileDialog;
use flume::{Receiver, Sender};
use image::io::Reader as ImageReader;
use image::RgbaImage;
use log::{debug, info};
use roxmltree::Node;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, process::Command};

use crate::{gui::GuiElement, mlt::get_property_value};

#[derive(Debug)]
pub enum Request {
    ExtractFrame { args: Vec<String>, output: PathBuf },
    Play { args: Vec<String> },
}

#[derive(Debug)]
pub enum Response {
    Image(RgbaImage),
    Error(String),
}

#[typetag::serde(tag = "type")]
pub trait CliOption: GuiElement {
    fn to_option_args(&self) -> Vec<String>;
}

#[typetag::serde(tag = "type")]
pub trait Filter: GuiElement {
    fn to_filter_string(&self) -> String;
}

#[derive(Default, Serialize, Deserialize)]
pub struct FilterOption {
    pub filters: Vec<Box<dyn Filter>>,
}

#[typetag::serde]
impl CliOption for FilterOption {
    fn to_option_args(&self) -> Vec<String> {
        if self.filters.is_empty() || self.filters.iter().all(|f| !f.is_active()) {
            return vec![];
        }
        let s = "-vf".to_string();
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

        vec![s, filter_string]
    }
}

#[typetag::serde]
impl GuiElement for FilterOption {
    fn name(&self) -> &'static str {
        "Filters"
    }

    fn draw(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        for filter in self.filters.iter_mut() {
            CollapsingHeader::new(filter.name()).show(ui, |ui| {
                filter.draw(ctx, ui);
            });
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct SkipOption {
    pub seconds: u64,
}

#[typetag::serde]
impl CliOption for SkipOption {
    fn to_option_args(&self) -> Vec<String> {
        vec!["-ss".to_string(), self.seconds.to_string()]
    }
}

#[typetag::serde]
impl GuiElement for SkipOption {
    fn name(&self) -> &'static str {
        "Skip seconds"
    }

    fn draw(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.add(DragValue::new(&mut self.seconds));
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct NumberOfFramesOption {
    pub frames: u64,
}

#[typetag::serde]
impl CliOption for NumberOfFramesOption {
    fn to_option_args(&self) -> Vec<String> {
        vec!["-frames:v".to_string(), self.frames.to_string()]
    }
}

#[typetag::serde]
impl GuiElement for NumberOfFramesOption {
    fn name(&self) -> &'static str {
        "Namber of frames"
    }

    fn draw(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.add(DragValue::new(&mut self.frames));
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct InputFile {
    pub path: PathBuf,
    #[serde(skip)]
    pub dialog: Option<FileDialog>,
}

#[typetag::serde]
impl CliOption for InputFile {
    fn to_option_args(&self) -> Vec<String> {
        vec!["-i".to_string(), self.path.to_string_lossy().to_string()]
    }
}

#[typetag::serde]
impl GuiElement for InputFile {
    fn name(&self) -> &'static str {
        "Input file"
    }

    fn draw(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let mut path = self.path.to_string_lossy();
        if ui.text_edit_singleline(path.to_mut()).changed() {
            self.path = PathBuf::from(path.to_string());
        }
        if ui.button("Open").clicked() {
            let mut dialog = FileDialog::open_file(if self.path.is_dir() || self.path.is_file() {
                Some(self.path.clone())
            } else {
                None
            });
            dialog.open();
            self.dialog = Some(dialog);
        }
        if let Some(dialog) = &mut self.dialog {
            if dialog.show(ctx).selected() {
                if let Some(path) = dialog.path() {
                    self.path = path.to_path_buf();
                }
            }
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct OutputFile {
    pub path: PathBuf,
    #[serde(skip)]
    pub dialog: Option<FileDialog>,
}

#[typetag::serde]
impl CliOption for OutputFile {
    fn to_option_args(&self) -> Vec<String> {
        vec![self.path.to_string_lossy().to_string()]
    }
}

#[typetag::serde]
impl GuiElement for OutputFile {
    fn name(&self) -> &'static str {
        "Output file"
    }

    fn draw(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        let mut path = self.path.to_string_lossy();
        if ui.text_edit_singleline(path.to_mut()).changed() {
            self.path = PathBuf::from(path.to_string());
        }
        if ui.button("Open").clicked() {
            let mut dialog = FileDialog::save_file(if self.path.is_file() {
                Some(self.path.clone())
            } else if let Some(parent) = self.path.parent() {
                if parent.is_dir() {
                    Some(parent.to_path_buf())
                } else {
                    None
                }
            } else {
                None
            });
            dialog.open();
            self.dialog = Some(dialog);
        }
        if let Some(dialog) = &mut self.dialog {
            if dialog.show(ctx).selected() {
                if let Some(path) = dialog.path() {
                    self.path = path.to_path_buf();
                }
            }
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct Encoder {
    pub expression: String,
}

#[typetag::serde]
impl CliOption for Encoder {
    fn to_option_args(&self) -> Vec<String> {
        if self.expression.is_empty() {
            vec![]
        } else {
            vec!["-c:v".to_string(), self.expression.clone()]
        }
    }
}

#[typetag::serde]
impl GuiElement for Encoder {
    fn draw(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.text_edit_singleline(&mut self.expression);
    }

    fn name(&self) -> &'static str {
        "Encoder"
    }
}

#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct FilterExposure {
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
    fn draw(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
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

impl TryFrom<&Node<'_, '_>> for FilterExposure {
    type Error = ();

    fn try_from(value: &Node) -> Result<Self, Self::Error> {
        if get_property_value(value, "mlt_service") != Some("avfilter.exposure".to_string()) {
            return Err(());
        }
        let exposure = get_property_value(value, "av.exposure").ok_or(())?;
        let black = get_property_value(value, "av.black").ok_or(())?;
        let disabled = get_property_value(value, "disable").unwrap_or(0) == 1;

        Ok(Self {
            is_active: !disabled,
            exposure,
            black,
        })
    }
}

#[derive(Serialize, Deserialize)]
pub struct FilterLut {
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

impl TryFrom<&Node<'_, '_>> for FilterLut {
    type Error = ();

    fn try_from(value: &Node) -> Result<Self, Self::Error> {
        if get_property_value(value, "mlt_service") != Some("avfilter.lut3d".to_string()) {
            return Err(());
        }
        let file = get_property_value(value, "av.file").ok_or(())?;
        let interpolation = get_property_value(value, "av.interp").ok_or(())?;
        let disabled = get_property_value(value, "disable").unwrap_or(0) == 1;

        Ok(Self {
            is_active: !disabled,
            file,
            interpolation,
        })
    }
}

#[typetag::serde]
impl GuiElement for FilterLut {
    fn draw(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
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
pub struct FilterScale {
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
    fn draw(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
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
pub struct FilterEq {
    pub is_active: bool,
    pub contrast: f32,
    pub brightness: f32,
    pub saturation: f32,
    pub gamma: f32,
    pub gamma_r: f32,
    pub gamma_g: f32,
    pub gamma_b: f32,
}

impl Default for FilterEq {
    fn default() -> Self {
        Self {
            is_active: false,
            contrast: 1.,
            brightness: 0.,
            saturation: 1.,
            gamma: 1.,
            gamma_r: 1.,
            gamma_g: 1.,
            gamma_b: 1.,
        }
    }
}

#[typetag::serde]
impl Filter for FilterEq {
    fn to_filter_string(&self) -> String {
        format!(
            "eq=contrast={}:brightness={}:saturation={}:gamma={}:gamma_r={}:gamma_g={}:gamma_b={}",
            self.contrast,
            self.brightness,
            self.saturation,
            self.gamma,
            self.gamma_r,
            self.gamma_g,
            self.gamma_b
        )
    }
}

#[typetag::serde]
impl GuiElement for FilterEq {
    fn draw(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
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
        ui.add(
            Slider::new(&mut self.gamma_r, 0.1..=10.0)
                .clamp_to_range(true)
                .logarithmic(true)
                .text("Gamma R"),
        );
        ui.add(
            Slider::new(&mut self.gamma_g, 0.1..=10.0)
                .clamp_to_range(true)
                .logarithmic(true)
                .text("Gamma G"),
        );
        ui.add(
            Slider::new(&mut self.gamma_b, 0.1..=10.0)
                .clamp_to_range(true)
                .logarithmic(true)
                .text("Gamma B"),
        );
    }

    fn name(&self) -> &'static str {
        "Eq"
    }

    fn is_active(&self) -> bool {
        self.is_active
    }
}

impl TryFrom<&Node<'_, '_>> for FilterEq {
    type Error = ();

    fn try_from(value: &Node) -> Result<Self, Self::Error> {
        if get_property_value(value, "mlt_service") != Some("avfilter.eq".to_string()) {
            return Err(());
        }
        let contrast = get_property_value(value, "av.contrast").ok_or(())?;
        let brightness = get_property_value(value, "av.brightness").ok_or(())?;
        let saturation = get_property_value(value, "av.saturation").ok_or(())?;
        let gamma = get_property_value(value, "av.gamma").ok_or(())?;
        let gamma_r = get_property_value(value, "av.gamma_r").ok_or(())?;
        let gamma_g = get_property_value(value, "av.gamma_g").ok_or(())?;
        let gamma_b = get_property_value(value, "av.gamma_b").ok_or(())?;
        let disabled = get_property_value(value, "disable").unwrap_or(0) == 1;

        Ok(Self {
            is_active: !disabled,
            contrast,
            brightness,
            saturation,
            gamma,
            gamma_r,
            gamma_g,
            gamma_b,
        })
    }
}

#[derive(Serialize, Deserialize)]
pub struct FilterColortemp {
    pub is_active: bool,
    pub temperature: u32,
}

impl Default for FilterColortemp {
    fn default() -> Self {
        Self {
            is_active: false,
            temperature: 6500,
        }
    }
}

#[typetag::serde]
impl Filter for FilterColortemp {
    fn to_filter_string(&self) -> String {
        format!("colortemperature=temperature={}:pl=1", self.temperature)
    }
}

#[typetag::serde]
impl GuiElement for FilterColortemp {
    fn draw(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.is_active, "Active");
        ui.add(
            Slider::new(&mut self.temperature, 1000..=40000)
                .clamp_to_range(true)
                .logarithmic(true)
                .text("Temperature"),
        );
    }

    fn name(&self) -> &'static str {
        "Color temperature"
    }

    fn is_active(&self) -> bool {
        self.is_active
    }
}

impl TryFrom<&Node<'_, '_>> for FilterColortemp {
    type Error = ();

    fn try_from(value: &Node) -> Result<Self, Self::Error> {
        if get_property_value(value, "mlt_service") != Some("avfilter.colortemperature".to_string())
        {
            return Err(());
        }
        let temperature = get_property_value(value, "av.temperature").ok_or(())?;
        let disabled = get_property_value(value, "disable").unwrap_or(0) == 1;

        Ok(Self {
            is_active: !disabled,
            temperature,
        })
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct FilterColorBalance {
    pub is_active: bool,
    pub shadows_red: f32,
    pub shadows_green: f32,
    pub shadows_blue: f32,
    pub midtones_red: f32,
    pub midtones_green: f32,
    pub midtones_blue: f32,
    pub highlights_red: f32,
    pub highlights_green: f32,
    pub highlights_blue: f32,
    pub preserve_lightness: bool,
}

#[typetag::serde]
impl Filter for FilterColorBalance {
    fn to_filter_string(&self) -> String {
        format!(
            "colorbalance=rs={}:gs={}:bs={}:rm={}:gm={}:bm={}:rh={}:gh={}:bh={}",
            self.shadows_red,
            self.shadows_green,
            self.shadows_blue,
            self.midtones_red,
            self.midtones_green,
            self.midtones_blue,
            self.highlights_red,
            self.highlights_green,
            self.highlights_blue
        )
    }
}

#[typetag::serde]
impl GuiElement for FilterColorBalance {
    fn draw(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.is_active, "Active");
        ui.label("Shadows");
        ui.add(
            Slider::new(&mut self.shadows_red, -1.0..=1.0)
                .clamp_to_range(true)
                .text("Red"),
        );
        ui.add(
            Slider::new(&mut self.shadows_green, -1.0..=1.0)
                .clamp_to_range(true)
                .text("Green"),
        );
        ui.add(
            Slider::new(&mut self.shadows_blue, -1.0..=1.0)
                .clamp_to_range(true)
                .text("Blue"),
        );
        ui.label("Midtones");
        ui.add(
            Slider::new(&mut self.midtones_red, -1.0..=1.0)
                .clamp_to_range(true)
                .text("Red"),
        );
        ui.add(
            Slider::new(&mut self.midtones_green, -1.0..=1.0)
                .clamp_to_range(true)
                .text("Green"),
        );
        ui.add(
            Slider::new(&mut self.midtones_blue, -1.0..=1.0)
                .clamp_to_range(true)
                .text("Blue"),
        );
        ui.label("Highlights");
        ui.add(
            Slider::new(&mut self.highlights_red, -1.0..=1.0)
                .clamp_to_range(true)
                .text("Red"),
        );
        ui.add(
            Slider::new(&mut self.highlights_green, -1.0..=1.0)
                .clamp_to_range(true)
                .text("Green"),
        );
        ui.add(
            Slider::new(&mut self.highlights_blue, -1.0..=1.0)
                .clamp_to_range(true)
                .text("Blue"),
        );
    }

    fn name(&self) -> &'static str {
        "Color balance"
    }

    fn is_active(&self) -> bool {
        self.is_active
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct FilterCustom {
    pub is_active: bool,
    pub expression: String,
}

#[typetag::serde]
impl Filter for FilterCustom {
    fn to_filter_string(&self) -> String {
        self.expression.clone()
    }
}

#[typetag::serde]
impl GuiElement for FilterCustom {
    fn draw(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.is_active, "Active");
        ui.text_edit_singleline(&mut self.expression);
    }

    fn name(&self) -> &'static str {
        "Custom filter(s)"
    }

    fn is_active(&self) -> bool {
        self.is_active
    }
}

pub struct Thread {
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
                log::info!("Received request: {request:?}");
                match request {
                    Request::ExtractFrame { args, output } => {
                        match self.extract_frame(args, output) {
                            Ok(response) => self.response_tx.send(response).unwrap(),
                            Err(e) => self.response_tx.send(Response::Error(e)).unwrap(),
                        }
                    }
                    Request::Play { args } => {
                        let ffmpeg_output = Command::new("ffplay").args(args).output().unwrap();
                        if !ffmpeg_output.status.success() {
                            log::error!(
                                "ffmpeg output:\ncode: {}, \n{}\n{}",
                                ffmpeg_output.status.code().unwrap(),
                                String::from_utf8(ffmpeg_output.stdout).unwrap(),
                                String::from_utf8(ffmpeg_output.stderr).unwrap(),
                            );
                        }
                    }
                }
            }
        }
    }

    fn extract_frame(&mut self, args: Vec<String>, output: PathBuf) -> Result<Response, String> {
        let ffmpeg_output = Command::new("ffmpeg").args(args).output().unwrap();
        info!("Command: {:?}", ffmpeg_output);
        if !ffmpeg_output.status.success() {
            log::error!(
                "Could not extract frame:\ncode: {},\n{}\n{}",
                ffmpeg_output.status.code().unwrap(),
                String::from_utf8(ffmpeg_output.stdout).unwrap(),
                String::from_utf8(ffmpeg_output.stderr).unwrap()
            );
            return Err("Could not extract frame!".to_string());
        }
        info!("Output: {:?}", output);
        let img = ImageReader::open(output).unwrap().decode().unwrap();
        Ok(Response::Image(img.into_rgba8()))
    }
}

#[cfg(test)]
mod tests {
    use roxmltree::Document;

    use super::*;

    #[test]
    fn exposure_from_xml() {
        let xml = r#"
               <filter id="filter6">
                <property name="mlt_service">avfilter.exposure</property>
                <property name="kdenlive_id">avfilter.exposure</property>
                <property name="av.exposure">00:00:00.000=0</property>
                <property name="av.black">00:00:00.000=0</property>
                <property name="kdenlive:collapsed">1</property>
                <property name="disable">1</property>
               </filter>
            "#;
        let doc = Document::parse(xml).unwrap();
        let root = &doc.root();

        let filter = root.try_into();
        assert_eq!(
            filter,
            Ok(FilterExposure {
                is_active: false,
                exposure: 0.0,
                black: 0.0
            })
        );
    }
}
