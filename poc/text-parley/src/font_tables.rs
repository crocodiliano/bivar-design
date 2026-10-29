//! Reads kerning and ligatures straight from the font tables.
//!
//! This is the independent reference for the tests: it does not use Parley
//! or its shaper, only the raw OpenType tables (GPOS `kern`, GSUB `liga`).
//! It covers the cases the tests need, not all of OpenType.

use skrifa::instance::{LocationRef, Size};
use skrifa::raw::tables::gpos::{PairPos, PositionSubtables};
use skrifa::raw::tables::gsub::SubstitutionSubtables;
use skrifa::raw::tables::layout::FeatureList;
use skrifa::raw::types::{GlyphId, GlyphId16, Tag};
use skrifa::raw::{ReadError, TableProvider};
use skrifa::{FontRef, MetadataProvider};

pub fn units_per_em(font: &FontRef) -> u16 {
    font.head().expect("font has a head table").units_per_em()
}

/// The glyph the font's character map gives for `ch`.
pub fn glyph_for_char(font: &FontRef, ch: char) -> Option<GlyphId> {
    font.charmap().map(ch)
}

/// The glyph's own width in font units, without any kerning.
pub fn advance_width(font: &FontRef, glyph: GlyphId) -> Option<f32> {
    font.glyph_metrics(Size::unscaled(), LocationRef::default())
        .advance_width(glyph)
}

pub fn has_cff_outlines(font: &FontRef) -> bool {
    font.cff().is_ok() || font.cff2().is_ok()
}

pub fn has_glyf_outlines(font: &FontRef) -> bool {
    font.glyf().is_ok()
}

/// Lookup indices of every feature with `tag`, in any script, without repeats.
fn feature_lookups(list: &FeatureList, tag: Tag) -> Result<Vec<u16>, ReadError> {
    let mut lookups = Vec::new();
    for record in list.feature_records() {
        if record.feature_tag() != tag {
            continue;
        }
        let feature = record.feature(list.offset_data())?;
        for index in feature.lookup_list_indices() {
            let index = index.get();
            if !lookups.contains(&index) {
                lookups.push(index);
            }
        }
    }
    lookups.sort_unstable();
    Ok(lookups)
}

/// Horizontal kerning between `left` and `right`, in font units, from the
/// `kern` feature of the GPOS table. `None` when the font has no such pair.
pub fn kerning(font: &FontRef, left: GlyphId, right: GlyphId) -> Result<Option<i32>, ReadError> {
    let gpos = font.gpos()?;
    let lookup_list = gpos.lookup_list()?;
    let (Ok(left), Ok(right)) = (GlyphId16::try_from(left), GlyphId16::try_from(right)) else {
        return Ok(None);
    };
    let mut total = None;
    for index in feature_lookups(&gpos.feature_list()?, Tag::new(b"kern"))? {
        let lookup = lookup_list.lookups().get(index as usize)?;
        let PositionSubtables::Pair(subtables) = lookup.subtables()? else {
            continue;
        };
        // In each lookup, the first subtable that applies to the pair wins.
        for subtable in subtables.iter() {
            if let Some(value) = pair_adjustment(&subtable?, left, right)? {
                *total.get_or_insert(0) += value;
                break;
            }
        }
    }
    Ok(total)
}

fn pair_adjustment(
    subtable: &PairPos,
    left: GlyphId16,
    right: GlyphId16,
) -> Result<Option<i32>, ReadError> {
    match subtable {
        PairPos::Format1(table) => {
            let Some(coverage_index) = table.coverage()?.get(left) else {
                return Ok(None);
            };
            let pair_set = table.pair_sets().get(coverage_index as usize)?;
            for record in pair_set.pair_value_records().iter() {
                let record = record?;
                if record.second_glyph() == right {
                    let value = record.value_record1().x_advance().unwrap_or(0);
                    return Ok(Some(i32::from(value)));
                }
            }
            Ok(None)
        }
        PairPos::Format2(table) => {
            if table.coverage()?.get(left).is_none() {
                return Ok(None);
            }
            let class1 = table.class_def1()?.get(left);
            let class2 = table.class_def2()?.get(right);
            let row = table.class1_records().get(class1 as usize)?;
            let cell = row.class2_records().get(class2 as usize)?;
            let value = cell.value_record1().x_advance().unwrap_or(0);
            Ok(Some(i32::from(value)))
        }
    }
}

