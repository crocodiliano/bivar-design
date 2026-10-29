//! Rendering without a GPU, with `vello_cpu`.

use std::sync::Arc;

use vello_cpu::kurbo::{Affine, Rect, Stroke};
use vello_cpu::peniko::{Blob, FontData};
use vello_cpu::{Glyph, Pixmap, RenderContext, RenderSettings, Resources};

use crate::fonts::TestFont;
use crate::image::Rgba;
use crate::page::{BACKGROUND, Page};

pub fn font_data(font: TestFont) -> FontData {
    FontData::new(Blob::new(Arc::new(font.bytes())), 0)
}

/// The SIMD instruction set `vello_cpu` picks on this processor, such as
/// "Avx2". Different sets may round differently, so tests report it.
pub fn simd_level() -> String {
    let level = format!("{:?}", RenderSettings::default().level);
    level.split('(').next().unwrap_or_default().to_string()
}

/// Renders `page` with the default settings, as the product would.
pub fn render(page: &Page) -> Rgba {
    let (width, height) = (page.width, page.height);
    let mut ctx = RenderContext::new(width, height);
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
}
