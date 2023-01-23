use eframe::App;
use egui::{
    plot::{MarkerShape, Plot, PlotPoints, Points},
    CollapsingHeader, Color32, ColorImage, ComboBox, RichText, ScrollArea, SidePanel, Slider,
    TextEdit, TextureHandle, TopBottomPanel, Vec2,
};
use flume::{Receiver, Sender};
use image::{Pixel, Rgba, RgbaImage};
use std::{
    collections::{HashMap, HashSet},
    fmt::{Display, Write},
    path::PathBuf,
    time::Duration,
};
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
    error: Option<String>,
}

#[derive(Debug, Copy, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub(crate) enum PreviewManipulationType {
    Zebra,
}

impl Display for PreviewManipulationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Zebra => write!(f, "Zebra"),
        }
    }
}

#[derive(Debug, Copy, Clone, serde::Deserialize, serde::Serialize)]
pub(crate) struct PreviewManipulation {
    is_active: bool,
    manip_type: PreviewManipulationType,
    zebra_value: u8,
    zebra_range: u8,
}

impl PreviewManipulation {
    pub fn apply(&self, img: &mut RgbaImage) {
        if self.is_active {
            log::info!("{:?}", self);
            match self.manip_type {
                PreviewManipulationType::Zebra => {
                    Self::apply_zebra(img, self.zebra_value, self.zebra_range)
                }
            }
        };
    }

    fn apply_zebra(img: &mut RgbaImage, value: u8, range: u8) {
        // let pattern = RgbaImage::from_pixel(img.width(), img.height(), Rgba([255, 255, 255, 255]));
        let pattern = RgbaImage::from_fn(img.width(), img.height(), |x, y| {
            let is_white = (x + y) % 10 < 5;
            if is_white {
                Rgba([255, 255, 255, 255])
            } else {
                Rgba([0, 0, 0, 255])
            }
        });

        for (x, y, pixel) in img.enumerate_pixels_mut() {
            let luma = pixel.to_luma()[0] as f64 * 100. / 255.;
            if (value.saturating_sub(range) as f64..=value.saturating_add(range) as f64)
                .contains(&luma)
            {
                *pixel = *pattern.get_pixel(x, y);
            }
        }
    }

