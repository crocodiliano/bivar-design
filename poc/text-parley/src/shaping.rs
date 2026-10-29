//! Shapes text with Parley: the font's rules pick the glyphs (ligatures,
//! composed accents) and place them (kerning). Paragraphs are broken into
//! lines at a fixed width.

use std::borrow::Cow;

use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamilyName, FontFeatures, Layout, LayoutContext,
    PositionedLayoutItem, StyleProperty,
};

use crate::fonts::{TestFont, font_context};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapeOptions {
    /// Font size in pixels per em.
    pub size: f32,
    /// Standard and contextual ligatures (`liga`, `clig`). On by default in
    /// every font, as in CSS.
    pub ligatures: bool,
    /// Pair kerning (`kern`). On by default in every font, as in CSS.
    pub kerning: bool,
}

impl ShapeOptions {
    /// Ligatures and kerning on, as fonts are meant to be used.
    pub const fn default_features(size: f32) -> Self {
        Self {
            size,
            ligatures: true,
            kerning: true,
        }
    }

    fn feature_settings(self) -> Option<&'static str> {
        match (self.ligatures, self.kerning) {
            (true, true) => None,
            (false, true) => Some("\"liga\" 0, \"clig\" 0"),
            (true, false) => Some("\"kern\" 0"),
            (false, false) => Some("\"liga\" 0, \"clig\" 0, \"kern\" 0"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    /// Flush left, ragged right.
    Start,
    /// Both edges flush; the last line stays flush left.
    Justify,
}

/// A glyph placed relative to the left end of its line's baseline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShapedGlyph {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    /// Distance to the next glyph, kerning included.
    pub advance: f32,
}

/// Consecutive glyphs drawn with the same font.
#[derive(Clone, Debug, PartialEq)]
pub struct ShapedRun {
    /// The font Parley actually used. It can differ from the requested one
    /// when that font lacks a character, so callers can see it.
    pub font: TestFont,
    pub size: f32,
    pub glyphs: Vec<ShapedGlyph>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShapedLine {
    pub runs: Vec<ShapedRun>,
    /// Width of the line's content.
    pub width: f32,
    /// Height of the line box.
    pub line_height: f32,
    /// Distance from the top of the text (the first line box) down to this
    /// line's baseline.
    pub baseline: f32,
}

impl ShapedLine {
    pub fn glyphs(&self) -> impl Iterator<Item = &ShapedGlyph> {
        self.runs.iter().flat_map(|run| &run.glyphs)
    }

    pub fn glyph_ids(&self) -> Vec<u32> {
        self.glyphs().map(|g| g.id).collect()
    }

    /// True when every run uses `font`.
    pub fn uses_only(&self, font: TestFont) -> bool {
        self.runs.iter().all(|run| run.font == font)
    }
}

/// Owns Parley's font database and scratch space, reused between lines.
pub struct Shaper {
    font_cx: FontContext,
    layout_cx: LayoutContext<()>,
}

impl Default for Shaper {
    fn default() -> Self {
        Self::new()
    }
}

impl Shaper {
    pub fn new() -> Self {
        Self {
            font_cx: font_context(),
            layout_cx: LayoutContext::new(),
        }
    }

    /// Shapes `text` as a single line (no wrapping).
    pub fn shape(&mut self, font: TestFont, text: &str, options: ShapeOptions) -> ShapedLine {
        let mut lines = self.layout(font, text, options, None, Align::Start);
        // Parley always makes at least one line, even for empty text.
        let line = lines.swap_remove(0);
        debug_assert!(lines.is_empty(), "no wrapping without a width");
        line
    }

    /// Breaks `text` into lines no wider than `max_width` pixels.
    pub fn shape_paragraph(
        &mut self,
        font: TestFont,
        text: &str,
        options: ShapeOptions,
        max_width: f32,
        align: Align,
    ) -> Vec<ShapedLine> {
        self.layout(font, text, options, Some(max_width), align)
    }

