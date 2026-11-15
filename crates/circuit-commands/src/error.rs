//! Why an edit was refused.

use circuit_model::limits::LimitExceeded;
use circuit_model::{DeviceId, IdExhausted, Subject};

/// An edit that was refused. The document is never changed when an edit,
/// an operation or a transaction returns one.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CommandError {
    /// An entity with this ID already exists.
    #[error("{0} already exists")]
    AlreadyExists(Subject),
    /// The entity does not exist.
    #[error("{0} does not exist")]
    Missing(Subject),
    /// The entity cannot be removed while another one uses it.
    #[error("{subject} is still used by {by}")]
    InUse {
        /// The entity to remove.
        subject: Subject,
        /// One entity that still uses it.
        by: Subject,
    },
    /// The text is not a valid reference designator.
    #[error("`{0}` is not a valid reference designator")]
    InvalidReference(String),
    /// Another device already has this reference designator.
    #[error("reference designator `{0}` is already used")]
    DuplicateReference(String),
    /// The component model is not in the catalog.
    #[error("model `{0}` is not in the catalog")]
    UnknownModel(String),
    /// No newer version of the model is registered as an upgrade.
    #[error("model `{0}` has no upgrade")]
    NoUpgrade(String),
    /// The model has no symbol with this key.
    #[error("model `{model}` has no symbol `{symbol}`")]
    UnknownSymbol {
        /// The model.
        model: String,
        /// The symbol key.
        symbol: String,
    },
    /// The device already has as many symbols of this kind as its model
    /// allows.
    #[error("device {device} already has the most `{symbol}` symbols its model allows")]
    InstanceLimit {
        /// The device.
        device: DeviceId,
        /// The symbol key.
        symbol: String,
    },
    /// A parameter value does not fit its definition.
    #[error("invalid parameter: {0}")]
    InvalidParameter(String),
    /// A supply profile is inconsistent with its kind.
    #[error("invalid supply profile: {0}")]
    InvalidSupply(&'static str),
    /// A wire would start and end at the same point.
    #[error("a wire cannot start and end at the same point")]
    SelfLoop,
    /// The point where a wire should be split is not on it.
    #[error("the point is not on wire {0}")]
    NotOnWire(circuit_model::WireId),
    /// A position would leave the coordinate range.
    #[error("the result is beyond the coordinate range")]
    OutOfRange,
    /// The document would exceed a size limit and could not be saved.
    #[error(transparent)]
    Limit(#[from] LimitExceeded),
    /// The document has no identifier left.
    #[error(transparent)]
    Ids(#[from] IdExhausted),
    /// The edit would add a structural error to the document.
    #[error("the edit would break the document: {0}")]
    BreaksIntegrity(String),
    /// A sheet would be invalid: a number used twice, a frame without zones,
    /// of a size the format does not allow, beyond the coordinate range or
    /// over another sheet.
    #[error("invalid sheet: {0}")]
    InvalidSheet(String),
}
