//! Acceptance criteria of phase 0.5 that can be checked with numbers.
//! Each one runs with both fonts: OTF (CFF outlines) and TTF (glyf outlines).

use bivar_poc_text_parley::font_tables;
use bivar_poc_text_parley::fonts::TestFont;
use bivar_poc_text_parley::page::{ACCENTS_COMBINING, ACCENTS_PRECOMPOSED};
use bivar_poc_text_parley::shaping::{ShapeOptions, ShapedLine, Shaper};
use skrifa::raw::types::GlyphId;

fn options(size: f32, ligatures: bool) -> ShapeOptions {
    ShapeOptions {
        ligatures,
        ..ShapeOptions::default_features(size)
    }
}

fn cmap(font: TestFont, ch: char) -> GlyphId {
    font_tables::glyph_for_char(&font.font_ref(), ch).expect("character is in the font")
}

fn assert_same_glyphs_and_positions(a: &ShapedLine, b: &ShapedLine, context: &str) {
    assert_eq!(a.glyph_ids(), b.glyph_ids(), "{context}: glyphs differ");
    for (ga, gb) in a.glyphs().zip(b.glyphs()) {
        assert!(
            (ga.x - gb.x).abs() < 1e-4 && (ga.y - gb.y).abs() < 1e-4,
            "{context}: glyph {} at ({}, {}) vs ({}, {})",
            ga.id,
            ga.x,
            ga.y,
            gb.x,
            gb.y
        );
    }
}

#[test]
fn accents_precomposed_and_combining_shape_identically() {
    let mut shaper = Shaper::new();
    for font in TestFont::ALL {
        for size in [16.0, 48.0] {
            let nfc = shaper.shape(font, ACCENTS_PRECOMPOSED, options(size, true));
            let nfd = shaper.shape(font, ACCENTS_COMBINING, options(size, true));
            assert!(nfc.uses_only(font) && nfd.uses_only(font), "{font:?}");
            assert_same_glyphs_and_positions(&nfc, &nfd, &format!("{font:?} at {size}px"));
        }
    }
}

#[test]
fn accented_letters_use_the_fonts_own_accented_glyphs() {
    let mut shaper = Shaper::new();
    for font in TestFont::ALL {
        let line = shaper.shape(font, ACCENTS_COMBINING, options(32.0, true));
        let ids = line.glyph_ids();
        assert!(
            !ids.contains(&0),
            "{font:?}: a character fell back to .notdef"
        );
        for ch in ['ç', 'í', 'à', 'ó', 'ã', 'ô'] {
            let expected = cmap(font, ch).to_u32();
            assert!(ids.contains(&expected), "{font:?}: no glyph for {ch}");
        }
    }
}

#[test]
fn kerning_matches_the_values_in_the_font() {
    let mut shaper = Shaper::new();
    for font in TestFont::ALL {
        let font_ref = font.font_ref();
        // At one pixel per font unit, positions are in font units.
        let size = f32::from(font_tables::units_per_em(&font_ref));
        for pair in ["AV", "To", "Ty"] {
            let mut chars = pair.chars();
            let (left_char, right_char) = (chars.next().unwrap(), chars.next().unwrap());
            let (left, right) = (cmap(font, left_char), cmap(font, right_char));
            let width = font_tables::advance_width(&font_ref, left).unwrap();
            let kern = font_tables::kerning(&font_ref, left, right)
                .unwrap()
                .unwrap();
            assert_ne!(kern, 0, "{font:?} {pair}: test pair must be kerned");

            let line = shaper.shape(font, pair, options(size, true));
            let glyphs: Vec<_> = line.glyphs().collect();
            assert_eq!(line.glyph_ids(), [left.to_u32(), right.to_u32()]);
            let expected = width + kern as f32;
            assert!(
                (glyphs[0].advance - expected).abs() < 1e-3,
                "{font:?} {pair}: advance {} but width {width} + kern {kern} = {expected}",
                glyphs[0].advance
            );
            assert!((glyphs[1].x - expected).abs() < 1e-3);
        }
    }
}

