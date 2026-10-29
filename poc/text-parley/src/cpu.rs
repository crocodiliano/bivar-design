//! Rendering without a GPU, with `vello_cpu`.

use std::sync::Arc;

use vello_cpu::kurbo::{Affine, Rect, Stroke};
use vello_cpu::peniko::{Blob, FontData};
use vello_cpu::{Glyph, Level, Pixmap, RenderContext, RenderSettings, Resources};

use crate::fonts::TestFont;
use crate::image::Rgba;
use crate::page::{BACKGROUND, Page};

pub fn font_data(font: TestFont) -> FontData {
    FontData::new(Blob::new(Arc::new(font.bytes())), 0)
}

/// The SIMD instruction set `vello_cpu` picks on this processor, such as
/// "Avx2". Different sets may round differently, so tests report it.
pub fn simd_level() -> String {
    level_name(RenderSettings::default().level)
}

pub fn level_name(level: Level) -> String {
    let name = format!("{level:?}");
    name.split('(').next().unwrap_or_default().to_string()
}

/// SSE4.2, when the processor has it. Obtained without `unsafe` by stepping
/// down from the detected level. Only x86 processors have it.
pub fn sse4_2_level() -> Option<Level> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    return RenderSettings::default()
        .level
        .as_sse4_2()
        .map(Level::Sse4_2);
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    None
}

/// AVX2, when the processor has it, even if it also has AVX-512. The
/// approved reference images were made at this level. Only x86 processors
/// have it; Apple and other ARM processors never do.
pub fn avx2_level() -> Option<Level> {
    #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
    return RenderSettings::default().level.as_avx2().map(Level::Avx2);
    #[cfg(not(any(target_arch = "x86", target_arch = "x86_64")))]
    None
}

/// True when this build's minimum SIMD level is SSE2, the plain x86-64
/// default. Building with `target-cpu` or `target-feature` raises it, and
/// vello_cpu would then silently raise a fixed level too (ADR-0020).
pub fn baseline_is_sse2() -> bool {
    level_name(Level::baseline()) == "Sse2"
}

/// Settings for the visual tests without a GPU: AVX2 and no worker threads,
/// so the pixels do not depend on which processor runs them (ADR-0020).
/// `None` when the processor has no AVX2; the test is then skipped with a
/// visible message.
pub fn reference_settings() -> Option<RenderSettings> {
    avx2_level().map(|level| RenderSettings {
        level,
        num_threads: 0,
    })
}

/// Renders `page` with the default settings, as the product would.
pub fn render(page: &Page) -> Rgba {
    render_with_settings(page, RenderSettings::default())
}

/// Renders `page` with a chosen SIMD level instead of the best one detected.
pub fn render_with_level(page: &Page, level: Level) -> Rgba {
    render_with_settings(
        page,
        RenderSettings {
            level,
            ..RenderSettings::default()
        },
    )
}

pub fn render_with_settings(page: &Page, settings: RenderSettings) -> Rgba {
    let (width, height) = (page.width, page.height);
    let mut ctx = RenderContext::new_with(width, height, settings);
    let mut resources = Resources::new();
    let fonts = TestFont::ALL.map(|font| (font, font_data(font)));

    ctx.set_paint(BACKGROUND);
    ctx.fill_rect(&Rect::new(0.0, 0.0, f64::from(width), f64::from(height)));

    ctx.set_stroke(Stroke::new(1.0));
    for frame in &page.frames {
        ctx.set_paint(frame.color);
        ctx.stroke_rect(&frame.rect);
    }

    for run in &page.runs {
        let (_, data) = fonts.iter().find(|(f, _)| *f == run.font).unwrap();
        let (x, y) = run.origin;
        ctx.set_transform(Affine::translate((f64::from(x), f64::from(y))));
        ctx.set_paint(run.color);
        ctx.glyph_run(&mut resources, data)
            .font_size(run.size)
            .hint(false)
            .atlas_cache(false)
            .fill_glyphs(run.glyphs.iter().map(|g| Glyph {
                id: g.id,
                x: g.x,
                y: g.y,
            }))
            .expect("outline glyphs always render");
    }
    ctx.set_transform(Affine::IDENTITY);

    ctx.flush();
    let mut pixmap = Pixmap::new(width, height);
    ctx.render(&mut pixmap, &mut resources);
    // The background is opaque, so premultiplied and straight alpha are equal.
    Rgba::new(
        u32::from(width),
        u32::from(height),
        pixmap.data_as_u8_slice().to_vec(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::single_line_page;
    use crate::shaping::Shaper;

    #[test]
    fn every_pixel_is_opaque_and_text_is_drawn() {
        let page = single_line_page(&mut Shaper::new(), TestFont::SourceSans3Otf, "Bivar");
        let image = render(&page);
        assert!(image.texels().iter().all(|p| p[3] == 255));
        let bg = BACKGROUND.to_rgba8().to_u8_array();
        let inked = image.texels().iter().filter(|p| **p != bg).count();
        assert!(inked > 200, "only {inked} pixels of text");
    }

    #[test]
    fn empty_page_is_only_background() {
        let page = single_line_page(&mut Shaper::new(), TestFont::SourceSerif4Ttf, "");
        let image = render(&page);
        let bg = BACKGROUND.to_rgba8().to_u8_array();
        assert!(image.texels().iter().all(|p| *p == bg));
    }

    #[test]
    fn simd_level_has_a_name() {
        let level = simd_level();
        assert!(!level.is_empty() && level.chars().all(|c| c.is_alphanumeric() || c == '_'));
    }

    #[test]
    fn stepped_down_levels_have_the_right_names() {
        if let Some(level) = sse4_2_level() {
            assert_eq!(level_name(level), "Sse4_2");
        }
        if let Some(level) = avx2_level() {
            assert_eq!(level_name(level), "Avx2");
        }
    }

    #[test]
    fn reference_settings_use_avx2_and_no_worker_threads() {
        match reference_settings() {
            Some(settings) => {
                assert_eq!(level_name(settings.level), "Avx2");
                assert_eq!(settings.num_threads, 0);
            }
            None => assert!(avx2_level().is_none(), "AVX2 present but not used"),
        }
    }

    #[test]
    fn reference_settings_draw_like_the_detected_level_on_this_machine() {
        // On the machines used so far the detected level is AVX2, so fixing
        // it must not change the pixels the owner approved.
        let Some(settings) = reference_settings() else {
            return;
        };
        if simd_level() != "Avx2" {
            return;
        }
        let page = single_line_page(&mut Shaper::new(), TestFont::SourceSans3Otf, "AVoca fi");
        assert_eq!(render(&page), render_with_settings(&page, settings));
    }

    #[test]
    fn every_level_draws_the_text() {
        let page = single_line_page(&mut Shaper::new(), TestFont::SourceSerif4Ttf, "Bivar");
        let bg = BACKGROUND.to_rgba8().to_u8_array();
        let levels = [Some(Level::baseline()), sse4_2_level(), avx2_level()];
        for level in levels.into_iter().flatten() {
            let image = render_with_level(&page, level);
            let inked = image.texels().iter().filter(|p| **p != bg).count();
            assert!(inked > 200, "{}: only {inked} pixels", level_name(level));
        }
    }
}
