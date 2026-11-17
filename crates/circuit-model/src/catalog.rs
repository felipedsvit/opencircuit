//! Component catalog: the source of truth for models, symbols, terminals and
//! parameters.
//!
//! The document stores only a versioned model reference per device; class,
//! symbols and terminals come from here. Terminals belong to symbols, not to
//! devices, and a symbol's terminals are fixed by its kind so that a catalog
//! can never describe a coil without `a1`/`a2`.
//!
//! The catalog is defined in Rust code for now. It also fixes the geometry
//! every view agrees on: where each terminal sits relative to its symbol and
//! the area a symbol covers. It draws nothing; the artwork belongs to the
//! renderer.

use std::collections::{BTreeMap, BTreeSet};

use crate::geometry::{GridPoint, GridRect};
use crate::library::{
    Artwork, Library, MAX_LIBRARY_MODELS, MAX_LIBRARY_SYMBOLS, check_artwork, is_valid_library_id,
    is_valid_model_name,
};
use crate::limits::MAX_TEXT_BYTES;
use crate::schematic::{ModelRef, ParamValue, SymbolKey, TerminalKey};
use crate::supply::Conductor;

/// Parameter key: conductor provided by a supply terminal.
pub const PARAM_CONDUCTOR: &str = "conductor";
/// Parameter key: supply profile a supply terminal belongs to.
pub const PARAM_SUPPLY: &str = "supply";
/// Parameter key: rated voltage label of a load, in millivolts.
pub const PARAM_RATED_VOLTAGE_MV: &str = "rated_voltage_mv";
/// Parameter key: how a two-terminal load connects to its supply.
pub const PARAM_CONNECTION: &str = "connection";
/// Parameter key: timer preset in milliseconds.
pub const PARAM_PRESET_MS: &str = "preset_ms";
/// Parameter key: whether a latching switch starts closed.
pub const PARAM_INITIAL_CLOSED: &str = "initial_closed";
/// Parameter key: supply profile a control transformer provides.
pub const PARAM_SECONDARY: &str = "secondary";
/// Parameter key: first contactor of a mechanical interlock.
pub const PARAM_FIRST: &str = "first";
/// Parameter key: second contactor of a mechanical interlock.
pub const PARAM_SECOND: &str = "second";

/// Largest pole count of a multi-pole symbol.
pub const MAX_POLES: u8 = 4;

/// Whether a timer delays switching on or switching off.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TimerMode {
    /// TON: the output follows the input after the preset.
    OnDelay,
    /// TOF: the output drops after the input has been off for the preset.
    OffDelay,
}

/// Behaviour class of a device. Closed on purpose: every class has behaviour
/// implemented by the simulator. It is a catalog property and is never
/// persisted in a document.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DeviceClass {
    /// Coil with main and auxiliary contacts.
    Contactor,
    /// Coil with auxiliary contacts.
    Relay,
    /// Thermal overload relay, modelled as a manual latch.
    ThermalOverload,
    /// Timing relay.
    Timer(TimerMode),
    /// Momentary pushbutton.
    Pushbutton,
    /// Maintained switch: selector, main switch, latching emergency stop.
    LatchingSwitch,
    /// Passive fuse; melting is not simulated.
    Fuse,
    /// Three-phase motor.
    Motor,
    /// Indicator lamp.
    Lamp,
    /// Buzzer.
    Buzzer,
    /// Tap of a supply conductor.
    PowerSupply,
    /// Mechanical interlock between two contactors: at most one of them is
    /// picked up at any time.
    MechanicalInterlock,
    /// Control transformer: a primary load and a secondary that provides a
    /// separate single-phase section while the primary is powered.
    Transformer,
}

/// How an operator or a machine acts on a device, for the drawing and the
/// designator. It changes nothing in the simulation: the class decides the
/// behaviour.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Operator {
    /// Driven by a coil, a timer, a thermal relay, or nothing.
    None,
    /// Momentary pushbutton.
    Push,
    /// Maintained switch.
    Latch,
    /// Emergency stop: a maintained mushroom head.
    Emergency,
    /// Position or level switch, actuated by the machine.
    Position,
}

impl Operator {
    /// The operator a class has unless its model says otherwise.
    #[must_use]
    pub const fn default_for(class: DeviceClass) -> Self {
        match class {
            DeviceClass::Pushbutton => Self::Push,
            DeviceClass::LatchingSwitch => Self::Latch,
            _ => Self::None,
        }
    }

    /// Whether a model of `class` may use this operator.
    #[must_use]
    pub const fn fits(self, class: DeviceClass) -> bool {
        match self {
            Self::Push | Self::Position => matches!(class, DeviceClass::Pushbutton),
            Self::Latch | Self::Emergency => matches!(class, DeviceClass::LatchingSwitch),
            Self::None => !matches!(class, DeviceClass::Pushbutton | DeviceClass::LatchingSwitch),
        }
    }
}

/// What a symbol does in the solver and in the logic.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SymbolKind {
    /// Coil: a two-terminal load.
    Coil,
    /// Normally open contact.
    ContactNo,
    /// Normally closed contact.
    ContactNc,
    /// Changeover contact: a common terminal that connects to `nc` at rest
    /// and to `no` while the device signal is high.
    Changeover,
    /// Main contacts; every pole closes together.
    PowerContacts {
        /// Number of poles.
        poles: u8,
    },
    /// Current path of a thermal overload relay; always conducting.
    ThermalElement {
        /// Number of poles.
        poles: u8,
    },
    /// Fuse link; always conducting.
    FuseLink,
    /// Three-terminal motor.
    Motor,
    /// Motor with both ends of its three windings brought out: `u1`-`u2`,
    /// `v1`-`v2`, `w1`-`w2`; connected in star or delta by the circuit.
    MotorSixTerminal,
    /// Lamp: a two-terminal load.
    Lamp,
    /// Buzzer: a two-terminal load.
    Buzzer,
    /// Point where a supply conductor enters the circuit.
    SupplyTerminal,
    /// Mark of a mechanical interlock; it has no terminals.
    InterlockLink,
    /// Primary winding of a transformer: a two-terminal load.
    TransformerPrimary,
    /// Secondary winding of a transformer: provides `l1` on `x1` and `n` on
    /// `x2` while the transformer is powered.
    TransformerSecondary,
}

/// Vertical offset of the terminals above the origin of a symbol, in grid
/// units.
const ABOVE: i32 = -2;

/// Vertical offset of the terminals below the origin of a symbol, in grid
/// units.
const BELOW: i32 = 2;

/// Horizontal distance between the poles of a multi-pole symbol, in grid
/// units.
pub const POLE_PITCH: i32 = 4;

/// Offset of the leftmost motor terminal, one pitch left of the origin.
const MOTOR_LEFT: i32 = -4;

/// Vertical offset of the second ends (`u2`, `v2`, `w2`) of the windings of
/// a six-terminal motor, below the motor.
const MOTOR_SECOND_ENDS: i32 = 8;

/// Horizontal offset of the `nc` terminal of a changeover contact; `com`
/// and `no` sit on the vertical through the origin.
const CHANGEOVER_NC: i32 = 2;

impl SymbolKind {
    /// The terminals of a symbol of this kind, in canonical order, with their
    /// offsets from the symbol origin.
    ///
    /// Two-terminal elements list their two ends; multi-pole elements list
    /// `poleN.in` then `poleN.out` for each pole; a changeover contact lists
    /// `com`, `nc`, `no`; the motor lists `u1`, `v1`, `w1`.
    ///
    /// Unrotated, a symbol is drawn top to bottom around its origin: the
    /// first terminal of a two-terminal element is 2 grid units above it and
    /// the second 2 below; a changeover contact has `com` 2 units below,
    /// `no` 2 units above and `nc` 2 units above and 2 to the right; the
    /// poles of a multi-pole element are
    /// [`POLE_PITCH`] apart and centred, with their inputs above and their
    /// outputs below; the motor terminals sit above the motor; a supply
    /// terminal is its own origin.
    #[must_use]
    pub fn terminals(self) -> Vec<TerminalDefinition> {
        let pair = |first: &str, second: &str, required: bool| {
            vec![
                TerminalDefinition::new(first, required, GridPoint::new(0, ABOVE)),
                TerminalDefinition::new(second, required, GridPoint::new(0, BELOW)),
            ]
        };
        match self {
            Self::Coil | Self::TransformerPrimary => pair("a1", "a2", true),
            Self::TransformerSecondary => pair("x1", "x2", false),
            Self::ContactNo | Self::ContactNc | Self::FuseLink => pair("p1", "p2", false),
            Self::Changeover => vec![
                TerminalDefinition::new("com", false, GridPoint::new(0, BELOW)),
                TerminalDefinition::new("nc", false, GridPoint::new(CHANGEOVER_NC, ABOVE)),
                TerminalDefinition::new("no", false, GridPoint::new(0, ABOVE)),
            ],
            Self::PowerContacts { poles } | Self::ThermalElement { poles } => (1..=poles)
                .flat_map(|pole| {
                    let x = pole_x(pole, poles);
                    [
                        TerminalDefinition::new(
                            &format!("pole{pole}.in"),
                            false,
                            GridPoint::new(x, ABOVE),
                        ),
                        TerminalDefinition::new(
                            &format!("pole{pole}.out"),
                            false,
                            GridPoint::new(x, BELOW),
                        ),
                    ]
                })
                .collect(),
            Self::Motor => vec![
                TerminalDefinition::new("u1", false, GridPoint::new(MOTOR_LEFT, ABOVE)),
                TerminalDefinition::new("v1", false, GridPoint::new(0, ABOVE)),
                TerminalDefinition::new("w1", false, GridPoint::new(POLE_PITCH, ABOVE)),
            ],
            Self::MotorSixTerminal => vec![
                TerminalDefinition::new("u1", false, GridPoint::new(MOTOR_LEFT, ABOVE)),
                TerminalDefinition::new("v1", false, GridPoint::new(0, ABOVE)),
                TerminalDefinition::new("w1", false, GridPoint::new(POLE_PITCH, ABOVE)),
                TerminalDefinition::new("u2", false, GridPoint::new(MOTOR_LEFT, MOTOR_SECOND_ENDS)),
                TerminalDefinition::new("v2", false, GridPoint::new(0, MOTOR_SECOND_ENDS)),
                TerminalDefinition::new("w2", false, GridPoint::new(POLE_PITCH, MOTOR_SECOND_ENDS)),
            ],
            Self::Lamp | Self::Buzzer => pair("x1", "x2", false),
            Self::SupplyTerminal => {
                vec![TerminalDefinition::new("out", false, GridPoint::new(0, 0))]
            }
            Self::InterlockLink => Vec::new(),
        }
    }

