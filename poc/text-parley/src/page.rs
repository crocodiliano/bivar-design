//! What each test image shows, independent of the renderer: shaped glyph
//! runs at fixed positions, plus thin guide frames.

use vello_cpu::kurbo::Rect;
use vello_cpu::peniko::Color;

use crate::fonts::TestFont;
use crate::shaping::{Align, ShapeOptions, ShapedGlyph, ShapedLine, Shaper};

/// The sentence from the phase 0.5 spec, with precomposed letters (NFC).
pub const ACCENTS_PRECOMPOSED: &str = "Açaí à brasileira — ótimo, pão, avô, ação";
/// The same sentence with each accent typed as a separate combining mark (NFD).
pub const ACCENTS_COMBINING: &str = "Ac\u{327}ai\u{301} a\u{300} brasileira — o\u{301}timo, pa\u{303}o, avo\u{302}, ac\u{327}a\u{303}o";

pub const KERNING_SAMPLE: &str = "AV  To  Ty  AVATAR  Toyota";

/// Paragraph for the line-breaking image (not a criterion of phase 0.5).
pub const PARAGRAPH: &str = "O Bivar é um programa de design livre que reúne vetor, \
    tipografia, animação, diagramação editorial, foto, áudio e 3D leve numa só interface. \
    Cada área é uma aba com ferramentas próprias, mas todas trabalham sobre o mesmo grafo, \
    que é a fonte da verdade do documento. Assim, um título desenhado na aba de tipografia \
    pode ser animado, diagramado numa página e exportado sem perder nenhuma informação \
    no caminho — e o arquivo final continua editável, fiel e flexível.";

// Neutral greys only (red = green = blue), so the canvas has no tint.
pub const BACKGROUND: Color = Color::from_rgb8(0xfa, 0xfa, 0xfa);
pub const TEXT: Color = Color::from_rgb8(0x1c, 0x1c, 0x1c);
pub const LABEL: Color = Color::from_rgb8(0x5e, 0x5e, 0x5e);
pub const GUIDE: Color = Color::from_rgb8(0xc8, 0xc8, 0xc8);

const MARGIN: f32 = 40.0;
const LABEL_SIZE: f32 = 16.0;

/// Glyphs of one font and size, drawn from `origin` (left end of the baseline).
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphRunItem {
    pub font: TestFont,
    pub size: f32,
    pub origin: (f32, f32),
    pub glyphs: Vec<ShapedGlyph>,
    pub color: Color,
}

/// A rectangle outlined with a 1 px line.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub rect: Rect,
    pub color: Color,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Page {
    pub width: u16,
    pub height: u16,
    pub runs: Vec<GlyphRunItem>,
    pub frames: Vec<Frame>,
}

impl Page {
    fn new(width: u16) -> Self {
        Self {
            width,
            height: 0,
            runs: Vec::new(),
            frames: Vec::new(),
        }
    }

    /// Adds every run of `line` with its baseline at `baseline_y`.
    fn push_line(&mut self, line: &ShapedLine, x: f32, baseline_y: f32, color: Color) {
        for run in &line.runs {
            self.runs.push(GlyphRunItem {
                font: run.font,
                size: run.size,
                origin: (x, baseline_y),
                glyphs: run.glyphs.clone(),
                color,
            });
        }
    }

    fn finish(mut self, content_bottom: f32) -> Self {
        self.height = (content_bottom + MARGIN).ceil() as u16;
        self
    }
}

/// Lays out lines from top to bottom, each on a whole-pixel baseline.
struct Column<'a> {
    shaper: &'a mut Shaper,
    page: Page,
    x: f32,
    y: f32,
}

impl Column<'_> {
    fn line(&mut self, font: TestFont, text: &str, options: ShapeOptions, color: Color) {
        let line = self.shaper.shape(font, text, options);
        let baseline = (self.y + line.baseline).round();
        self.page.push_line(&line, self.x, baseline, color);
        self.y += line.line_height;
    }

    fn gap(&mut self, pixels: f32) {
        self.y += pixels;
    }
}

/// One image per font with the four criteria of phase 0.5: accents in both
/// spellings, kerning on and off, ligatures on and off. The font itself is
/// the format criterion (OTF or TTF).
pub fn criteria_page(shaper: &mut Shaper, font: TestFont) -> Page {
    let label = ShapeOptions::default_features(LABEL_SIZE);
    let text = ShapeOptions::default_features(40.0);
    let big = ShapeOptions::default_features(64.0);
    let ligatures = ligature_line(font);
    let format = match font {
        TestFont::SourceSans3Otf => "OTF, contornos CFF",
        TestFont::SourceSerif4Ttf => "TTF, contornos TrueType",
    };

    let mut column = Column {
        shaper,
        page: Page::new(1200),
        x: MARGIN,
        y: MARGIN,
    };
    let title = format!("{} — {format}", font.family());
    column.line(font, &title, ShapeOptions::default_features(24.0), TEXT);
    column.gap(20.0);

    let sections: [(&str, &str, ShapeOptions); 6] = [
        (
            "1. Acentos: letras acentuadas prontas",
            ACCENTS_PRECOMPOSED,
            text,
        ),
        (
            "1. Acentos: letra + acento separado",
            ACCENTS_COMBINING,
            text,
        ),
        ("2. Kerning ligado", KERNING_SAMPLE, big),
        (
            "2. Kerning desligado",
            KERNING_SAMPLE,
            ShapeOptions {
                kerning: false,
                ..big
            },
        ),
        ("3. Ligaduras ligadas", ligatures, big),
        (
            "3. Ligaduras desligadas",
            ligatures,
            ShapeOptions {
                ligatures: false,
                ..big
            },
        ),
    ];
    for (caption, sample, options) in sections {
        column.line(font, caption, label, LABEL);
        column.line(font, sample, options, TEXT);
        column.gap(16.0);
    }
    let bottom = column.y;
    column.page.finish(bottom)
}