    fn draw(&mut self, _ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.checkbox(&mut self.is_active, "Active");
        ComboBox::from_label("Type")
            .selected_text(self.manip_type.to_string())
            .show_ui(ui, |ui| {
                ui.selectable_value(
                    &mut self.manip_type,
                    PreviewManipulationType::Zebra,
                    PreviewManipulationType::Zebra.to_string(),
                );
            });
        match self.manip_type {
            PreviewManipulationType::Zebra => {
                ui.add(
                    Slider::new(&mut self.zebra_value, 0..=255)
                        .clamp_to_range(true)
                        .text("Value"),
                );
                ui.add(
                    Slider::new(&mut self.zebra_range, 1..=128)
                        .clamp_to_range(true)
                        .text("Range"),
                );
            }
        };
    }
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
    conversion_template: String,
    preview_manipulation: PreviewManipulation,
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
            conversion_template: "ffmpeg ##input## ##cli## ##filter## ##encoder## ##output##"
                .to_string(),
            preview_manipulation: PreviewManipulation {
                is_active: false,
                manip_type: PreviewManipulationType::Zebra,
                zebra_value: 52,
                zebra_range: 2,
            },
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
            error: None,
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
            CollapsingHeader::new("Preview Manipulation").show(ui, |ui| {
                self.state.preview_manipulation.draw(ctx, ui);
            });
            ui.horizontal(|ui| {
                if ui.button("Create preview").clicked() {
                    let preview_file = self.temp_dir.child("preview.bmp");
                    let mut args = vec![
                        "-y".to_string(),
                        "-loglevel".to_string(),
                        "warning".to_string(),
                    ];
                    args.append(&mut self.state.active_file_state.skip_seconds.to_option_args());
                    args.append(&mut self.state.active_file_state.input_file.to_option_args());
                    args.append(&mut NumberOfFramesOption { frames: 1 }.to_option_args());
                    args.append(
                        &mut self
                            .state
                            .active_file_state
                            .cli_options
                            .iter()
                            .filter_map(|o| {
                                if o.is_active() {
                                    Some(o.to_option_args())
                                } else {
                                    None
                                }
                            })
                            .flatten()
                            .collect(),
                    );
                    args.append(&mut self.state.active_file_state.filter_options.to_option_args());
                    args.append(
                        &mut OutputFile {
                            path: preview_file.clone(),
                            dialog: None,
                        }
                        .to_option_args(),
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
                    let mut args = vec![];
                    args.append(&mut self.state.active_file_state.skip_seconds.to_option_args());
                    args.append(&mut self.state.active_file_state.input_file.to_option_args());
                    args.append(
                        &mut self
                            .state
                            .active_file_state
                            .cli_options
                            .iter()
                            .filter_map(|o| {
                                if o.is_active() {
                                    Some(o.to_option_args())
                                } else {
                                    None
                                }
                            })
                            .flatten()
                            .collect(),
                    );
                    args.append(&mut self.state.active_file_state.filter_options.to_option_args());

                    self.request_tx.send(Request::Play { args }).unwrap();
                }
            });
            ui.separator();
            CollapsingHeader::new("Conversion template").show(ui, |ui| {
                ui.text_edit_singleline(&mut self.state.conversion_template);
            });
            if ui.button("Generate conversion command").clicked() {
                let mut template = self.state.conversion_template.clone();
                template = template.replace(
                    "##input##",
                    &self
                        .state
                        .active_file_state
                        .input_file
                        .to_option_args()
                        .join(" "),
                );
                template = template.replace(
                    "##cli##",
                    &self
                        .state
                        .active_file_state
                        .cli_options
                        .iter()
                        .filter_map(|o| {
                            if o.is_active() {
                                Some(o.to_option_args())
                            } else {
                                None
                            }
                        })
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" "),
                );
                template = template.replace(
                    "##filter##",
                    &self
                        .state
                        .active_file_state
                        .filter_options
                        .to_option_args()
                        .join(" "),
                );
                template = template.replace(
                    "##encoder##",
                    &self
                        .state
                        .active_file_state
                        .encoder
                        .to_option_args()
                        .join(" "),
                );
                template = template.replace(
                    "##output##",
                    &self
                        .state
                        .active_file_state
                        .output_file
                        .to_option_args()
                        .join(" "),
                );
                writeln!(&mut self.state.conversion_commands, "{}", template).unwrap();
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
                            log::error!("Could not parse state from JSON!");
                        }
                    } else {
                        log::warn!("No state available!");
                    }
                }
            });
        });
    }

    fn draw_bottom_panel(&mut self, ctx: &egui::Context) {
        TopBottomPanel::bottom("conversion_commands")
            .resizable(true)
            .max_height(500.)
            .show(ctx, |ui| {
                ScrollArea::new([false, true]).show(ui, |ui| {
                    let available_size = ui.available_size();
                    ui.add_sized(
                        Vec2::new(available_size.x, available_size.y - 40.),
                        TextEdit::multiline(&mut self.state.conversion_commands),
                    );
                    ui.separator();
                    match &self.error {
                        Some(error) => {
                            ui.label(RichText::new(format!("Error: {}", error)).color(Color32::RED))
                        }
                        None => ui.label(RichText::new("OK").color(Color32::GREEN)),
                    };
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
                Response::Image(mut img) => {
                    self.error = None;
                    self.waveform = Some(Waveform::from_image(&img));
                    self.waiting_for_image = false;
                    self.state.preview_manipulation.apply(&mut img);
                    let pixels = img.as_flat_samples();
                    let img = ColorImage::from_rgba_unmultiplied(
                        [img.width() as _, img.height() as _],
                        pixels.as_slice(),
                    );
                    self.image_texture = Some(ctx.load_texture("img", img, Default::default()));
                }
                Response::Error(error) => self.error = Some(error),
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
