//! The `.ocircuit` project format.
//!
//! A project is JSON. This crate converts between project text and the
//! document model; it never opens a file or knows a path. The application
//! chooses paths and performs atomic writes.
//!
//! Reading happens in two phases. The first reads only `schema_version`: a
//! newer version is refused outright and an older one is migrated. The
//! second deserializes the current schema strictly: an unknown field, a
//! repeated field or a floating-point number is an error, never something to
//! skip or round.
//!
//! Writing is canonical: fixed field order, collections sorted by ID, two
//! spaces of indentation, line feeds and a final newline. Saving a canonical
//! file that was just loaded reproduces it byte for byte.
//!
//! Loading checks syntax, schema and size limits. Whether the document makes
//! sense (IDs, references, models) is reported as diagnostics by
//! [`Schematic::validate`] and by the compiler, so that a broken file can
//! still be opened and explained.

#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]
#![cfg_attr(
    not(test),
    deny(clippy::indexing_slicing, clippy::arithmetic_side_effects)
)]

pub mod library;
mod limits;
mod migrate;
pub mod stdlib;

use circuit_model::{CURRENT_SCHEMA_VERSION, Schematic};
use serde_json::Value;

pub use limits::{
    MAX_ENTITIES_PER_COLLECTION, MAX_PARAMS_PER_DEVICE, MAX_PROJECT_BYTES, MAX_ROUTE_POINTS,
    MAX_TEXT_BYTES,
};
pub use migrate::migrate;

/// Why a project could not be read or written.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ProjectError {
    /// The text is larger than [`MAX_PROJECT_BYTES`].
    #[error("the project is {size} bytes; the limit is {limit}")]
    TooLarge {
        /// Size of the text.
        size: usize,
        /// The limit.
        limit: usize,
    },
    /// The text is not JSON.
    #[error("the project is not valid JSON: {0}")]
    Syntax(String),
    /// `schema_version` is missing or is not a non-negative integer.
    #[error("the project has no integer `schema_version`")]
    MissingSchemaVersion,
    /// The project was written by a newer build.
    #[error(
        "the project uses schema version {found}, but this build supports up to version {supported}"
    )]
    NewerSchema {
        /// Version found in the file.
        found: u64,
        /// Newest version this build reads.
        supported: u32,
    },
    /// No migration starts at this version.
    #[error("there is no migration from schema version {found}")]
    NoMigration {
        /// Version found in the file.
        found: u64,
    },
    /// A migration failed.
    #[error("migrating from schema version {from} failed: {message}")]
    Migration {
        /// Version the migration started from.
        from: u64,
        /// What went wrong.
        message: String,
    },
    /// The text does not match the schema.
    #[error("the project does not match schema version {version}: {message}")]
    Schema {
        /// Schema version the text was checked against.
        version: u32,
        /// What does not match, with line and column when known.
        message: String,
    },
    /// A size limit was exceeded.
    #[error("the project has too many {what}; the limit is {limit}")]
    LimitExceeded {
        /// What there is too much of.
        what: &'static str,
        /// The limit.
        limit: usize,
    },
    /// The document could not be written as JSON.
    #[error("the project could not be serialized: {0}")]
    Serialize(String),
}

/// Read a project from its text.
///
/// # Errors
///
/// Returns [`ProjectError`] when the text is too large, is not JSON, was
/// written by a newer build, cannot be migrated, does not match the schema or
/// exceeds a size limit.
pub fn from_str(text: &str) -> Result<Schematic, ProjectError> {
    if text.len() > MAX_PROJECT_BYTES {
        return Err(ProjectError::TooLarge {
            size: text.len(),
            limit: MAX_PROJECT_BYTES,
        });
    }

    // Phase 1: only the version is trusted.
    let value: Value =
        serde_json::from_str(text).map_err(|error| ProjectError::Syntax(error.to_string()))?;
    let found = migrate::schema_version(&value)?;

    // Phase 2: strict. A document already at the current version is read
    // from the text itself, which also catches repeated fields; a migrated
    // one is read from the migrated value.
    let schema = |error: serde_json::Error| ProjectError::Schema {
        version: CURRENT_SCHEMA_VERSION,
        message: error.to_string(),
    };
    let doc: Schematic = if found == u64::from(CURRENT_SCHEMA_VERSION) {
        serde_json::from_str(text).map_err(schema)?
    } else {
        serde_json::from_value(migrate(value)?).map_err(schema)?
    };

    limits::check(&doc)?;
    Ok(doc)
}

/// Write a project as canonical text.
///
/// # Errors
///
/// Returns [`ProjectError::LimitExceeded`] for a document that could not be
/// read back, and [`ProjectError::Serialize`] if JSON serialization fails.
pub fn to_string(doc: &Schematic) -> Result<String, ProjectError> {
    limits::check(doc)?;
    // A document saved after an edit is already in order; copy it only when
    // it is not.
    let sorted;
    let canonical = if doc.is_canonical() {
        doc
    } else {
        let mut copy = doc.clone();
        copy.sort_canonical();
        sorted = copy;
        &sorted
    };
    let mut text = serde_json::to_string_pretty(canonical)
        .map_err(|error| ProjectError::Serialize(error.to_string()))?;
    text.push('\n');
    Ok(text)
}

#[cfg(test)]
mod tests;
