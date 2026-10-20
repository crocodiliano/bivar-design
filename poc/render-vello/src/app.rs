//! Window: a toolbar and a canvas drawn by Vello into a wgpu texture that egui
//! shows pixel for pixel.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::Instant;

use eframe::egui::{
    self, Color32, ColorImage, EventFilter, FontData, FontDefinitions, FontFamily, Key, Rect, Role,
    Sense, TextureId, Ui, WidgetInfo, pos2,
};
use eframe::egui_wgpu::{self, RenderState};
use vello::kurbo::{Point, Vec2};
use vello::wgpu;

use crate::camera::{self, Camera};
use crate::cpu;
use crate::gpu::{self, Gpu};
use crate::image::{GPU_REPEAT_TOLERANCE, Rgba, compare, diff_mask};
use crate::scene::SceneDesc;
use crate::stats::{FrameTimes, Summary};
use crate::text::{self, Font};

const KEY_PAN: f64 = 64.0;
const KEY_ZOOM: f64 = 1.25;
const WHEEL_ZOOM_SPEED: f64 = 0.002;
pub const CANVAS_LABEL: &str =
    "Canvas. Setas movem a vista, + e - fazem zoom, 0 mostra a cena inteira.";

struct Target {
    texture: wgpu::Texture,
    id: TextureId,
    width: u32,
    height: u32,
}

/// A screen capture in progress: what was on the canvas when it was asked.
struct Capture {
    origin: (u32, u32),
    camera: Camera,
    displayed: Rgba,
    screenshot: Receiver<Arc<ColorImage>>,
}

struct Tour {
    start: Instant,
    last_frame: Option<Instant>,
    times: FrameTimes,
}

/// Steps of the unattended self-test (`--autoteste`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AutoStep {
    Warmup(u32),
    Touring,
    /// Waits for the layout to settle after the tour result appears.
    Settle(u32),
    Capturing,
}

const AUTO_WARMUP_FRAMES: u32 = 60;
const AUTO_SETTLE_FRAMES: u32 = 10;

#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// Run the smoothness tour and the screen capture by itself, print the
    /// results and close the window.
    pub self_test: bool,
    /// Make the 10,000 rectangles share this many different gradients.
    pub distinct_gradients: Option<usize>,
}

pub struct RenderApp {
    render_state: RenderState,
    auto: Option<AutoStep>,
    desc: SceneDesc,
    /// `desc` encoded once for Vello, in world units.
    world: vello::Scene,
    font: Font,
    gpu: Gpu,
    camera: Option<Camera>,
    target: Option<Target>,
    dirty: bool,
    last_render_ms: f64,
    tour: Option<Tour>,
    tour_result: Option<Summary>,
    capture: Option<Capture>,
    status: String,
}

impl RenderApp {
    pub fn new(cc: &eframe::CreationContext<'_>, options: Options) -> Result<Self, String> {
        install_fonts(&cc.egui_ctx);
        let render_state = cc
            .wgpu_render_state
            .clone()
            .ok_or("eframe did not start with wgpu")?;
        let gpu = Gpu::new(&render_state.device).map_err(|e| e.to_string())?;
        let info = render_state.adapter.get_info();
        let mut desc = SceneDesc::test_scene();
        if let Some(n) = options.distinct_gradients {
            desc = desc.with_distinct_gradients(n);
        }
        let font = Font::source_sans_3();
        Ok(Self {
            status: format!("Placa de vídeo: {} ({:?})", info.name, info.backend),
            render_state,
            auto: options.self_test.then_some(AutoStep::Warmup(0)),
            world: gpu::world_scene(&desc, &font),
            desc,
            font,
            gpu,
            camera: None,
            target: None,
            dirty: true,
            last_render_ms: 0.0,
            tour: None,
            tour_result: None,
            capture: None,
        })
    }

    fn device(&self) -> &wgpu::Device {
        &self.render_state.device
    }

    fn queue(&self) -> &wgpu::Queue {
        &self.render_state.queue
    }

    /// Creates or resizes the canvas texture and registers it with egui.
    fn ensure_target(&mut self, width: u32, height: u32) {
        if self
            .target
            .as_ref()
            .is_some_and(|t| (t.width, t.height) == (width, height))
        {
            return;
        }
        let texture = gpu::target_texture(self.device(), width, height);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let mut renderer = self.render_state.renderer.write();
        let id = match &self.target {
            Some(old) => {
                renderer.update_egui_texture_from_wgpu_texture(
                    self.device(),
                    &view,
                    wgpu::FilterMode::Nearest,
                    old.id,
                );
                old.id
            }
            None => {
                renderer.register_native_texture(self.device(), &view, wgpu::FilterMode::Nearest)
            }
        };
        drop(renderer);
        self.target = Some(Target {
            texture,
            id,
            width,
            height,
        });
        self.dirty = true;
    }