#[test]
fn kerning_scales_with_the_font_size() {
    let mut shaper = Shaper::new();
    for font in TestFont::ALL {
        let font_ref = font.font_ref();
        let upem = f32::from(font_tables::units_per_em(&font_ref));
        let (a, v) = (cmap(font, 'A'), cmap(font, 'V'));
        let width = font_tables::advance_width(&font_ref, a).unwrap();
        let kern = font_tables::kerning(&font_ref, a, v).unwrap().unwrap();
        let size = 37.0;
        let line = shaper.shape(font, "AV", options(size, true));
        let expected = (width + kern as f32) * size / upem;
        let first = line.glyphs().next().unwrap();
        assert!((first.advance - expected).abs() < 1e-3, "{font:?}");
    }
}

/// The ligature glyph the font's GSUB table defines for `text`.
fn ligature_glyph(font: TestFont, text: &str) -> u32 {
    let parts: Vec<GlyphId> = text.chars().map(|c| cmap(font, c)).collect();
    font_tables::ligature(&font.font_ref(), &parts)
        .unwrap()
        .unwrap_or_else(|| panic!("{font:?} defines no {text} ligature"))
        .to_u32()
}

#[test]
fn fi_and_fl_become_the_fonts_ligature_glyphs() {
    // Source Sans 3 has no fi/fl ligature; see `TestFont::ligature_samples`.
    let font = TestFont::SourceSerif4Ttf;
    let mut shaper = Shaper::new();
    for text in ["fi", "fl"] {
        let line = shaper.shape(font, text, options(32.0, true));
        assert_eq!(line.glyph_ids(), [ligature_glyph(font, text)], "{text}");
    }
}

#[test]
fn each_fonts_ligatures_are_applied() {
    let mut shaper = Shaper::new();
    for font in TestFont::ALL {
        for text in font.ligature_samples() {
            let line = shaper.shape(font, text, options(32.0, true));
            assert_eq!(
                line.glyph_ids(),
                [ligature_glyph(font, text)],
                "{font:?} {text}"
            );
        }
    }
}

#[test]
fn ligatures_switched_off_give_separate_letters() {
    let mut shaper = Shaper::new();
    for font in TestFont::ALL {
        for text in font.ligature_samples() {
            let line = shaper.shape(font, text, options(32.0, false));
            let expected: Vec<u32> = text.chars().map(|c| cmap(font, c).to_u32()).collect();
            assert_eq!(line.glyph_ids(), expected, "{font:?} {text}");
        }
    }
}

#[test]
fn ligatures_inside_words_follow_the_switch() {
    let mut shaper = Shaper::new();
    for (font, word, pair) in [
        (TestFont::SourceSerif4Ttf, "afinal", "fi"),
        (TestFont::SourceSans3Otf, "afta", "ft"),
    ] {
        let lig = ligature_glyph(font, pair);
        let letters = word.chars().count();
        let on = shaper.shape(font, word, options(32.0, true));
        let off = shaper.shape(font, word, options(32.0, false));
        assert_eq!(on.glyph_ids().len(), letters - 1, "{font:?} {word}");
        assert!(on.glyph_ids().contains(&lig));
        assert_eq!(off.glyph_ids().len(), letters, "{font:?} {word}");
        assert!(!off.glyph_ids().contains(&lig));
    }
}

#[test]
fn both_outline_formats_are_shaped() {
    let otf = TestFont::SourceSans3Otf.font_ref();
    let ttf = TestFont::SourceSerif4Ttf.font_ref();
    assert!(font_tables::has_cff_outlines(&otf));
    assert!(font_tables::has_glyf_outlines(&ttf));

    let mut shaper = Shaper::new();
    let sans = shaper.shape(TestFont::SourceSans3Otf, "Bivar", options(32.0, true));
    let serif = shaper.shape(TestFont::SourceSerif4Ttf, "Bivar", options(32.0, true));
    assert!(sans.uses_only(TestFont::SourceSans3Otf));
    assert!(serif.uses_only(TestFont::SourceSerif4Ttf));
    assert_ne!(sans.width, serif.width);
}
