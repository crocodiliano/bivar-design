//! Visual (golden image) tests, with and without a GPU.
//!
//! A missing or different reference image makes the test fail and writes
//! `<name>.new.png` (and `<name>.diff.png`) next to it. Only the owner
//! approves turning it into the reference.
//!
//! GPU tests are ignored by default because GitHub has no GPU. On the owner's
//! PC: `cargo test --manifest-path poc/text-parley/Cargo.toml -- --include-ignored`

use std::path::PathBuf;

use bivar_poc_text_parley::cpu;
use bivar_poc_text_parley::fonts::TestFont;
use bivar_poc_text_parley::image::{Rgba, Tolerance, compare, diff_mask};
use bivar_poc_text_parley::page::{
    self, ACCENTS_COMBINING, ACCENTS_PRECOMPOSED, BACKGROUND, Page, single_line_page,
};
use bivar_poc_text_parley::shaping::Shaper;

const EXACT: Tolerance = Tolerance {
    max_channel_delta: 0,
    max_differing_pixels: 0,
};

fn golden_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn criteria_page(font: TestFont) -> Page {
    page::criteria_page(&mut Shaper::new(), font)
}

/// Renders without a GPU at the fixed level of the reference images: AVX2,
/// no worker threads (ADR-0020). On a processor without AVX2 (Apple or other
/// ARM chips) it returns `None` after printing why; the test is then skipped
/// there and stays mandatory on GitHub and on the owner's PC.
fn reference_render(name: &str, page: &Page) -> Option<Rgba> {
    let Some(settings) = cpu::reference_settings() else {
        eprintln!(
            "SKIPPED {name}: this processor has no AVX2 (detected {}), so its pixels \
             cannot match the reference images. This test runs on GitHub and on the \
             owner's PC.",
            cpu::simd_level()
        );
        return None;
    };
    #[cfg(target_arch = "x86_64")]
    assert!(
        cpu::baseline_is_sse2(),
        "{name}: built with target-cpu or target-feature, which would silently raise the \
         fixed AVX2 level (ADR-0020); build without them"
    );
    Some(cpu::render_with_settings(page, settings))
}

fn check_golden(name: &str, image: &Rgba, tolerance: Tolerance) {
    let reference = golden_dir().join(format!("{name}.png"));
    let candidate = golden_dir().join(format!("{name}.new.png"));
    let mask = golden_dir().join(format!("{name}.diff.png"));
    let level = format!(
        "vello_cpu at fixed AVX2; this processor detects {}",
        cpu::simd_level()
    );
    match Rgba::load_png(&reference) {
        Ok(expected) => {
            let diff = compare(&expected, image);
            if diff.as_ref().is_ok_and(|d| d.within(tolerance)) {
                std::fs::remove_file(&candidate).ok();
                std::fs::remove_file(&mask).ok();
                return;
            }
            image.save_png(&candidate).unwrap();
            if diff.is_ok() {
                diff_mask(&expected, image).save_png(&mask).unwrap();
            }
            panic!(
                "{name}: differs from the reference ({diff:?}, {level}); new image at {}",
                candidate.display()
            );
        }
        Err(_) => {
            image.save_png(&candidate).unwrap();
            panic!(
                "{name}: no reference image yet ({level}); \
                 candidate at {} for the owner to review",
                candidate.display()
            );
        }
    }
}

fn background_pixel() -> [u8; 4] {
    BACKGROUND.to_rgba8().to_u8_array()
}

// --- Without a GPU (vello_cpu): exact comparison, here and on GitHub. ---

#[test]
fn cpu_golden_otf_source_sans_3() {
    let name = "cpu_otf_source_sans_3";
    if let Some(image) = reference_render(name, &criteria_page(TestFont::SourceSans3Otf)) {
        check_golden(name, &image, EXACT);
    }
}

#[test]
fn cpu_golden_ttf_source_serif_4() {
    let name = "cpu_ttf_source_serif_4";
    if let Some(image) = reference_render(name, &criteria_page(TestFont::SourceSerif4Ttf)) {
        check_golden(name, &image, EXACT);
    }
}

#[test]
fn cpu_rendering_twice_gives_the_same_pixels() {
    let page = criteria_page(TestFont::SourceSerif4Ttf);
    assert_eq!(cpu::render(&page), cpu::render(&page));
}

#[test]
fn cpu_accents_render_identically_in_both_spellings() {
    let mut shaper = Shaper::new();
    for font in TestFont::ALL {
        let nfc = cpu::render(&single_line_page(&mut shaper, font, ACCENTS_PRECOMPOSED));
        let nfd = cpu::render(&single_line_page(&mut shaper, font, ACCENTS_COMBINING));
        assert!(nfc.texels().iter().any(|p| *p != background_pixel()));
        let diff = compare(&nfc, &nfd).unwrap();
        assert!(diff.identical(), "{font:?}: {diff:?}");
    }
}