    fn render_canvas(&mut self) {
        let (Some(target), Some(camera)) = (&self.target, self.camera) else {
            return;
        };
        let started = Instant::now();
        let scene = gpu::view_scene(&self.world, &camera);
        let view = target
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let result = self.gpu.render(
            &self.render_state.device,
            &self.render_state.queue,
            &scene,
            &view,
            target.width,
            target.height,
        );
        self.last_render_ms = started.elapsed().as_secs_f64() * 1000.0;
        if let Err(e) = result {
            self.status = format!("Erro ao desenhar: {e}");
        }
        self.dirty = false;
    }

    fn handle_input(&mut self, ui: &Ui, response: &egui::Response, ppp: f64, origin: Point) {
        let Some(camera) = self.camera.as_mut() else {
            return;
        };
        let before = *camera;
        let to_canvas = |p: egui::Pos2| {
            Point::new(f64::from(p.x) * ppp, f64::from(p.y) * ppp) - origin.to_vec2()
        };

        if response.dragged() {
            let d = response.drag_delta();
            camera.pan_by(Vec2::new(f64::from(d.x), f64::from(d.y)) * ppp);
        }
        if let Some(hover) = response.hover_pos() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let factor = f64::from(pinch) * (f64::from(scroll) * WHEEL_ZOOM_SPEED).exp();
            if factor != 1.0 {
                camera.zoom_at(to_canvas(hover), factor);
            }
        }
        if response.clicked() {
            response.request_focus();
        }
        if response.has_focus() {
            let center = self.target.as_ref().map_or(Point::ZERO, |t| {
                Point::new(f64::from(t.width) / 2.0, f64::from(t.height) / 2.0)
            });
            ui.input(|i| {
                let pan = |key: Key| if i.key_pressed(key) { KEY_PAN } else { 0.0 };
                camera.pan_by(Vec2::new(
                    pan(Key::ArrowLeft) - pan(Key::ArrowRight),
                    pan(Key::ArrowUp) - pan(Key::ArrowDown),
                ));
                if i.key_pressed(Key::Plus) || i.key_pressed(Key::Equals) {
                    camera.zoom_at(center, KEY_ZOOM);
                }
                if i.key_pressed(Key::Minus) {
                    camera.zoom_at(center, 1.0 / KEY_ZOOM);
                }
            });
            if ui.input(|i| i.key_pressed(Key::Num0))
                && let Some(t) = &self.target
            {
                *camera = Camera::fit(SceneDesc::bounds(), f64::from(t.width), f64::from(t.height));
            }
        }
        if *camera != before {
            self.dirty = true;
        }
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            let touring = self.tour.is_some();
            if ui
                .add_enabled(!touring, egui::Button::new("Teste de fluidez (12 s)"))
                .clicked()
            {
                self.start_tour();
            }
            if ui.button("Mostrar a cena inteira").clicked()
                && let Some(t) = &self.target
            {
                self.camera = Some(Camera::fit(
                    SceneDesc::bounds(),
                    f64::from(t.width),
                    f64::from(t.height),
                ));
                self.dirty = true;
            }
            if ui.button("Exportar PNG (placa de vídeo)").clicked() {
                self.export_gpu();
            }
            if ui.button("Exportar PNG (sem placa de vídeo)").clicked() {
                self.export_cpu();
            }
            if ui.button("Capturar a tela e comparar").clicked() {
                self.start_capture(ui.ctx());
            }
        });
        let zoom = self.camera.map_or(0.0, |c| c.zoom * 100.0);
        ui.label(format!(
            "Zoom: {zoom:.0}%   ·   Último desenho (CPU, montar e enviar a cena): {:.1} ms",
            self.last_render_ms
        ));
        if let Some(s) = &self.tour_result {
            ui.label(format!(
                "Fluidez: {} quadros · média {:.2} ms · mediana {:.2} ms · 99% abaixo de {:.2} ms · pior {:.2} ms · \
                 acima de 16,7 ms (60 Hz): {} · acima de 6,9 ms (144 Hz): {}",
                s.frames, s.mean, s.p50, s.p99, s.worst, s.over_60_hz, s.over_144_hz
            ));
        }
        ui.label(&self.status);
    }

    fn export_gpu(&mut self) {
        let Some((camera, width, height)) = self.view() else {
            return;
        };
        let started = Instant::now();
        let result = self.gpu.render_to_image(
            &self.render_state.device,
            &self.render_state.queue,
            &self.world,
            &camera,
            width,
            height,
        );
        self.status = match result.and_then(|image| save(&image, "exportacao-placa.png")) {
            Ok(path) => format!(
                "PNG da placa de vídeo salvo em {} ({:.0} ms).",
                path.display(),
                started.elapsed().as_secs_f64() * 1000.0
            ),
            Err(e) => format!("Erro ao exportar: {e}"),
        };
    }

    fn export_cpu(&mut self) {
        let Some((camera, width, height)) = self.view() else {
            return;
        };
        let (Ok(w), Ok(h)) = (u16::try_from(width), u16::try_from(height)) else {
            self.status = "Canvas grande demais para o modo sem placa de vídeo.".into();
            return;
        };
        let started = Instant::now();
        let image = cpu::render(&self.desc, &self.font, &camera, w, h);
        let cpu_ms = started.elapsed().as_secs_f64() * 1000.0;
        let gpu_image = self.gpu.render_to_image(
            &self.render_state.device,
            &self.render_state.queue,
            &self.world,
            &camera,
            width,
            height,
        );
        let comparison = match gpu_image.and_then(|g| compare(&g, &image)) {
            Ok(d) => format!(
                "Diferença para a placa de vídeo: {:.2}% dos pixels, no máximo {} e em média {:.3} (escala 0–255).",
                d.differing_percent(),
                d.max_channel_delta,
                d.mean_channel_delta
            ),
            Err(e) => format!("Comparação falhou: {e}"),
        };
        self.status = match save(&image, "exportacao-sem-placa.png") {
            Ok(path) => format!(
                "PNG sem placa de vídeo salvo em {} ({cpu_ms:.0} ms). {comparison}",
                path.display()
            ),
            Err(e) => format!("Erro ao exportar: {e}"),
        };
    }

    fn view(&self) -> Option<(Camera, u32, u32)> {
        let target = self.target.as_ref()?;
        Some((self.camera?, target.width, target.height))
    }

    fn start_capture(&mut self, ctx: &egui::Context) {
        // The comparison needs the canvas to show the current camera.
        if self.dirty {
            self.render_canvas();
        }
        let Some(target) = &self.target else { return };
        let Some(camera) = self.camera else { return };
        match gpu::read_texture(self.device(), self.queue(), &target.texture) {
            Ok(displayed) => {
                let (sender, screenshot) = channel();
                let repaint = ctx.clone();
                ctx.request_screenshot(move |image| {
                    sender.send(image).ok();
                    repaint.request_repaint();
                });
                self.capture = Some(Capture {
                    origin: (0, 0),
                    camera,
                    displayed,
                    screenshot,
                });
                self.status = "Capturando a tela…".into();
            }
            Err(e) => self.status = format!("Erro ao ler o canvas: {e}"),
        }
    }

    fn finish_capture(&mut self) {
        let Some(screenshot) = self
            .capture
            .as_ref()
            .and_then(|c| c.screenshot.try_recv().ok())
        else {
            return;
        };
        let Some(capture) = self.capture.take() else {
            return;
        };
        let (x, y) = capture.origin;
        let (w, h) = (capture.displayed.width, capture.displayed.height);
        if self
            .target
            .as_ref()
            .is_none_or(|t| (t.width, t.height) != (w, h))
        {
            self.status = "O canvas mudou de tamanho durante a captura; tente de novo.".into();
            return;
        }
        let [sw, sh] = screenshot.size;
        if x + w > sw as u32 || y + h > sh as u32 {
            self.status = format!(
                "A captura ({sw}x{sh}) não cobre o canvas ({w}x{h} a partir de {x},{y}); tente de novo."
            );
            return;
        }
        let screen = color_image_to_rgba(&screenshot).crop(x, y, w, h);
        let export = self.gpu.render_to_image(
            &self.render_state.device,
            &self.render_state.queue,
            &self.world,
            &capture.camera,
            w,
            h,
        );
        let result = (|| -> Result<String, String> {
            let export = export?;
            let shown = compare(&capture.displayed, &screen)?;
            let fresh = compare(&export, &screen)?;
            let screen_path = save(&screen, "captura-tela.png")?;
            save(&export, "captura-exportacao.png")?;
            if !fresh.identical() {
                save(&diff_mask(&export, &screen), "captura-diferencas.png")?;
            }
            Ok(format!(
                "Tela × imagem que a placa desenhou: {}. Tela × nova exportação PNG: {}. Arquivos em {}.",
                verdict(
                    shown.identical(),
                    shown.differing_pixels,
                    shown.max_channel_delta
                ),
                if fresh.identical() {
                    "idênticas pixel a pixel".to_string()
                } else {
                    format!(
                        "{} pixels diferentes, no máximo {} em 255 ({})",
                        fresh.differing_pixels,
                        fresh.max_channel_delta,
                        if fresh.within(GPU_REPEAT_TOLERANCE) {
                            "dentro da variação medida do Vello"
                        } else {
                            "ACIMA da variação medida do Vello"
                        }
                    )
                },
                screen_path.parent().unwrap_or(&screen_path).display()
            ))
        })();
        self.status = result.unwrap_or_else(|e| format!("Erro na comparação: {e}"));
    }

    fn start_tour(&mut self) {
        self.tour = Some(Tour {
            start: Instant::now(),
            last_frame: None,
            times: FrameTimes::default(),
        });
        self.status = "Teste de fluidez em andamento…".into();
    }

    fn drive_self_test(&mut self, ctx: &egui::Context) {
        let Some(step) = self.auto else { return };
        ctx.request_repaint();
        self.auto = match step {
            AutoStep::Warmup(n) if n < AUTO_WARMUP_FRAMES => Some(AutoStep::Warmup(n + 1)),
            AutoStep::Warmup(_) => {
                self.start_tour();
                Some(AutoStep::Touring)
            }
            AutoStep::Touring if self.tour.is_some() => Some(step),
            AutoStep::Touring => Some(AutoStep::Settle(0)),
            AutoStep::Settle(n) if n < AUTO_SETTLE_FRAMES => {
                self.camera = self.target.as_ref().map(|t| {
                    Camera::fit(SceneDesc::bounds(), f64::from(t.width), f64::from(t.height))
                });
                self.dirty = true;
                Some(AutoStep::Settle(n + 1))
            }
            AutoStep::Settle(_) => {
                self.start_capture(ctx);
                Some(AutoStep::Capturing)
            }
            AutoStep::Capturing if self.capture.is_some() => Some(step),
            AutoStep::Capturing => {
                println!("{}", self.status_text_for_report());
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                None
            }
        };
    }

    fn status_text_for_report(&self) -> String {
        let info = self.render_state.adapter.get_info();
        let size = self.target.as_ref().map_or((0, 0), |t| (t.width, t.height));
        let tour = self.tour_result.map_or("sem dados".into(), |s| {
            format!(
                "{} quadros, média {:.2} ms, mediana {:.2} ms, p99 {:.2} ms, pior {:.2} ms, \
                 acima de 16,7 ms: {}, acima de 6,9 ms: {}",
                s.frames, s.mean, s.p50, s.p99, s.worst, s.over_60_hz, s.over_144_hz
            )
        });
        format!(
            "Placa: {} ({:?}); canvas {}x{} px\nFluidez: {tour}\nCaptura: {}",
            info.name, info.backend, size.0, size.1, self.status
        )
    }

    fn update_tour(&mut self, ctx: &egui::Context) {
        let (Some(tour), Some(target)) = (self.tour.as_mut(), &self.target) else {
            return;
        };
        let now = Instant::now();
        if let Some(last) = tour.last_frame {
            tour.times.push((now - last).as_secs_f64() * 1000.0);
        }
        tour.last_frame = Some(now);
        let seconds = (now - tour.start).as_secs_f64();
        self.camera = Some(camera::tour(
            seconds,
            SceneDesc::bounds(),
            f64::from(target.width),
            f64::from(target.height),
        ));
        self.dirty = true;
        if seconds >= camera::TOUR_SECONDS {
            self.tour_result = tour.times.summary();
            self.tour = None;
            self.status = "Teste de fluidez concluído.".into();
        } else {
            ctx.request_repaint();
        }
    }

    fn canvas(&mut self, ui: &mut Ui) {
        let ppp = f64::from(ui.ctx().pixels_per_point());
        let rect = ui.max_rect();
        // Snap the canvas to whole physical pixels so each texel is one pixel.
        let min = (
            (f64::from(rect.min.x) * ppp).ceil(),
            (f64::from(rect.min.y) * ppp).ceil(),
        );
        let max = (
            (f64::from(rect.max.x) * ppp).floor(),
            (f64::from(rect.max.y) * ppp).floor(),
        );
        let width = (max.0 - min.0).max(1.0) as u32;
        let height = (max.1 - min.1).max(1.0) as u32;
        self.ensure_target(width, height);
        if self.camera.is_none() {
            self.camera = Some(Camera::fit(
                SceneDesc::bounds(),
                f64::from(width),
                f64::from(height),
            ));
        }
        if let Some(capture) = &mut self.capture {
            capture.origin = (min.0 as u32, min.1 as u32);
        }

        let shown = Rect::from_min_max(
            pos2((min.0 / ppp) as f32, (min.1 / ppp) as f32),
            pos2(
                ((min.0 + f64::from(width)) / ppp) as f32,
                ((min.1 + f64::from(height)) / ppp) as f32,
            ),
        );
        let response = ui.interact(shown, ui.scope_id().with("canvas"), Sense::click_and_drag());
        response.widget_info(|| WidgetInfo::labeled(Role::Canvas, true, CANVAS_LABEL));
        ui.memory_mut(|m| {
            m.set_focus_lock_filter(
                response.id,
                EventFilter {
                    horizontal_arrows: true,
                    vertical_arrows: true,
                    ..Default::default()
                },
            );
        });
        if self.tour.is_none() {
            self.handle_input(ui, &response, ppp, Point::new(min.0, min.1));
        }
        self.update_tour(ui.ctx());
        if self.dirty {
            self.render_canvas();
        }

        let Some(target) = &self.target else { return };
        ui.painter().image(
            target.id,
            shown,
            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        if response.has_focus() {
            // Focus ring drawn outside the canvas, so it never covers a compared pixel.
            ui.painter().rect_stroke(
                shown.expand(1.0),
                0.0,
                egui::Stroke::new(2.0, Color32::from_rgb(0xb3, 0x8c, 0xff)),
                egui::StrokeKind::Outside,
            );
        }
    }
}

