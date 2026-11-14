//! Size limits for project files.
//!
//! The limits themselves live in `circuit_model::limits`, so that the editor
//! refuses an edit the loader would refuse. This module applies them to a
//! document being read or written.

use circuit_model::Schematic;
use circuit_model::limits::LimitExceeded;

use crate::ProjectError;

pub use circuit_model::limits::{
    MAX_ENTITIES_PER_COLLECTION, MAX_PARAMS_PER_DEVICE, MAX_PROJECT_BYTES, MAX_ROUTE_POINTS,
    MAX_TEXT_BYTES,
};

impl From<LimitExceeded> for ProjectError {
    fn from(exceeded: LimitExceeded) -> Self {
        Self::LimitExceeded {
            what: exceeded.what,
            limit: exceeded.limit,
        }
    }
}

/// Check a document against every limit.
pub(crate) fn check(doc: &Schematic) -> Result<(), ProjectError> {
    circuit_model::limits::check(doc).map_err(ProjectError::from)
}
