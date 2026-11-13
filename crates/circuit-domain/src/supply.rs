//! Conductors and explicit supply profiles.
//!
//! Conductors identify the function of a wire (phase, neutral, protective
//! earth, DC pole). They are not instantaneous electrical quantities, and the
//! discrete solver does not calculate voltage or current.

use crate::ids::SupplyId;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Conductor {
    L1,
    L2,
    L3,
    Neutral,
    ProtectiveEarth,
    DcPositive,
    DcNegative,
}

impl Conductor {
    /// Whether this conductor can carry energy to a load.
    ///
    /// Neutral is a return conductor and protective earth must never energize
    /// a load, so neither is considered live in the discrete model.
    pub const fn is_live(self) -> bool {
        matches!(
            self,
            Self::L1 | Self::L2 | Self::L3 | Self::DcPositive | Self::DcNegative
        )
    }

    pub const fn is_protective(self) -> bool {
        matches!(self, Self::ProtectiveEarth)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SupplyKind {
    Dc,
    SinglePhase,
    ThreePhase,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PhaseSequence {
    L1L2L3,
    L1L3L2,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SupplyProfile {
    pub id: SupplyId,
    pub kind: SupplyKind,
    pub nominal_voltage_mv: u32,
    pub frequency_millihz: Option<u32>,
    pub phase_sequence: Option<PhaseSequence>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protective_earth_is_not_a_live_conductor() {
        assert!(!Conductor::ProtectiveEarth.is_live());
        assert!(Conductor::ProtectiveEarth.is_protective());
    }

    #[test]
    fn neutral_is_a_return_conductor_not_a_source() {
        assert!(!Conductor::Neutral.is_live());
        assert!(!Conductor::Neutral.is_protective());
    }

    #[test]
    fn phases_and_dc_poles_are_live() {
        for conductor in [
            Conductor::L1,
            Conductor::L2,
            Conductor::L3,
            Conductor::DcPositive,
            Conductor::DcNegative,
        ] {
            assert!(conductor.is_live());
        }
    }
}