    fn layout(
        &mut self,
        font: TestFont,
        text: &str,
        options: ShapeOptions,
        max_width: Option<f32>,
        align: Align,
    ) -> Vec<ShapedLine> {
        // Positions are not rounded to whole pixels, so they can be compared
        // with the values in the font.
        let mut builder = self
            .layout_cx
            .ranged_builder(&mut self.font_cx, text, 1.0, false);
        builder.push_default(FontFamilyName::Named(Cow::Borrowed(font.family())));
        builder.push_default(StyleProperty::FontSize(options.size));
        if let Some(settings) = options.feature_settings() {
            builder.push_default(FontFeatures::from(settings));
        }
        let mut layout: Layout<()> = builder.build(text);
        layout.break_all_lines(max_width);
        let alignment = match align {
            Align::Start => Alignment::Start,
            Align::Justify => Alignment::Justify,
        };
        layout.align(alignment, AlignmentOptions::default());

        layout
            .lines()
            .map(|line| {
                let metrics = line.metrics();
                let mut runs = Vec::new();
                for item in line.items() {
                    let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                        continue;
                    };
                    let run = glyph_run.run();
                    let baseline = glyph_run.baseline();
                    runs.push(ShapedRun {
                        font: font_of_run(run.font().data.data()),
                        size: run.font_size(),
                        glyphs: glyph_run
                            .positioned_glyphs()
                            .map(|g| ShapedGlyph {
                                id: g.id,
                                x: g.x,
                                y: g.y - baseline,
                                advance: g.advance,
                            })
                            .collect(),
                    });
                }
                ShapedLine {
                    runs,
                    width: metrics.advance,
                    line_height: metrics.line_height,
                    baseline: metrics.baseline,
                }
            })
            .collect()
    }
}

fn font_of_run(bytes: &[u8]) -> TestFont {
    TestFont::ALL
        .into_iter()
        .find(|font| font.bytes() == bytes)
        .expect("only the embedded fonts are registered")
}

#[cfg(test)]
mod tests {
    use super::*;

    const OPTIONS: ShapeOptions = ShapeOptions::default_features(32.0);

    const PARAGRAPH: &str = "O Bivar une vetor, tipografia, animação, diagramação, foto, \
        áudio e 3D leve numa só interface, sobre um mesmo grafo.";

    #[test]
    fn shapes_with_the_requested_font() {
        let mut shaper = Shaper::new();
        for font in TestFont::ALL {
            let line = shaper.shape(font, "Bivar", OPTIONS);
            assert!(line.uses_only(font), "{font:?}");
            assert_eq!(line.glyphs().count(), 5);
            assert!(line.width > 0.0 && line.line_height > line.baseline && line.baseline > 0.0);
        }
    }

    #[test]
    fn glyphs_go_left_to_right_on_the_baseline() {
        let mut shaper = Shaper::new();
        let line = shaper.shape(TestFont::SourceSans3Otf, "Bivar", OPTIONS);
        let glyphs: Vec<_> = line.glyphs().collect();
        assert_eq!(glyphs[0].x, 0.0);
        for pair in glyphs.windows(2) {
            assert!((pair[1].x - (pair[0].x + pair[0].advance)).abs() < 1e-4);
        }
        assert!(glyphs.iter().all(|g| g.y == 0.0));
        let total: f32 = glyphs.iter().map(|g| g.advance).sum();
        assert!((line.width - total).abs() < 1e-3);
    }

    #[test]
    fn doubling_the_size_doubles_the_positions() {
        let mut shaper = Shaper::new();
        let small = shaper.shape(TestFont::SourceSerif4Ttf, "AVoca", OPTIONS);
        let big = shaper.shape(
            TestFont::SourceSerif4Ttf,
            "AVoca",
            ShapeOptions {
                size: 64.0,
                ..OPTIONS
            },
        );
        assert_eq!(small.glyph_ids(), big.glyph_ids());
        for (s, b) in small.glyphs().zip(big.glyphs()) {
            assert!((b.x - 2.0 * s.x).abs() < 1e-3);
        }
    }

    #[test]
    fn empty_text_has_no_glyphs() {
        let mut shaper = Shaper::new();
        let line = shaper.shape(TestFont::SourceSans3Otf, "", OPTIONS);
        assert_eq!(line.glyphs().count(), 0);
    }

    #[test]
    fn character_missing_from_both_fonts_becomes_notdef() {
        let mut shaper = Shaper::new();
        for font in TestFont::ALL {
            let line = shaper.shape(font, "\u{10FFFD}", OPTIONS);
            assert_eq!(line.glyph_ids(), [0], "{font:?}");
        }
    }

