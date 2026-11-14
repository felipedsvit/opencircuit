//! Fixed-length bit set used for signal values and switch masks.
//!
//! Out-of-range access never panics: reading returns `false` and writing is
//! ignored. Dense indices come from a compiled circuit, so an out-of-range
//! index can only mean a circuit and a state that do not belong together.

/// A fixed number of bits.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct BitSet {
    words: Vec<u64>,
    len: usize,
}

impl BitSet {
    /// A set of `len` bits, all clear.
    #[must_use]
    pub fn new(len: usize) -> Self {
        Self {
            words: vec![0; len.div_ceil(64)],
            len,
        }
    }

    /// Number of bits.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Whether the set holds no bit at all.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Value of a bit; `false` when out of range.
    #[must_use]
    pub fn get(&self, index: usize) -> bool {
        index < self.len
            && self
                .words
                .get(index / 64)
                .is_some_and(|word| word & Self::mask(index) != 0)
    }

    /// Set a bit; ignored when out of range.
    pub fn set(&mut self, index: usize, value: bool) {
        if index >= self.len {
            return;
        }
        if let Some(word) = self.words.get_mut(index / 64) {
            if value {
                *word |= Self::mask(index);
            } else {
                *word &= !Self::mask(index);
            }
        }
    }

    /// Make this set equal to `other` without reallocating when the lengths
    /// already match.
    pub fn copy_from(&mut self, other: &Self) {
        self.words.clone_from(&other.words);
        self.len = other.len;
    }

    /// Indices of the bits that are set, ascending.
    pub fn ones(&self) -> impl Iterator<Item = usize> + '_ {
        (0..self.len).filter(|index| self.get(*index))
    }

    const fn mask(index: usize) -> u64 {
        // `index % 64` is below 64, so the shift never overflows.
        1_u64.wrapping_shl((index % 64) as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bits_start_clear_and_keep_what_is_written() {
        let mut bits = BitSet::new(130);
        bits.set(0, true);
        bits.set(64, true);
        bits.set(129, true);

        assert_eq!(bits.ones().collect::<Vec<_>>(), [0, 64, 129]);

        bits.set(64, false);

        assert_eq!(bits.ones().collect::<Vec<_>>(), [0, 129]);
    }

    #[test]
    fn out_of_range_access_is_ignored_instead_of_panicking() {
        let mut bits = BitSet::new(3);
        bits.set(3, true);
        bits.set(usize::MAX, true);

        assert!(!bits.get(3));
        assert!(!bits.get(usize::MAX));
        assert_eq!(bits.ones().count(), 0);
    }

    #[test]
    fn copy_from_makes_sets_equal() {
        let mut source = BitSet::new(70);
        source.set(69, true);
        let mut target = BitSet::new(70);

        target.copy_from(&source);

        assert_eq!(target, source);
    }
}
