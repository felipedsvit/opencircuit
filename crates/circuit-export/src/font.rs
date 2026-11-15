//! The font of the PDF export: Liberation Mono, subset to the characters of
//! the scene.
//!
//! The font file is compiled in (see `ATTRIBUTION.md` for its licence).
//! [`Subset`] reads the characters a scene draws, keeps only their glyphs in
//! a copy of the font for embedding, and encodes a text as the 16-bit glyph
//! IDs of that copy. A character the font does not have is drawn as the
//! font's own empty-box glyph, never as another character.

use std::collections::{BTreeMap, BTreeSet};

use crate::sfnt::Sfnt;
use subsetter::GlyphRemapper;

/// Liberation Mono Regular, unmodified (SIL Open Font License 1.1).
static FONT: &[u8] =
    include_bytes!("../../../assets/fonts/liberation-mono/LiberationMono-Regular.ttf");

/// The glyph drawn for a character the font lacks.
const NOTDEF: u16 = 0;

/// A subset of the font and the table that encodes text for it.
pub(crate) struct Subset {
    /// The font file with only the glyphs of the scene.
    pub(crate) data: Vec<u8>,
    /// A six-letter tag that names this subset, as PDF asks of subsets.
    pub(crate) tag: [u8; 6],
    /// Metrics of the font in thousandths of an em.
    pub(crate) metrics: Metrics,
    /// Glyph ID in the subset of every character of the scene.
    glyphs: BTreeMap<char, u16>,
}

/// Font metrics for the PDF font descriptor, in thousandths of an em.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Metrics {
    pub(crate) advance: i32,
    pub(crate) ascent: i32,
    pub(crate) descent: i32,
    pub(crate) cap_height: i32,
    pub(crate) bbox: [i32; 4],
}

impl Subset {
    /// Subset the font to `chars`. `None` when the compiled-in font cannot
    /// be read, which a test rules out.
    pub(crate) fn new(chars: &BTreeSet<char>) -> Option<Self> {
        let font = Sfnt::new(FONT)?;
        let raw = font.metrics()?;
        let per_em = i32::from(raw.units_per_em);
        let scale = |value: i16| i32::from(value).saturating_mul(1000).checked_div(per_em);

        // The glyphs of the characters, in character order, so that the
        // numbering of the subset depends on the scene's text only.
        let mut mapper = GlyphRemapper::new();
        mapper.remap(NOTDEF);
        let mut original: BTreeMap<char, u16> = BTreeMap::new();
        for c in chars {
            let glyph = font.glyph(*c).unwrap_or(NOTDEF);
            mapper.remap(glyph);
            original.insert(*c, glyph);
        }
        let data = subsetter::subset(FONT, 0, &mapper).ok()?;
        let glyphs = original
            .into_iter()
            .map(|(c, glyph)| (c, mapper.get(glyph).unwrap_or(NOTDEF)))
            .collect();

        Some(Self {
            tag: tag(&data),
            data,
            metrics: Metrics {
                advance: scale(i16::try_from(raw.advance).ok()?)?,
                ascent: scale(raw.ascender)?,
                descent: scale(raw.descender)?,
                cap_height: scale(raw.cap_height.unwrap_or(raw.ascender))?,
                bbox: [
                    scale(raw.bbox[0])?,
                    scale(raw.bbox[1])?,
                    scale(raw.bbox[2])?,
                    scale(raw.bbox[3])?,
                ],
            },
            glyphs,
        })
    }

    /// The glyph IDs of a text as the big-endian 16-bit codes of an
    /// `Identity-H` font.
    pub(crate) fn encode(&self, text: &str) -> Vec<u8> {
        text.chars()
            .flat_map(|c| self.glyphs.get(&c).copied().unwrap_or(NOTDEF).to_be_bytes())
            .collect()
    }

    /// The characters of the scene with the glyph ID that shows each, for
    /// the `ToUnicode` map, in character order. The empty-box glyph is left
    /// out: it stands for no character.
    pub(crate) fn unicode(&self) -> impl Iterator<Item = (u16, char)> + '_ {
        self.glyphs
            .iter()
            .filter(|(_, glyph)| **glyph != NOTDEF)
            .map(|(c, glyph)| (*glyph, *c))
    }
}

/// Six capital letters from the glyph data, so that the same subset always
/// has the same name and two different ones differ.
fn tag(data: &[u8]) -> [u8; 6] {
    // FNV-1a, 64 bits.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in data {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    let mut out = [b'A'; 6];
    for slot in &mut out {
        let letter = u8::try_from(hash % 26).unwrap_or(0);
        *slot = b'A'.saturating_add(letter);
        hash /= 26;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(text: &str) -> BTreeSet<char> {
        text.chars().collect()
    }

    #[test]
    fn the_font_has_the_characters_the_pdf_used_to_lose() {
        let font = Sfnt::new(FONT).unwrap();
        for c in "ção Açúcar Δ Ω µ ° ± Ж".chars() {
            assert!(font.glyph(c).is_some(), "{c:?}");
        }
    }

    #[test]
    fn the_subset_is_small_and_still_a_font_with_those_glyphs() {
        let subset = Subset::new(&chars("K1 \u{394}")).unwrap();

        assert!(subset.data.len() < 10_000, "{}", subset.data.len());
        // .notdef plus the four characters.
        let font = Sfnt::new(&subset.data).unwrap();
        assert_eq!(font.glyph_count(), Some(5));
        assert_eq!(font.metrics().unwrap().units_per_em, 2048);
    }

    #[test]
    fn text_is_encoded_as_big_endian_glyph_ids_and_unknown_characters_as_notdef() {
        let subset = Subset::new(&chars("AB")).unwrap();
        let [a, b] = ["A", "B"].map(|text| subset.encode(text));

        assert_eq!(a.len(), 2);
        assert_ne!(a, b);
        assert_eq!(subset.encode("AB"), [a.clone(), b].concat());
        // A character the scene did not declare, and one the font lacks.
        assert_eq!(subset.encode("Z"), [0, 0]);
        assert_eq!(
            Subset::new(&chars("\u{4e2d}")).unwrap().encode("\u{4e2d}"),
            [0, 0]
        );
        let listed: Vec<char> = subset.unicode().map(|(_, c)| c).collect();
        assert_eq!(listed, ['A', 'B']);
    }

    #[test]
    fn the_same_characters_give_the_same_bytes_in_any_order() {
        let forward = Subset::new(&chars("ABC \u{394}")).unwrap();
        let reverse = Subset::new(&chars("\u{394} CBA")).unwrap();

        assert_eq!(forward.data, reverse.data);
        assert_eq!(forward.tag, reverse.tag);
        let other = Subset::new(&chars("ABD")).unwrap();
        assert_ne!(forward.tag, other.tag);
    }

    #[test]
    fn metrics_are_in_thousandths_of_an_em() {
        let metrics = Subset::new(&chars("A")).unwrap().metrics;

        assert_eq!(metrics.advance, 600);
        assert!(metrics.ascent > 700 && metrics.ascent < 1100);
        assert!(metrics.descent < 0);
        assert!(metrics.bbox[0] < metrics.bbox[2] && metrics.bbox[1] < metrics.bbox[3]);
    }
}
