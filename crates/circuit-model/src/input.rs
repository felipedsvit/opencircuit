//! External inputs of a simulation.
//!
//! Inputs are identified by [`DeviceId`]. Front ends resolve designators such
//! as `S1` to an ID before calling the simulator; pointer and frame events
//! never reach the core.

use serde::{Deserialize, Serialize};

use crate::catalog::DeviceClass;
use crate::ids::DeviceId;

/// One operator action applied at a tick boundary.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum InputAction {
    /// Press a pushbutton; it stays pressed until released.
    Press(DeviceId),
    /// Release a pushbutton.
    Release(DeviceId),
    /// Close (`true`) or open (`false`) a latching switch.
    SetClosed(DeviceId, bool),
    /// Trip a thermal overload relay; the trip latches.
    Trip(DeviceId),
    /// Reset a tripped thermal overload relay.
    ResetTrip(DeviceId),
}

impl InputAction {
    /// The device the action targets.
    #[must_use]
    pub const fn device(self) -> DeviceId {
        match self {
            Self::Press(device)
            | Self::Release(device)
            | Self::SetClosed(device, _)
            | Self::Trip(device)
            | Self::ResetTrip(device) => device,
        }
    }
}

/// What an operator can do to a device: the pair of actions a front end
/// offers for it.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Control {
    /// Hold a pushbutton: [`InputAction::Press`] while held,
    /// [`InputAction::Release`] after.
    Hold,
    /// Close or open a maintained switch with [`InputAction::SetClosed`].
    Toggle,
    /// Trip a thermal overload relay with [`InputAction::Trip`] and reset it
    /// with [`InputAction::ResetTrip`].
    Trip,
}

impl Control {
    /// Every control.
    pub const ALL: [Self; 3] = [Self::Hold, Self::Toggle, Self::Trip];

    /// The two actions of this control on a device: engage, then disengage.
    #[must_use]
    pub const fn actions(self, device: DeviceId) -> [InputAction; 2] {
        match self {
            Self::Hold => [InputAction::Press(device), InputAction::Release(device)],
            Self::Toggle => [
                InputAction::SetClosed(device, true),
                InputAction::SetClosed(device, false),
            ],
            Self::Trip => [InputAction::Trip(device), InputAction::ResetTrip(device)],
        }
    }
}

/// The controls an operator has over a device of this class. Front ends
/// offer exactly these, so that they never decide on their own which device
/// can be operated; the simulator rejects any other action.
#[must_use]
pub const fn operator_controls(class: DeviceClass) -> &'static [Control] {
    match class {
        DeviceClass::Pushbutton => &[Control::Hold],
        DeviceClass::LatchingSwitch => &[Control::Toggle],
        DeviceClass::ThermalOverload => &[Control::Trip],
        DeviceClass::Contactor
        | DeviceClass::Relay
        | DeviceClass::Timer(_)
        | DeviceClass::Fuse
        | DeviceClass::Motor
        | DeviceClass::Lamp
        | DeviceClass::Buzzer
        | DeviceClass::PowerSupply
        | DeviceClass::MechanicalInterlock => &[],
    }
}

/// An action together with the tick it applies to: the unit of a scenario and
/// of a replay log.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct InputEvent {
    /// Tick in which the action takes effect.
    pub tick: u64,
    /// The action.
    pub action: InputAction,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::TimerMode;

    #[test]
    fn each_control_engages_then_releases_the_same_device() {
        let device = DeviceId(7);
        for control in Control::ALL {
            let [engage, release] = control.actions(device);

            assert_eq!(
                (engage.device(), release.device()),
                (device, device),
                "{control:?}"
            );
            assert_ne!(engage, release, "{control:?}");
        }
    }

    #[test]
    fn only_buttons_switches_and_thermal_relays_have_controls() {
        assert_eq!(
            [
                DeviceClass::Pushbutton,
                DeviceClass::LatchingSwitch,
                DeviceClass::ThermalOverload,
            ]
            .map(operator_controls),
            [&[Control::Hold][..], &[Control::Toggle], &[Control::Trip]]
        );
        for class in [
            DeviceClass::Contactor,
            DeviceClass::Relay,
            DeviceClass::Timer(TimerMode::OnDelay),
            DeviceClass::Fuse,
            DeviceClass::Motor,
            DeviceClass::Lamp,
            DeviceClass::Buzzer,
            DeviceClass::PowerSupply,
            DeviceClass::MechanicalInterlock,
        ] {
            assert_eq!(operator_controls(class), &[], "{class:?}");
        }
    }
}
