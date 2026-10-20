//! GPU and CPU rendering tests, including visual (golden image) tests.
//!
//! A missing or different reference image makes the test fail and writes
//! `<name>.new.png` next to it. Only the owner approves turning it into the
//! reference.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

use bivar_poc_render_vello::camera::Camera;
use bivar_poc_render_vello::gpu::{self, Gpu, Headless};
use bivar_poc_render_vello::image::{GPU_REPEAT_TOLERANCE, Rgba, Tolerance, compare, diff_mask};
use bivar_poc_render_vello::scene::SceneDesc;
use bivar_poc_render_vello::text::Font;
use bivar_poc_render_vello::{cpu, scene};
use vello::kurbo::{Point, Vec2};

const WIDTH: u32 = 1280;
const HEIGHT: u32 = 800;

/// GPU rendering crashed when several tests rendered at once on the reference
/// machine in phase 0.2 (ADR-0017, limitation 4), so tests that render take
/// turns and share one device.
static GPU: OnceLock<Mutex<(Headless, Gpu)>> = OnceLock::new();

fn gpu_turn() -> MutexGuard<'static, (Headless, Gpu)> {
    GPU.get_or_init(|| {
        let headless = Headless::new().expect("a GPU adapter");
        let gpu = Gpu::new(&headless.device).expect("Vello renderer");
        Mutex::new((headless, gpu))
    })
    .lock()
    .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn render_gpu(camera: &Camera) -> Rgba {
    render_gpu_scene(
        &gpu::world_scene(&SceneDesc::test_scene(), &Font::source_sans_3()),
        camera,
    )
}

fn render_gpu_scene(world: &vello::Scene, camera: &Camera) -> Rgba {
    let mut turn = gpu_turn();
    let (headless, gpu) = &mut *turn;
    gpu.render_to_image(
        &headless.device,
        &headless.queue,
        world,
        camera,
        WIDTH,
        HEIGHT,
    )
    .expect("GPU render")
}

fn render_cpu(camera: &Camera) -> Rgba {
    cpu::render(
        &SceneDesc::test_scene(),
        &Font::source_sans_3(),
        camera,
        WIDTH as u16,
        HEIGHT as u16,
    )
}

fn whole_scene() -> Camera {
    Camera::fit(SceneDesc::bounds(), f64::from(WIDTH), f64::from(HEIGHT))
}

/// 800% zoom on the start of the 12 px text line.
fn text_zoom() -> Camera {
    let line = SceneDesc::test_scene()
        .lines
        .into_iter()
        .find(|l| l.size == 12.0)
        .unwrap();
    let zoom = 8.0;
    let screen = Point::new(40.0, f64::from(HEIGHT) / 2.0);
    Camera {
        zoom,
        pan: screen.to_vec2() - line.origin.to_vec2() * zoom,
    }
}

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn out_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
}

const EXACT: Tolerance = Tolerance {
    max_channel_delta: 0,
    max_differing_pixels: 0,
};

fn check_golden(name: &str, image: &Rgba, tolerance: Tolerance) {
    let reference = golden_dir().join(format!("{name}.png"));
    let candidate = golden_dir().join(format!("{name}.new.png"));
    match Rgba::load_png(&reference) {
        Ok(expected) => {
            let diff = compare(&expected, image);
            if diff.as_ref().is_ok_and(|d| d.within(tolerance)) {
                std::fs::remove_file(&candidate).ok();
                return;
            }
            image.save_png(&candidate).unwrap();
            panic!(
                "{name}: differs from the reference ({diff:?}); new image at {}",
                candidate.display()
            );
        }
        Err(_) => {
            image.save_png(&candidate).unwrap();
            panic!(
                "{name}: no approved reference yet; candidate at {}",
                candidate.display()
            );
        }
    }
}

#[test]
fn golden_gpu_whole_scene() {
    check_golden(
        "gpu_cena_inteira",
        &render_gpu(&whole_scene()),
        GPU_REPEAT_TOLERANCE,
    );
}

#[test]
fn golden_gpu_text_zoom() {
    check_golden(
        "gpu_texto_zoom_800",
        &render_gpu(&text_zoom()),
        GPU_REPEAT_TOLERANCE,
    );
}

#[test]
fn golden_cpu_whole_scene() {
    check_golden("cpu_cena_inteira", &render_cpu(&whole_scene()), EXACT);
}

#[test]
fn golden_cpu_text_zoom() {
    check_golden("cpu_texto_zoom_800", &render_cpu(&text_zoom()), EXACT);
}

/// The window and the PNG export use the same renderer; two renders of the
/// same view may differ only by Vello's measured GPU non-determinism.
#[test]
fn gpu_render_is_repeatable() {
    let camera = whole_scene();
    let first = render_gpu(&camera);
    for _ in 0..10 {
        let diff = compare(&first, &render_gpu(&camera)).unwrap();
        assert!(diff.within(GPU_REPEAT_TOLERANCE), "{diff:?}");
    }
}

/// Encoding the world once and applying the view per frame draws the same
/// picture as encoding every item with the view.
#[test]
fn reusing_the_world_scene_draws_the_same_picture() {
    let desc = SceneDesc::test_scene();
    let font = Font::source_sans_3();
    for camera in [whole_scene(), text_zoom()] {
        let reused = render_gpu(&camera);
        let direct = render_gpu_scene(
            &gpu::encode(&desc, &font, camera.transform()),
            &Camera {
                zoom: 1.0,
                pan: Vec2::ZERO,
            },
        );
        let diff = compare(&reused, &direct).unwrap();
        assert!(diff.within(GPU_REPEAT_TOLERANCE), "{diff:?}");
    }
}

#[test]
fn cpu_render_is_exactly_repeatable() {
    let camera = whole_scene();
    assert_eq!(render_cpu(&camera), render_cpu(&camera));
}

/// Measures how far the CPU renderer is from the GPU one and saves a mask of
/// the differing pixels. They are different engines of the same family, so
/// small anti-aliasing differences are expected; large ones are not.
#[test]
fn cpu_is_close_to_gpu() {
    for (name, camera) in [("cena_inteira", whole_scene()), ("texto_zoom", text_zoom())] {
        let gpu = render_gpu(&camera);
        let cpu = render_cpu(&camera);
        let diff = compare(&gpu, &cpu).unwrap();
        diff_mask(&gpu, &cpu)
            .save_png(&out_dir().join(format!("cpu_vs_gpu_{name}.png")))
            .unwrap();
        println!(
            "{name}: {:.2}% of pixels differ, max {} and mean {:.3} per channel (0-255)",
            diff.differing_percent(),
            diff.max_channel_delta,
            diff.mean_channel_delta
        );
        assert!(diff.mean_channel_delta < 2.0, "{name}: {diff:?}");
    }
}

#[test]
fn background_matches_between_engines() {
    let far = Camera {
        zoom: 1.0,
        pan: Vec2::new(-1.0e6, -1.0e6),
    };
    let gpu = render_gpu(&far);
    let cpu = render_cpu(&far);
    let bg = scene::BACKGROUND.to_rgba8().to_u8_array();
    assert!(gpu.texels().iter().all(|p| *p == bg));
    assert_eq!(gpu, cpu);
}
