//! Every way a save file can be refused, in plain words.
//!
//! The loader never returns a half snapshot: any error here
//! means `load` returns `Err` and the caller keeps whatever
//! state it already had. Corruption, truncation, trailing
//! garbage, and versions from the future are all refusals,
//! never repairs — the engine does not guess at bytes.

use std::fmt;

/// A save file that will not load, and why, plainly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistError {
    /// First four bytes are not the `C35S` magic.
    BadMagic,
    /// Version stamp is not one this loader knows. The payload
    /// carries the version found, so the message can name it.
    /// A version from the future is refused, never coerced.
    UnsupportedVersion(u16),
    /// The file ends before the declared body or checksum.
    Truncated,
    /// Bytes remain after the declared body and checksum.
    TrailingData,
    /// The checksum over the envelope does not match: some
    /// byte changed after the save was written.
    ChecksumMismatch,
    /// A length-prefixed string is not valid UTF-8.
    InvalidUtf8,
    /// An enum discriminant is not one this version defines.
    InvalidEnum(u8),
    /// A length prefix runs past the end of the body.
    Overrun,
}

impl fmt::Display for PersistError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PersistError::BadMagic => write!(f, "not a C35 save file (bad magic)"),
            PersistError::UnsupportedVersion(v) => write!(
                f,
                "save version {} is not supported by this loader (supports version {})",
                v,
                super::format::FORMAT_VERSION
            ),
            PersistError::Truncated => write!(f, "save file is truncated"),
            PersistError::TrailingData => write!(f, "save file has trailing data"),
            PersistError::ChecksumMismatch => {
                write!(f, "save file checksum mismatch (corrupted)")
            }
            PersistError::InvalidUtf8 => write!(f, "save file contains invalid UTF-8"),
            PersistError::InvalidEnum(d) => {
                write!(f, "save file contains unknown enum value {}", d)
            }
            PersistError::Overrun => {
                write!(f, "save file length prefix runs past the end")
            }
        }
    }
}

impl std::error::Error for PersistError {}
