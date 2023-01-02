use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
    path::PathBuf,
    time::Duration,
};

use eframe::App;
use egui::{
    plot::{MarkerShape, Plot, PlotPoints, Points},
    CollapsingHeader, Color32, ColorImage, ScrollArea, SidePanel, Slider, TextEdit, TextureHandle,
    TopBottomPanel,
};
use flume::{Receiver, Sender};
use image::RgbaImage;
use temp_dir::TempDir;

use crate::ffmpeg::{
    CliOption, Encoder, FilterColorBalance, FilterColortemp, FilterCustom, FilterEq,
    FilterExposure, FilterLut, FilterOption, FilterScale, InputFile, NumberOfFramesOption,
    OutputFile, Request, Response, SkipOption,
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
pub(crate) struct FileState {
    input_file: InputFile,
    output_file: OutputFile,
    encoder: Encoder,
    skip_seconds: SkipOption,
    cli_options: Vec<Box<dyn CliOption>>,
    filter_options: FilterOption,
}

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(default)]
pub(crate) struct ColorustState {
    active_file_state: FileState,
    waveform_multiplier: f64,
    conversion_commands: String,
    file_history: HashMap<PathBuf, String>,
}

impl ColorustState {}

impl Default for ColorustState {
    fn default() -> Self {
        ColorustState {
            active_file_state: FileState {
                input_file: Default::default(),
                output_file: Default::default(),
                encoder: Default::default(),
                cli_options: vec![],
                filter_options: FilterOption {
                    filters: vec![
                        Box::new(FilterScale {
                            is_active: true,
                            width: 1280,
                            height: 720,
                        }),
                        Box::<FilterExposure>::default(),
                        Box::<FilterColortemp>::default(),
                        Box::<FilterLut>::default(),
                        Box::<FilterEq>::default(),
                        Box::<FilterColorBalance>::default(),
                        Box::<FilterCustom>::default(),
                    ],
                },
                skip_seconds: Default::default(),
            },
            waveform_multiplier: 25.,
            conversion_commands: Default::default(),
            file_history: Default::default(),
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
            CollapsingHeader::new(self.state.active_file_state.input_file.name()).show(ui, |ui| {
                self.state.active_file_state.input_file.draw(ctx, ui);
            });
            CollapsingHeader::new(self.state.active_file_state.output_file.name()).show(ui, |ui| {
                self.state.active_file_state.output_file.draw(ctx, ui);
            });
            CollapsingHeader::new(self.state.active_file_state.encoder.name()).show(ui, |ui| {
                self.state.active_file_state.encoder.draw(ctx, ui);
            });
            CollapsingHeader::new(self.state.active_file_state.skip_seconds.name()).show(
                ui,
                |ui| {
                    self.state.active_file_state.skip_seconds.draw(ctx, ui);
                },
            );
            ui.separator();
            for opt in self.state.active_file_state.cli_options.iter_mut() {
                CollapsingHeader::new(opt.name()).show(ui, |ui| {
                    opt.draw(ctx, ui);
                });
            }
            CollapsingHeader::new("Filters").show(ui, |ui| {
                self.state.active_file_state.filter_options.draw(ctx, ui);
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Create preview").clicked() {
                    let preview_file = self.temp_dir.child("preview.bmp");
                    let args = format!(
                        "-y -loglevel warning {} {} {} {} {} {}",
                        self.state.active_file_state.skip_seconds.to_option_string(),
                        self.state.active_file_state.input_file.to_option_string(),
                        NumberOfFramesOption { frames: 1 }.to_option_string(),
                        &self
                            .state
                            .active_file_state
                            .cli_options
                            .iter()
                            .filter_map(|o| if o.is_active() {
                                Some(o.to_option_string())
                            } else {
                                None
                            })
                            .collect::<Vec<_>>()
                            .join(" "),
                        self.state
                            .active_file_state
                            .filter_options
                            .to_option_string(),
                        OutputFile {
                            path: preview_file.clone(),
                            dialog: None
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
                if ui.button("Play preview").clicked() {
                    let args = format!(
                        "{} {} {} {}",
                        self.state.active_file_state.skip_seconds.to_option_string(),
                        self.state.active_file_state.input_file.to_option_string(),
                        &self
                            .state
                            .active_file_state
                            .cli_options
                            .iter()
                            .filter_map(|o| if o.is_active() {
                                Some(o.to_option_string())
                            } else {
                                None
                            })
                            .collect::<Vec<_>>()
                            .join(" "),
                        self.state
                            .active_file_state
                            .filter_options
                            .to_option_string(),
                    );

                    self.request_tx.send(Request::Play { args }).unwrap();
                }
            });
            if ui.button("Generate conversion command").clicked() {
                writeln!(
                    &mut self.state.conversion_commands,
                    "ffmpeg {} {} {} {} {}",
                    self.state.active_file_state.input_file.to_option_string(),
                    &self
                        .state
                        .active_file_state
                        .cli_options
                        .iter()
                        .filter_map(|o| if o.is_active() {
                            Some(o.to_option_string())
                        } else {
                            None
                        })
                        .collect::<Vec<_>>()
                        .join(" "),
                    self.state
                        .active_file_state
                        .filter_options
                        .to_option_string(),
                    self.state.active_file_state.encoder.to_option_string(),
                    self.state.active_file_state.output_file.to_option_string(),
                )
                .unwrap();
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Save file state").clicked() {
                    self.state.file_history.insert(
                        self.state.active_file_state.input_file.path.clone(),
                        serde_json::to_string(&self.state.active_file_state).unwrap(),
                    );
                }
                if ui.button("Load file state").clicked() {
                    if let Some(file_state_string) = self
                        .state
                        .file_history
                        .get(&self.state.active_file_state.input_file.path)
                    {
                        if let Ok(file_state) = serde_json::from_str(file_state_string) {
                            self.state.active_file_state = file_state;
                        } else {
                            println!("Could not parse state from JSON!");
                        }
                    } else {
                        println!("No state available!");
                    }
                }
            });
        });
    }

    fn draw_bottom_panel(&mut self, ctx: &egui::Context) {
        TopBottomPanel::bottom("conversion_commands").show(ctx, |ui| {
            ScrollArea::new([false, true]).show(ui, |ui| {
                ui.add_sized(
                    ui.available_size(),
                    TextEdit::multiline(&mut self.state.conversion_commands),
                );
            });
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
        self.draw_bottom_panel(ctx);
        self.draw_central_panel(ctx);
        self.draw_windows(ctx);
    }
}

#[typetag::serde(tag = "type")]
pub(crate) trait GuiElement {
    fn draw(&mut self, ctx: &egui::Context, ui: &mut egui::Ui);
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
