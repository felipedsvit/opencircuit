//! The signature of the release list (ADR-0020, phase 2). `SHA256SUMS` of each
//! release is signed with the project's minisign key; the program holds the
//! public key and refuses a list that does not verify, or whose trusted comment
//! names another version (so an old, signed list cannot be replayed).

use minisign_verify::{PublicKey, Signature};

use super::UpdateError;
use super::version::Version;

/// The public key of the project, as `minisign -G` writes it: an untrusted
/// comment line, then the key in base64.
pub const PUBLIC_KEY_FILE: &str = include_str!("../../../../packaging/update-public-key.minisign");

/// Check `list`, the bytes of a `SHA256SUMS` file, against `signature`, the text
/// of its `.minisig`, made with the key in `key_file`. The trusted comment of
/// the signature must be `version:<version>`.
///
/// # Errors
///
/// [`UpdateError::SignatureInvalid`] when the key cannot be read, the
/// signature cannot be read, does not verify with that key, or names another
/// version.
pub fn check_list(
    key_file: &str,
    list: &[u8],
    signature: &str,
    version: &Version,
) -> Result<(), UpdateError> {
    let key = public_key(key_file)?;
    let signature = Signature::decode(signature).map_err(|_| UpdateError::SignatureInvalid)?;
    key.verify(list, &signature, false)
        .map_err(|_| UpdateError::SignatureInvalid)?;
    let expected = format!("version:{version}");
    if signature.trusted_comment() != expected {
        return Err(UpdateError::SignatureInvalid);
    }
    Ok(())
}

/// The key of a key file: its last line that is not empty.
fn public_key(key_file: &str) -> Result<PublicKey, UpdateError> {
    let line = key_file
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .ok_or(UpdateError::SignatureInvalid)?;
    PublicKey::from_base64(line.trim()).map_err(|_| UpdateError::SignatureInvalid)
}

#[cfg(test)]
pub(crate) mod fixtures {
    //! A key pair made for the tests, without a password. Its secret key is not
    //! kept anywhere: the signature below is all the tests need.

    /// The public key file of the test key.
    pub const KEY_FILE: &str = "untrusted comment: minisign public key\n\
        RWQWDXShX5gdF6beKTWUeaTa042fJ4klbfIxRkEC4MsjvziXFfFec9Ay\n";

    /// A `SHA256SUMS` with one line: the SHA-256 of `b"an installer"`, for the
    /// file `opencircuit_0.0.3_amd64.deb`.
    pub const LIST: &str = "b2b3a11c02f14e36f2a5c148db1b5924fa96141da4b2fbfb262a469df53f6750  opencircuit_0.0.3_amd64.deb\n";

    /// The contents the installer has in the tests.
    pub const INSTALLER: &[u8] = b"an installer";

    /// The signature of `LIST`, with the trusted comment `version:0.0.3`.
    pub const SIGNATURE: &str = "untrusted comment: signature from minisign secret key\n\
        RUQWDXShX5gdF9MMVMsUzwDEHbkxKOa7Ugz0f+DoTOq1DQpdpvNP1HE4s9Ruh1CgyYsBg1xnO7j0+RpSsFCkaOaYxgJx0v4smwI=\n\
        trusted comment: version:0.0.3\n\
        NSXos5gGPTsaPxdAyxe+2wJIW/1YM1PMFEaR5VjhIvm5b4VM8qqRltSd8LceWu0SmxjOpPP1exVBEuRREda3Dw==\n";
}

#[cfg(test)]
mod tests {
    use super::fixtures::{KEY_FILE, LIST, SIGNATURE};
    use super::{PUBLIC_KEY_FILE, check_list, public_key};
    use crate::update::UpdateError;
    use crate::update::version::Version;

    fn version(text: &str) -> Version {
        match Version::parse(text) {
            Some(version) => version,
            None => panic!("{text} is a version"),
        }
    }

    #[test]
    fn a_list_signed_for_its_version_is_accepted() {
        let result = check_list(KEY_FILE, LIST.as_bytes(), SIGNATURE, &version("0.0.3"));

        assert_eq!(result, Ok(()));
    }

    #[test]
    fn a_signed_list_is_refused_for_another_version() {
        let result = check_list(KEY_FILE, LIST.as_bytes(), SIGNATURE, &version("0.0.4"));

        assert_eq!(result, Err(UpdateError::SignatureInvalid));
    }

    #[test]
    fn a_changed_list_is_refused() {
        let changed = LIST.replacen("b2b3", "b2b4", 1);

        let result = check_list(KEY_FILE, changed.as_bytes(), SIGNATURE, &version("0.0.3"));

        assert_eq!(result, Err(UpdateError::SignatureInvalid));
    }

    #[test]
    fn a_signature_from_another_key_is_refused() {
        let result = check_list(
            PUBLIC_KEY_FILE,
            LIST.as_bytes(),
            SIGNATURE,
            &version("0.0.3"),
        );

        assert_eq!(result, Err(UpdateError::SignatureInvalid));
    }

    #[test]
    fn damaged_signatures_and_keys_are_refused_without_a_panic() {
        for signature in ["", "x", "untrusted comment: x\nnot base64\n", "\n\n\n\n"] {
            assert_eq!(
                check_list(KEY_FILE, LIST.as_bytes(), signature, &version("0.0.3")),
                Err(UpdateError::SignatureInvalid),
                "{signature:?}"
            );
        }
        for key in ["", "\n\n", "untrusted comment: x\nRWQ"] {
            assert_eq!(
                check_list(key, LIST.as_bytes(), SIGNATURE, &version("0.0.3")),
                Err(UpdateError::SignatureInvalid),
                "{key:?}"
            );
        }
    }

    #[test]
    fn the_project_key_file_has_a_readable_key() {
        assert!(public_key(PUBLIC_KEY_FILE).is_ok());
    }
}
