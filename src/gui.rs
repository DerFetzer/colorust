use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

use eframe::App;
use egui::{
    plot::{MarkerShape, Plot, PlotPoints, Points},
    CollapsingHeader, Color32, ColorImage, SidePanel, Slider, TextureHandle,
};
use flume::{Receiver, Sender};
use image::RgbaImage;
use temp_dir::TempDir;

use crate::ffmpeg::{
    CliOption, FilterEq, FilterExposure, FilterLut, FilterOption, FilterScale, InputFile,
    NumberOfFramesOption, OutputFile, Request, Response, SkipOption,
};

pub(crate) struct ColorustApp {
    state: ColorustState,
    image_texture: Option<TextureHandle>,
    request_tx: Sender<Request>,
    response_rx: Receiver<Response>,
    temp_dir: TempDir,
    waiting_for_image: bool,
    waveform: Option<Waveform>,
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub(crate) struct ColorustState {
    input_file: InputFile,
    output_file: OutputFile,
    skip_seconds: SkipOption,
    cli_options: Vec<Box<dyn CliOption>>,
    filter_options: FilterOption,
    waveform_multiplier: f64,
}

impl ColorustState {}

impl Default for ColorustState {
    fn default() -> Self {
        ColorustState {
            input_file: Default::default(),
            output_file: Default::default(),
            cli_options: vec![],
            filter_options: FilterOption {
                filters: vec![
                    Box::new(FilterScale {
                        is_active: true,
                        width: 1280,
                        height: 720,
                    }),
                    Box::<FilterExposure>::default(),
                    Box::<FilterLut>::default(),
                    Box::<FilterEq>::default(),
                ],
            },
            skip_seconds: Default::default(),
            waveform_multiplier: 15.,
        }
    }
}

impl ColorustApp {
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        request_tx: Sender<Request>,
        response_rx: Receiver<Response>,
    ) -> Self {
        let state = if let Some(storage) = cc.storage {
            eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default()
        } else {
            Default::default()
        };
        Self {
            state,
            image_texture: None,
            request_tx,
            response_rx,
            temp_dir: TempDir::new().unwrap(),
            waiting_for_image: false,
            waveform: None,
        }
    }

    fn draw_side_panel(&mut self, ctx: &egui::Context) {
        SidePanel::left("Parameters").show(ctx, |ui| {
            CollapsingHeader::new(self.state.input_file.name()).show(ui, |ui| {
                self.state.input_file.draw(ui);
            });
            CollapsingHeader::new(self.state.output_file.name()).show(ui, |ui| {
                self.state.output_file.draw(ui);
            });
            CollapsingHeader::new(self.state.skip_seconds.name()).show(ui, |ui| {
                self.state.skip_seconds.draw(ui);
            });
            for opt in self.state.cli_options.iter_mut() {
                CollapsingHeader::new(opt.name()).show(ui, |ui| {
                    opt.draw(ui);
                });
            }
            CollapsingHeader::new("Filters").show(ui, |ui| {
                self.state.filter_options.draw(ui);
            });
            if ui.button("Create preview").clicked() {
                let preview_file = self.temp_dir.child("preview.bmp");
                let args = format!(
                    "-y -loglevel warning {} {} {} {} {} {}",
                    self.state.skip_seconds.to_option_string(),
                    self.state.input_file.to_option_string(),
                    NumberOfFramesOption { frames: 1 }.to_option_string(),
                    &self
                        .state
                        .cli_options
                        .iter()
                        .filter_map(|o| if o.is_active() {
                            Some(o.to_option_string())
                        } else {
                            None
                        })
                        .collect::<Vec<_>>()
                        .join(" "),
                    self.state.filter_options.to_option_string(),
                    OutputFile {
                        path: preview_file.to_str().unwrap().to_string()
                    }
                    .to_option_string(),
                );

                self.request_tx
                    .send(Request::ExtractFrame {
                        args,
                        output: preview_file,
                    })
                    .unwrap();
                self.waiting_for_image = true;
            }
        });
    }

    fn draw_windows(&mut self, ctx: &egui::Context) {
        egui::Window::new("waveforms").show(ctx, |ui| {
            ui.add(Slider::new(&mut self.state.waveform_multiplier, 1.0..=100.).text("Multiplier"));
            ui.horizontal(|ui| {
                if let Some(waveform) = self.waveform.as_ref() {
                    Plot::new("waveform_r")
                        .width(350.)
                        .height(400.)
                        .include_y(-10.)
                        .include_y(110.)
                        .show(ui, |plot_ui| {
                            for (points, value) in waveform.get_plot_points(RgbComponent::Red) {
                                plot_ui.points(
                                    Points::new(points)
                                        .color(Color32::from_rgb(
                                            (value * 255. * self.state.waveform_multiplier) as u8,
                                            0,
                                            0,
                                        ))
                                        .shape(MarkerShape::Circle),
                                )
                            }
                        });
                    Plot::new("waveform_g")
                        .width(350.)
                        .height(400.)
                        .include_y(-10.)
                        .include_y(110.)
                        .show(ui, |plot_ui| {
                            for (points, value) in waveform.get_plot_points(RgbComponent::Green) {
                                plot_ui.points(
                                    Points::new(points)
                                        .color(Color32::from_rgb(
                                            0,
                                            (value * 255. * self.state.waveform_multiplier) as u8,
                                            0,
                                        ))
                                        .shape(MarkerShape::Circle),
                                )
                            }
                        });
                    Plot::new("waveform_b")
                        .width(350.)
                        .height(400.)
                        .include_y(-10.)
                        .include_y(110.)
                        .show(ui, |plot_ui| {
                            for (points, value) in waveform.get_plot_points(RgbComponent::Blue) {
                                plot_ui.points(
                                    Points::new(points)
                                        .color(Color32::from_rgb(
                                            0,
                                            0,
                                            (value * 255. * self.state.waveform_multiplier) as u8,
                                        ))
                                        .shape(MarkerShape::Circle),
                                )
                            }
                        });
                }
            });
        });
    }

