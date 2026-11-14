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
    /// Lamp: a two-terminal load.
    Lamp,
    /// Buzzer: a two-terminal load.
    Buzzer,
    /// Point where a supply conductor enters the circuit.
    SupplyTerminal,
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

impl SymbolKind {
    /// The terminals of a symbol of this kind, in canonical order, with their
    /// offsets from the symbol origin.
    ///
    /// Two-terminal elements list their two ends; multi-pole elements list
    /// `poleN.in` then `poleN.out` for each pole; the motor lists `u1`, `v1`,
    /// `w1`.
    ///
    /// Unrotated, a symbol is drawn top to bottom around its origin: the
    /// first terminal of a two-terminal element is 2 grid units above it and
    /// the second 2 below; the poles of a multi-pole element are
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
            Self::Coil => pair("a1", "a2", true),
            Self::ContactNo | Self::ContactNc | Self::FuseLink => pair("p1", "p2", false),
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
            Self::Lamp | Self::Buzzer => pair("x1", "x2", false),
            Self::SupplyTerminal => {
                vec![TerminalDefinition::new("out", false, GridPoint::new(0, 0))]
            }
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
            | Self::FuseLink
            | Self::Lamp
            | Self::Buzzer => corners(-2, ABOVE, 2, BELOW),
            Self::PowerContacts { poles } | Self::ThermalElement { poles } => {
                let outer = pole_x(poles, poles).saturating_add(2);
                corners(outer.saturating_neg(), ABOVE, outer, BELOW)
            }
            Self::Motor => corners(MOTOR_LEFT, ABOVE, POLE_PITCH, 6),
            Self::SupplyTerminal => corners(-1, -2, 1, 0),
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
        }
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
    fn accepts(&self, value: &ParamValue) -> Result<(), String> {
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
            (ParamType::SupplyRef, ParamValue::Int(v)) if *v > 0 => Ok(()),
            (ParamType::Bool, _) => Err(format!("parameter `{}` must be a boolean", self.key)),
            (ParamType::Int { .. }, _) => {
                Err(format!("parameter `{}` must be an integer", self.key))
            }
            (ParamType::Enum(_), _) => Err(format!("parameter `{}` must be a text key", self.key)),
            (ParamType::SupplyRef, _) => Err(format!(
                "parameter `{}` must be the positive id of a supply",
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

/// A versioned component model. A published `(id, version)` never changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComponentModel {
    id: String,
    version: u32,
    class: DeviceClass,
    params: Vec<ParamDefinition>,
    symbols: Vec<SymbolDefinition>,
    provenance: Provenance,
}

impl ComponentModel {
    /// Define a model. Consistency is checked by [`ComponentCatalog::new`].
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
            params,
            symbols,
            provenance,
        }
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

    /// Look up a model by its versioned reference.
    #[must_use]
    pub fn model(&self, reference: &ModelRef) -> Option<&ComponentModel> {
        self.models
            .iter()
            .find(|model| model.id == reference.id && model.version == reference.version)
    }
}

fn check_model(model: &ComponentModel) -> Result<(), String> {
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
            SymbolKind::Coil | SymbolKind::ContactNo | SymbolKind::ContactNc
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
        DeviceClass::Motor => matches!(kind, SymbolKind::Motor),
        DeviceClass::Lamp => matches!(kind, SymbolKind::Lamp),
        DeviceClass::Buzzer => matches!(kind, SymbolKind::Buzzer),
        DeviceClass::PowerSupply => matches!(kind, SymbolKind::SupplyTerminal),
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
}

impl ExpectedParam {
    fn matches(self, ty: &ParamType) -> bool {
        match (self, ty) {
            (Self::Bool, ParamType::Bool) | (Self::SupplyRef, ParamType::SupplyRef) => true,
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

    vec![
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
            vec![
                SymbolDefinition::new("coil", SymbolKind::Coil, 1, 1, false),
                aux("aux_no", SymbolKind::ContactNo, 8),
                aux("aux_nc", SymbolKind::ContactNc, 8),
            ],
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
            vec![
                SymbolDefinition::new("coil", SymbolKind::Coil, 1, 1, false),
                aux("delayed_no", SymbolKind::ContactNo, 4),
                aux("delayed_nc", SymbolKind::ContactNc, 4),
            ],
        ),
        model(
            "timer_off.generic",
            DeviceClass::Timer(TimerMode::OffDelay),
            vec![rated_voltage(), connection(), preset()],
            vec![
                SymbolDefinition::new("coil", SymbolKind::Coil, 1, 1, false),
                aux("delayed_no", SymbolKind::ContactNo, 4),
                aux("delayed_nc", SymbolKind::ContactNc, 4),
            ],
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
    ]
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
            SymbolKind::FuseLink,
            SymbolKind::Motor,
            SymbolKind::Lamp,
            SymbolKind::Buzzer,
            SymbolKind::SupplyTerminal,
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
                "fuse.generic@1",
                "lamp.generic@1",
                "main_switch.3p@1",
                "motor.3ph@1",
                "pushbutton_nc@1",
                "pushbutton_no@1",
                "relay.generic@1",
                "selector.latching@1",
                "supply.terminal@1",
                "thermal_overload.generic@1",
                "timer_off.generic@1",
                "timer_on.generic@1",
            ]
        );
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
}
