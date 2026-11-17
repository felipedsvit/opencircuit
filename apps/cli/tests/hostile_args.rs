//! The command line refuses arguments it cannot read instead of crashing.

use std::ffi::OsString;
use std::process::Command;

/// An argument that is not valid Unicode: bytes on Unix, a lone surrogate on
/// Windows.
fn not_unicode() -> OsString {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt as _;
        OsString::from_vec(b"bad\xff.ocircuit".to_vec())
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt as _;
        OsString::from_wide(&[0x62, 0xD800, 0x2E, 0x6F])
    }
}

#[test]
fn an_argument_that_is_not_unicode_is_a_usage_error_not_a_crash() {
    let output = Command::new(env!("CARGO_BIN_EXE_opencircuit"))
        .arg("validate")
        .arg(not_unicode())
        .output()
        .expect("the binary runs");

    assert_eq!(output.status.code(), Some(2));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("is not valid UTF-8"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
