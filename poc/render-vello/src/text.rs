//! Font loading and a minimal left-to-right glyph layout.
//!
//! There is no shaping (no ligatures or kerning): each character maps to one
//! glyph and advances by its width. Enough for this proof of concept.

use std::sync::Arc;

use skrifa::instance::{LocationRef, Size};
use skrifa::raw::{ReadError, TableProvider};
use skrifa::{FontRef, MetadataProvider};
use vello::peniko::{Blob, FontData};

pub const SOURCE_SANS_3: &[u8] = include_bytes!("../assets/fonts/SourceSans3-Regular.otf");

/// A glyph placed on a line, relative to the left end of the baseline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlacedGlyph {
    pub id: u32,
    pub x: f32,
}

pub struct Font {
    data: FontData,
}

impl Font {
    /// Parses the font (in Rust, with skrifa) before anything draws with it.
    pub fn load(bytes: &'static [u8]) -> Result<Self, ReadError> {
        FontRef::new(bytes)?;
        Ok(Self {
            data: FontData::new(Blob::new(Arc::new(bytes)), 0),
        })
    }

    pub fn source_sans_3() -> Self {
        Self::load(SOURCE_SANS_3).expect("the embedded font is valid")
    }

    pub fn data(&self) -> &FontData {
        &self.data
    }

    fn font_ref(&self) -> FontRef<'_> {
        FontRef::new(self.data.data.data()).expect("validated in Font::load")
    }

    /// True when the outlines are PostScript (CFF/CFF2), as in an OTF font.
    pub fn has_cff_outlines(&self) -> bool {
        let font = self.font_ref();
        font.cff().is_ok() || font.cff2().is_ok()
    }

    /// Places each character of `text` at `size` pixels per em.
    pub fn layout(&self, text: &str, size: f32) -> Vec<PlacedGlyph> {
        let font = self.font_ref();
        let charmap = font.charmap();
        let metrics = font.glyph_metrics(Size::new(size), LocationRef::default());
        let mut x = 0.0;
        text.chars()
            .map(|ch| {
                let id = charmap.map(ch).unwrap_or_default();
                let glyph = PlacedGlyph { id: id.to_u32(), x };
                x += metrics.advance_width(id).unwrap_or_default();
                glyph
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_font_is_otf_with_cff_outlines() {
        assert_eq!(&SOURCE_SANS_3[..4], b"OTTO");
        assert!(Font::source_sans_3().has_cff_outlines());
    }

    #[test]
    fn rejects_bytes_that_are_not_a_font() {
        assert!(Font::load(b"not a font at all").is_err());
    }

    #[test]
    fn every_test_character_has_a_glyph() {
        let font = Font::source_sans_3();
        for glyph in font.layout("Bivar AaBbGgQq 0123 ção —", 16.0) {
            assert_ne!(glyph.id, 0, "character fell back to .notdef");
        }
    }

    #[test]
    fn glyphs_advance_left_to_right_and_scale_with_size() {
        let font = Font::source_sans_3();
        let small = font.layout("AaBb", 10.0);
        let big = font.layout("AaBb", 20.0);
        assert_eq!(small[0].x, 0.0);
        for pair in small.windows(2) {
            assert!(pair[1].x > pair[0].x);
        }
        for (s, b) in small.iter().zip(&big) {
            assert_eq!(s.id, b.id);
            assert!((b.x - 2.0 * s.x).abs() < 1e-3);
        }
    }

    #[test]
    fn unknown_character_uses_notdef_without_panicking() {
        let font = Font::source_sans_3();
        let glyphs = font.layout("\u{10FFFD}", 16.0);
        assert_eq!(glyphs, [PlacedGlyph { id: 0, x: 0.0 }]);
    }
}