    /// The area a symbol of this kind covers around its origin, unrotated,
    /// in grid units. Its terminals lie inside it.
    #[must_use]
    pub fn bounds(self) -> GridRect {
        let corners = |left: i32, top: i32, right: i32, bottom: i32| {
            GridRect::from_corners(GridPoint::new(left, top), GridPoint::new(right, bottom))
        };
        match self {
            Self::Coil
            | Self::ContactNo
            | Self::ContactNc
            | Self::Changeover
            | Self::FuseLink
            | Self::Lamp
            | Self::Buzzer
            | Self::TransformerPrimary
            | Self::TransformerSecondary => corners(-2, ABOVE, 2, BELOW),
            Self::PowerContacts { poles } | Self::ThermalElement { poles } => {
                let outer = pole_x(poles, poles).saturating_add(2);
                corners(outer.saturating_neg(), ABOVE, outer, BELOW)
            }
            Self::Motor => corners(MOTOR_LEFT, ABOVE, POLE_PITCH, 6),
            Self::MotorSixTerminal => corners(MOTOR_LEFT, ABOVE, POLE_PITCH, MOTOR_SECOND_ENDS),
            Self::SupplyTerminal => corners(-1, -2, 1, 0),
            Self::InterlockLink => corners(-2, -1, 2, 1),
        }
    }

    const fn poles(self) -> Option<u8> {
        match self {
            Self::PowerContacts { poles } | Self::ThermalElement { poles } => Some(poles),
            _ => None,
        }
    }
}

/// Horizontal offset of pole `pole` (1-based) of a symbol with `poles`
/// poles: `(2 * pole - poles - 1) * POLE_PITCH / 2`, which centres the poles
/// on the origin. Pole counts are at most [`MAX_POLES`], so the arithmetic
/// cannot overflow; it saturates anyway.
fn pole_x(pole: u8, poles: u8) -> i32 {
    let steps = i32::from(pole)
        .saturating_mul(2)
        .saturating_sub(i32::from(poles))
        .saturating_sub(1);
    steps.saturating_mul(POLE_PITCH / 2)
}

/// How a two-terminal load connects to its supply.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Connection {
    /// Between the two DC poles.
    Dc,
    /// Between a phase and neutral.
    LineNeutral,
    /// Between two phases.
    LineLine,
}

impl Connection {
    /// Every connection, in canonical order.
    pub const ALL: [Self; 3] = [Self::Dc, Self::LineNeutral, Self::LineLine];

    /// Stable key used in project files.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Dc => "dc",
            Self::LineNeutral => "line_neutral",
            Self::LineLine => "line_line",
        }
    }

    /// Parse a key produced by [`Connection::key`].
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|c| c.key() == key)
    }
}

/// One terminal of a symbol.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalDefinition {
    /// Structural key.
    pub key: TerminalKey,
    /// Whether the terminal must have at least one wire.
    pub required: bool,
    /// Position relative to the symbol origin, before rotation, in grid
    /// units. Part of a published model: moving a terminal would move the
    /// end of every wire drawn to it, so it needs a new model version.
    pub offset: GridPoint,
}

impl TerminalDefinition {
    fn new(key: &str, required: bool, offset: GridPoint) -> Self {
        Self {
            key: TerminalKey(key.to_owned()),
            required,
            offset,
        }
    }
}

/// One symbol a component model offers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SymbolDefinition {
    key: SymbolKey,
    kind: SymbolKind,
    terminals: Vec<TerminalDefinition>,
    min_instances: u8,
    max_instances: u8,
    slotted: bool,
    artwork: Option<Artwork>,
}

impl SymbolDefinition {
    /// Define a symbol. Its terminals follow from the kind.
    #[must_use]
    pub fn new(
        key: &str,
        kind: SymbolKind,
        min_instances: u8,
        max_instances: u8,
        slotted: bool,
    ) -> Self {
        Self {
            key: SymbolKey(key.to_owned()),
            kind,
            terminals: kind.terminals(),
            min_instances,
            max_instances,
            slotted,
            artwork: None,
        }
    }

    /// The same symbol with its own artwork (extension libraries). Checked
    /// against the kind when a library is added to a catalog.
    #[must_use]
    pub fn with_artwork(mut self, artwork: Artwork) -> Self {
        self.artwork = Some(artwork);
        self
    }

    /// The artwork of a library symbol, if it has one; otherwise the
    /// renderer draws the artwork of its kind.
    #[must_use]
    pub const fn artwork(&self) -> Option<&Artwork> {
        self.artwork.as_ref()
    }

    /// Key of the symbol inside its model.
    #[must_use]
    pub const fn key(&self) -> &SymbolKey {
        &self.key
    }

    /// Behaviour of the symbol.
    #[must_use]
    pub const fn kind(&self) -> SymbolKind {
        self.kind
    }

    /// Terminals in canonical order.
    #[must_use]
    pub fn terminals(&self) -> &[TerminalDefinition] {
        &self.terminals
    }

    /// Position of a terminal in the canonical order.
    #[must_use]
    pub fn terminal_index(&self, key: &TerminalKey) -> Option<usize> {
        self.terminals.iter().position(|t| &t.key == key)
    }

    /// Fewest instances a device must have.
    #[must_use]
    pub const fn min_instances(&self) -> u8 {
        self.min_instances
    }

    /// Most instances a device may have.
    #[must_use]
    pub const fn max_instances(&self) -> u8 {
        self.max_instances
    }

    /// Whether every instance needs a `slot` that is unique per device.
    #[must_use]
    pub const fn slotted(&self) -> bool {
        self.slotted
    }
}

/// Type and domain of a parameter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParamType {
    /// A flag.
    Bool,
    /// An integer inside an inclusive range.
    Int {
        /// Smallest accepted value.
        min: i64,
        /// Largest accepted value.
        max: i64,
    },
    /// One of a fixed list of keys.
    Enum(Vec<String>),
    /// The ID of a supply profile of the document.
    SupplyRef,
    /// The ID of a device of the document.
    DeviceRef,
}

/// One parameter a component model accepts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParamDefinition {
    /// Parameter key.
    pub key: String,
    /// Type and domain.
    pub ty: ParamType,
    /// Value used when the device does not set the parameter; `None` makes
    /// the parameter mandatory.
    pub default: Option<ParamValue>,
}

impl ParamDefinition {
    /// Whether a value fits the type and domain of this parameter.
    ///
    /// # Errors
    ///
    /// Returns a message naming the parameter and what is wrong with the
    /// value.
    pub fn accepts(&self, value: &ParamValue) -> Result<(), String> {
        match (&self.ty, value) {
            (ParamType::Bool, ParamValue::Bool(_)) => Ok(()),
            (ParamType::Int { min, max }, ParamValue::Int(v)) => {
                if v >= min && v <= max {
                    Ok(())
                } else {
                    Err(format!(
                        "parameter `{}` must be between {min} and {max}, found {v}",
                        self.key
                    ))
                }
            }
            (ParamType::Enum(choices), ParamValue::Text(text)) => {
                if choices.iter().any(|choice| choice == text) {
                    Ok(())
                } else {
                    Err(format!(
                        "parameter `{}` must be one of [{}], found `{text}`",
                        self.key,
                        choices.join(", ")
                    ))
                }
            }
            (ParamType::SupplyRef | ParamType::DeviceRef, ParamValue::Int(v)) if *v > 0 => Ok(()),
            (ParamType::Bool, _) => Err(format!("parameter `{}` must be a boolean", self.key)),
            (ParamType::Int { .. }, _) => {
                Err(format!("parameter `{}` must be an integer", self.key))
            }
            (ParamType::Enum(_), _) => Err(format!("parameter `{}` must be a text key", self.key)),
            (ParamType::SupplyRef, _) => Err(format!(
                "parameter `{}` must be the positive id of a supply",
                self.key
            )),
            (ParamType::DeviceRef, _) => Err(format!(
                "parameter `{}` must be the positive id of a device",
                self.key
            )),
        }
    }
}

/// Where a component definition comes from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Provenance {
    /// Author or project that wrote the definition.
    pub origin: String,
    /// Licence the definition is distributed under.
    pub license: String,
    /// Public standard the definition follows, when one was consulted.
    pub standard_reference: Option<String>,
}

impl Provenance {
    fn project_owned() -> Self {
        Self {
            origin: "OpenCircuit project".to_owned(),
            license: "MIT OR Apache-2.0".to_owned(),
            standard_reference: None,
        }
    }
}

/// Longest note of a model, in bytes.
pub const MAX_NOTE_BYTES: usize = 512;

/// The palette tab a model is listed under. It is a catalog property for
/// the user interface only: it changes nothing in the simulation and is
/// never persisted in a document.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Category {
    /// Supplies and sources: conductors, batteries, transformers, sockets.
    Supplies,
    /// Fuses, circuit breakers and other protective devices.
    Protection,
    /// Commands and actuation: contactors, pushbuttons, selectors, limit
    /// switches, thermal relays, signalling.
    Controls,
    /// Motors and power loads.
    Motors,
    /// Auxiliary contacts and terminal blocks.
    Contacts,
    /// Coils, timers, electronic relays, sensors and logic.
    Automation,
    /// Pneumatic and hydraulic actuators.
    Actuators,
    /// Lines, busbars and connections.
    Lines,
}