/// Words that show the ligatures each font has (see `TestFont::ligature_samples`).
pub fn ligature_line(font: TestFont) -> &'static str {
    match font {
        TestFont::SourceSans3Otf => "ff  ft  afta  efeito  oftalmo",
        TestFont::SourceSerif4Ttf => "fi  fl  ff  ft  afinal  flor  efeito",
    }
}

/// A single line on its own, for comparing two texts pixel by pixel.
pub fn single_line_page(shaper: &mut Shaper, font: TestFont, text: &str) -> Page {
    let mut column = Column {
        shaper,
        page: Page::new(1000),
        x: MARGIN,
        y: MARGIN,
    };
    column.line(font, text, ShapeOptions::default_features(40.0), TEXT);
    let bottom = column.y;
    column.page.finish(bottom)
}

/// Width of each paragraph column, in pixels.
pub const COLUMN_WIDTH: f32 = 520.0;

/// The paragraph broken into lines at a fixed width, flush left and
/// justified, in both fonts. Only for the owner to look at.
pub fn paragraph_page(shaper: &mut Shaper) -> Page {
    let mut page = Page::new(1200);
    let label = ShapeOptions::default_features(LABEL_SIZE);
    let body = ShapeOptions::default_features(20.0);
    let mut y = MARGIN;
    for font in TestFont::ALL {
        let mut bottom = y;
        for (column, (caption, align)) in [
            ("alinhado à esquerda", Align::Start),
            ("justificado", Align::Justify),
        ]
        .into_iter()
        .enumerate()
        {
            let x = MARGIN + column as f32 * (COLUMN_WIDTH + 80.0);
            let heading = shaper.shape(font, &format!("{}, {caption}", font.family()), label);
            page.push_line(&heading, x, (y + heading.baseline).round(), LABEL);
            let top = y + heading.line_height + 12.0;
            let lines = shaper.shape_paragraph(font, PARAGRAPH, body, COLUMN_WIDTH, align);
            for line in &lines {
                page.push_line(line, x, (top + line.baseline).round(), TEXT);
            }
            let height: f32 = lines.iter().map(|l| l.line_height).sum();
            // The guide sits just outside the column, so text never touches it.
            page.frames.push(Frame {
                rect: Rect::new(
                    f64::from(x) - 8.5,
                    f64::from(top) - 8.5,
                    f64::from(x + COLUMN_WIDTH) + 8.5,
                    f64::from(top + height) + 8.5,
                ),
                color: GUIDE,
            });
            bottom = bottom.max(top + height + 8.0);
        }
        y = bottom + 40.0;
    }
    page.finish(y - 40.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn criteria_pages_hold_every_section_inside_the_image() {
        let mut shaper = Shaper::new();
        for font in TestFont::ALL {
            let page = criteria_page(&mut shaper, font);
            // Title, then six captions and six samples.
            assert_eq!(page.runs.len(), 13, "{font:?}");
            assert!(page.runs.iter().all(|r| r.font == font));
            for run in &page.runs {
                let (x, y) = run.origin;
                assert!(x >= MARGIN && y > 0.0 && y < f32::from(page.height));
                assert_eq!(y, y.round(), "baselines sit on whole pixels");
                let right = run.glyphs.last().map_or(0.0, |g| g.x + g.advance);
                assert!(x + right < f32::from(page.width), "{font:?}: run too wide");
            }
        }
    }

    #[test]
    fn criteria_pages_differ_only_by_font() {
        let mut shaper = Shaper::new();
        let a = criteria_page(&mut shaper, TestFont::SourceSans3Otf);
        let b = criteria_page(&mut shaper, TestFont::SourceSerif4Ttf);
        assert_ne!(a, b);
        assert_eq!(a.width, b.width);
    }

    #[test]
    fn ligature_lines_contain_each_fonts_samples() {
        for font in TestFont::ALL {
            for sample in font.ligature_samples() {
                assert!(ligature_line(font).contains(sample), "{font:?} {sample}");
            }
        }
    }

    #[test]
    fn paragraph_page_stays_inside_the_image() {
        let mut shaper = Shaper::new();
        let page = paragraph_page(&mut shaper);
        assert_eq!(page.frames.len(), 4);
        for frame in &page.frames {
            assert!(frame.rect.x1 < f64::from(page.width));
            assert!(frame.rect.y1 < f64::from(page.height));
        }
        let rows = page.runs.iter().filter(|r| r.color == TEXT).count();
        assert!(rows >= 4 * 6, "only {rows} paragraph lines");
    }

    #[test]
    fn the_two_accent_spellings_are_different_text() {
        assert_ne!(ACCENTS_PRECOMPOSED, ACCENTS_COMBINING);
        assert_eq!(ACCENTS_PRECOMPOSED.chars().count(), 41);
        // Eight accented letters: ç, í, à, ó, ã, ô, ç, ã.
        assert_eq!(ACCENTS_COMBINING.chars().count(), 41 + 8);
    }
}
