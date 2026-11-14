//! Conductors and explicit supply profiles.
//!
//! A conductor names the function of a piece of copper (phase, neutral,
//! protective earth, DC pole). It is not an electrical quantity: the discrete
//! solver tracks which conductors are present, never voltage or current.
//! Voltage and frequency fields are labels used only to check that a load
//! matches the supply that feeds it.

use std::fmt;

use serde::ser::Serializer;
use serde::{Deserialize, Serialize};

use crate::ids::SupplyId;

/// Function of a conductor.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Conductor {
    /// First phase.
    L1,
    /// Second phase.
    L2,
    /// Third phase.
    L3,
    /// Neutral.
    N,
    /// Protective earth. Never a source of energy for a load.
    Pe,
    /// Positive DC pole.
    DcPos,
    /// Negative DC pole.
    DcNeg,
}

impl Conductor {
    /// Every conductor, in canonical order.
    pub const ALL: [Self; 7] = [
        Self::L1,
        Self::L2,
        Self::L3,
        Self::N,
        Self::Pe,
        Self::DcPos,
        Self::DcNeg,
    ];

    /// Stable lowercase key used in project files and traces.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::L1 => "l1",
            Self::L2 => "l2",
            Self::L3 => "l3",
            Self::N => "n",
            Self::Pe => "pe",
            Self::DcPos => "dc_pos",
            Self::DcNeg => "dc_neg",
        }
    }

    /// Parse a key produced by [`Conductor::key`].
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|conductor| conductor.key() == key)
    }

    /// Whether this conductor can carry energy to a load.
    ///
    /// Neutral is a return conductor and protective earth must never energize
    /// a load, so neither is live in the discrete model.
    #[must_use]
    pub const fn is_live(self) -> bool {
        matches!(
            self,
            Self::L1 | Self::L2 | Self::L3 | Self::DcPos | Self::DcNeg
        )
    }

    /// Whether this conductor is one of the three AC phases.
    #[must_use]
    pub const fn is_phase(self) -> bool {
        matches!(self, Self::L1 | Self::L2 | Self::L3)
    }

    /// Whether this conductor is protective earth.
    #[must_use]
    pub const fn is_protective(self) -> bool {
        matches!(self, Self::Pe)
    }

    const fn bit(self) -> u8 {
        match self {
            Self::L1 => 0b000_0001,
            Self::L2 => 0b000_0010,
            Self::L3 => 0b000_0100,
            Self::N => 0b000_1000,
            Self::Pe => 0b001_0000,
            Self::DcPos => 0b010_0000,
            Self::DcNeg => 0b100_0000,
        }
    }
}

impl fmt::Display for Conductor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}

/// Outcome of applying the conflict table to the conductors of one group.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ConductorConflict {
    /// Empty set or a single conductor.
    None,
    /// Exactly neutral and protective earth: common at the installation
    /// origin, reported as a warning.
    NeutralEarthBond,
    /// Protective earth together with a live conductor.
    ProtectiveEarthFault,
    /// Two or more conductors that must not meet (including a phase with
    /// neutral, and any AC/DC mix).
    ShortCircuit,
}

/// Set of conductors present in a group, stored as one byte.
#[derive(Clone, Copy, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConductorSet(u8);

impl ConductorSet {
    /// The empty set.
    pub const EMPTY: Self = Self(0);

    const LIVE: u8 = Conductor::L1.bit()
        | Conductor::L2.bit()
        | Conductor::L3.bit()
        | Conductor::DcPos.bit()
        | Conductor::DcNeg.bit();

    /// Set holding exactly one conductor.
    #[must_use]
    pub const fn of(conductor: Conductor) -> Self {
        Self(conductor.bit())
    }

    /// Add a conductor to the set.
    pub const fn insert(&mut self, conductor: Conductor) {
        self.0 |= conductor.bit();
    }

    /// Whether the conductor is present.
    #[must_use]
    pub const fn contains(self, conductor: Conductor) -> bool {
        self.0 & conductor.bit() != 0
    }

    /// Whether no conductor is present.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Number of conductors present.
    #[must_use]
    pub const fn len(self) -> u32 {
        self.0.count_ones()
    }

    /// Union of two sets.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// The conductor, if the set holds exactly one.
    #[must_use]
    pub fn single(self) -> Option<Conductor> {
        if self.len() == 1 {
            self.iter().next()
        } else {
            None
        }
    }

    /// Conductors present, in canonical order.
    pub fn iter(self) -> impl Iterator<Item = Conductor> {
        Conductor::ALL
            .into_iter()
            .filter(move |conductor| self.contains(*conductor))
    }

    /// Apply the conflict table to this set.
    #[must_use]
    pub const fn conflict(self) -> ConductorConflict {
        if self.len() <= 1 {
            return ConductorConflict::None;
        }
        let has_earth = self.contains(Conductor::Pe);
        let has_live = self.0 & Self::LIVE != 0;
        if has_earth && has_live {
            ConductorConflict::ProtectiveEarthFault
        } else if has_earth {
            // Two or more conductors, earth present and none live: the only
            // remaining combination is neutral plus earth.
            ConductorConflict::NeutralEarthBond
        } else {
            ConductorConflict::ShortCircuit
        }
    }
}

