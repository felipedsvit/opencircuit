//! Stable 64-bit fingerprint of a compiled circuit.
//!
//! FNV-1a over an explicit little-endian encoding. The standard library
//! hasher is not used because its output is not stable across releases.

use std::fmt;

/// Hash that identifies a compiled circuit in traces and checkpoints.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Fingerprint(pub u64);

/// Formats as 16 lowercase hexadecimal digits.
impl fmt::Display for Fingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

/// Incremental FNV-1a 64-bit hasher.
pub(crate) struct Fnv64(u64);

impl Fnv64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    pub(crate) const fn new() -> Self {
        Self(Self::OFFSET_BASIS)
    }

    pub(crate) fn bytes(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(Self::PRIME);
        }
    }

    pub(crate) fn u8(&mut self, value: u8) {
        self.bytes(&[value]);
    }

    pub(crate) fn u32(&mut self, value: u32) {
        self.bytes(&value.to_le_bytes());
    }

    pub(crate) fn u64(&mut self, value: u64) {
        self.bytes(&value.to_le_bytes());
    }

    /// Length-prefixed, so that adjacent strings cannot be confused.
    pub(crate) fn str(&mut self, value: &str) {
        self.u64(value.len() as u64);
        self.bytes(value.as_bytes());
    }

    pub(crate) const fn finish(&self) -> Fingerprint {
        Fingerprint(self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_published_fnv1a_test_vectors() {
        let hash = |text: &str| {
            let mut hasher = Fnv64::new();
            hasher.bytes(text.as_bytes());
            hasher.finish().0
        };

        assert_eq!(hash(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(hash("a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(hash("foobar"), 0x8594_4171_f739_67e8);
    }

    #[test]
    fn length_prefix_separates_adjacent_strings() {
        let mut first = Fnv64::new();
        first.str("ab");
        first.str("c");
        let mut second = Fnv64::new();
        second.str("a");
        second.str("bc");

        assert_ne!(first.finish(), second.finish());
    }

    #[test]
    fn displays_as_sixteen_hex_digits() {
        assert_eq!(Fingerprint(0xab).to_string(), "00000000000000ab");
    }
}
