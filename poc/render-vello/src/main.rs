use std::path::Path;
use std::process::ExitCode;
use std::time::Instant;

use bivar_poc_render_vello::app::{self, RenderApp};
use bivar_poc_render_vello::camera::Camera;
use bivar_poc_render_vello::gpu::{self, Gpu, Headless};
use bivar_poc_render_vello::scene::SceneDesc;
use bivar_poc_render_vello::text::Font;
use bivar_poc_render_vello::{cpu, image::Rgba};
use eframe::egui;

const EXPORT_WIDTH: u16 = 1600;
const EXPORT_HEIGHT: u16 = 1000;

const USAGE: &str = "Uso:
  bivar-poc-render-vello                 abre a janela
  bivar-poc-render-vello --autoteste     abre a janela, mede a fluidez, compara tela e PNG e fecha
  bivar-poc-render-vello --degrades-distintos N
                                         os 10.000 retângulos usam só N degradês diferentes
  bivar-poc-render-vello --cpu ARQ.png   exporta a cena sem placa de vídeo
  bivar-poc-render-vello --gpu ARQ.png   exporta a cena com a placa de vídeo, sem janela";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.as_slice() {
        [flag, path] if flag == "--cpu" => export(path, false),
        [flag, path] if flag == "--gpu" => export(path, true),
        _ => window_options(&args).and_then(run_window),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn window_options(args: &[String]) -> Result<app::Options, String> {
    let mut options = app::Options::default();
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--autoteste" => options.self_test = true,
            "--degrades-distintos" => {
                let n = args.next().and_then(|n| n.parse().ok()).ok_or(USAGE)?;
                options.distinct_gradients = Some(n);
            }
            _ => return Err(USAGE.to_string()),
        }
    }
    Ok(options)
}

fn run_window(app_options: app::Options) -> Result<(), String> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Bivar — prova de conceito do renderizador")
            .with_inner_size([1280.0, 860.0])
            .with_min_inner_size([640.0, 480.0]),
        renderer: eframe::Renderer::Wgpu,
        // Dithering would change canvas pixels and break the screen/PNG comparison.
        dithering: false,
        wgpu_options: app::wgpu_options(),
        ..Default::default()
    };
    eframe::run_native(
        "bivar-poc-render-vello",
        options,
        Box::new(move |cc| Ok(Box::new(RenderApp::new(cc, app_options)?))),
    )
    .map_err(|e| e.to_string())
}

fn export(path: &str, with_gpu: bool) -> Result<(), String> {
    let desc = SceneDesc::test_scene();
    let font = Font::source_sans_3();
    let camera = Camera::fit(
        SceneDesc::bounds(),
        f64::from(EXPORT_WIDTH),
        f64::from(EXPORT_HEIGHT),
    );
    let started = Instant::now();
    let (image, engine): (Rgba, String) = if with_gpu {
        let headless = Headless::new()?;
        let mut gpu = Gpu::new(&headless.device).map_err(|e| e.to_string())?;
        let image = gpu.render_to_image(
            &headless.device,
            &headless.queue,
            &gpu::world_scene(&desc, &font),
            &camera,
            u32::from(EXPORT_WIDTH),
            u32::from(EXPORT_HEIGHT),
        )?;
        (image, format!("placa de vídeo: {}", headless.adapter_name))
    } else {
        let image = cpu::render(&desc, &font, &camera, EXPORT_WIDTH, EXPORT_HEIGHT);
        (image, "sem placa de vídeo (vello_cpu)".into())
    };
    let millis = started.elapsed().as_secs_f64() * 1000.0;
    image.save_png(Path::new(path))?;
    println!("Salvo em {path} ({EXPORT_WIDTH}x{EXPORT_HEIGHT}, {engine}, {millis:.0} ms).");
    Ok(())
}