impl FromIterator<Conductor> for ConductorSet {
    fn from_iter<I: IntoIterator<Item = Conductor>>(iter: I) -> Self {
        let mut set = Self::EMPTY;
        for conductor in iter {
            set.insert(conductor);
        }
        set
    }
}

impl fmt::Debug for ConductorSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_set().entries(self.iter()).finish()
    }
}

/// Formats as conductor keys joined by `+`, for example `l1+l3`.
impl fmt::Display for ConductorSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut first = true;
        for conductor in self.iter() {
            if !first {
                f.write_str("+")?;
            }
            first = false;
            f.write_str(conductor.key())?;
        }
        Ok(())
    }
}

impl Serialize for ConductorSet {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.iter())
    }
}

/// Kind of supply system.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SupplyKind {
    /// Direct current, for example a 24 V control supply.
    Dc,
    /// One phase and neutral.
    SinglePhase,
    /// Three phases, optionally with neutral.
    ThreePhase,
}

impl SupplyKind {
    /// Whether a supply of this kind can provide the conductor.
    #[must_use]
    pub const fn provides(self, conductor: Conductor) -> bool {
        match self {
            Self::Dc => matches!(
                conductor,
                Conductor::DcPos | Conductor::DcNeg | Conductor::Pe
            ),
            Self::SinglePhase => {
                matches!(conductor, Conductor::L1 | Conductor::N | Conductor::Pe)
            }
            Self::ThreePhase => matches!(
                conductor,
                Conductor::L1 | Conductor::L2 | Conductor::L3 | Conductor::N | Conductor::Pe
            ),
        }
    }
}

/// Order in which the phases of a three-phase supply follow each other.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub enum PhaseSequence {
    /// L1, then L2, then L3.
    #[serde(rename = "l1_l2_l3")]
    L1L2L3,
    /// L1, then L3, then L2.
    #[serde(rename = "l1_l3_l2")]
    L1L3L2,
}

/// A supply system declared by the document.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SupplyProfile {
    /// Stable identifier.
    pub id: SupplyId,
    /// Kind of supply system.
    pub kind: SupplyKind,
    /// Voltage label in millivolts: the DC voltage, the phase-to-neutral
    /// voltage of a single-phase supply, or the line (phase-to-phase) voltage
    /// of a three-phase supply.
    pub nominal_voltage_mv: u32,
    /// Phase-to-neutral voltage label of a three-phase supply with neutral.
    pub phase_voltage_mv: Option<u32>,
    /// Frequency label in millihertz; AC supplies only.
    pub frequency_millihz: Option<u32>,
    /// Phase order; three-phase supplies only.
    pub phase_sequence: Option<PhaseSequence>,
}