impl Category {
    /// Every category, in the order of the palette tabs.
    pub const ALL: [Self; 8] = [
        Self::Supplies,
        Self::Protection,
        Self::Controls,
        Self::Motors,
        Self::Contacts,
        Self::Automation,
        Self::Actuators,
        Self::Lines,
    ];

    /// The key used in library files.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Supplies => "supplies",
            Self::Protection => "protection",
            Self::Controls => "controls",
            Self::Motors => "motors",
            Self::Contacts => "contacts",
            Self::Automation => "automation",
            Self::Actuators => "actuators",
            Self::Lines => "lines",
        }
    }

    /// The category whose [`key`](Self::key) is `key`.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|category| category.key() == key)
    }

    /// The name shown on the palette tab.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Supplies => "Supplies and sources",
            Self::Protection => "Protection",
            Self::Controls => "Controls and actuation",
            Self::Motors => "Motors and power",
            Self::Contacts => "Contacts and terminals",
            Self::Automation => "Automation and logic",
            Self::Actuators => "Mechanical actuators",
            Self::Lines => "Lines and connections",
        }
    }

    /// The category a model of `class` has unless it says otherwise.
    #[must_use]
    pub const fn default_for(class: DeviceClass) -> Self {
        match class {
            DeviceClass::PowerSupply | DeviceClass::Transformer => Self::Supplies,
            DeviceClass::Fuse => Self::Protection,
            DeviceClass::Contactor
            | DeviceClass::ThermalOverload
            | DeviceClass::Pushbutton
            | DeviceClass::LatchingSwitch
            | DeviceClass::Lamp
            | DeviceClass::Buzzer
            | DeviceClass::MechanicalInterlock => Self::Controls,
            DeviceClass::Motor => Self::Motors,
            DeviceClass::Relay | DeviceClass::Timer(_) => Self::Automation,
        }
    }
}

/// A versioned component model. A published `(id, version)` never changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentModel {
    id: String,
    version: u32,
    class: DeviceClass,
    operator: Operator,
    category: Category,
    title: String,
    note: Option<String>,
    params: Vec<ParamDefinition>,
    symbols: Vec<SymbolDefinition>,
    provenance: Provenance,
}

impl ComponentModel {
    /// Define a model. Consistency is checked by [`ComponentCatalog::new`].
    /// The category follows the class and the title is the identifier until
    /// [`with_category`](Self::with_category) and
    /// [`with_title`](Self::with_title) say otherwise.
    #[must_use]
    pub fn new(
        id: &str,
        version: u32,
        class: DeviceClass,
        params: Vec<ParamDefinition>,
        symbols: Vec<SymbolDefinition>,
        provenance: Provenance,
    ) -> Self {
        Self {
            id: id.to_owned(),
            version,
            class,
            operator: Operator::default_for(class),
            category: Category::default_for(class),
            title: id.to_owned(),
            note: None,
            params,
            symbols,
            provenance,
        }
    }

    /// The same model under another palette tab.
    #[must_use]
    pub const fn with_category(mut self, category: Category) -> Self {
        self.category = category;
        self
    }

    /// The same model with a human-readable title. Checked by
    /// [`ComponentCatalog::new`].
    #[must_use]
    pub fn with_title(mut self, title: &str) -> Self {
        title.clone_into(&mut self.title);
        self
    }

    /// The same model with a note for the user, such as the limits of an
    /// approximation. Checked by [`ComponentCatalog::new`].
    #[must_use]
    pub fn with_note(mut self, note: &str) -> Self {
        self.note = Some(note.to_owned());
        self
    }

    /// The palette tab of the model.
    #[must_use]
    pub const fn category(&self) -> Category {
        self.category
    }

    /// Human-readable title; the identifier when the model has none.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// A note for the user, when the model has one.
    #[must_use]
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    /// The same model with another operator. Checked against the class by
    /// [`ComponentCatalog::new`].
    #[must_use]
    pub const fn with_operator(mut self, operator: Operator) -> Self {
        self.operator = operator;
        self
    }

    /// How the device is operated, for the drawing and the designator.
    #[must_use]
    pub const fn operator(&self) -> Operator {
        self.operator
    }

    /// Model identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Model version.
    #[must_use]
    pub const fn version(&self) -> u32 {
        self.version
    }

    /// Behaviour class.
    #[must_use]
    pub const fn class(&self) -> DeviceClass {
        self.class
    }

    /// Accepted parameters.
    #[must_use]
    pub fn params(&self) -> &[ParamDefinition] {
        &self.params
    }

    /// Offered symbols.
    #[must_use]
    pub fn symbols(&self) -> &[SymbolDefinition] {
        &self.symbols
    }

    /// Origin and licence of the definition.
    #[must_use]
    pub const fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    /// Look up a symbol by key.
    #[must_use]
    pub fn symbol(&self, key: &SymbolKey) -> Option<&SymbolDefinition> {
        self.symbols.iter().find(|symbol| symbol.key() == key)
    }

    /// Check device parameters against this model. Returns one message per
    /// problem: unknown key, wrong type, value outside the domain, or a
    /// mandatory parameter that is missing.
    #[must_use]
    pub fn check_params(&self, given: &BTreeMap<String, ParamValue>) -> Vec<String> {
        let mut problems = Vec::new();
        for (key, value) in given {
            match self.params.iter().find(|definition| &definition.key == key) {
                Some(definition) => {
                    if let Err(problem) = definition.accepts(value) {
                        problems.push(problem);
                    }
                }
                None => problems.push(format!(
                    "parameter `{key}` is not defined by model `{}`",
                    self.id
                )),
            }
        }
        for definition in &self.params {
            if definition.default.is_none() && !given.contains_key(&definition.key) {
                problems.push(format!("parameter `{}` is required", definition.key));
            }
        }
        problems
    }

    /// The effective value of a parameter: the device value, or the default.
    #[must_use]
    pub fn param<'a>(
        &'a self,
        given: &'a BTreeMap<String, ParamValue>,
        key: &str,
    ) -> Option<&'a ParamValue> {
        given.get(key).or_else(|| {
            self.params
                .iter()
                .find(|definition| definition.key == key)
                .and_then(|definition| definition.default.as_ref())
        })
    }

    /// Effective integer parameter.
    #[must_use]
    pub fn int_param(&self, given: &BTreeMap<String, ParamValue>, key: &str) -> Option<i64> {
        match self.param(given, key) {
            Some(ParamValue::Int(value)) => Some(*value),
            _ => None,
        }
    }

    /// Effective text parameter.
    #[must_use]
    pub fn text_param<'a>(
        &'a self,
        given: &'a BTreeMap<String, ParamValue>,
        key: &str,
    ) -> Option<&'a str> {
        match self.param(given, key) {
            Some(ParamValue::Text(value)) => Some(value),
            _ => None,
        }
    }

    /// Effective boolean parameter.
    #[must_use]
    pub fn bool_param(&self, given: &BTreeMap<String, ParamValue>, key: &str) -> Option<bool> {
        match self.param(given, key) {
            Some(ParamValue::Bool(value)) => Some(*value),
            _ => None,
        }
    }
}

/// A catalog definition that contradicts itself.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CatalogError {
    /// Two models share an identifier and a version.
    #[error("model `{id}` version {version} is defined more than once")]
    DuplicateModel {
        /// Model identifier.
        id: String,
        /// Model version.
        version: u32,
    },
    /// A library cannot be added to the catalog.
    #[error("library `{library}`: {reason}")]
    InvalidLibrary {
        /// Library identifier.
        library: String,
        /// What is wrong.
        reason: String,
    },
    /// A model is internally inconsistent.
    #[error("model `{id}` version {version}: {reason}")]
    InvalidModel {
        /// Model identifier.
        id: String,
        /// Model version.
        version: u32,
        /// What is wrong.
        reason: String,
    },
}

/// The set of component models a document may refer to.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentCatalog {
    models: Vec<ComponentModel>,
}

impl ComponentCatalog {
    /// Build a catalog from models, checking that every model is consistent
    /// with its class.
    ///
    /// # Errors
    ///
    /// Returns the first inconsistency found, in `(id, version)` order.
    pub fn new(mut models: Vec<ComponentModel>) -> Result<Self, CatalogError> {
        models.sort_by(|a, b| (a.id(), a.version()).cmp(&(b.id(), b.version())));
        for pair in models.windows(2) {
            if let [a, b] = pair {
                if a.id == b.id && a.version == b.version {
                    return Err(CatalogError::DuplicateModel {
                        id: a.id.clone(),
                        version: a.version,
                    });
                }
            }
        }
        for model in &models {
            check_model(model).map_err(|reason| CatalogError::InvalidModel {
                id: model.id.clone(),
                version: model.version,
                reason,
            })?;
        }
        Ok(Self { models })
    }

    /// The models that ship with OpenCircuit.
    #[must_use]
    pub fn builtin() -> Self {
        let mut models = builtin_models();
        models.sort_by(|a, b| (a.id(), a.version()).cmp(&(b.id(), b.version())));
        Self { models }
    }

    /// Every model, sorted by `(id, version)`.
    #[must_use]
    pub fn models(&self) -> &[ComponentModel] {
        &self.models
    }