    fn draw_central_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(img) = self.image_texture.as_ref() {
                ui.image(img, img.size_vec2());
            }
        });
    }

    fn handle_events(&mut self, ctx: &egui::Context) {
        if let Ok(response) = self.response_rx.try_recv() {
            match response {
                Response::Image(img) => {
                    self.waveform = Some(Waveform::from_image(&img));
                    self.waiting_for_image = false;
                    let pixels = img.as_flat_samples();
                    let img = ColorImage::from_rgba_unmultiplied(
                        [img.width() as _, img.height() as _],
                        pixels.as_slice(),
                    );
                    self.image_texture = Some(ctx.load_texture("img", img, Default::default()));
                }
                Response::Error(error) => println!("{error}"),
            }
        }
    }
}

impl App for ColorustApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.state);
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.waiting_for_image {
            ctx.request_repaint_after(Duration::from_millis(50));
        }

        self.handle_events(ctx);

        self.draw_side_panel(ctx);
        self.draw_windows(ctx);
        self.draw_central_panel(ctx);
    }
}

#[typetag::serde(tag = "type")]
pub(crate) trait GuiElement {
    fn draw(&mut self, ui: &mut egui::Ui);
    fn name(&self) -> &'static str;
    fn is_active(&self) -> bool {
        true
    }
}

#[derive(Debug)]
enum RgbComponent {
    Red,
    Green,
    Blue,
}

#[derive(Debug)]
struct Waveform {
    plot_points_r: Vec<(Vec<[f64; 2]>, f64)>,
    plot_points_g: Vec<(Vec<[f64; 2]>, f64)>,
    plot_points_b: Vec<(Vec<[f64; 2]>, f64)>,
}

impl Waveform {
    fn from_image(img: &RgbaImage) -> Self {
        let width = img.width();
        let height = img.height();

        let mut values_r = Vec::with_capacity(width as usize);
        let mut values_g = Vec::with_capacity(width as usize);
        let mut values_b = Vec::with_capacity(width as usize);

        for x in 0..width {
            let mut row_r = HashMap::new();
            let mut row_g = HashMap::new();
            let mut row_b = HashMap::new();

            for y in 0..height {
                let pixel = img.get_pixel(x, y);
                *row_r
                    .entry(pixel.0[0] as u32 * 10000 / u8::MAX as u32)
                    .or_default() += 1;
                *row_g
                    .entry(pixel.0[1] as u32 * 10000 / u8::MAX as u32)
                    .or_default() += 1;
                *row_b
                    .entry(pixel.0[2] as u32 * 10000 / u8::MAX as u32)
                    .or_default() += 1;
            }

            values_r.push(row_r);
            values_g.push(row_g);
            values_b.push(row_b);
        }

        Self {
            plot_points_r: Self::values_to_plot_points(values_r, height.into()),
            plot_points_g: Self::values_to_plot_points(values_g, height.into()),
            plot_points_b: Self::values_to_plot_points(values_b, height.into()),
        }
    }

    fn values_to_plot_points(
        values: Vec<HashMap<u32, u64>>,
        max_value: u64,
    ) -> Vec<(Vec<[f64; 2]>, f64)> {
        let mut points = Vec::new();
        let values_set: HashSet<_> = values.iter().flat_map(|column| column.values()).collect();

        for value in values_set {
            let plot_points = values
                .iter()
                .enumerate()
                .flat_map(|(i, m)| {
                    m.iter()
                        .filter(|(_, v)| *v == value)
                        .map(move |(k, _)| [i as f64, *k as f64 / 100.])
                })
                .collect();
            points.push((plot_points, *value as f64 / max_value as f64));
        }

        points
    }

    fn get_plot_points(&self, component: RgbComponent) -> Vec<(PlotPoints, f64)> {
        let values = match component {
            RgbComponent::Red => &self.plot_points_r,
            RgbComponent::Green => &self.plot_points_g,
            RgbComponent::Blue => &self.plot_points_b,
        };
        values
            .iter()
            .cloned()
            .map(|(points, value)| (points.into(), value))
            .collect()
    }
}
