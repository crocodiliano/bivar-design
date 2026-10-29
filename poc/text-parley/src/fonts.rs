//! The two embedded test fonts, both OFL-1.1.
//!
//! Only these fonts exist for the proof: no system font is ever looked up,
//! so the same text shapes the same way on every machine.

use std::sync::Arc;

use parley::FontContext;
use parley::fontique::{Blob, Collection, CollectionOptions};
use skrifa::FontRef;

pub const SOURCE_SANS_3_OTF: &[u8] = include_bytes!("../assets/fonts/SourceSans3-Regular.otf");
pub const SOURCE_SERIF_4_TTF: &[u8] = include_bytes!("../assets/fonts/SourceSerif4-Regular.ttf");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TestFont {
    /// OpenType with PostScript (CFF) outlines.
    SourceSans3Otf,
    /// TrueType outlines (`glyf`).
    SourceSerif4Ttf,
}

impl TestFont {
    pub const ALL: [TestFont; 2] = [TestFont::SourceSans3Otf, TestFont::SourceSerif4Ttf];

    pub fn bytes(self) -> &'static [u8] {
        match self {
            TestFont::SourceSans3Otf => SOURCE_SANS_3_OTF,
            TestFont::SourceSerif4Ttf => SOURCE_SERIF_4_TTF,
        }
    }

    /// The family name stored in the font's `name` table.
    pub fn family(self) -> &'static str {
        match self {
            TestFont::SourceSans3Otf => "Source Sans 3",
            TestFont::SourceSerif4Ttf => "Source Serif 4",
        }
    }

    /// Short name used in file names and test messages.
    pub fn slug(self) -> &'static str {
        match self {
            TestFont::SourceSans3Otf => "otf_source_sans_3",
            TestFont::SourceSerif4Ttf => "ttf_source_serif_4",
        }
    }

    /// Two-letter ligatures the font defines in its `liga` feature. Source
    /// Sans 3 has no "fi" or "fl" ligature (only "ff", "ft" and "fft"), so the
    /// "fi"/"fl" criterion runs with Source Serif 4.
    pub fn ligature_samples(self) -> &'static [&'static str] {
        match self {
            TestFont::SourceSans3Otf => &["ff", "ft"],
            TestFont::SourceSerif4Ttf => &["fi", "fl", "ff", "ft"],
        }
    }

    pub fn font_ref(self) -> FontRef<'static> {
        FontRef::new(self.bytes()).expect("the embedded font is valid")
    }
}

/// A font database holding only the embedded test fonts.
pub fn font_context() -> FontContext {
    let mut collection = Collection::new(CollectionOptions {
        shared: false,
        system_fonts: false,
    });
    for font in TestFont::ALL {
        collection.register_fonts(Blob::new(Arc::new(font.bytes())), None);
    }
    FontContext {
        collection,
        ..FontContext::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn otf_and_ttf_have_the_expected_signatures() {
        assert_eq!(&SOURCE_SANS_3_OTF[..4], b"OTTO");
        assert_eq!(&SOURCE_SERIF_4_TTF[..4], &[0, 1, 0, 0]);
    }

    #[test]
    fn only_the_embedded_families_are_known() {
        let mut cx = font_context();
        let mut names: Vec<String> = cx.collection.family_names().map(str::to_owned).collect();
        names.sort();
        assert_eq!(names, ["Source Sans 3", "Source Serif 4"]);
    }
}
