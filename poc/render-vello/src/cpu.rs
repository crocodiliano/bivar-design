//! Rendering without a GPU, with `vello_cpu`.

use vello_cpu::kurbo::{Affine, Rect, Shape, Stroke};
use vello_cpu::peniko::Gradient;
use vello_cpu::{Glyph, Pixmap, RenderContext, Resources};

use crate::camera::Camera;
use crate::image::Rgba;
use crate::scene::{BACKGROUND, SceneDesc, TEXT_COLOR};
use crate::text::Font;

/// Path flattening tolerance, in world units.
const TOLERANCE: f64 = 0.1;

/// Renders `desc` through `camera` into a `width` × `height` image.
pub fn render(desc: &SceneDesc, font: &Font, camera: &Camera, width: u16, height: u16) -> Rgba {
    let view = camera.transform();
    let mut ctx = RenderContext::new(width, height);
    let mut resources = Resources::new();

    ctx.set_paint(BACKGROUND);
    ctx.fill_rect(&Rect::new(0.0, 0.0, f64::from(width), f64::from(height)));

    ctx.set_transform(view);
    for tile in &desc.tiles {
        let path = tile.shape.to_path(TOLERANCE);
        ctx.set_paint(
            Gradient::new_linear(tile.gradient_start, tile.gradient_end).with_stops(tile.colors),
        );
        ctx.fill_path(&path);
        ctx.set_stroke(Stroke::new(tile.stroke_width));
        ctx.set_paint(tile.stroke);
        ctx.stroke_path(&path);
    }

    ctx.set_paint(TEXT_COLOR);
    for line in &desc.lines {
        let glyphs = font.layout(&line.text, line.size);
        ctx.set_transform(view * Affine::translate(line.origin.to_vec2()));
        ctx.glyph_run(&mut resources, font.data())
            .font_size(line.size)
            .hint(false)
            .atlas_cache(false)
            .fill_glyphs(glyphs.iter().map(|g| Glyph {
                id: g.id,
                x: g.x,
                y: 0.0,
            }))
            .expect("outline glyphs always render");
    }

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

    fn small_scene() -> SceneDesc {
        let mut desc = SceneDesc::test_scene();
        desc.tiles.truncate(50);
        desc
    }

    #[test]
    fn every_pixel_is_opaque() {
        let desc = small_scene();
        let camera = Camera::fit(SceneDesc::bounds(), 200.0, 150.0);
        let image = render(&desc, &Font::source_sans_3(), &camera, 200, 150);
        assert!(image.texels().iter().all(|p| p[3] == 255));
    }

    #[test]
    fn rendering_twice_gives_the_same_pixels() {
        let desc = small_scene();
        let font = Font::source_sans_3();
        let camera = Camera::fit(SceneDesc::bounds(), 120.0, 90.0);
        assert_eq!(
            render(&desc, &font, &camera, 120, 90),
            render(&desc, &font, &camera, 120, 90)
        );
    }

    #[test]
    fn empty_view_is_only_background() {
        let desc = small_scene();
        let camera = Camera {
            zoom: 1.0,
            pan: vello_cpu::kurbo::Vec2::new(-1.0e6, -1.0e6),
        };
        let image = render(&desc, &Font::source_sans_3(), &camera, 16, 16);
        let bg = BACKGROUND.to_rgba8().to_u8_array();
        assert!(image.texels().iter().all(|p| *p == bg));
    }
}