    /// This catalog with the models of an extension library added
    /// (ADR-0008).
    ///
    /// The library namespace must be a valid identifier that no model of
    /// this catalog uses as its first segment (which also refuses a library
    /// added twice and the namespaces of built-in models) and is not
    /// `opencircuit`. Every model must be `<namespace>.<name>`, within the
    /// limits, consistent with its class like a built-in model, and every
    /// artwork must fit its symbol kind. Each model takes the provenance of
    /// the library.
    ///
    /// # Errors
    ///
    /// Returns [`CatalogError::InvalidLibrary`] for a library-level problem
    /// and the errors of [`ComponentCatalog::new`] for a model.
    pub fn with_library(&self, library: Library) -> Result<Self, CatalogError> {
        let invalid = |reason: String| CatalogError::InvalidLibrary {
            library: library.id.clone(),
            reason,
        };
        if !is_valid_library_id(&library.id) || library.id == "opencircuit" {
            return Err(invalid(
                "the identifier is not a valid namespace".to_owned(),
            ));
        }
        let taken = self
            .models
            .iter()
            .any(|model| model.id.split('.').next() == Some(library.id.as_str()));
        if taken {
            return Err(invalid("the namespace is already used".to_owned()));
        }
        if library.models.len() > MAX_LIBRARY_MODELS {
            return Err(invalid(format!("more than {MAX_LIBRARY_MODELS} models")));
        }
        let mut models = self.models.clone();
        for mut model in library.models {
            let name = model
                .id
                .strip_prefix(library.id.as_str())
                .and_then(|rest| rest.strip_prefix('.'));
            if !name.is_some_and(is_valid_model_name) {
                return Err(invalid(format!(
                    "model `{}` is not `{}.<name>`",
                    model.id, library.id
                )));
            }
            if model.symbols.len() > MAX_LIBRARY_SYMBOLS {
                return Err(invalid(format!(
                    "model `{}` has more than {MAX_LIBRARY_SYMBOLS} symbols",
                    model.id
                )));
            }
            for symbol in &model.symbols {
                if let Some(artwork) = &symbol.artwork {
                    check_artwork(symbol.kind, artwork).map_err(|reason| {
                        invalid(format!(
                            "symbol `{}` of model `{}`: {reason}",
                            symbol.key.0, model.id
                        ))
                    })?;
                }
            }
            model.provenance = library.provenance.clone();
            models.push(model);
        }
        Self::new(models)
    }

    /// Look up a model by its versioned reference.
    #[must_use]
    pub fn model(&self, reference: &ModelRef) -> Option<&ComponentModel> {
        self.models
            .iter()
            .find(|model| model.id == reference.id && model.version == reference.version)
    }

    /// The newest version of every model, sorted by ID: what new devices
    /// are made of. Older versions stay in the catalog so that existing
    /// documents keep loading.
    pub fn latest(&self) -> impl Iterator<Item = &ComponentModel> {
        // Models are sorted by `(id, version)`, so the newest version of an
        // ID is the last one before the ID changes.
        self.models.iter().enumerate().filter_map(|(index, model)| {
            let next = index.checked_add(1).and_then(|next| self.models.get(next));
            match next {
                Some(next) if next.id == model.id => None,
                _ => Some(model),
            }
        })
    }

    /// The model a device of `from` can be upgraded to, when a model
    /// migration is registered for it and both versions are in this
    /// catalog.
    ///
    /// Every registered migration keeps symbol and parameter keys: the
    /// target offers every symbol of the source with the same kind, slot
    /// rule and at least the same instance range, and the same parameters.
    /// A migration whose versions do not satisfy that is ignored, so an
    /// upgrade never invalidates a document.
    #[must_use]
    pub fn upgrade(&self, from: &ModelRef) -> Option<&ComponentModel> {
        let (_, _, to) = MODEL_MIGRATIONS
            .iter()
            .find(|(id, version, _)| *id == from.id && *version == from.version)?;
        let source = self.model(from)?;
        let target = self.model(&ModelRef {
            id: from.id.clone(),
            version: *to,
        })?;
        keeps_keys(source, target).then_some(target)
    }
}

/// Registered model migrations: model ID, source version, target version.
/// Applying one is an explicit edit; loading never upgrades a model.
const MODEL_MIGRATIONS: &[(&str, u32, u32)] = &[
    ("relay.generic", 1, 2),
    ("timer_off.generic", 1, 2),
    ("timer_on.generic", 1, 2),
];

/// Whether every device of `source` stays valid as a device of `target`
/// with its symbols and parameters unchanged.
fn keeps_keys(source: &ComponentModel, target: &ComponentModel) -> bool {
    source.class == target.class
        && source.params == target.params
        && source.symbols.iter().all(|old| {
            target.symbol(old.key()).is_some_and(|new| {
                new.kind == old.kind
                    && new.slotted == old.slotted
                    && new.min_instances <= old.min_instances
                    && new.max_instances >= old.max_instances
            })
        })
        && target
            .symbols
            .iter()
            .all(|new| source.symbol(new.key()).is_some() || new.min_instances == 0)
}

/// A title or note: not empty, within `max` bytes, and without control
/// characters, so that it can be shown on one line.
fn check_text(what: &str, text: &str, max: usize) -> Result<(), String> {
    if text.is_empty() {
        return Err(format!("the {what} is empty"));
    }
    if text.len() > max {
        return Err(format!("the {what} is longer than {max} bytes"));
    }
    if text.chars().any(char::is_control) {
        return Err(format!("the {what} has a control character"));
    }
    Ok(())
}

fn check_model(model: &ComponentModel) -> Result<(), String> {
    if !model.operator.fits(model.class) {
        return Err(format!(
            "operator {:?} is not valid for class {:?}",
            model.operator, model.class
        ));
    }
    check_text("title", &model.title, MAX_TEXT_BYTES)?;
    if let Some(note) = &model.note {
        check_text("note", note, MAX_NOTE_BYTES)?;
    }
    let mut symbol_keys = BTreeSet::new();
    let mut coils = 0_u32;
    for symbol in &model.symbols {
        if !symbol_keys.insert(symbol.key()) {
            return Err(format!("symbol `{}` is defined twice", symbol.key().0));
        }
        if symbol.max_instances == 0 || symbol.min_instances > symbol.max_instances {
            return Err(format!(
                "symbol `{}` has an empty instance range",
                symbol.key().0
            ));
        }
        if let Some(poles) = symbol.kind.poles() {
            if poles == 0 || poles > MAX_POLES {
                return Err(format!(
                    "symbol `{}` must have between 1 and {MAX_POLES} poles",
                    symbol.key().0
                ));
            }
        }
        if !class_allows(model.class, symbol.kind) {
            return Err(format!(
                "symbol `{}` of kind {:?} is not valid for class {:?}",
                symbol.key().0,
                symbol.kind,
                model.class
            ));
        }
        if symbol.kind == SymbolKind::Coil {
            coils = coils.saturating_add(1);
            if symbol.min_instances != 1 || symbol.max_instances != 1 {
                return Err("a coil symbol must have exactly one instance".to_owned());
            }
        }
    }

    let needs_coil = matches!(
        model.class,
        DeviceClass::Contactor | DeviceClass::Relay | DeviceClass::Timer(_)
    );
    if needs_coil && coils != 1 {
        return Err("the class needs exactly one coil symbol".to_owned());
    }

    let mut param_keys = BTreeSet::new();
    for definition in &model.params {
        if !param_keys.insert(definition.key.as_str()) {
            return Err(format!("parameter `{}` is defined twice", definition.key));
        }
        if let Some(default) = &definition.default {
            definition
                .accepts(default)
                .map_err(|problem| format!("default value rejected: {problem}"))?;
        }
    }
    for (key, expected) in class_params(model.class) {
        let found = model
            .params
            .iter()
            .find(|definition| definition.key == *key)
            .ok_or_else(|| format!("the class needs parameter `{key}`"))?;
        if !expected.matches(&found.ty) {
            return Err(format!(
                "parameter `{key}` has the wrong type for the class"
            ));
        }
    }
    Ok(())
}

const fn class_allows(class: DeviceClass, kind: SymbolKind) -> bool {
    match class {
        DeviceClass::Contactor => matches!(
            kind,
            SymbolKind::Coil
                | SymbolKind::ContactNo
                | SymbolKind::ContactNc
                | SymbolKind::PowerContacts { .. }
        ),
        DeviceClass::Relay | DeviceClass::Timer(_) => matches!(
            kind,
            SymbolKind::Coil
                | SymbolKind::ContactNo
                | SymbolKind::ContactNc
                | SymbolKind::Changeover
        ),
        DeviceClass::ThermalOverload => matches!(
            kind,
            SymbolKind::ThermalElement { .. } | SymbolKind::ContactNo | SymbolKind::ContactNc
        ),
        DeviceClass::Pushbutton => {
            matches!(kind, SymbolKind::ContactNo | SymbolKind::ContactNc)
        }
        DeviceClass::LatchingSwitch => matches!(
            kind,
            SymbolKind::ContactNo | SymbolKind::ContactNc | SymbolKind::PowerContacts { .. }
        ),
        DeviceClass::Fuse => matches!(kind, SymbolKind::FuseLink),
        DeviceClass::Motor => matches!(kind, SymbolKind::Motor | SymbolKind::MotorSixTerminal),
        DeviceClass::Lamp => matches!(kind, SymbolKind::Lamp),
        DeviceClass::Buzzer => matches!(kind, SymbolKind::Buzzer),
        DeviceClass::PowerSupply => matches!(kind, SymbolKind::SupplyTerminal),
        DeviceClass::MechanicalInterlock => matches!(kind, SymbolKind::InterlockLink),
        DeviceClass::Transformer => matches!(
            kind,
            SymbolKind::TransformerPrimary | SymbolKind::TransformerSecondary
        ),
    }
}

/// Shape a class expects from one of its parameters.
#[derive(Clone, Copy)]
enum ExpectedParam {
    Bool,
    /// Integer whose whole range fits `1..=u32::MAX`.
    PositiveU32,
    ConnectionKey,
    ConductorKey,
    SupplyRef,
    DeviceRef,
}