impl eframe::App for RenderApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        self.finish_capture();
        self.drive_self_test(ui.ctx());

        egui::Panel::top("toolbar")
            .frame(
                egui::Frame::new()
                    .inner_margin(egui::Margin::same(8))
                    .fill(Color32::from_rgb(0x24, 0x24, 0x2c)),
            )
            .show(ui, |ui| self.toolbar(ui));
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .inner_margin(egui::Margin::same(4))
                    .fill(Color32::from_rgb(0x12, 0x12, 0x16)),
            )
            .show(ui, |ui| self.canvas(ui));
    }
}

/// eframe runs without egui's default fonts (one is not OFL), so the UI uses
/// the same OTF font as the canvas.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::empty();
    fonts.font_data.insert(
        "source-sans-3".into(),
        FontData::from_static(text::SOURCE_SANS_3).into(),
    );
    for family in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts.families.insert(family, vec!["source-sans-3".into()]);
    }
    ctx.set_fonts(fonts);
}

fn color_image_to_rgba(image: &ColorImage) -> Rgba {
    let [w, h] = image.size;
    let pixels = image.pixels.iter().flat_map(|c| c.to_array()).collect();
    Rgba::new(w as u32, h as u32, pixels)
}

fn verdict(identical: bool, pixels: usize, delta: u8) -> String {
    if identical {
        "idênticas pixel a pixel".into()
    } else {
        format!("{pixels} pixels diferentes, no máximo {delta} em 255")
    }
}

fn exports_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_default()
        .join("exportacoes")
}

fn save(image: &Rgba, name: &str) -> Result<PathBuf, String> {
    let path = exports_dir().join(name);
    image.save_png(&path)?;
    Ok(path)
}

/// wgpu setup for eframe: same as egui's default, with the adapter's real
/// texture size limit (see [`gpu::device_descriptor`]).
pub fn wgpu_options() -> egui_wgpu::WgpuConfiguration {
    egui_wgpu::WgpuConfiguration {
        wgpu_setup: egui_wgpu::WgpuSetupCreateNew {
            device_descriptor: std::sync::Arc::new(|adapter| {
                gpu::device_descriptor(&adapter.limits())
            }),
            ..egui_wgpu::WgpuSetupCreateNew::without_display_handle()
        }
        .into(),
        ..Default::default()
    }
}
