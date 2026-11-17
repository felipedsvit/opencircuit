//! Stable identifiers persisted in project files.
//!
//! Every entity of a document draws its ID from one counter, so IDs of
//! different entity kinds never collide. IDs start at 1, are never reused and
//! stay below [`MAX_ENTITY_ID`]. Dense execution indices live in the simulator
//! and never appear in a document.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Largest accepted entity ID: `2^53 - 1`, which JSON and JavaScript consumers
/// represent exactly.
pub const MAX_ENTITY_ID: u64 = 9_007_199_254_740_991;

macro_rules! entity_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(
            Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub u64);

        impl $name {
            /// Raw numeric value of the identifier.
            #[must_use]
            pub const fn get(self) -> u64 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

entity_id!(
    /// Physical or logical equipment (one contactor, one pushbutton).
    DeviceId
);
entity_id!(
    /// One graphical instance of a device symbol on the sheet.
    SymbolId
);
entity_id!(
    /// A wire between two endpoints.
    WireId
);
entity_id!(
    /// An explicit electrical junction between wires.
    JunctionId
);
entity_id!(
    /// A supply profile declared by the document.
    SupplyId
);
entity_id!(
    /// A drawing sheet: a frame over the canvas (ADR-0011).
    SheetId
);
entity_id!(
    /// A text note on the drawing (ADR-0015).
    TextId
);
entity_id!(
    /// A layer of the drawing (ADR-0015).
    LayerId
);

/// The document ran out of identifiers below [`MAX_ENTITY_ID`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("entity id space exhausted: ids must stay at or below {MAX_ENTITY_ID}")]
pub struct IdExhausted;

/// Single counter that hands out IDs for every entity kind of one document.
///
/// The counter value is persisted as `next_id`, which is why an ID is never
/// handed out twice even after the entity that owned it was deleted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IdAllocator {
    next: u64,
}

impl IdAllocator {
    /// Allocator for an empty document; the first ID is 1.
    #[must_use]
    pub const fn new() -> Self {
        Self { next: 1 }
    }

    /// Allocator that continues a loaded document. Zero is not a valid ID, so
    /// a `next_id` of zero is treated as 1.
    #[must_use]
    pub const fn starting_at(next_id: u64) -> Self {
        Self {
            next: if next_id == 0 { 1 } else { next_id },
        }
    }

    /// The value to persist as `next_id`.
    #[must_use]
    pub const fn next_id(&self) -> u64 {
        self.next
    }

    /// Hand out the next ID.
    ///
    /// # Errors
    ///
    /// Returns [`IdExhausted`] instead of wrapping or exceeding
    /// [`MAX_ENTITY_ID`].
    pub fn alloc(&mut self) -> Result<u64, IdExhausted> {
        let id = self.next;
        if id == 0 || id > MAX_ENTITY_ID {
            return Err(IdExhausted);
        }
        self.next = id.checked_add(1).ok_or(IdExhausted)?;
        Ok(id)
    }
}

impl Default for IdAllocator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocator_starts_at_one_and_never_repeats() {
        let mut ids = IdAllocator::new();

        assert_eq!(
            [ids.alloc(), ids.alloc(), ids.alloc()],
            [Ok(1), Ok(2), Ok(3)]
        );
        assert_eq!(ids.next_id(), 4);
    }

    #[test]
    fn allocator_hands_out_the_last_valid_id_then_fails() {
        let mut ids = IdAllocator::starting_at(MAX_ENTITY_ID);

        assert_eq!(ids.alloc(), Ok(MAX_ENTITY_ID));
        assert_eq!(ids.alloc(), Err(IdExhausted));
        assert_eq!(ids.alloc(), Err(IdExhausted));
    }

    #[test]
    fn allocator_does_not_overflow_at_u64_max() {
        let mut ids = IdAllocator::starting_at(u64::MAX);

        assert_eq!(ids.alloc(), Err(IdExhausted));
    }

    #[test]
    fn allocator_never_hands_out_zero() {
        let mut ids = IdAllocator::starting_at(0);

        assert_eq!(ids.alloc(), Ok(1));
    }

    #[test]
    fn max_entity_id_is_two_to_the_53_minus_one() {
        assert_eq!(MAX_ENTITY_ID, (1_u64 << 53) - 1);
    }
}
