//! Physical or logical equipment.
//!
//! A device is not a graphical symbol: one contactor has a single device
//! identity but may be represented by a coil, auxiliary contacts and main
//! contacts.

use crate::ids::DeviceId;

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ReferenceDesignator(pub String);

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DeviceKind {
    Contactor,
    Relay,
    ThermalOverload,
    Timer,
    Pushbutton,
    Motor,
    Lamp,
    Buzzer,
    PowerSupply,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Device {
    pub id: DeviceId,
    pub reference: ReferenceDesignator,
    pub kind: DeviceKind,
}