#[test]
fn cpu_ligature_switch_changes_the_pixels() {
    let mut shaper = Shaper::new();
    for font in TestFont::ALL {
        let text = font.ligature_samples()[0];
        let on = single_line_page(&mut shaper, font, text);
        let mut off = on.clone();
        let separate = shaper.shape(
            font,
            text,
            bivar_poc_text_parley::shaping::ShapeOptions {
                ligatures: false,
                ..bivar_poc_text_parley::shaping::ShapeOptions::default_features(40.0)
            },
        );
        off.runs[0].glyphs = separate.runs[0].glyphs.clone();
        assert_eq!(off.runs[0].glyphs.len(), on.runs[0].glyphs.len() + 1);
        let diff = compare(&cpu::render(&on), &cpu::render(&off)).unwrap();
        assert!(
            !diff.identical(),
            "{font:?}: {text} looks the same either way"
        );
    }
}

/// Proves the check used between GPU and CPU images: dropping a single
/// glyph, even a letter of a 16 px grey caption, leaves pixels that differ
/// by half the range or more.
#[test]
fn strong_difference_catches_one_missing_letter() {
    for font in TestFont::ALL {
        let page = criteria_page(font);
        let full = cpu::render(&page);
        for (run_index, glyph_index) in [(0, 0), (1, 3), (5, 0)] {
            let mut missing = page.clone();
            missing.runs[run_index].glyphs.remove(glyph_index);
            let diff = compare(&full, &cpu::render(&missing)).unwrap();
            assert!(
                diff.strongly_differing_pixels > 0,
                "{font:?}: run {run_index}, glyph {glyph_index} not detected: {diff:?}"
            );
        }
    }
}

// --- With a GPU (Vello): tolerance from ADR-0018, owner's PC only. ---

#[cfg(feature = "gpu")]
mod gpu_tests {
    use std::sync::{Mutex, MutexGuard, OnceLock};

    use bivar_poc_text_parley::gpu::Headless;

    use super::*;

    /// Vello on the GPU is not bit-for-bit repeatable: up to 3 pixels may
    /// differ by 1 level of 255 (ADR-0018, limitation 3).
    const GPU_REPEAT_TOLERANCE: Tolerance = Tolerance {
        max_channel_delta: 1,
        max_differing_pixels: 3,
    };

    /// Rendering on the GPU from several tests at once crashed on the
    /// reference machine (ADR-0017, limitation 4), so tests take turns.
    static GPU: OnceLock<Mutex<Headless>> = OnceLock::new();

    fn gpu_turn() -> MutexGuard<'static, Headless> {
        GPU.get_or_init(|| Mutex::new(Headless::new().expect("a GPU adapter")))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn render(page: &Page) -> Rgba {
        gpu_turn().render(page).expect("GPU render")
    }

    #[test]
    #[ignore = "needs a GPU; run with --include-ignored"]
    fn gpu_golden_otf_source_sans_3() {
        let image = render(&criteria_page(TestFont::SourceSans3Otf));
        check_golden("gpu_otf_source_sans_3", &image, GPU_REPEAT_TOLERANCE);
    }

    #[test]
    #[ignore = "needs a GPU; run with --include-ignored"]
    fn gpu_golden_ttf_source_serif_4() {
        let image = render(&criteria_page(TestFont::SourceSerif4Ttf));
        check_golden("gpu_ttf_source_serif_4", &image, GPU_REPEAT_TOLERANCE);
    }

    #[test]
    #[ignore = "needs a GPU; run with --include-ignored"]
    fn gpu_accents_render_identically_in_both_spellings() {
        let mut shaper = Shaper::new();
        for font in TestFont::ALL {
            let nfc = render(&single_line_page(&mut shaper, font, ACCENTS_PRECOMPOSED));
            let nfd = render(&single_line_page(&mut shaper, font, ACCENTS_COMBINING));
            assert!(nfc.texels().iter().any(|p| *p != background_pixel()));
            let diff = compare(&nfc, &nfd).unwrap();
            assert!(diff.within(GPU_REPEAT_TOLERANCE), "{font:?}: {diff:?}");
        }
    }

    /// The CPU and GPU differ at the anti-aliased edges (ADR-0018), by at
    /// most 72 of 255 here. No pixel may differ by half the range or more:
    /// that is what a glyph drawn by one and not the other looks like (see
    /// `strong_difference_catches_one_missing_letter`).
    #[test]
    #[ignore = "needs a GPU; run with --include-ignored"]
    fn gpu_and_cpu_draw_the_same_text() {
        for font in TestFont::ALL {
            let page = criteria_page(font);
            let diff = compare(&render(&page), &cpu::render(&page)).unwrap();
            eprintln!("{font:?}: GPU vs CPU {diff:?}");
            assert_eq!(diff.strongly_differing_pixels, 0, "{font:?}: {diff:?}");
        }
    }
}