/// The ligature glyph that the `liga` feature of the GSUB table makes from
/// `components`, if the font defines one.
pub fn ligature(font: &FontRef, components: &[GlyphId]) -> Result<Option<GlyphId>, ReadError> {
    let Some((first, rest)) = components.split_first() else {
        return Ok(None);
    };
    let Ok(first) = GlyphId16::try_from(*first) else {
        return Ok(None);
    };
    let gsub = font.gsub()?;
    let lookup_list = gsub.lookup_list()?;
    for index in feature_lookups(&gsub.feature_list()?, Tag::new(b"liga"))? {
        let lookup = lookup_list.lookups().get(index as usize)?;
        let SubstitutionSubtables::Ligature(subtables) = lookup.subtables()? else {
            continue;
        };
        for subtable in subtables.iter() {
            let subtable = subtable?;
            let Some(coverage_index) = subtable.coverage()?.get(first) else {
                continue;
            };
            let set = subtable.ligature_sets().get(coverage_index as usize)?;
            for candidate in set.ligatures().iter() {
                let candidate = candidate?;
                let tail: Vec<GlyphId> = candidate
                    .component_glyph_ids()
                    .iter()
                    .map(|id| GlyphId::from(id.get()))
                    .collect();
                if tail == rest {
                    return Ok(Some(candidate.ligature_glyph().into()));
                }
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fonts::TestFont;

    fn glyph(font: &FontRef, ch: char) -> GlyphId {
        glyph_for_char(font, ch).expect("character is in the font")
    }

    #[test]
    fn outline_formats_match_the_file_types() {
        let otf = TestFont::SourceSans3Otf.font_ref();
        let ttf = TestFont::SourceSerif4Ttf.font_ref();
        assert!(has_cff_outlines(&otf) && !has_glyf_outlines(&otf));
        assert!(has_glyf_outlines(&ttf) && !has_cff_outlines(&ttf));
    }

    #[test]
    fn known_kerning_pairs_have_a_value_in_both_fonts() {
        for font in TestFont::ALL {
            let font_ref = font.font_ref();
            for pair in ["AV", "To", "Ty"] {
                let mut chars = pair.chars();
                let left = glyph(&font_ref, chars.next().unwrap());
                let right = glyph(&font_ref, chars.next().unwrap());
                let value = kerning(&font_ref, left, right).unwrap();
                assert!(
                    value.is_some_and(|v| v < 0),
                    "{pair} in {font:?}: expected a negative kern, got {value:?}"
                );
            }
        }
    }

    #[test]
    fn pair_without_kerning_is_not_adjusted() {
        // Class-based kerning answers "0" for pairs in the catch-all class.
        for font in TestFont::ALL {
            let font_ref = font.font_ref();
            let (h, l) = (glyph(&font_ref, 'H'), glyph(&font_ref, 'l'));
            let value = kerning(&font_ref, h, l).unwrap();
            assert_eq!(value.unwrap_or(0), 0, "{font:?}");
        }
    }

    #[test]
    fn sample_ligatures_exist_in_each_font() {
        for font in TestFont::ALL {
            let font_ref = font.font_ref();
            for sample in font.ligature_samples() {
                let parts: Vec<GlyphId> = sample.chars().map(|c| glyph(&font_ref, c)).collect();
                let lig = ligature(&font_ref, &parts).unwrap();
                let lig = lig.unwrap_or_else(|| panic!("{sample} ligature in {font:?}"));
                assert!(!parts.contains(&lig));
            }
        }
    }

    #[test]
    fn source_sans_3_has_no_fi_or_fl_ligature() {
        let font_ref = TestFont::SourceSans3Otf.font_ref();
        let f = glyph(&font_ref, 'f');
        for second in ['i', 'l'] {
            let parts = [f, glyph(&font_ref, second)];
            assert_eq!(ligature(&font_ref, &parts).unwrap(), None);
        }
    }

    #[test]
    fn no_ligature_for_letters_that_do_not_form_one() {
        for font in TestFont::ALL {
            let font_ref = font.font_ref();
            let (a, b) = (glyph(&font_ref, 'a'), glyph(&font_ref, 'b'));
            assert_eq!(ligature(&font_ref, &[a, b]).unwrap(), None);
            assert_eq!(ligature(&font_ref, &[]).unwrap(), None);
        }
    }
}
