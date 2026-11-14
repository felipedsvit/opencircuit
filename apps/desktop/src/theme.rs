//! Colours and strokes: the only place where a [`StyleKey`] gets a look.
//!
//! Conductor colours follow the usual wire identification (brown, black and
//! grey phases, blue neutral, green protective earth); a wire carrying more
//! than one conductor is drawn in a conflict colour.

use circuit_model::{Conductor, ConductorSet, MotorState};
use circuit_renderer::StyleKey;
use eframe::egui::{Color32, Stroke};

/// How one item is drawn, at zoom 1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    /// Colour of lines and texts, and fill of dots.
    pub color: Color32,
    /// Stroke width in screen points.
    pub width: f32,
}

impl Look {
    /// The stroke of this look.
    #[must_use]
    pub fn stroke(self) -> Stroke {
        Stroke::new(self.width, self.color)
    }
}

const ENERGIZED: Color32 = Color32::from_rgb(240, 160, 0);
const ACTIVE: Color32 = Color32::from_rgb(40, 190, 90);
const FAULT: Color32 = Color32::from_rgb(230, 40, 40);
const SELECTED: Color32 = Color32::from_rgb(0, 170, 230);
const CONFLICT: Color32 = Color32::from_rgb(220, 0, 220);
const REVERSE: Color32 = Color32::from_rgb(60, 120, 240);

/// The colour of a single conductor. Black phase `L2` is drawn in the
/// foreground colour so that it stays visible on a dark background.
fn conductor_color(conductor: Conductor, foreground: Color32) -> Color32 {
    match conductor {
        Conductor::L1 => Color32::from_rgb(160, 90, 40),
        Conductor::L2 => foreground,
        Conductor::L3 => Color32::from_gray(128),
        Conductor::N => Color32::from_rgb(40, 110, 230),
        Conductor::Pe => Color32::from_rgb(40, 170, 60),
        Conductor::DcPos => Color32::from_rgb(220, 50, 50),
        Conductor::DcNeg => Color32::from_rgb(0, 150, 210),
    }
}

/// The colour of a wire or junction with the given conductors.
fn live_color(conductors: ConductorSet, foreground: Color32, dead: Color32) -> Color32 {
    if conductors.is_empty() {
        dead
    } else {
        conductors
            .single()
            .map_or(CONFLICT, |conductor| conductor_color(conductor, foreground))
    }
}

/// The look of a style on a background that is dark or light.
#[must_use]
pub fn look(style: StyleKey, dark_mode: bool) -> Look {
    let foreground = if dark_mode {
        Color32::from_gray(220)
    } else {
        Color32::from_gray(20)
    };
    let dim = if dark_mode {
        Color32::from_gray(120)
    } else {
        Color32::from_gray(140)
    };
    let (color, width) = match style {
        StyleKey::Wire { conductors } => (
            live_color(conductors, foreground, dim),
            if conductors.is_empty() { 1.0 } else { 2.0 },
        ),
        StyleKey::Junction { conductors } => (live_color(conductors, foreground, dim), 1.0),
        StyleKey::Symbol | StyleKey::Label => (foreground, 1.5),
        StyleKey::Linkage => (dim, 1.0),
        StyleKey::Energized | StyleKey::Actuated => (ENERGIZED, 2.0),
        StyleKey::Active | StyleKey::ContactClosed => (ACTIVE, 2.0),
        StyleKey::Tripped => (FAULT, 2.0),
        StyleKey::Motor(MotorState::Stopped) => (foreground, 1.5),
        StyleKey::Motor(MotorState::Forward) => (ACTIVE, 2.5),
        StyleKey::Motor(MotorState::Reverse) => (REVERSE, 2.5),
        StyleKey::Fault => (FAULT, 3.0),
        StyleKey::Selected => (SELECTED, 2.5),
        StyleKey::Unknown => (CONFLICT, 1.5),
    };
    Look { color, width }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dead_wire_is_dim_and_thin() {
        let dead = look(
            StyleKey::Wire {
                conductors: ConductorSet::default(),
            },
            true,
        );

        assert_eq!(
            dead,
            Look {
                color: Color32::from_gray(120),
                width: 1.0
            }
        );
    }

    #[test]
    fn wire_with_one_conductor_takes_its_colour() {
        let live = look(
            StyleKey::Wire {
                conductors: ConductorSet::of(Conductor::N),
            },
            false,
        );

        assert_eq!(live.color, Color32::from_rgb(40, 110, 230));
    }

    #[test]
    fn wire_with_several_conductors_is_drawn_as_a_conflict() {
        let mut both = ConductorSet::of(Conductor::DcPos);
        both.insert(Conductor::DcNeg);

        assert_eq!(
            look(StyleKey::Wire { conductors: both }, true).color,
            CONFLICT
        );
    }

    #[test]
    fn black_phase_stays_visible_on_a_dark_background() {
        let l2 = look(
            StyleKey::Wire {
                conductors: ConductorSet::of(Conductor::L2),
            },
            true,
        );

        assert_eq!(l2.color, Color32::from_gray(220));
    }
}
