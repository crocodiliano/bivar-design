use bivar_poc_ui_egui::app::PocApp;
use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Bivar — prova de conceito da interface")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 600.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "bivar-poc-ui-egui",
        options,
        Box::new(|cc| Ok(Box::new(PocApp::new(&cc.egui_ctx)))),
    )
}