impl ExpectedParam {
    fn matches(self, ty: &ParamType) -> bool {
        match (self, ty) {
            (Self::Bool, ParamType::Bool)
            | (Self::SupplyRef, ParamType::SupplyRef)
            | (Self::DeviceRef, ParamType::DeviceRef) => true,
            (Self::PositiveU32, ParamType::Int { min, max }) => {
                *min >= 1 && *max <= i64::from(u32::MAX) && min <= max
            }
            (Self::ConnectionKey, ParamType::Enum(choices)) => {
                !choices.is_empty() && choices.iter().all(|c| Connection::from_key(c).is_some())
            }
            (Self::ConductorKey, ParamType::Enum(choices)) => {
                !choices.is_empty() && choices.iter().all(|c| Conductor::from_key(c).is_some())
            }
            _ => false,
        }
    }
}

/// The parameters the simulator reads for each class.
const fn class_params(class: DeviceClass) -> &'static [(&'static str, ExpectedParam)] {
    const LOAD: &[(&str, ExpectedParam)] = &[
        (PARAM_RATED_VOLTAGE_MV, ExpectedParam::PositiveU32),
        (PARAM_CONNECTION, ExpectedParam::ConnectionKey),
    ];
    const TIMER: &[(&str, ExpectedParam)] = &[
        (PARAM_RATED_VOLTAGE_MV, ExpectedParam::PositiveU32),
        (PARAM_CONNECTION, ExpectedParam::ConnectionKey),
        (PARAM_PRESET_MS, ExpectedParam::PositiveU32),
    ];
    match class {
        DeviceClass::Contactor | DeviceClass::Relay | DeviceClass::Lamp | DeviceClass::Buzzer => {
            LOAD
        }
        DeviceClass::Timer(_) => TIMER,
        DeviceClass::Motor => &[(PARAM_RATED_VOLTAGE_MV, ExpectedParam::PositiveU32)],
        DeviceClass::LatchingSwitch => &[(PARAM_INITIAL_CLOSED, ExpectedParam::Bool)],
        DeviceClass::PowerSupply => &[
            (PARAM_CONDUCTOR, ExpectedParam::ConductorKey),
            (PARAM_SUPPLY, ExpectedParam::SupplyRef),
        ],
        DeviceClass::MechanicalInterlock => &[
            (PARAM_FIRST, ExpectedParam::DeviceRef),
            (PARAM_SECOND, ExpectedParam::DeviceRef),
        ],
        DeviceClass::Transformer => &[
            (PARAM_RATED_VOLTAGE_MV, ExpectedParam::PositiveU32),
            (PARAM_CONNECTION, ExpectedParam::ConnectionKey),
            (PARAM_SECONDARY, ExpectedParam::SupplyRef),
        ],
        DeviceClass::ThermalOverload | DeviceClass::Pushbutton | DeviceClass::Fuse => &[],
    }
}

fn rated_voltage() -> ParamDefinition {
    ParamDefinition {
        key: PARAM_RATED_VOLTAGE_MV.to_owned(),
        ty: ParamType::Int {
            min: 1,
            max: i64::from(u32::MAX),
        },
        default: None,
    }
}

fn connection() -> ParamDefinition {
    ParamDefinition {
        key: PARAM_CONNECTION.to_owned(),
        ty: ParamType::Enum(
            Connection::ALL
                .into_iter()
                .map(|c| c.key().to_owned())
                .collect(),
        ),
        default: None,
    }
}

fn preset() -> ParamDefinition {
    ParamDefinition {
        key: PARAM_PRESET_MS.to_owned(),
        ty: ParamType::Int {
            min: 1,
            max: i64::from(u32::MAX),
        },
        default: None,
    }
}

fn device_ref(key: &str) -> ParamDefinition {
    ParamDefinition {
        key: key.to_owned(),
        ty: ParamType::DeviceRef,
        default: None,
    }
}

fn initial_closed() -> ParamDefinition {
    ParamDefinition {
        key: PARAM_INITIAL_CLOSED.to_owned(),
        ty: ParamType::Bool,
        default: Some(ParamValue::Bool(false)),
    }
}

fn builtin_models() -> Vec<ComponentModel> {
    let model = |id: &str, class, params, symbols| {
        ComponentModel::new(id, 1, class, params, symbols, Provenance::project_owned())
    };
    let aux = |key: &str, kind, max| SymbolDefinition::new(key, kind, 0, max, true);
    // Version 2 adds changeover contacts to version 1; every symbol and
    // parameter of version 1 is kept, so the upgrade maps keys to themselves.
    let relay_symbols = || {
        vec![
            SymbolDefinition::new("coil", SymbolKind::Coil, 1, 1, false),
            aux("aux_no", SymbolKind::ContactNo, 8),
            aux("aux_nc", SymbolKind::ContactNc, 8),
        ]
    };
    let timer_symbols = || {
        vec![
            SymbolDefinition::new("coil", SymbolKind::Coil, 1, 1, false),
            aux("delayed_no", SymbolKind::ContactNo, 4),
            aux("delayed_nc", SymbolKind::ContactNc, 4),
        ]
    };
    let with = |mut symbols: Vec<SymbolDefinition>, extra: SymbolDefinition| {
        symbols.push(extra);
        symbols
    };
    let version_2 = |id: &str, class, params, symbols| {
        ComponentModel::new(id, 2, class, params, symbols, Provenance::project_owned())
    };

    let models = vec![
        model(
            "supply.terminal",
            DeviceClass::PowerSupply,
            vec![
                ParamDefinition {
                    key: PARAM_CONDUCTOR.to_owned(),
                    ty: ParamType::Enum(
                        Conductor::ALL
                            .into_iter()
                            .map(|c| c.key().to_owned())
                            .collect(),
                    ),
                    default: None,
                },
                ParamDefinition {
                    key: PARAM_SUPPLY.to_owned(),
                    ty: ParamType::SupplyRef,
                    default: None,
                },
            ],
            vec![SymbolDefinition::new(
                "terminal",
                SymbolKind::SupplyTerminal,
                1,
                u8::MAX,
                false,
            )],
        ),
        model(
            "pushbutton_no",
            DeviceClass::Pushbutton,
            Vec::new(),
            vec![SymbolDefinition::new(
                "contact",
                SymbolKind::ContactNo,
                1,
                1,
                false,
            )],
        ),
        model(
            "pushbutton_nc",
            DeviceClass::Pushbutton,
            Vec::new(),
            vec![SymbolDefinition::new(
                "contact",
                SymbolKind::ContactNc,
                1,
                1,
                false,
            )],
        ),
        model(
            "pushbutton.combined",
            DeviceClass::Pushbutton,
            Vec::new(),
            vec![
                SymbolDefinition::new("no", SymbolKind::ContactNo, 1, 1, false),
                SymbolDefinition::new("nc", SymbolKind::ContactNc, 1, 1, false),
            ],
        ),
        model(
            "interlock.mechanical",
            DeviceClass::MechanicalInterlock,
            vec![device_ref(PARAM_FIRST), device_ref(PARAM_SECOND)],
            vec![SymbolDefinition::new(
                "link",
                SymbolKind::InterlockLink,
                1,
                1,
                false,
            )],
        ),
        model(
            "estop.latching",
            DeviceClass::LatchingSwitch,
            vec![initial_closed()],
            vec![SymbolDefinition::new(
                "contact",
                SymbolKind::ContactNc,
                1,
                1,
                false,
            )],
        )
        .with_operator(Operator::Emergency),
        model(
            "limit_switch",
            DeviceClass::Pushbutton,
            Vec::new(),
            vec![
                SymbolDefinition::new("no", SymbolKind::ContactNo, 0, 1, false),
                SymbolDefinition::new("nc", SymbolKind::ContactNc, 0, 1, false),
            ],
        )
        .with_operator(Operator::Position),
        model(
            "selector.latching",
            DeviceClass::LatchingSwitch,
            vec![initial_closed()],
            vec![SymbolDefinition::new(
                "contact",
                SymbolKind::ContactNo,
                1,
                1,
                false,
            )],
        ),
        model(
            "main_switch.3p",
            DeviceClass::LatchingSwitch,
            vec![initial_closed()],
            vec![SymbolDefinition::new(
                "main",
                SymbolKind::PowerContacts { poles: 3 },
                1,
                1,
                false,
            )],
        ),
        model(
            "contactor.generic",
            DeviceClass::Contactor,
            vec![rated_voltage(), connection()],
            vec![
                SymbolDefinition::new("coil", SymbolKind::Coil, 1, 1, false),
                SymbolDefinition::new("main", SymbolKind::PowerContacts { poles: 3 }, 0, 1, false),
                aux("aux_no", SymbolKind::ContactNo, 8),
                aux("aux_nc", SymbolKind::ContactNc, 8),
            ],
        ),
        model(
            "relay.generic",
            DeviceClass::Relay,
            vec![rated_voltage(), connection()],
            relay_symbols(),
        ),
        version_2(
            "relay.generic",
            DeviceClass::Relay,
            vec![rated_voltage(), connection()],
            with(
                relay_symbols(),
                aux("changeover", SymbolKind::Changeover, 8),
            ),
        ),
        model(
            "thermal_overload.generic",
            DeviceClass::ThermalOverload,
            Vec::new(),
            vec![
                SymbolDefinition::new(
                    "element",
                    SymbolKind::ThermalElement { poles: 3 },
                    0,
                    1,
                    false,
                ),
                SymbolDefinition::new("aux_nc", SymbolKind::ContactNc, 0, 1, false),
                SymbolDefinition::new("aux_no", SymbolKind::ContactNo, 0, 1, false),
            ],
        ),
        model(
            "timer_on.generic",
            DeviceClass::Timer(TimerMode::OnDelay),
            vec![rated_voltage(), connection(), preset()],
            timer_symbols(),
        ),
        version_2(
            "timer_on.generic",
            DeviceClass::Timer(TimerMode::OnDelay),
            vec![rated_voltage(), connection(), preset()],
            with(
                timer_symbols(),
                aux("delayed_co", SymbolKind::Changeover, 4),
            ),
        ),
        model(
            "timer_off.generic",
            DeviceClass::Timer(TimerMode::OffDelay),
            vec![rated_voltage(), connection(), preset()],
            timer_symbols(),
        ),
        version_2(
            "timer_off.generic",
            DeviceClass::Timer(TimerMode::OffDelay),
            vec![rated_voltage(), connection(), preset()],
            with(
                timer_symbols(),
                aux("delayed_co", SymbolKind::Changeover, 4),
            ),
        ),
        model(
            "fuse.generic",
            DeviceClass::Fuse,
            Vec::new(),
            vec![SymbolDefinition::new(
                "link",
                SymbolKind::FuseLink,
                1,
                1,
                false,
            )],
        ),
        model(
            "motor.3ph",
            DeviceClass::Motor,
            vec![rated_voltage()],
            vec![SymbolDefinition::new(
                "motor",
                SymbolKind::Motor,
                1,
                1,
                false,
            )],
        ),
        model(
            "motor.3ph.6t",
            DeviceClass::Motor,
            vec![rated_voltage()],
            vec![SymbolDefinition::new(
                "motor",
                SymbolKind::MotorSixTerminal,
                1,
                1,
                false,
            )],
        ),
        model(
            "transformer.control",
            DeviceClass::Transformer,
            vec![
                rated_voltage(),
                connection(),
                ParamDefinition {
                    key: PARAM_SECONDARY.to_owned(),
                    ty: ParamType::SupplyRef,
                    default: None,
                },
            ],
            vec![
                SymbolDefinition::new("primary", SymbolKind::TransformerPrimary, 1, 1, false),
                SymbolDefinition::new("secondary", SymbolKind::TransformerSecondary, 1, 1, false),
            ],
        ),
        model(
            "lamp.generic",
            DeviceClass::Lamp,
            vec![rated_voltage(), connection()],
            vec![SymbolDefinition::new("lamp", SymbolKind::Lamp, 1, 1, false)],
        ),
        model(
            "buzzer.generic",
            DeviceClass::Buzzer,
            vec![rated_voltage(), connection()],
            vec![SymbolDefinition::new(
                "buzzer",
                SymbolKind::Buzzer,
                1,
                1,
                false,
            )],
        ),
    ];
    models
        .into_iter()
        .map(|model| match builtin_title(model.id()) {
            Some(title) => model.with_title(title),
            None => model,
        })
        .collect()
}

