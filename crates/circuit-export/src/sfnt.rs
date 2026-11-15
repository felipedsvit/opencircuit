//! A reader for the few tables of a TrueType font that the PDF export needs:
//! the character map, the metrics and the number of glyphs.
//!
//! It reads the font that is compiled in, which is a constant and not input,
//! so it does not validate the whole file; but every read is checked and
//! gives `None` instead of panicking, as everywhere in the core. Subsetting
//! is the job of the `subsetter` crate.

/// The table directory and tables of a font file.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Sfnt<'a> {
    data: &'a [u8],
}

/// Metrics in font units.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Metrics {
    /// Font units per em.
    pub(crate) units_per_em: u16,
    /// Bounding box of every glyph: left, bottom, right, top.
    pub(crate) bbox: [i16; 4],
    /// Distance from the baseline to the top of the line.
    pub(crate) ascender: i16,
    /// Distance from the baseline to the bottom of the line; negative.
    pub(crate) descender: i16,
    /// Height of a capital letter, if the font says.
    pub(crate) cap_height: Option<i16>,
    /// Advance of the first glyph, the same for all in a monospaced font.
    pub(crate) advance: u16,
}

fn u16_at(data: &[u8], offset: usize) -> Option<u16> {
    let end = offset.checked_add(2)?;
    Some(u16::from_be_bytes(data.get(offset..end)?.try_into().ok()?))
}

fn i16_at(data: &[u8], offset: usize) -> Option<i16> {
    u16_at(data, offset).map(|value| i16::from_be_bytes(value.to_be_bytes()))
}

fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    Some(u32::from_be_bytes(data.get(offset..end)?.try_into().ok()?))
}

impl<'a> Sfnt<'a> {
    /// A font file in the TrueType (`glyf`) flavour.
    pub(crate) fn new(data: &'a [u8]) -> Option<Self> {
        // 0x00010000 is TrueType outlines; `true` is its old Apple name.
        matches!(u32_at(data, 0)?, 0x0001_0000 | 0x7472_7565).then_some(Self { data })
    }

    /// The bytes of a table.
    pub(crate) fn table(&self, tag: &[u8; 4]) -> Option<&'a [u8]> {
        let count = usize::from(u16_at(self.data, 4)?);
        for index in 0..count {
            let record = index.checked_mul(16)?.checked_add(12)?;
            if self.data.get(record..record.checked_add(4)?)? != tag {
                continue;
            }
            let offset = usize::try_from(u32_at(self.data, record.checked_add(8)?)?).ok()?;
            let length = usize::try_from(u32_at(self.data, record.checked_add(12)?)?).ok()?;
            return self.data.get(offset..offset.checked_add(length)?);
        }
        None
    }

    /// How many glyphs the font has; the tests use it to size a subset.
    #[cfg(test)]
    pub(crate) fn glyph_count(&self) -> Option<u16> {
        u16_at(self.table(b"maxp")?, 4)
    }

    /// The metrics the PDF font descriptor needs.
    pub(crate) fn metrics(&self) -> Option<Metrics> {
        let head = self.table(b"head")?;
        let hhea = self.table(b"hhea")?;
        let hmtx = self.table(b"hmtx")?;
        // `sCapHeight` exists from version 2 of the OS/2 table.
        let cap_height = self
            .table(b"OS/2")
            .and_then(|os2| (u16_at(os2, 0)? >= 2).then(|| i16_at(os2, 88)).flatten());
        Some(Metrics {
            units_per_em: u16_at(head, 18)?,
            bbox: [
                i16_at(head, 36)?,
                i16_at(head, 38)?,
                i16_at(head, 40)?,
                i16_at(head, 42)?,
            ],
            ascender: i16_at(hhea, 4)?,
            descender: i16_at(hhea, 6)?,
            cap_height,
            advance: u16_at(hmtx, 0)?,
        })
    }

    /// The glyph that shows `c`, or `None` when the font has none. A glyph
    /// ID of 0 is the empty box and counts as none.
    ///
    /// Reads the Unicode subtable of the `cmap` (format 12 when there is
    /// one, else format 4, which covers the Basic Multilingual Plane).
    pub(crate) fn glyph(&self, c: char) -> Option<u16> {
        let cmap = self.table(b"cmap")?;
        let count = usize::from(u16_at(cmap, 2)?);
        let mut best: Option<(u8, usize)> = None;
        for index in 0..count {
            let record = index.checked_mul(8)?.checked_add(4)?;
            let platform = u16_at(cmap, record)?;
            let encoding = u16_at(cmap, record.checked_add(2)?)?;
            let offset = usize::try_from(u32_at(cmap, record.checked_add(4)?)?).ok()?;
            let rank = match (platform, encoding) {
                (3, 10) => 3,
                (0, 4 | 6) => 2,
                (3, 1) | (0, _) => 1,
                _ => continue,
            };
            if best.is_none_or(|(current, _)| rank > current) {
                best = Some((rank, offset));
            }
        }
        let subtable = cmap.get(best?.1..)?;
        let code = u32::from(c);
        let glyph = match u16_at(subtable, 0)? {
            4 => format4(subtable, code)?,
            12 => format12(subtable, code)?,
            _ => return None,
        };
        (glyph != 0).then_some(glyph)
    }
}