impl SupplyProfile {
    /// Why the profile is inconsistent with its kind, if it is.
    ///
    /// The motor direction rule reads `phase_sequence` and the load rules read
    /// the voltage labels, so an inconsistent profile is rejected before
    /// compilation instead of being interpreted.
    #[must_use]
    pub const fn inconsistency(&self) -> Option<&'static str> {
        if self.nominal_voltage_mv == 0 {
            return Some("nominal voltage must be greater than zero");
        }
        if matches!(self.phase_voltage_mv, Some(0)) {
            return Some("phase voltage must be greater than zero when present");
        }
        if matches!(self.frequency_millihz, Some(0)) {
            return Some("frequency must be greater than zero when present");
        }
        match self.kind {
            SupplyKind::Dc => {
                if self.frequency_millihz.is_some() {
                    return Some("a DC supply has no frequency");
                }
                if self.phase_sequence.is_some() {
                    return Some("a DC supply has no phase sequence");
                }
                if self.phase_voltage_mv.is_some() {
                    return Some("a DC supply has no phase voltage");
                }
            }
            SupplyKind::SinglePhase => {
                if self.frequency_millihz.is_none() {
                    return Some("a single-phase supply needs a frequency");
                }
                if self.phase_sequence.is_some() {
                    return Some("a single-phase supply has no phase sequence");
                }
                if self.phase_voltage_mv.is_some() {
                    return Some("a single-phase supply states its voltage as the nominal voltage");
                }
            }
            SupplyKind::ThreePhase => {
                if self.frequency_millihz.is_none() {
                    return Some("a three-phase supply needs a frequency");
                }
                if self.phase_sequence.is_none() {
                    return Some("a three-phase supply needs a phase sequence");
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(conductors: &[Conductor]) -> ConductorSet {
        conductors.iter().copied().collect()
    }

    fn three_phase() -> SupplyProfile {
        SupplyProfile {
            id: SupplyId(1),
            kind: SupplyKind::ThreePhase,
            nominal_voltage_mv: 400_000,
            phase_voltage_mv: Some(230_000),
            frequency_millihz: Some(50_000),
            phase_sequence: Some(PhaseSequence::L1L2L3),
        }
    }

    #[test]
    fn protective_earth_is_not_a_live_conductor() {
        assert!(!Conductor::Pe.is_live());
        assert!(Conductor::Pe.is_protective());
    }

    #[test]
    fn neutral_is_a_return_conductor_not_a_source() {
        assert!(!Conductor::N.is_live());
        assert!(!Conductor::N.is_protective());
    }

    #[test]
    fn phases_and_dc_poles_are_live() {
        let live: Vec<Conductor> = Conductor::ALL
            .into_iter()
            .filter(|conductor| conductor.is_live())
            .collect();

        assert_eq!(
            live,
            [
                Conductor::L1,
                Conductor::L2,
                Conductor::L3,
                Conductor::DcPos,
                Conductor::DcNeg
            ]
        );
    }

    #[test]
    fn conductor_keys_round_trip() {
        for conductor in Conductor::ALL {
            assert_eq!(Conductor::from_key(conductor.key()), Some(conductor));
        }
        assert_eq!(Conductor::from_key("L1"), None);
    }

    #[test]
    fn conductor_set_reports_membership_and_size() {
        let conductors = set(&[Conductor::L3, Conductor::L1]);

        assert!(conductors.contains(Conductor::L1));
        assert!(!conductors.contains(Conductor::L2));
        assert_eq!(conductors.len(), 2);
        assert_eq!(conductors.to_string(), "l1+l3");
        assert_eq!(conductors.single(), None);
        assert_eq!(ConductorSet::of(Conductor::N).single(), Some(Conductor::N));
    }

    #[test]
    fn empty_and_single_conductor_sets_do_not_conflict() {
        assert_eq!(ConductorSet::EMPTY.conflict(), ConductorConflict::None);
        for conductor in Conductor::ALL {
            assert_eq!(
                ConductorSet::of(conductor).conflict(),
                ConductorConflict::None
            );
        }
    }

    #[test]
    fn neutral_with_earth_is_a_bond_not_a_fault() {
        assert_eq!(
            set(&[Conductor::N, Conductor::Pe]).conflict(),
            ConductorConflict::NeutralEarthBond
        );
    }

    #[test]
    fn earth_with_any_live_conductor_is_an_earth_fault() {
        for live in Conductor::ALL.into_iter().filter(|c| c.is_live()) {
            assert_eq!(
                set(&[Conductor::Pe, live]).conflict(),
                ConductorConflict::ProtectiveEarthFault
            );
        }
        assert_eq!(
            set(&[Conductor::Pe, Conductor::N, Conductor::L2]).conflict(),
            ConductorConflict::ProtectiveEarthFault
        );
    }

    #[test]
    fn two_conductors_without_earth_are_a_short_circuit() {
        for pair in [
            [Conductor::L1, Conductor::L2],
            [Conductor::L1, Conductor::N],
            [Conductor::DcPos, Conductor::DcNeg],
            [Conductor::L3, Conductor::DcPos],
            [Conductor::N, Conductor::DcNeg],
        ] {
            assert_eq!(set(&pair).conflict(), ConductorConflict::ShortCircuit);
        }
    }

    #[test]
    fn supply_kind_limits_the_conductors_it_provides() {
        assert!(SupplyKind::Dc.provides(Conductor::DcPos));
        assert!(!SupplyKind::Dc.provides(Conductor::L1));
        assert!(SupplyKind::SinglePhase.provides(Conductor::N));
        assert!(!SupplyKind::SinglePhase.provides(Conductor::L2));
        assert!(SupplyKind::ThreePhase.provides(Conductor::L3));
        assert!(!SupplyKind::ThreePhase.provides(Conductor::DcNeg));
    }

    #[test]
    fn consistent_three_phase_profile_has_no_inconsistency() {
        assert_eq!(three_phase().inconsistency(), None);
    }

    #[test]
    fn three_phase_profile_without_sequence_is_inconsistent() {
        let profile = SupplyProfile {
            phase_sequence: None,
            ..three_phase()
        };

        assert_eq!(
            profile.inconsistency(),
            Some("a three-phase supply needs a phase sequence")
        );
    }

    #[test]
    fn dc_profile_with_phase_sequence_is_inconsistent() {
        let profile = SupplyProfile {
            kind: SupplyKind::Dc,
            nominal_voltage_mv: 24_000,
            phase_voltage_mv: None,
            frequency_millihz: None,
            phase_sequence: Some(PhaseSequence::L1L3L2),
            ..three_phase()
        };

        assert_eq!(
            profile.inconsistency(),
            Some("a DC supply has no phase sequence")
        );
    }

    #[test]
    fn zero_voltage_profile_is_inconsistent() {
        let profile = SupplyProfile {
            nominal_voltage_mv: 0,
            ..three_phase()
        };

        assert_eq!(
            profile.inconsistency(),
            Some("nominal voltage must be greater than zero")
        );
    }
}
