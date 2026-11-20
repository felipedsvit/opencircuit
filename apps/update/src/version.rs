//! Versions of OpenCircuit: `MAJOR.MINOR.PATCH`, with an optional suffix such
//! as `-dev`. A suffixed version sorts before the release with the same
//! numbers, so `0.0.2-dev` is older than `0.0.2`.

use std::cmp::Ordering;
use std::fmt;

/// Longest text read as a version. A release tag is far shorter.
const MAX_LEN: usize = 64;

/// A version such as `0.0.2` or `0.0.2-dev`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Version {
    numbers: [u64; 3],
    suffix: Option<String>,
}

impl Version {
    /// Read `text`, with an optional `v` in front (a release tag). `None` when
    /// it is not `MAJOR.MINOR.PATCH` with an optional `-suffix`.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.strip_prefix('v').unwrap_or(text);
        if text.is_empty() || text.len() > MAX_LEN {
            return None;
        }
        let (core, suffix) = match text.split_once('-') {
            Some((core, suffix)) => {
                let plain = !suffix.is_empty()
                    && suffix
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.');
                if !plain {
                    return None;
                }
                (core, Some(suffix.to_owned()))
            }
            None => (text, None),
        };
        let mut numbers = [0_u64; 3];
        let mut parts = core.split('.');
        for slot in &mut numbers {
            *slot = number(parts.next()?)?;
        }
        if parts.next().is_some() {
            return None;
        }
        Some(Self { numbers, suffix })
    }
}

/// A plain run of digits, read without a sign.
fn number(part: &str) -> Option<u64> {
    if part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    part.parse().ok()
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        self.numbers
            .cmp(&other.numbers)
            .then_with(|| other.suffix.is_some().cmp(&self.suffix.is_some()))
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [major, minor, patch] = self.numbers;
        write!(f, "{major}.{minor}.{patch}")?;
        match &self.suffix {
            Some(suffix) => write!(f, "-{suffix}"),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Version;

    fn version(text: &str) -> Version {
        match Version::parse(text) {
            Some(version) => version,
            None => panic!("{text} is a version"),
        }
    }

    #[test]
    fn a_tag_and_a_plain_version_are_the_same_version() {
        assert_eq!(version("v0.0.3"), version("0.0.3"));
        assert_eq!(version("0.0.3").to_string(), "0.0.3");
    }

    #[test]
    fn newer_numbers_win_whatever_the_digits() {
        assert!(version("0.0.3") > version("0.0.2"));
        assert!(version("0.10.0") > version("0.9.9"));
        assert!(version("1.0.0") > version("0.99.99"));
    }

    #[test]
    fn a_suffixed_version_is_older_than_its_release() {
        assert!(version("0.0.2-dev") < version("0.0.2"));
        assert!(version("0.0.2-dev") < version("0.0.3-dev"));
        assert_eq!(version("0.0.2-dev").to_string(), "0.0.2-dev");
    }

    #[test]
    fn text_that_is_not_a_version_is_refused() {
        for text in [
            "",
            "v",
            "0.0",
            "0.0.3.1",
            "a.b.c",
            "0.0.-1",
            "+0.0.1",
            "0.0.3-",
            "0.0.3-rc 1",
            "0.0.3\n",
            "99999999999999999999999.0.0",
        ] {
            assert!(Version::parse(text).is_none(), "{text:?} is not a version");
        }
    }

    #[test]
    fn a_long_text_is_refused_without_reading_it() {
        let long = format!("0.0.{}", "1".repeat(200));
        assert!(Version::parse(&long).is_none());
    }
}