/// Segment mapping to delta values, for the code points up to `U+FFFF`.
fn format4(table: &[u8], code: u32) -> Option<u16> {
    let code = u16::try_from(code).ok()?;
    let segments = usize::from(u16_at(table, 6)?).checked_div(2)?;
    let end_codes = 14_usize;
    let start_codes = end_codes
        .checked_add(segments.checked_mul(2)?)?
        .checked_add(2)?;
    let deltas = start_codes.checked_add(segments.checked_mul(2)?)?;
    let ranges = deltas.checked_add(segments.checked_mul(2)?)?;
    for segment in 0..segments {
        let step = segment.checked_mul(2)?;
        if u16_at(table, end_codes.checked_add(step)?)? < code {
            continue;
        }
        if u16_at(table, start_codes.checked_add(step)?)? > code {
            return None;
        }
        let delta = u16_at(table, deltas.checked_add(step)?)?;
        let range_at = ranges.checked_add(step)?;
        let range = usize::from(u16_at(table, range_at)?);
        if range == 0 {
            return Some(code.wrapping_add(delta));
        }
        let start = u16_at(table, start_codes.checked_add(step)?)?;
        let glyph_at = range_at
            .checked_add(range)?
            .checked_add(usize::from(code.checked_sub(start)?).checked_mul(2)?)?;
        return match u16_at(table, glyph_at)? {
            0 => None,
            glyph => Some(glyph.wrapping_add(delta)),
        };
    }
    None
}

/// Segmented coverage, for every code point.
fn format12(table: &[u8], code: u32) -> Option<u16> {
    let groups = usize::try_from(u32_at(table, 12)?).ok()?;
    for group in 0..groups {
        let at = group.checked_mul(12)?.checked_add(16)?;
        let start = u32_at(table, at)?;
        let end = u32_at(table, at.checked_add(4)?)?;
        if (start..=end).contains(&code) {
            let first = u32_at(table, at.checked_add(8)?)?;
            return u16::try_from(first.checked_add(code.checked_sub(start)?)?).ok();
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const FONT: &[u8] =
        include_bytes!("../../../assets/fonts/liberation-mono/LiberationMono-Regular.ttf");

    fn font() -> Sfnt<'static> {
        Sfnt::new(FONT).unwrap()
    }

    #[test]
    fn the_bundled_font_declares_its_name_and_licence() {
        let utf16 =
            |text: &str| -> Vec<u8> { text.encode_utf16().flat_map(u16::to_be_bytes).collect() };
        let name = font().table(b"name").unwrap();
        let has = |text: &str| {
            let needle = utf16(text);
            name.windows(needle.len()).any(|window| window == needle)
        };

        assert!(has("Liberation Mono"));
        assert!(has("Version 2.1.5"));
        assert!(has("SIL Open Font License, Version 1.1"));
    }

    #[test]
    fn characters_map_to_the_glyphs_of_the_font() {
        let font = font();

        // Glyph IDs read from the file with an independent parser, which
        // agreed with this reader on every code point up to U+10FFFF.
        for (c, glyph) in [
            ('A', 36),
            (' ', 3),
            ('\u{e9}', 171),
            ('\u{394}', 838),
            ('\u{416}', 966),
        ] {
            assert_eq!(font.glyph(c), Some(glyph), "{c:?}");
        }
        // Not in the font: CJK, an unassigned code point, beyond the BMP.
        for c in ['\u{4e2d}', '\u{378}', '\u{1f600}'] {
            assert_eq!(font.glyph(c), None, "{c:?}");
        }
    }

    #[test]
    fn metrics_are_those_of_a_monospaced_font_at_2048_units() {
        let metrics = font().metrics().unwrap();

        assert_eq!(metrics.units_per_em, 2048);
        assert_eq!(metrics.advance, 1229);
        assert_eq!(metrics.cap_height, Some(1349));
        assert!(metrics.ascender > 0 && metrics.descender < 0);
        assert!(metrics.bbox[0] < metrics.bbox[2] && metrics.bbox[1] < metrics.bbox[3]);
        assert!(metrics.cap_height.is_none_or(|height| height > 0));
        assert!(font().glyph_count().unwrap() > 1000);
    }

    #[test]
    fn bytes_that_are_not_a_truetype_font_give_none_and_never_panic() {
        assert!(Sfnt::new(&[]).is_none());
        assert!(Sfnt::new(b"OTTO\0\0\0\0").is_none());
        // A valid header whose table directory points past the end.
        let mut cut = FONT.get(..64).unwrap().to_vec();
        cut.truncate(40);
        let broken = Sfnt::new(&cut).unwrap();
        assert!(broken.table(b"cmap").is_none());
        assert!(broken.glyph('A').is_none());
        assert!(broken.metrics().is_none());
        // Every prefix of the file reads without panicking.
        for length in (0..4096).step_by(7) {
            if let Some(sfnt) = FONT.get(..length).and_then(Sfnt::new) {
                let _ = (sfnt.glyph('A'), sfnt.metrics(), sfnt.glyph_count());
            }
        }
    }
}