/// The title of a built-in model. A model without an entry keeps its
/// identifier as title; a test requires every built-in model to have one.
fn builtin_title(id: &str) -> Option<&'static str> {
    Some(match id {
        "supply.terminal" => "Supply terminal",
        "pushbutton_no" => "Pushbutton, normally open",
        "pushbutton_nc" => "Pushbutton, normally closed",
        "pushbutton.combined" => "Pushbutton, NO and NC",
        "interlock.mechanical" => "Mechanical interlock",
        "estop.latching" => "Emergency stop, latching",
        "limit_switch" => "Limit switch",
        "selector.latching" => "Selector switch, maintained",
        "main_switch.3p" => "Main switch, three-pole",
        "contactor.generic" => "Contactor",
        "relay.generic" => "Relay",
        "thermal_overload.generic" => "Thermal overload relay",
        "timer_on.generic" => "Timer, on-delay",
        "timer_off.generic" => "Timer, off-delay",
        "fuse.generic" => "Fuse",
        "motor.3ph" => "Three-phase motor",
        "motor.3ph.6t" => "Three-phase motor, six terminals",
        "transformer.control" => "Control transformer",
        "lamp.generic" => "Indicator lamp",
        "buzzer.generic" => "Buzzer",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn builtin_model(id: &str) -> ComponentModel {
        ComponentCatalog::builtin()
            .model(&ModelRef {
                id: id.to_owned(),
                version: 1,
            })
            .cloned()
            .unwrap_or_else(|| panic!("builtin model {id} is missing"))
    }

    fn terminal_keys(kind: SymbolKind) -> Vec<String> {
        kind.terminals().into_iter().map(|t| t.key.0).collect()
    }

    fn params(pairs: &[(&str, ParamValue)]) -> BTreeMap<String, ParamValue> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect()
    }

    /// Every symbol kind the catalog can use, with each pole count.
    fn every_kind() -> Vec<SymbolKind> {
        let mut kinds = vec![
            SymbolKind::Coil,
            SymbolKind::ContactNo,
            SymbolKind::ContactNc,
            SymbolKind::Changeover,
            SymbolKind::FuseLink,
            SymbolKind::Motor,
            SymbolKind::MotorSixTerminal,
            SymbolKind::Lamp,
            SymbolKind::Buzzer,
            SymbolKind::SupplyTerminal,
            SymbolKind::InterlockLink,
            SymbolKind::TransformerPrimary,
            SymbolKind::TransformerSecondary,
        ];
        for poles in 1..=MAX_POLES {
            kinds.push(SymbolKind::PowerContacts { poles });
            kinds.push(SymbolKind::ThermalElement { poles });
        }
        kinds
    }

    fn offsets(kind: SymbolKind) -> Vec<(i32, i32)> {
        kind.terminals()
            .into_iter()
            .map(|t| (t.offset.x, t.offset.y))
            .collect()
    }

    #[test]
    fn terminals_of_one_symbol_never_share_a_position() {
        for kind in every_kind() {
            let positions: BTreeSet<(i32, i32)> = offsets(kind).into_iter().collect();

            assert_eq!(positions.len(), kind.terminals().len(), "{kind:?}");
        }
    }

    #[test]
    fn terminals_lie_inside_the_symbol_bounds() {
        for kind in every_kind() {
            let bounds = kind.bounds();
            for terminal in kind.terminals() {
                assert!(
                    bounds.contains(terminal.offset),
                    "{kind:?} {}",
                    terminal.key.0
                );
            }
        }
    }

    #[test]
    fn two_terminal_symbols_run_from_above_to_below_the_origin() {
        assert_eq!(offsets(SymbolKind::Coil), [(0, -2), (0, 2)]);
    }

    #[test]
    fn poles_are_centred_on_the_origin_one_pitch_apart() {
        assert_eq!(
            offsets(SymbolKind::PowerContacts { poles: 3 }),
            [(-4, -2), (-4, 2), (0, -2), (0, 2), (4, -2), (4, 2)]
        );
    }

    #[test]
    fn two_poles_are_centred_between_grid_points() {
        assert_eq!(
            offsets(SymbolKind::ThermalElement { poles: 2 }),
            [(-2, -2), (-2, 2), (2, -2), (2, 2)]
        );
    }

    #[test]
    fn motor_terminals_sit_above_the_motor() {
        assert_eq!(offsets(SymbolKind::Motor), [(-4, -2), (0, -2), (4, -2)]);
    }

    #[test]
    fn six_terminal_motor_has_its_second_ends_below_the_first() {
        assert_eq!(
            terminal_keys(SymbolKind::MotorSixTerminal),
            ["u1", "v1", "w1", "u2", "v2", "w2"]
        );
        assert_eq!(
            offsets(SymbolKind::MotorSixTerminal),
            [(-4, -2), (0, -2), (4, -2), (-4, 8), (0, 8), (4, 8)]
        );
    }

    #[test]
    fn transformer_has_a_required_primary_and_a_secondary_of_two_terminals() {
        assert_eq!(terminal_keys(SymbolKind::TransformerPrimary), ["a1", "a2"]);
        assert!(
            SymbolKind::TransformerPrimary
                .terminals()
                .iter()
                .all(|t| t.required)
        );
        assert_eq!(
            terminal_keys(SymbolKind::TransformerSecondary),
            ["x1", "x2"]
        );
        let transformer = builtin_model("transformer.control");
        assert_eq!(transformer.class(), DeviceClass::Transformer);
        assert!(
            transformer
                .params()
                .iter()
                .any(|p| p.key == PARAM_SECONDARY && p.ty == ParamType::SupplyRef)
        );
    }

    #[test]
    fn emergency_stop_and_limit_switch_name_their_operator() {
        let estop = builtin_model("estop.latching");
        let limit = builtin_model("limit_switch");

        assert_eq!(
            (estop.class(), estop.operator()),
            (DeviceClass::LatchingSwitch, Operator::Emergency)
        );
        assert_eq!(
            (limit.class(), limit.operator()),
            (DeviceClass::Pushbutton, Operator::Position)
        );
        assert_eq!(builtin_model("pushbutton_no").operator(), Operator::Push);
        assert_eq!(builtin_model("relay.generic").operator(), Operator::None);
    }

    #[test]
    fn an_operator_that_does_not_fit_the_class_is_refused() {
        let wrong = builtin_model("relay.generic").with_operator(Operator::Emergency);

        assert!(matches!(
            ComponentCatalog::new(vec![wrong]),
            Err(CatalogError::InvalidModel { .. })
        ));
    }

    mod libraries {
        use super::*;
        use crate::library::{ArtPoint, ArtPrimitive, Artwork, Library};

        fn provenance() -> Provenance {
            Provenance {
                origin: "ACME test vendor".to_owned(),
                license: "MIT".to_owned(),
                standard_reference: None,
            }
        }

        /// A relay with a coil and a normally open contact drawn its own way.
        fn relay(id: &str) -> ComponentModel {
            let contact = Artwork {
                rest: vec![
                    ArtPrimitive::Line {
                        from: ArtPoint { x: 0, y: -20 },
                        to: ArtPoint { x: 0, y: -8 },
                    },
                    ArtPrimitive::Line {
                        from: ArtPoint { x: 0, y: 8 },
                        to: ArtPoint { x: 0, y: 20 },
                    },
                ],
                operated: None,
            };
            ComponentModel::new(
                id,
                1,
                DeviceClass::Relay,
                vec![rated_voltage(), connection()],
                vec![
                    SymbolDefinition::new("coil", SymbolKind::Coil, 1, 1, false),
                    SymbolDefinition::new("no", SymbolKind::ContactNo, 0, 2, true)
                        .with_artwork(contact),
                ],
                Provenance::project_owned(),
            )
        }

        fn library(id: &str, models: Vec<ComponentModel>) -> Library {
            Library {
                id: id.to_owned(),
                version: 1,
                name: "ACME".to_owned(),
                provenance: provenance(),
                models,
            }
        }

        #[test]
        fn a_library_adds_its_models_with_its_provenance() {
            let catalog = ComponentCatalog::builtin()
                .with_library(library("acme", vec![relay("acme.r4")]))
                .unwrap();
            let model = catalog
                .model(&ModelRef {
                    id: "acme.r4".to_owned(),
                    version: 1,
                })
                .unwrap();

            assert_eq!(model.provenance(), &provenance());
            assert!(
                model
                    .symbol(&SymbolKey("no".to_owned()))
                    .unwrap()
                    .artwork()
                    .is_some()
            );
            assert_eq!(
                catalog.models().len(),
                ComponentCatalog::builtin().models().len() + 1
            );
        }

        #[test]
        fn namespaces_of_built_in_models_and_of_loaded_libraries_are_taken() {
            let once = ComponentCatalog::builtin()
                .with_library(library("acme", vec![relay("acme.r4")]))
                .unwrap();

            for (catalog, id, model) in [
                (&once, "acme", "acme.r5"),
                (&ComponentCatalog::builtin(), "relay", "relay.r4"),
                (
                    &ComponentCatalog::builtin(),
                    "opencircuit",
                    "opencircuit.r4",
                ),
                (&ComponentCatalog::builtin(), "Bad", "Bad.r4"),
            ] {
                assert!(
                    matches!(
                        catalog.with_library(library(id, vec![relay(model)])),
                        Err(CatalogError::InvalidLibrary { .. })
                    ),
                    "{id}"
                );
            }
        }

        #[test]
        fn models_outside_the_namespace_or_inconsistent_with_their_class_are_refused() {
            let builtin = ComponentCatalog::builtin();
            let outside = builtin.with_library(library("acme", vec![relay("other.r4")]));
            let wrong_class = relay("acme.r4").with_operator(Operator::Emergency);
            let inconsistent = builtin.with_library(library("acme", vec![wrong_class]));
            let duplicate =
                builtin.with_library(library("acme", vec![relay("acme.r4"), relay("acme.r4")]));

            assert!(matches!(outside, Err(CatalogError::InvalidLibrary { .. })));
            assert!(matches!(
                inconsistent,
                Err(CatalogError::InvalidModel { .. })
            ));
            assert!(matches!(
                duplicate,
                Err(CatalogError::DuplicateModel { .. })
            ));
        }

        #[test]
        fn artwork_that_does_not_reach_the_terminals_is_refused() {
            let mut model = relay("acme.r4");
            model.symbols[1] = SymbolDefinition::new("no", SymbolKind::ContactNo, 0, 2, true)
                .with_artwork(Artwork {
                    rest: vec![ArtPrimitive::Circle {
                        center: ArtPoint { x: 0, y: 0 },
                        radius: 5,
                    }],
                    operated: None,
                });

            assert!(matches!(
                ComponentCatalog::builtin().with_library(library("acme", vec![model])),
                Err(CatalogError::InvalidLibrary { .. })
            ));
        }
    }

    #[test]
    fn supply_terminal_is_its_own_origin() {
        assert_eq!(offsets(SymbolKind::SupplyTerminal), [(0, 0)]);
    }

    #[test]
    fn builtin_catalog_passes_its_own_consistency_check() {
        let builtin = ComponentCatalog::builtin();

        assert_eq!(
            ComponentCatalog::new(builtin.models().to_vec()),
            Ok(builtin)
        );
    }

    #[test]
    fn builtin_catalog_lists_the_published_models() {
        let ids: Vec<String> = ComponentCatalog::builtin()
            .models()
            .iter()
            .map(|model| format!("{}@{}", model.id(), model.version()))
            .collect();

        assert_eq!(
            ids,
            [
                "buzzer.generic@1",
                "contactor.generic@1",
                "estop.latching@1",
                "fuse.generic@1",
                "interlock.mechanical@1",
                "lamp.generic@1",
                "limit_switch@1",
                "main_switch.3p@1",
                "motor.3ph@1",
                "motor.3ph.6t@1",
                "pushbutton.combined@1",
                "pushbutton_nc@1",
                "pushbutton_no@1",
                "relay.generic@1",
                "relay.generic@2",
                "selector.latching@1",
                "supply.terminal@1",
                "thermal_overload.generic@1",
                "timer_off.generic@1",
                "timer_off.generic@2",
                "timer_on.generic@1",
                "timer_on.generic@2",
                "transformer.control@1",
            ]
        );
    }

    #[test]
    fn latest_lists_the_newest_version_of_each_model() {
        let latest: Vec<String> = ComponentCatalog::builtin()
            .latest()
            .map(|model| format!("{}@{}", model.id(), model.version()))
            .collect();

        assert_eq!(latest.len(), 20);
        assert!(latest.contains(&"relay.generic@2".to_owned()));
        assert!(latest.contains(&"timer_on.generic@2".to_owned()));
        assert!(latest.contains(&"timer_off.generic@2".to_owned()));
        assert!(latest.contains(&"contactor.generic@1".to_owned()));
        assert!(!latest.contains(&"relay.generic@1".to_owned()));
    }

    #[test]
    fn combined_pushbutton_has_one_no_and_one_nc_contact() {
        let button = builtin_model("pushbutton.combined");
        let kinds: Vec<(String, SymbolKind, u8, u8)> = button
            .symbols()
            .iter()
            .map(|s| {
                (
                    s.key().0.clone(),
                    s.kind(),
                    s.min_instances(),
                    s.max_instances(),
                )
            })
            .collect();

        assert_eq!(button.class(), DeviceClass::Pushbutton);
        assert_eq!(
            kinds,
            [
                ("no".to_owned(), SymbolKind::ContactNo, 1, 1),
                ("nc".to_owned(), SymbolKind::ContactNc, 1, 1)
            ]
        );
    }

    #[test]
    fn mechanical_interlock_names_two_devices_and_has_a_link_without_terminals() {
        let interlock = builtin_model("interlock.mechanical");
        let params: Vec<(&str, &ParamType)> = interlock
            .params()
            .iter()
            .map(|p| (p.key.as_str(), &p.ty))
            .collect();

        assert_eq!(
            params,
            [
                ("first", &ParamType::DeviceRef),
                ("second", &ParamType::DeviceRef)
            ]
        );
        let link = interlock.symbol(&SymbolKey("link".to_owned())).unwrap();
        assert_eq!(link.kind(), SymbolKind::InterlockLink);
        assert!(link.terminals().is_empty());
    }

    #[test]
    fn device_reference_must_be_a_positive_integer() {
        let interlock = builtin_model("interlock.mechanical");
        let first = &interlock.params()[0];

        assert_eq!(first.accepts(&ParamValue::Int(3)), Ok(()));
        assert!(first.accepts(&ParamValue::Int(0)).is_err());
        assert!(first.accepts(&ParamValue::Text("K1".to_owned())).is_err());
    }

    #[test]
    fn changeover_terminals_are_common_below_and_both_sides_above() {
        assert_eq!(terminal_keys(SymbolKind::Changeover), ["com", "nc", "no"]);
        assert_eq!(offsets(SymbolKind::Changeover), [(0, 2), (2, -2), (0, -2)]);
        assert!(
            SymbolKind::Changeover
                .terminals()
                .iter()
                .all(|t| !t.required)
        );
    }

    #[test]
    fn version_2_relays_and_timers_offer_slotted_changeover_contacts() {
        for (id, key, max) in [
            ("relay.generic", "changeover", 8),
            ("timer_on.generic", "delayed_co", 4),
            ("timer_off.generic", "delayed_co", 4),
        ] {
            let model = ComponentCatalog::builtin()
                .model(&ModelRef {
                    id: id.to_owned(),
                    version: 2,
                })
                .cloned()
                .unwrap();
            let changeover = model.symbol(&SymbolKey(key.to_owned())).unwrap();

            assert_eq!(changeover.kind(), SymbolKind::Changeover, "{id}");
            assert_eq!(
                (changeover.min_instances(), changeover.max_instances()),
                (0, max)
            );
            assert!(changeover.slotted());
            let version_1 = builtin_model(id);
            assert!(version_1.symbol(&SymbolKey(key.to_owned())).is_none());
        }
    }

    #[test]
    fn relays_and_timers_upgrade_from_version_1_to_2() {
        let catalog = ComponentCatalog::builtin();
        for id in ["relay.generic", "timer_on.generic", "timer_off.generic"] {
            let from = ModelRef {
                id: id.to_owned(),
                version: 1,
            };

            let target = catalog.upgrade(&from).map(ComponentModel::version);

            assert_eq!(target, Some(2), "{id}");
        }
    }

    #[test]
    fn models_without_a_migration_do_not_upgrade() {
        let catalog = ComponentCatalog::builtin();
        for (id, version) in [("contactor.generic", 1), ("relay.generic", 2)] {
            let from = ModelRef {
                id: id.to_owned(),
                version,
            };

            assert!(catalog.upgrade(&from).is_none(), "{id}@{version}");
        }
    }

    #[test]
    fn an_upgrade_that_would_drop_a_symbol_is_ignored() {
        // A catalog whose version 2 lost `aux_nc`: the registered migration
        // must not be offered, or devices with that symbol would break.
        let mut broken = builtin_model("relay.generic");
        broken.version = 2;
        broken.symbols.retain(|symbol| symbol.key().0 != "aux_nc");
        let catalog = ComponentCatalog::new(vec![builtin_model("relay.generic"), broken]).unwrap();
        let from = ModelRef {
            id: "relay.generic".to_owned(),
            version: 1,
        };

        assert!(catalog.upgrade(&from).is_none());
    }

    #[test]
    fn every_builtin_model_declares_its_provenance() {
        for model in ComponentCatalog::builtin().models() {
            assert!(!model.provenance().origin.is_empty(), "{}", model.id());
            assert!(!model.provenance().license.is_empty(), "{}", model.id());
        }
    }

    #[test]
    fn coil_terminals_are_required_and_contact_terminals_are_not() {
        assert!(SymbolKind::Coil.terminals().iter().all(|t| t.required));
        assert!(
            SymbolKind::ContactNo
                .terminals()
                .iter()
                .all(|t| !t.required)
        );
    }

    #[test]
    fn terminal_keys_are_structural() {
        assert_eq!(terminal_keys(SymbolKind::Coil), ["a1", "a2"]);
        assert_eq!(terminal_keys(SymbolKind::ContactNc), ["p1", "p2"]);
        assert_eq!(terminal_keys(SymbolKind::Motor), ["u1", "v1", "w1"]);
        assert_eq!(terminal_keys(SymbolKind::Lamp), ["x1", "x2"]);
        assert_eq!(terminal_keys(SymbolKind::SupplyTerminal), ["out"]);
        assert_eq!(
            terminal_keys(SymbolKind::PowerContacts { poles: 2 }),
            ["pole1.in", "pole1.out", "pole2.in", "pole2.out"]
        );
    }

    #[test]
    fn contactor_offers_one_coil_and_slotted_auxiliary_contacts() {
        let contactor = builtin_model("contactor.generic");
        let coil = contactor.symbol(&SymbolKey("coil".to_owned())).unwrap();
        let aux_no = contactor.symbol(&SymbolKey("aux_no".to_owned())).unwrap();

        assert_eq!((coil.min_instances(), coil.max_instances()), (1, 1));
        assert!(!coil.slotted());
        assert_eq!((aux_no.min_instances(), aux_no.max_instances()), (0, 8));
        assert!(aux_no.slotted());
        assert_eq!(contactor.symbol(&SymbolKey("changeover".to_owned())), None);
    }

    #[test]
    fn check_params_accepts_a_complete_valid_set() {
        let contactor = builtin_model("contactor.generic");
        let given = params(&[
            (PARAM_RATED_VOLTAGE_MV, ParamValue::Int(24_000)),
            (PARAM_CONNECTION, ParamValue::Text("dc".to_owned())),
        ]);

        assert_eq!(contactor.check_params(&given), Vec::<String>::new());
    }

    #[test]
    fn check_params_reports_missing_unknown_and_out_of_domain_values() {
        let contactor = builtin_model("contactor.generic");
        let given = params(&[
            (PARAM_RATED_VOLTAGE_MV, ParamValue::Int(0)),
            ("colour", ParamValue::Text("red".to_owned())),
        ]);

        assert_eq!(
            contactor.check_params(&given),
            [
                "parameter `colour` is not defined by model `contactor.generic`",
                "parameter `rated_voltage_mv` must be between 1 and 4294967295, found 0",
                "parameter `connection` is required",
            ]
        );
    }

    #[test]
    fn check_params_rejects_wrong_types_and_unknown_enum_keys() {
        let contactor = builtin_model("contactor.generic");
        let given = params(&[
            (PARAM_RATED_VOLTAGE_MV, ParamValue::Text("24V".to_owned())),
            (PARAM_CONNECTION, ParamValue::Text("delta".to_owned())),
        ]);

        assert_eq!(
            contactor.check_params(&given),
            [
                "parameter `connection` must be one of [dc, line_neutral, line_line], found `delta`",
                "parameter `rated_voltage_mv` must be an integer",
            ]
        );
    }

    #[test]
    fn default_value_is_used_when_the_device_does_not_set_the_parameter() {
        let selector = builtin_model("selector.latching");

        assert_eq!(
            selector.bool_param(&BTreeMap::new(), PARAM_INITIAL_CLOSED),
            Some(false)
        );
        assert_eq!(
            selector.bool_param(
                &params(&[(PARAM_INITIAL_CLOSED, ParamValue::Bool(true))]),
                PARAM_INITIAL_CLOSED
            ),
            Some(true)
        );
    }

    #[test]
    fn catalog_rejects_duplicate_model_versions() {
        let lamp = builtin_model("lamp.generic");

        assert_eq!(
            ComponentCatalog::new(vec![lamp.clone(), lamp]),
            Err(CatalogError::DuplicateModel {
                id: "lamp.generic".to_owned(),
                version: 1
            })
        );
    }

    #[test]
    fn catalog_rejects_symbol_kind_that_does_not_fit_the_class() {
        let model = ComponentModel::new(
            "broken.motor",
            1,
            DeviceClass::Motor,
            vec![rated_voltage()],
            vec![SymbolDefinition::new("coil", SymbolKind::Coil, 1, 1, false)],
            Provenance::project_owned(),
        );

        assert!(matches!(
            ComponentCatalog::new(vec![model]),
            Err(CatalogError::InvalidModel { .. })
        ));
    }

    #[test]
    fn catalog_rejects_contactor_without_the_parameters_its_class_reads() {
        let model = ComponentModel::new(
            "broken.contactor",
            1,
            DeviceClass::Contactor,
            Vec::new(),
            vec![SymbolDefinition::new("coil", SymbolKind::Coil, 1, 1, false)],
            Provenance::project_owned(),
        );

        assert_eq!(
            ComponentCatalog::new(vec![model]),
            Err(CatalogError::InvalidModel {
                id: "broken.contactor".to_owned(),
                version: 1,
                reason: "the class needs parameter `rated_voltage_mv`".to_owned()
            })
        );
    }

    #[test]
    fn catalog_rejects_zero_pole_contacts() {
        let model = ComponentModel::new(
            "broken.switch",
            1,
            DeviceClass::LatchingSwitch,
            vec![initial_closed()],
            vec![SymbolDefinition::new(
                "main",
                SymbolKind::PowerContacts { poles: 0 },
                1,
                1,
                false,
            )],
            Provenance::project_owned(),
        );

        assert!(matches!(
            ComponentCatalog::new(vec![model]),
            Err(CatalogError::InvalidModel { .. })
        ));
    }

    #[test]
    fn every_builtin_model_has_a_title_of_its_own() {
        for model in ComponentCatalog::builtin().models() {
            assert_ne!(model.title(), model.id(), "{}", model.id());
        }
    }

    #[test]
    fn builtin_models_follow_the_category_of_their_class() {
        for model in ComponentCatalog::builtin().models() {
            assert_eq!(
                model.category(),
                Category::default_for(model.class()),
                "{}",
                model.id()
            );
        }
        assert_eq!(
            builtin_model("fuse.generic").category(),
            Category::Protection
        );
        assert_eq!(builtin_model("motor.3ph").category(), Category::Motors);
        assert_eq!(
            builtin_model("supply.terminal").category(),
            Category::Supplies
        );
    }

    #[test]
    fn category_keys_round_trip_and_are_unique() {
        let keys: BTreeSet<&str> = Category::ALL.into_iter().map(Category::key).collect();

        assert_eq!(keys.len(), Category::ALL.len());
        for category in Category::ALL {
            assert_eq!(Category::from_key(category.key()), Some(category));
            assert!(!category.label().is_empty());
        }
        assert_eq!(Category::from_key("Supplies"), None);
        assert_eq!(Category::from_key(""), None);
    }

    fn fuse_with(title: &str, note: Option<&str>) -> ComponentModel {
        let model = ComponentModel::new(
            "text.fuse",
            1,
            DeviceClass::Fuse,
            Vec::new(),
            vec![SymbolDefinition::new(
                "link",
                SymbolKind::FuseLink,
                1,
                1,
                false,
            )],
            Provenance::project_owned(),
        )
        .with_title(title);
        match note {
            Some(note) => model.with_note(note),
            None => model,
        }
    }

    #[test]
    fn title_and_note_are_kept_and_checked() {
        let catalog = ComponentCatalog::new(vec![fuse_with("Cartridge fuse", Some("Never melts"))]);
        let catalog = catalog.unwrap_or_else(|error| panic!("{error}"));
        let model = builtin_model_in(&catalog, "text.fuse");

        assert_eq!(model.title(), "Cartridge fuse");
        assert_eq!(model.note(), Some("Never melts"));
        assert_eq!(builtin_model("fuse.generic").note(), None);
    }

    fn builtin_model_in<'a>(catalog: &'a ComponentCatalog, id: &str) -> &'a ComponentModel {
        catalog
            .models()
            .iter()
            .find(|model| model.id() == id)
            .unwrap_or_else(|| panic!("model {id} is missing"))
    }

    #[test]
    fn catalog_rejects_empty_oversized_and_multiline_texts() {
        let long_title = "t".repeat(MAX_TEXT_BYTES + 1);
        let long_note = "n".repeat(MAX_NOTE_BYTES + 1);
        for model in [
            fuse_with("", None),
            fuse_with(&long_title, None),
            fuse_with("two\nlines", None),
            fuse_with("Fuse", Some("")),
            fuse_with("Fuse", Some(&long_note)),
            fuse_with("Fuse", Some("a\u{7}bell")),
        ] {
            assert!(
                matches!(
                    ComponentCatalog::new(vec![model]),
                    Err(CatalogError::InvalidModel { .. })
                ),
                "a bad text was accepted"
            );
        }
        let at_the_limit = "t".repeat(MAX_TEXT_BYTES);

        assert!(ComponentCatalog::new(vec![fuse_with(&at_the_limit, None)]).is_ok());
    }
}
