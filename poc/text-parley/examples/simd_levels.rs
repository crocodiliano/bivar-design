//! Measures how much the vello_cpu SIMD level changes the pixels: renders the
//! two criteria pages at each level this processor offers and compares them
//! with the approved reference images (made at AVX2).
//!
//! cargo run --release --manifest-path poc/text-parley/Cargo.toml --example simd_levels

use std::path::PathBuf;
use std::time::Instant;

use bivar_poc_text_parley::cpu;
use bivar_poc_text_parley::fonts::TestFont;
use bivar_poc_text_parley::image::{Rgba, compare};
use bivar_poc_text_parley::page;
use bivar_poc_text_parley::shaping::Shaper;
use vello_cpu::{Level, RenderSettings};

fn main() {
    let golden = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let levels = [
        Some(RenderSettings::default().level),
        cpu::avx2_level(),
        cpu::sse4_2_level(),
        Some(Level::baseline()),
    ];
    println!("Detected: {}", cpu::simd_level());
    let mut shaper = Shaper::new();
    for font in TestFont::ALL {
        let page = page::criteria_page(&mut shaper, font);
        let path = golden.join(format!("cpu_{}.png", font.slug()));
        let reference = Rgba::load_png(&path).expect("approved reference image");
        for level in levels.into_iter().flatten() {
            let start = Instant::now();
            let image = cpu::render_with_level(&page, level);
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            let diff = compare(&reference, &image).expect("same size");
            println!(
                "{font:?} at {}: {} pixels differ, largest by {} of 255 ({ms:.1} ms)",
                cpu::level_name(level),
                diff.differing_pixels,
                diff.max_channel_delta
            );
        }
    }
}
