//! External inputs of a simulation.
//!
//! Inputs are identified by [`DeviceId`]. Front ends resolve designators such
//! as `S1` to an ID before calling the simulator; pointer and frame events
//! never reach the core.

use serde::{Deserialize, Serialize};

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

/// An action together with the tick it applies to: the unit of a scenario and
/// of a replay log.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
pub struct InputEvent {
    /// Tick in which the action takes effect.
    pub tick: u64,
    /// The action.
    pub action: InputAction,
}