    #[test]
    fn kerning_switched_off_uses_plain_widths() {
        let mut shaper = Shaper::new();
        for font in TestFont::ALL {
            let on = shaper.shape(font, "AV", OPTIONS);
            let off = shaper.shape(
                font,
                "AV",
                ShapeOptions {
                    kerning: false,
                    ..OPTIONS
                },
            );
            assert_eq!(on.glyph_ids(), off.glyph_ids());
            assert!(off.width > on.width, "{font:?}: AV is kerned tighter");
            // Without kerning, each glyph advances by its own width.
            let font_ref = font.font_ref();
            let scale = OPTIONS.size / f32::from(crate::font_tables::units_per_em(&font_ref));
            for glyph in off.glyphs() {
                let id = skrifa::GlyphId::new(glyph.id);
                let width = crate::font_tables::advance_width(&font_ref, id).unwrap();
                assert!((glyph.advance - width * scale).abs() < 1e-3, "{font:?}");
            }
        }
    }

    #[test]
    fn paragraph_lines_fit_the_width_and_keep_every_letter() {
        let mut shaper = Shaper::new();
        let single = shaper.shape(TestFont::SourceSerif4Ttf, PARAGRAPH, OPTIONS);
        let width = 300.0;
        let lines = shaper.shape_paragraph(
            TestFont::SourceSerif4Ttf,
            PARAGRAPH,
            OPTIONS,
            width,
            Align::Start,
        );
        assert!(lines.len() >= 3, "{} lines", lines.len());
        for line in &lines {
            assert!(line.width <= width + 1e-3, "line of {} px", line.width);
            assert_eq!(line.glyphs().next().map(|g| g.x), Some(0.0));
        }
        // Line breaks drop only the spaces at the ends of lines.
        let space = single.glyph_ids()[1];
        let wrapped: Vec<u32> = lines
            .iter()
            .flat_map(|l| l.glyph_ids())
            .filter(|id| *id != space)
            .collect();
        let unwrapped: Vec<u32> = single
            .glyph_ids()
            .into_iter()
            .filter(|id| *id != space)
            .collect();
        assert_eq!(wrapped, unwrapped);
    }

    #[test]
    fn paragraph_baselines_go_down_by_one_line_each() {
        let mut shaper = Shaper::new();
        let lines = shaper.shape_paragraph(
            TestFont::SourceSans3Otf,
            PARAGRAPH,
            OPTIONS,
            300.0,
            Align::Start,
        );
        assert!(lines[0].baseline > 0.0);
        for pair in lines.windows(2) {
            let step = pair[1].baseline - pair[0].baseline;
            assert!((step - pair[0].line_height).abs() < 1e-3, "step {step}");
        }
    }

    #[test]
    fn justified_lines_reach_the_right_edge_except_the_last() {
        let mut shaper = Shaper::new();
        let width = 400.0;
        let size = ShapeOptions::default_features(20.0);
        let lines = shaper.shape_paragraph(
            TestFont::SourceSerif4Ttf,
            PARAGRAPH,
            size,
            width,
            Align::Justify,
        );
        assert!(lines.len() >= 3, "{} lines", lines.len());
        let font = TestFont::SourceSerif4Ttf.font_ref();
        let space = crate::font_tables::glyph_for_char(&font, ' ')
            .unwrap()
            .to_u32();
        let letters: Vec<Vec<&ShapedGlyph>> = lines
            .iter()
            .map(|line| {
                // A trailing space may hang past the edge; only letters count.
                let mut glyphs: Vec<_> = line.glyphs().collect();
                while glyphs.last().is_some_and(|g| g.id == space) {
                    glyphs.pop();
                }
                glyphs
            })
            .collect();
        let end = |glyphs: &[&ShapedGlyph]| glyphs.last().map_or(0.0, |g| g.x + g.advance);
        let (last, full) = letters.split_last().unwrap();
        for glyphs in full {
            // A single word has no space to stretch, so it stays flush left.
            if glyphs.iter().any(|g| g.id == space) {
                let end = end(glyphs);
                assert!((end - width).abs() < 0.5, "line ends at {end}");
            }
        }
        assert!(end(last) < width - 1.0);
    }
}
