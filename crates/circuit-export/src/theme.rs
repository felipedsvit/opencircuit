//! Colours and stroke widths of an export: the only place where a
//! [`StyleKey`] gets a look on paper.

use circuit_model::{Conductor, ConductorSet, MotorState};
use circuit_renderer::StyleKey;

/// A colour, eight bits per channel.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    const BLACK: Self = Self(0, 0, 0);
    const GREY: Self = Self(128, 128, 128);
    const FAULT: Self = Self(220, 30, 30);
    const ENERGIZED: Self = Self(225, 140, 0);
    const ACTIVE: Self = Self(30, 160, 70);
    const REVERSE: Self = Self(50, 100, 220);
    const SELECTED: Self = Self(0, 150, 210);
    const TRACED: Self = Self(230, 140, 0);
    const CONFLICT: Self = Self(200, 0, 200);
}

/// How one item is drawn.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Look {
    /// Colour of lines and texts, and fill of dots.
    pub color: Rgb,
    /// Stroke width in tenths of a scene subunit.
    pub width_tenths: u32,
}

/// The theme of an export.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum Theme {
    /// Black on white for printing and documentation: the drawing only.
    /// Simulation state shows as stroke width, and faults stay red.
    #[default]
    Print,
    /// Colours of the running simulation: conductors by their usual wire
    /// colours, energized, active and faulted elements highlighted.
    Live,
}

/// The colour of a single conductor: brown, black and grey phases, blue
/// neutral, green protective earth, red and blue DC poles.
const fn conductor_color(conductor: Conductor) -> Rgb {
    match conductor {
        Conductor::L1 => Rgb(150, 85, 35),
        Conductor::L2 => Rgb::BLACK,
        Conductor::L3 => Rgb::GREY,
        Conductor::N => Rgb(30, 100, 220),
        Conductor::Pe => Rgb(30, 150, 50),
        Conductor::DcPos => Rgb(210, 40, 40),
        Conductor::DcNeg => Rgb(0, 140, 200),
    }
}

/// The colour of a wire or junction carrying `conductors`: grey when dead,
/// the conductor colour when it carries one, the conflict colour otherwise.
fn live_color(conductors: ConductorSet) -> Rgb {
    if conductors.is_empty() {
        Rgb::GREY
    } else {
        conductors.single().map_or(Rgb::CONFLICT, conductor_color)
    }
}

impl Theme {
    /// The look of a style in this theme.
    #[must_use]
    pub fn look(self, style: StyleKey) -> Look {
        let look = |color, width_tenths| Look {
            color,
            width_tenths,
        };
        match self {
            Self::Print => match style {
                StyleKey::Wire { conductors } | StyleKey::Junction { conductors } => {
                    look(Rgb::BLACK, if conductors.is_empty() { 10 } else { 20 })
                }
                StyleKey::Symbol | StyleKey::Label | StyleKey::Linkage => look(Rgb::BLACK, 12),
                StyleKey::Energized
                | StyleKey::Active
                | StyleKey::ContactClosed
                | StyleKey::Actuated
                | StyleKey::Motor(_) => look(Rgb::BLACK, 20),
                StyleKey::Tripped | StyleKey::Fault => look(Rgb::FAULT, 25),
                StyleKey::Selected => look(Rgb::SELECTED, 20),
                StyleKey::Traced => look(Rgb::TRACED, 20),
                StyleKey::Frame => look(Rgb::BLACK, 8),
                StyleKey::Unknown => look(Rgb::GREY, 12),
            },
            Self::Live => match style {
                StyleKey::Wire { conductors } => look(
                    live_color(conductors),
                    if conductors.is_empty() { 10 } else { 20 },
                ),
                StyleKey::Junction { conductors } => look(live_color(conductors), 10),
                StyleKey::Symbol | StyleKey::Label => look(Rgb::BLACK, 12),
                StyleKey::Linkage => look(Rgb::GREY, 10),
                StyleKey::Energized | StyleKey::Actuated => look(Rgb::ENERGIZED, 20),
                StyleKey::Active | StyleKey::ContactClosed => look(Rgb::ACTIVE, 20),
                StyleKey::Tripped => look(Rgb::FAULT, 20),
                StyleKey::Motor(MotorState::Stopped) => look(Rgb::BLACK, 12),
                StyleKey::Motor(MotorState::Forward) => look(Rgb::ACTIVE, 25),
                StyleKey::Motor(MotorState::Reverse) => look(Rgb::REVERSE, 25),
                StyleKey::Fault => look(Rgb::FAULT, 30),
                StyleKey::Selected => look(Rgb::SELECTED, 25),
                StyleKey::Traced => look(Rgb::TRACED, 25),
                StyleKey::Frame => look(Rgb::BLACK, 8),
                StyleKey::Unknown => look(Rgb::CONFLICT, 12),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_theme_draws_the_schematic_in_black() {
        for style in [
            StyleKey::Symbol,
            StyleKey::Label,
            StyleKey::Wire {
                conductors: ConductorSet::EMPTY,
            },
        ] {
            assert_eq!(Theme::Print.look(style).color, Rgb::BLACK, "{style:?}");
        }
        assert_eq!(Theme::Print.look(StyleKey::Fault).color, Rgb::FAULT);
    }

    #[test]
    fn live_theme_colours_wires_by_conductor() {
        let wire = |conductors| Theme::Live.look(StyleKey::Wire { conductors }).color;

        assert_eq!(wire(ConductorSet::of(Conductor::N)), Rgb(30, 100, 220));
        assert_eq!(wire(ConductorSet::EMPTY), Rgb::GREY);
        let both: ConductorSet = [Conductor::L1, Conductor::N].into_iter().collect();
        assert_eq!(wire(both), Rgb::CONFLICT);
    }

    #[test]
    fn every_width_is_positive() {
        let styles = [
            StyleKey::Symbol,
            StyleKey::Energized,
            StyleKey::Active,
            StyleKey::ContactClosed,
            StyleKey::Linkage,
            StyleKey::Actuated,
            StyleKey::Tripped,
            StyleKey::Motor(MotorState::Forward),
            StyleKey::Fault,
            StyleKey::Selected,
            StyleKey::Traced,
            StyleKey::Label,
            StyleKey::Unknown,
        ];
        for theme in [Theme::Print, Theme::Live] {
            for style in styles {
                assert!(theme.look(style).width_tenths > 0, "{theme:?} {style:?}");
            }
        }
    }
}
