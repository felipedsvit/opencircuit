//! The command line: what a typed command asks for.
//!
//! A line such as `place std.mcb_3p main 10,5` is read by [`run`] against
//! the document, the catalog and the selection, and becomes [`Request`]s
//! for the program that shows it: a [`Transaction`] to apply, a tool to
//! choose, a view change. Nothing here draws or touches a file, and a
//! transaction comes from the same [`ops`] as every other way of editing, so
//! every rule of the editor holds. A line is hostile input: it is bounded in
//! size and in words, and a number is checked before it is used.

use std::collections::BTreeSet;

use circuit_model::{
    ComponentCatalog, ComponentModel, GridPoint, ModelRef, QuarterTurn, Schematic, Subject,
    SymbolId, SymbolKey,
};

use crate::{CommandError, Transaction, designators, layers, ops};

/// Longest line that is read, in bytes.
pub const MAX_LINE_BYTES: usize = 512;
/// Most words in a line.
pub const MAX_WORDS: usize = 12;
/// Largest coordinate, in grid units, in either direction.
pub const MAX_COORDINATE: i32 = 1_000_000;
/// Most suggestions of [`suggest`].
pub const MAX_SUGGESTIONS: usize = 12;

/// A command: its name, its short forms, how it is typed and what it does.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommandSpec {
    /// The name, in lowercase.
    pub name: &'static str,
    /// Other names for it, in lowercase.
    pub aliases: &'static [&'static str],
    /// How it is typed.
    pub usage: &'static str,
    /// What it does, in one line.
    pub help: &'static str,
}

/// Every command, sorted by name.
pub const COMMANDS: &[CommandSpec] = &[
    CommandSpec {
        name: "delete",
        aliases: &["erase", "e"],
        usage: "delete",
        help: "Delete the selection.",
    },
    CommandSpec {
        name: "help",
        aliases: &["?"],
        usage: "help [command]",
        help: "List the commands, or explain one.",
    },
    CommandSpec {
        name: "layer",
        aliases: &[],
        usage: "layer [new|show|hide|lock|unlock|delete|assign name | release]",
        help: "List the layers, add one, show, hide, lock or delete one, or put the selection on one (release: back on the default layer).",
    },
    CommandSpec {
        name: "mode",
        aliases: &[],
        usage: "mode edit|simulate",
        help: "Switch between drawing and simulating.",
    },
    CommandSpec {
        name: "move",
        aliases: &["m"],
        usage: "move dx,dy",
        help: "Move the selection by a number of grid units.",
    },
    CommandSpec {
        name: "parts",
        aliases: &["list"],
        usage: "parts [text]",
        help: "List the parts whose title, model or symbol has the text.",
    },
    CommandSpec {
        name: "place",
        aliases: &["p", "insert"],
        usage: "place model [symbol] [x,y] [0|90|180|270]",
        help: "Place a part at a point, or choose it to click where it goes.",
    },
    CommandSpec {
        name: "redo",
        aliases: &[],
        usage: "redo",
        help: "Redo the change that was undone.",
    },
    CommandSpec {
        name: "renumber",
        aliases: &[],
        usage: "renumber [sheet]",
        help: "Give the designators the next numbers in reading order, on every sheet or on one.",
    },
    CommandSpec {
        name: "rotate",
        aliases: &["r"],
        usage: "rotate",
        help: "Turn the selected symbols a quarter turn clockwise.",
    },
    CommandSpec {
        name: "save",
        aliases: &[],
        usage: "save",
        help: "Save the project.",
    },
    CommandSpec {
        name: "select",
        aliases: &["s"],
        usage: "select",
        help: "Choose the select tool.",
    },
    CommandSpec {
        name: "sheets",
        aliases: &[],
        usage: "sheets renumber",
        help: "Number the sheets 1, 2, 3 in reading order of their frames.",
    },
    CommandSpec {
        name: "sim",
        aliases: &[],
        usage: "sim play|pause|step|reset",
        help: "Run, pause, advance or restart the simulation.",
    },
    CommandSpec {
        name: "text",
        aliases: &[],
        usage: "text [x,y words]",
        help: "Choose the text tool, or add a note with the words at a point.",
    },
    CommandSpec {
        name: "undo",
        aliases: &["u"],
        usage: "undo",
        help: "Undo the last change.",
    },
    CommandSpec {
        name: "wire",
        aliases: &["w"],
        usage: "wire",
        help: "Choose the wire tool.",
    },
    CommandSpec {
        name: "zoom",
        aliases: &["z"],
        usage: "zoom e|s|in|out",
        help: "Show the whole drawing (e), the selection (s), or zoom in or out.",
    },
];

/// The tool a command chooses.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ToolRequest {
    /// Select and move.
    Select,
    /// Draw wires.
    Wire,
    /// Add a text note at each click.
    Text,
    /// Place this symbol where the user clicks.
    Place {
        /// The model.
        model: ModelRef,
        /// The symbol.
        symbol: SymbolKey,
        /// Its rotation.
        rotation: QuarterTurn,
    },
}

/// A change of the view.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ViewRequest {
    /// Show the whole drawing.
    Extents,
    /// Show the selection.
    Selection,
    /// Zoom in.
    ZoomIn,
    /// Zoom out.
    ZoomOut,
}

/// What the program is for.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModeRequest {
    /// Draw.
    Edit,
    /// Simulate.
    Simulate,
}

/// An action on the simulation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SimRequest {
    /// Run.
    Play,
    /// Stop running.
    Pause,
    /// Advance one tick.
    Step,
    /// Start again.
    Reset,
}

/// What a command asks the program to do.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Request {
    /// Apply this change to the document.
    Edit(Transaction),
    /// Choose a tool.
    Tool(ToolRequest),
    /// Change the view.
    View(ViewRequest),
    /// Change the mode.
    Mode(ModeRequest),
    /// Act on the simulation.
    Simulation(SimRequest),
    /// Undo the last change.
    Undo,
    /// Redo the change that was undone.
    Redo,
    /// Save the project.
    Save,
    /// Show this text to the user.
    Message(String),
}

impl Request {
    /// Whether the request changes or draws the document, so that it only
    /// makes sense while editing.
    #[must_use]
    pub const fn needs_editing(&self) -> bool {
        match self {
            Self::Edit(_) | Self::Undo | Self::Redo => true,
            Self::Tool(tool) => matches!(tool, ToolRequest::Wire | ToolRequest::Place { .. }),
            Self::View(_) | Self::Mode(_) | Self::Simulation(_) | Self::Save | Self::Message(_) => {
                false
            }
        }
    }
}

/// Why a line could not be carried out.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CommandLineError {
    /// The line is longer than [`MAX_LINE_BYTES`].
    #[error("the line is longer than {MAX_LINE_BYTES} bytes")]
    TooLong,
    /// The line has more than [`MAX_WORDS`] words.
    #[error("the line has more than {MAX_WORDS} words")]
    TooManyWords,
    /// The line has a control character.
    #[error("the line has a control character")]
    ControlCharacter,
    /// There is no such command.
    #[error("unknown command `{0}`; `help` lists them")]
    Unknown(String),
    /// The command is typed another way.
    #[error("usage: {0}")]
    Usage(&'static str),
    /// A word is not a point such as `10,5`.
    #[error("`{0}` is not a point such as 10,5")]
    BadPoint(String),
    /// The model is not in the catalog.
    #[error("no part `{0}`; `parts` lists them")]
    UnknownModel(String),
    /// More than one model ends with the text.
    #[error("`{0}` is ambiguous: {1}")]
    AmbiguousModel(String, String),
    /// The model has no such symbol.
    #[error("{model} has no symbol `{symbol}`; it has: {symbols}")]
    UnknownSymbol {
        /// The model.
        model: String,
        /// What was typed.
        symbol: String,
        /// The symbols it has.
        symbols: String,
    },
    /// There is no layer with this name.
    #[error("no layer `{0}`; `layer` lists them")]
    NoSuchLayer(String),
    /// The command works on the selection and nothing is selected.
    #[error("nothing is selected")]
    NothingSelected,
    /// The editor refused the change.
    #[error("{0}")]
    Refused(#[from] CommandError),
}

/// What a line is read against.
#[derive(Clone, Copy, Debug)]
pub struct Context<'a> {
    /// The document.
    pub doc: &'a Schematic,
    /// The catalog.
    pub catalog: &'a ComponentCatalog,
    /// What is selected.
    pub selection: &'a BTreeSet<Subject>,
}

/// The command a word names: its name or one of its aliases.
#[must_use]
pub fn find(word: &str) -> Option<&'static CommandSpec> {
    let word = word.to_lowercase();
    COMMANDS
        .iter()
        .find(|command| command.name == word || command.aliases.contains(&word.as_str()))
}

/// Read and carry out one line.
///
/// # Errors
///
/// Returns [`CommandLineError`] for a line that is too long or has too many
/// words, a command that does not exist or is typed wrongly, a part that is
/// not in the catalog, and a change that the editor refuses.
pub fn run(line: &str, context: &Context<'_>) -> Result<Vec<Request>, CommandLineError> {
    if line.len() > MAX_LINE_BYTES {
        return Err(CommandLineError::TooLong);
    }
    if line.chars().any(char::is_control) {
        return Err(CommandLineError::ControlCharacter);
    }
    let words: Vec<&str> = line.split_whitespace().collect();
    if words.len() > MAX_WORDS {
        return Err(CommandLineError::TooManyWords);
    }
    let Some((first, rest)) = words.split_first() else {
        return Ok(Vec::new());
    };
    let command = find(first).ok_or_else(|| CommandLineError::Unknown((*first).to_owned()))?;
    match command.name {
        "place" => place(rest, context),
        "move" => move_selection(rest, context),
        "renumber" => renumber_designators(rest, context),
        "sheets" => match rest {
            ["renumber"] => Ok(vec![Request::Edit(crate::sheets::renumber_sheets(
                context.doc,
            ))]),
            _ => Err(CommandLineError::Usage("sheets renumber")),
        },
        "rotate" => rotate_selection(rest, context),
        "delete" => delete_selection(rest, context),
        "undo" => bare(rest, command, Request::Undo),
        "redo" => bare(rest, command, Request::Redo),
        "save" => bare(rest, command, Request::Save),
        "wire" => bare(rest, command, Request::Tool(ToolRequest::Wire)),
        "text" => text_note(rest, command, context),
        "layer" => layer_command(rest, command, context),
        "select" => bare(rest, command, Request::Tool(ToolRequest::Select)),
        "zoom" => zoom(rest, command),
        "mode" => mode(rest, command),
        "sim" => simulation(rest, command),
        "parts" => Ok(vec![Request::Message(parts(context.catalog, rest))]),
        _ => Ok(vec![Request::Message(help(rest))]),
    }
}

fn bare(
    rest: &[&str],
    command: &CommandSpec,
    request: Request,
) -> Result<Vec<Request>, CommandLineError> {
    if rest.is_empty() {
        Ok(vec![request])
    } else {
        Err(CommandLineError::Usage(command.usage))
    }
}

/// The only word of `rest`, in lowercase.
fn one_word(rest: &[&str], command: &CommandSpec) -> Result<String, CommandLineError> {
    match rest {
        [word] => Ok(word.to_lowercase()),
        _ => Err(CommandLineError::Usage(command.usage)),
    }
}

fn zoom(rest: &[&str], command: &CommandSpec) -> Result<Vec<Request>, CommandLineError> {
    let view = match one_word(rest, command)?.as_str() {
        "e" | "extents" => ViewRequest::Extents,
        "s" | "selection" => ViewRequest::Selection,
        "in" => ViewRequest::ZoomIn,
        "out" => ViewRequest::ZoomOut,
        _ => return Err(CommandLineError::Usage(command.usage)),
    };
    Ok(vec![Request::View(view)])
}

fn mode(rest: &[&str], command: &CommandSpec) -> Result<Vec<Request>, CommandLineError> {
    let mode = match one_word(rest, command)?.as_str() {
        "edit" | "draw" => ModeRequest::Edit,
        "simulate" | "sim" => ModeRequest::Simulate,
        _ => return Err(CommandLineError::Usage(command.usage)),
    };
    Ok(vec![Request::Mode(mode)])
}

fn simulation(rest: &[&str], command: &CommandSpec) -> Result<Vec<Request>, CommandLineError> {
    let action = match one_word(rest, command)?.as_str() {
        "play" | "run" => SimRequest::Play,
        "pause" | "stop" => SimRequest::Pause,
        "step" => SimRequest::Step,
        "reset" => SimRequest::Reset,
        _ => return Err(CommandLineError::Usage(command.usage)),
    };
    Ok(vec![Request::Simulation(action)])
}

/// A point typed at the pointer: `x,y`, or `@dx,dy` from `from`. Spaces
/// are ignored. `None` for anything else, or a point outside the coordinate
/// range.
#[must_use]
pub fn typed_point(text: &str, from: GridPoint) -> Option<GridPoint> {
    let word: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    match word.strip_prefix('@') {
        None => point(&word).ok(),
        Some(offset) => {
            let delta = point(offset).ok()?;
            let x = from.x.checked_add(delta.x)?;
            let y = from.y.checked_add(delta.y)?;
            let inside = |value: i32| (-MAX_COORDINATE..=MAX_COORDINATE).contains(&value);
            (inside(x) && inside(y)).then_some(GridPoint::new(x, y))
        }
    }
}

/// A point such as `10,5` or `-3,0`.
fn point(word: &str) -> Result<GridPoint, CommandLineError> {
    let bad = || CommandLineError::BadPoint(word.to_owned());
    let (x, y) = word.split_once(',').ok_or_else(bad)?;
    let coordinate = |text: &str| {
        text.parse::<i32>()
            .ok()
            .filter(|value| (-MAX_COORDINATE..=MAX_COORDINATE).contains(value))
    };
    match (coordinate(x), coordinate(y)) {
        (Some(x), Some(y)) => Ok(GridPoint::new(x, y)),
        _ => Err(bad()),
    }
}

fn rotation(word: &str) -> Option<QuarterTurn> {
    match word {
        "0" => Some(QuarterTurn::Deg0),
        "90" => Some(QuarterTurn::Deg90),
        "180" => Some(QuarterTurn::Deg180),
        "270" => Some(QuarterTurn::Deg270),
        _ => None,
    }
}

/// The model a word names: its identifier, or the end of one when only one
/// model ends that way (`mcb_3p` for `std.mcb_3p`). The newest version.
fn model_named<'a>(
    catalog: &'a ComponentCatalog,
    word: &str,
) -> Result<&'a ComponentModel, CommandLineError> {
    let word = word.to_lowercase();
    if let Some(exact) = catalog.latest().find(|model| model.id() == word) {
        return Ok(exact);
    }
    let dotted = format!(".{word}");
    let mut found = catalog
        .latest()
        .filter(|model| model.id().ends_with(&dotted));
    match (found.next(), found.next()) {
        (Some(only), None) => Ok(only),
        (None, _) => Err(CommandLineError::UnknownModel(word)),
        (Some(first), Some(second)) => {
            let names: Vec<&str> = [first, second]
                .into_iter()
                .chain(found)
                .map(ComponentModel::id)
                .take(MAX_SUGGESTIONS)
                .collect();
            Err(CommandLineError::AmbiguousModel(word, names.join(", ")))
        }
    }
}

fn place(rest: &[&str], context: &Context<'_>) -> Result<Vec<Request>, CommandLineError> {
    const USAGE: &str = "place model [symbol] [x,y] [0|90|180|270]";
    let Some((model_word, options)) = rest.split_first() else {
        return Err(CommandLineError::Usage(USAGE));
    };
    let model = model_named(context.catalog, model_word)?;
    let mut symbol = None;
    let mut at = None;
    let mut turn = None;
    for word in options {
        if word.contains(',') {
            if at.replace(point(word)?).is_some() {
                return Err(CommandLineError::Usage(USAGE));
            }
        } else if let Some(found) = rotation(word) {
            if turn.replace(found).is_some() {
                return Err(CommandLineError::Usage(USAGE));
            }
        } else if symbol.replace(word.to_lowercase()).is_some() {
            return Err(CommandLineError::Usage(USAGE));
        }
    }
    let definition = match &symbol {
        Some(key) => model
            .symbols()
            .iter()
            .find(|definition| definition.key().0 == *key)
            .ok_or_else(|| CommandLineError::UnknownSymbol {
                model: model.id().to_owned(),
                symbol: key.clone(),
                symbols: model
                    .symbols()
                    .iter()
                    .map(|definition| definition.key().0.as_str())
                    .collect::<Vec<_>>()
                    .join(", "),
            })?,
        // The first symbol a device must have, or else the first one.
        None => model
            .symbols()
            .iter()
            .find(|definition| definition.min_instances() > 0)
            .or_else(|| model.symbols().first())
            .ok_or(CommandLineError::Usage(USAGE))?,
    };
    let reference = ModelRef {
        id: model.id().to_owned(),
        version: model.version(),
    };
    let rotation = turn.unwrap_or_default();
    match at {
        Some(at) => Ok(vec![Request::Edit(ops::place(
            context.doc,
            context.catalog,
            &reference,
            definition.key(),
            at,
            rotation,
        )?)]),
        None => Ok(vec![Request::Tool(ToolRequest::Place {
            model: reference,
            symbol: definition.key().clone(),
            rotation,
        })]),
    }
}

fn selected(context: &Context<'_>) -> Result<(), CommandLineError> {
    if context.selection.is_empty() {
        Err(CommandLineError::NothingSelected)
    } else {
        Ok(())
    }
}

fn move_selection(rest: &[&str], context: &Context<'_>) -> Result<Vec<Request>, CommandLineError> {
    let command = find("move").ok_or(CommandLineError::Usage("move dx,dy"))?;
    let [word] = rest else {
        return Err(CommandLineError::Usage(command.usage));
    };
    let delta = point(word)?;
    selected(context)?;
    Ok(vec![Request::Edit(ops::move_by(
        context.doc,
        context.catalog,
        context.selection,
        delta,
    )?)])
}

/// `text` chooses the tool; `text x,y words…` adds a note, its words joined by
/// single spaces.
fn text_note(
    rest: &[&str],
    command: &CommandSpec,
    context: &Context<'_>,
) -> Result<Vec<Request>, CommandLineError> {
    let Some((at, words)) = rest.split_first() else {
        return Ok(vec![Request::Tool(ToolRequest::Text)]);
    };
    if words.is_empty() {
        return Err(CommandLineError::Usage(command.usage));
    }
    Ok(vec![Request::Edit(ops::add_text(
        context.doc,
        point(at)?,
        &words.join(" "),
        circuit_model::TextAnchor::Left,
    )?)])
}

/// The layer a name says: exactly, or without regard to case when only one
/// layer matches.
fn layer_named<'a>(
    doc: &'a Schematic,
    name: &str,
) -> Result<&'a circuit_model::Layer, CommandLineError> {
    if let Some(exact) = doc.layers.iter().find(|layer| layer.name == name) {
        return Ok(exact);
    }
    let lower = name.to_lowercase();
    let mut found = doc
        .layers
        .iter()
        .filter(|layer| layer.name.to_lowercase() == lower);
    match (found.next(), found.next()) {
        (Some(layer), None) => Ok(layer),
        _ => Err(CommandLineError::NoSuchLayer(name.to_owned())),
    }
}

fn layer_command(
    rest: &[&str],
    command: &CommandSpec,
    context: &Context<'_>,
) -> Result<Vec<Request>, CommandLineError> {
    let doc = context.doc;
    let Some((action, words)) = rest.split_first() else {
        let lines: Vec<String> = doc
            .layers
            .iter()
            .map(|layer| {
                format!(
                    "{}{}{}",
                    layer.name,
                    if layer.visible { "" } else { " (hidden)" },
                    if layer.locked { " (locked)" } else { "" }
                )
            })
            .collect();
        return Ok(vec![Request::Message(if lines.is_empty() {
            "no layers; `layer new name` adds one".to_owned()
        } else {
            lines.join("\n")
        })]);
    };
    let name = words.join(" ");
    let action = action.to_lowercase();
    if action == "release" {
        if !words.is_empty() {
            return Err(CommandLineError::Usage(command.usage));
        }
        selected(context)?;
        return Ok(vec![Request::Edit(layers::assign_layer(
            doc,
            context.selection,
            None,
        )?)]);
    }
    if words.is_empty() {
        return Err(CommandLineError::Usage(command.usage));
    }
    let transaction = match action.as_str() {
        "new" => layers::add_layer(doc, &name, true, false)?,
        "show" | "hide" | "lock" | "unlock" => {
            let id = layer_named(doc, &name)?.id;
            match action.as_str() {
                "show" => layers::set_layer(doc, id, None, Some(true), None)?,
                "hide" => layers::set_layer(doc, id, None, Some(false), None)?,
                "lock" => layers::set_layer(doc, id, None, None, Some(true))?,
                _ => layers::set_layer(doc, id, None, None, Some(false))?,
            }
        }
        "delete" => layers::remove_layer(doc, layer_named(doc, &name)?.id)?,
        "assign" => {
            let id = layer_named(doc, &name)?.id;
            selected(context)?;
            layers::assign_layer(doc, context.selection, Some(id))?
        }
        _ => return Err(CommandLineError::Usage(command.usage)),
    };
    Ok(vec![Request::Edit(transaction)])
}

fn renumber_designators(
    rest: &[&str],
    context: &Context<'_>,
) -> Result<Vec<Request>, CommandLineError> {
    let scope = match rest {
        [] => designators::Scope::All,
        [sheet] => designators::Scope::Sheet(
            sheet
                .parse::<u32>()
                .map_err(|_| CommandLineError::Usage("renumber [sheet]"))?,
        ),
        _ => return Err(CommandLineError::Usage("renumber [sheet]")),
    };
    Ok(vec![Request::Edit(designators::renumber(
        context.doc,
        scope,
    )?)])
}

fn rotate_selection(
    rest: &[&str],
    context: &Context<'_>,
) -> Result<Vec<Request>, CommandLineError> {
    if !rest.is_empty() {
        return Err(CommandLineError::Usage("rotate"));
    }
    selected(context)?;
    let symbols: BTreeSet<SymbolId> = context
        .selection
        .iter()
        .filter_map(|subject| match subject {
            Subject::Symbol(id) => Some(*id),
            _ => None,
        })
        .collect();
    if symbols.is_empty() {
        return Err(CommandLineError::NothingSelected);
    }
    Ok(vec![Request::Edit(ops::rotate(
        context.doc,
        context.catalog,
        &symbols,
    )?)])
}

fn delete_selection(
    rest: &[&str],
    context: &Context<'_>,
) -> Result<Vec<Request>, CommandLineError> {
    if !rest.is_empty() {
        return Err(CommandLineError::Usage("delete"));
    }
    selected(context)?;
    Ok(vec![Request::Edit(ops::delete(
        context.doc,
        context.catalog,
        context.selection,
    )?)])
}

/// The parts that match `words`, one per line, at most [`MAX_SUGGESTIONS`].
fn parts(catalog: &ComponentCatalog, words: &[&str]) -> String {
    let needle = words.join(" ").to_lowercase();
    let lines: Vec<String> = catalog
        .latest()
        .filter(|model| {
            needle.is_empty()
                || model.id().contains(&needle)
                || model.title().to_lowercase().contains(&needle)
                || model
                    .symbols()
                    .iter()
                    .any(|symbol| symbol.key().0.contains(&needle))
        })
        .map(|model| format!("{}: {}", model.id(), model.title()))
        .collect();
    let shown = lines.len().min(MAX_SUGGESTIONS);
    let mut text = lines
        .iter()
        .take(MAX_SUGGESTIONS)
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");
    if lines.len() > shown {
        text.push_str(&format!(
            "\n… and {} more",
            lines.len().saturating_sub(shown)
        ));
    }
    if text.is_empty() {
        "no part matches".to_owned()
    } else {
        text
    }
}

fn help(words: &[&str]) -> String {
    if let [word] = words {
        if let Some(command) = find(word) {
            let aliases = if command.aliases.is_empty() {
                String::new()
            } else {
                format!(" (also {})", command.aliases.join(", "))
            };
            return format!("{}{aliases}\n{}", command.usage, command.help);
        }
    }
    COMMANDS
        .iter()
        .map(|command| format!("{}: {}", command.usage, command.help))
        .collect::<Vec<_>>()
        .join("\n")
}

/// What the user may type next: the commands and their aliases that start
/// like the only word of `line`, or, after `place`, the models whose
/// identifier starts like the next word. At most [`MAX_SUGGESTIONS`], in a
/// stable order, each one a replacement for the last word.
#[must_use]
pub fn suggest(line: &str, catalog: &ComponentCatalog) -> Vec<String> {
    if line.len() > MAX_LINE_BYTES {
        return Vec::new();
    }
    let ends_in_space = line.ends_with(char::is_whitespace);
    let words: Vec<&str> = line.split_whitespace().collect();
    match (words.as_slice(), ends_in_space) {
        ([], _) => Vec::new(),
        ([word], false) => {
            let word = word.to_lowercase();
            let mut names: Vec<String> = COMMANDS
                .iter()
                .flat_map(|command| {
                    std::iter::once(command.name).chain(command.aliases.iter().copied())
                })
                .filter(|name| name.starts_with(&word) && name.len() > 1)
                .map(str::to_owned)
                .collect();
            names.sort();
            names.dedup();
            names.truncate(MAX_SUGGESTIONS);
            names
        }
        ([command, word], false) if find(command).is_some_and(|c| c.name == "place") => {
            let word = word.to_lowercase();
            catalog
                .latest()
                .map(ComponentModel::id)
                .filter(|id| id.starts_with(&word))
                .take(MAX_SUGGESTIONS)
                .map(str::to_owned)
                .collect()
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use circuit_model::SchematicBuilder;
    use proptest::prelude::*;

    use super::*;
    use crate::History;

    fn context<'a>(
        doc: &'a Schematic,
        catalog: &'a ComponentCatalog,
        selection: &'a BTreeSet<Subject>,
    ) -> Context<'a> {
        Context {
            doc,
            catalog,
            selection,
        }
    }

    fn empty() -> (Schematic, ComponentCatalog, BTreeSet<Subject>) {
        (
            SchematicBuilder::new("test").finish(),
            ComponentCatalog::builtin(),
            BTreeSet::new(),
        )
    }

    fn requests(line: &str) -> Result<Vec<Request>, CommandLineError> {
        let (doc, catalog, selection) = empty();
        run(line, &context(&doc, &catalog, &selection))
    }

    #[test]
    fn renumber_asks_for_a_transaction_on_all_sheets_or_on_one() {
        let (_, catalog, selection) = empty();
        let mut b = circuit_model::SchematicBuilder::new("n");
        let device = b.device("S5", "pushbutton_no", &[]).unwrap();
        b.symbol(device, "contact", None, GridPoint::new(0, 0))
            .unwrap();
        let doc = b.finish();

        let all = run("renumber", &context(&doc, &catalog, &selection)).unwrap();
        assert!(matches!(&all[..], [Request::Edit(tx)] if tx.edits.len() == 1));

        // There is no sheet 2.
        assert!(run("renumber 2", &context(&doc, &catalog, &selection)).is_err());
        for line in ["renumber x", "renumber 1 2", "renumber -1"] {
            assert_eq!(
                run(line, &context(&doc, &catalog, &selection)),
                Err(CommandLineError::Usage("renumber [sheet]")),
                "{line}"
            );
        }
    }

    #[test]
    fn a_typed_point_is_absolute_or_relative_to_the_pointer() {
        let from = GridPoint::new(10, 5);
        assert_eq!(typed_point("3, -4", from), Some(GridPoint::new(3, -4)));
        assert_eq!(typed_point("@ 2,-1", from), Some(GridPoint::new(12, 4)));
        for bad in ["", "3", "a,b", "@", "@1", "1,2,3"] {
            assert_eq!(typed_point(bad, from), None, "{bad}");
        }
        let edge = GridPoint::new(MAX_COORDINATE, 0);
        assert_eq!(typed_point("@1,0", edge), None);
    }

    #[test]
    fn sheets_renumber_asks_for_one_transaction() {
        let catalog = ComponentCatalog::builtin();
        let doc = Schematic::new("none");
        let selection = BTreeSet::new();

        let asked = run("sheets renumber", &context(&doc, &catalog, &selection)).unwrap();
        assert!(matches!(&asked[..], [Request::Edit(tx)] if tx.label == "Renumber sheets"));
        for line in ["sheets", "sheets renumber now", "sheets add"] {
            assert_eq!(
                run(line, &context(&doc, &catalog, &selection)),
                Err(CommandLineError::Usage("sheets renumber")),
                "{line}"
            );
        }
    }

    #[test]
    fn commands_and_their_aliases_are_unique_sorted_and_found() {
        let names: Vec<&str> = COMMANDS.iter().map(|command| command.name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted);
        let mut every: Vec<&str> = COMMANDS
            .iter()
            .flat_map(|command| {
                std::iter::once(command.name).chain(command.aliases.iter().copied())
            })
            .collect();
        let total = every.len();
        every.sort_unstable();
        every.dedup();
        assert_eq!(every.len(), total, "a name or an alias is used twice");
        for command in COMMANDS {
            assert_eq!(find(command.name), Some(command));
            assert_eq!(find(&command.name.to_uppercase()), Some(command));
            for alias in command.aliases {
                assert_eq!(find(alias), Some(command));
            }
        }
    }

    #[test]
    fn an_empty_line_does_nothing_and_an_unknown_command_is_named() {
        assert_eq!(requests("   "), Ok(Vec::new()));
        assert_eq!(
            requests("frobnicate 1"),
            Err(CommandLineError::Unknown("frobnicate".to_owned()))
        );
    }

    #[test]
    fn simple_commands_become_requests_whatever_their_case() {
        for (line, request) in [
            ("W", Request::Tool(ToolRequest::Wire)),
            ("wire", Request::Tool(ToolRequest::Wire)),
            ("TEXT", Request::Tool(ToolRequest::Text)),
            ("SELECT", Request::Tool(ToolRequest::Select)),
            ("u", Request::Undo),
            ("Redo", Request::Redo),
            ("save", Request::Save),
            ("zoom e", Request::View(ViewRequest::Extents)),
            ("Z IN", Request::View(ViewRequest::ZoomIn)),
            ("zoom out", Request::View(ViewRequest::ZoomOut)),
            ("zoom s", Request::View(ViewRequest::Selection)),
            ("mode edit", Request::Mode(ModeRequest::Edit)),
            ("mode simulate", Request::Mode(ModeRequest::Simulate)),
            ("sim play", Request::Simulation(SimRequest::Play)),
            ("sim step", Request::Simulation(SimRequest::Step)),
            ("sim reset", Request::Simulation(SimRequest::Reset)),
            ("sim pause", Request::Simulation(SimRequest::Pause)),
        ] {
            assert_eq!(requests(line), Ok(vec![request]), "{line}");
        }
    }

    #[test]
    fn a_command_typed_wrongly_says_how_it_is_typed() {
        for line in [
            "undo now",
            "zoom",
            "zoom sideways",
            "sim",
            "mode",
            "wire 1,1",
        ] {
            assert!(
                matches!(requests(line), Err(CommandLineError::Usage(_))),
                "{line}"
            );
        }
    }

    #[test]
    fn place_with_a_point_is_a_transaction_that_places_the_part() {
        let (mut doc, catalog, selection) = empty();
        let request = run(
            "place lamp.generic 4,6",
            &context(&doc, &catalog, &selection),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        let [Request::Edit(transaction)] = request.as_slice() else {
            panic!("expected one edit: {request:?}");
        };
        let mut history = History::new();
        history.execute(&mut doc, transaction.clone()).unwrap();

        assert_eq!(doc.devices.len(), 1);
        assert_eq!(doc.symbols[0].position, GridPoint::new(4, 6));
        assert_eq!(doc.devices[0].model.id, "lamp.generic");
    }

    #[test]
    fn place_without_a_point_chooses_the_tool_with_the_first_required_symbol() {
        assert_eq!(
            requests("place contactor.generic"),
            Ok(vec![Request::Tool(ToolRequest::Place {
                model: ModelRef {
                    id: "contactor.generic".to_owned(),
                    version: 1
                },
                symbol: SymbolKey("coil".to_owned()),
                rotation: QuarterTurn::Deg0,
            })])
        );
        let with_symbol = requests("P contactor.generic AUX_NO 90");
        assert!(matches!(
            with_symbol.as_deref(),
            Ok([Request::Tool(ToolRequest::Place { symbol, rotation: QuarterTurn::Deg90, .. })])
                if symbol.0 == "aux_no"
        ));
    }

    #[test]
    fn the_end_of_a_model_identifier_names_it_when_that_is_unique() {
        let catalog = ComponentCatalog::builtin();
        let found = model_named(&catalog, "relay.generic").map(ComponentModel::id);

        assert_eq!(found, Ok("relay.generic"));
        assert!(matches!(
            model_named(&catalog, "generic"),
            Err(CommandLineError::AmbiguousModel(..))
        ));
        assert!(matches!(
            model_named(&catalog, "zzz"),
            Err(CommandLineError::UnknownModel(_))
        ));
    }

    #[test]
    fn place_refuses_what_it_cannot_understand() {
        assert!(matches!(requests("place"), Err(CommandLineError::Usage(_))));
        assert!(matches!(
            requests("place nothing"),
            Err(CommandLineError::UnknownModel(_))
        ));
        assert!(matches!(
            requests("place lamp.generic nope"),
            Err(CommandLineError::UnknownSymbol { .. })
        ));
        for line in [
            "place lamp.generic 1,1 2,2",
            "place lamp.generic 90 180",
            "place lamp.generic lamp lamp",
        ] {
            assert!(
                matches!(requests(line), Err(CommandLineError::Usage(_))),
                "{line}"
            );
        }
    }

    #[test]
    fn points_are_checked_before_they_are_used() {
        for word in [
            "1",
            "1,",
            ",1",
            "a,b",
            "1.5,2",
            "1,2,3",
            "1000001,0",
            "0,-1000001",
            "99999999999999999999,0",
            "--1,0",
            "+,+",
        ] {
            assert!(
                matches!(point(word), Err(CommandLineError::BadPoint(_))),
                "{word}"
            );
        }
        assert_eq!(point("-3,12"), Ok(GridPoint::new(-3, 12)));
        assert_eq!(
            point("1000000,-1000000"),
            Ok(GridPoint::new(1_000_000, -1_000_000))
        );
    }

    #[test]
    fn commands_on_the_selection_need_one() {
        for line in ["move 1,0", "rotate", "delete"] {
            assert_eq!(
                requests(line),
                Err(CommandLineError::NothingSelected),
                "{line}"
            );
        }
        assert!(matches!(
            requests("move one,two"),
            Err(CommandLineError::BadPoint(_))
        ));
    }

    #[test]
    fn the_selection_is_moved_rotated_and_deleted_through_the_editor_operations() {
        let (mut doc, catalog, none) = empty();
        let place = run("place lamp.generic 0,0", &context(&doc, &catalog, &none)).unwrap();
        let [Request::Edit(transaction)] = place.as_slice() else {
            panic!("{place:?}");
        };
        let mut history = History::new();
        history.execute(&mut doc, transaction.clone()).unwrap();
        let selection = BTreeSet::from([Subject::Symbol(doc.symbols[0].id)]);

        let moved = run("move 2,3", &context(&doc, &catalog, &selection)).unwrap();
        let rotated = run("rotate", &context(&doc, &catalog, &selection)).unwrap();
        let deleted = run("erase", &context(&doc, &catalog, &selection)).unwrap();

        for request in [moved, rotated, deleted] {
            assert!(
                matches!(request.as_slice(), [Request::Edit(_)]),
                "{request:?}"
            );
        }
    }

    #[test]
    fn what_needs_editing_is_known() {
        assert!(Request::Undo.needs_editing());
        assert!(Request::Tool(ToolRequest::Wire).needs_editing());
        assert!(!Request::Tool(ToolRequest::Select).needs_editing());
        assert!(!Request::View(ViewRequest::Extents).needs_editing());
        assert!(!Request::Save.needs_editing());
    }

    #[test]
    fn help_and_parts_answer_with_text() {
        let all = requests("help").unwrap();
        let one = requests("help place").unwrap();
        let parts = requests("parts fuse").unwrap();

        let text = |requests: &[Request]| match requests {
            [Request::Message(text)] => text.clone(),
            other => panic!("{other:?}"),
        };
        assert!(text(&all).contains("place model"));
        assert!(text(&one).contains("also p, insert"));
        assert!(text(&parts).contains("fuse.generic"));
        assert_eq!(text(&requests("parts zzzz").unwrap()), "no part matches");
    }

    #[test]
    fn a_hostile_line_is_refused_before_it_is_read() {
        assert_eq!(
            requests(&"w".repeat(MAX_LINE_BYTES + 1)),
            Err(CommandLineError::TooLong)
        );
        assert_eq!(requests("w\u{7}"), Err(CommandLineError::ControlCharacter));
        assert_eq!(requests("w\nw"), Err(CommandLineError::ControlCharacter));
        let many = "w ".repeat(MAX_WORDS + 1);
        assert_eq!(requests(&many), Err(CommandLineError::TooManyWords));
        // The limit itself is fine.
        assert!(requests(&"w".repeat(MAX_LINE_BYTES)).is_err());
    }

    #[test]
    fn suggestions_complete_commands_and_models() {
        let catalog = ComponentCatalog::builtin();

        assert_eq!(suggest("", &catalog), Vec::<String>::new());
        assert_eq!(suggest("zo", &catalog), ["zoom"]);
        assert_eq!(suggest("p", &catalog), ["parts", "place"]);
        assert!(suggest("pl", &catalog).contains(&"place".to_owned()));
        let models = suggest("place contactor", &catalog);
        assert_eq!(models, ["contactor.generic"]);
        assert!(suggest("place ", &catalog).is_empty());
        assert!(suggest("wire x", &catalog).is_empty());
        assert!(suggest(&"p".repeat(MAX_LINE_BYTES + 1), &catalog).is_empty());
    }

    proptest! {
        #[test]
        fn no_line_makes_the_command_line_panic(line in ".{0,200}") {
            let (doc, catalog, selection) = empty();
            let _ = run(&line, &context(&doc, &catalog, &selection));
            let _ = suggest(&line, &catalog);
        }

        #[test]
        fn no_numbers_make_a_point_or_a_place_panic(x in any::<i64>(), y in any::<i64>(), r in any::<u16>()) {
            let (doc, catalog, selection) = empty();
            let line = format!("place lamp.generic {x},{y} {r}");
            let _ = run(&line, &context(&doc, &catalog, &selection));
            let _ = run(&format!("move {x},{y}"), &context(&doc, &catalog, &selection));
        }
    }

    #[test]
    fn text_alone_chooses_the_tool_and_with_a_point_and_words_adds_a_note() {
        assert_eq!(requests("text"), Ok(vec![Request::Tool(ToolRequest::Text)]));

        let doc = Schematic::new("notes");
        let catalog = ComponentCatalog::builtin();
        let selection = BTreeSet::new();
        let context = Context {
            doc: &doc,
            catalog: &catalog,
            selection: &selection,
        };
        let made = run("text 4,3  Motor   feed", &context).unwrap();

        let [Request::Edit(tx)] = made.as_slice() else {
            panic!("one edit expected: {made:?}");
        };
        let (next, _) = tx.apply_to(&doc).unwrap();
        assert_eq!(next.texts[0].text, "Motor feed");
        assert_eq!(next.texts[0].position, GridPoint::new(4, 3));
        // A point without words, a bad point and a refused text say so.
        assert_eq!(
            run("text 4,3", &context),
            Err(CommandLineError::Usage("text [x,y words]"))
        );
        assert!(matches!(
            run("text nope hello", &context),
            Err(CommandLineError::BadPoint(_))
        ));
        let long = format!("text 0,0 {}", "x".repeat(121));
        assert!(matches!(
            run(&long, &context),
            Err(CommandLineError::Refused(_))
        ));
    }

    #[test]
    fn layer_commands_add_show_hide_lock_assign_and_release() {
        let catalog = ComponentCatalog::builtin();
        let mut b = circuit_model::SchematicBuilder::new("n");
        let device = b.device("S1", "pushbutton_no", &[]).unwrap();
        let symbol = b
            .symbol(device, "contact", None, GridPoint::new(0, 0))
            .unwrap();
        let mut doc = b.finish();
        let none = BTreeSet::new();
        let apply = |doc: &mut Schematic, line: &str, selection: &BTreeSet<Subject>| {
            let made = run(line, &context(doc, &catalog, selection)).unwrap();
            let [Request::Edit(tx)] = made.as_slice() else {
                panic!("one edit expected for {line}: {made:?}");
            };
            *doc = tx.apply_to(doc).unwrap().0;
        };

        apply(&mut doc, "layer new Power lines", &none);
        assert_eq!(doc.layers[0].name, "Power lines");
        apply(&mut doc, "layer hide power lines", &none);
        assert!(!doc.layers[0].visible);
        apply(&mut doc, "layer show Power lines", &none);
        apply(&mut doc, "layer lock Power lines", &none);
        assert!(doc.layers[0].locked);
        apply(&mut doc, "layer unlock Power lines", &none);

        let chosen = BTreeSet::from([Subject::Symbol(symbol)]);
        apply(&mut doc, "layer assign Power lines", &chosen);
        assert_eq!(doc.symbols[0].layer, Some(doc.layers[0].id));
        apply(&mut doc, "layer release", &chosen);
        assert_eq!(doc.symbols[0].layer, None);
        apply(&mut doc, "layer delete Power lines", &none);
        assert!(doc.layers.is_empty());
    }

    #[test]
    fn layer_alone_lists_them_and_a_bad_layer_command_says_how_it_is_typed() {
        let catalog = ComponentCatalog::builtin();
        let mut doc = Schematic::new("n");
        let none = BTreeSet::new();
        let listing = |doc: &Schematic| run("layer", &context(doc, &catalog, &none)).unwrap();
        assert!(
            matches!(&listing(&doc)[..], [Request::Message(text)] if text.contains("no layers"))
        );

        doc.layers.push(circuit_model::Layer {
            id: circuit_model::LayerId(5),
            name: "Control".to_owned(),
            visible: false,
            locked: true,
        });
        assert!(matches!(&listing(&doc)[..],
            [Request::Message(text)] if text == "Control (hidden) (locked)"));

        let run_line = |line: &str| run(line, &context(&doc, &catalog, &none));
        assert!(matches!(
            run_line("layer hide"),
            Err(CommandLineError::Usage(_))
        ));
        assert!(matches!(
            run_line("layer frob x"),
            Err(CommandLineError::Usage(_))
        ));
        assert!(matches!(
            run_line("layer hide Nope"),
            Err(CommandLineError::NoSuchLayer(_))
        ));
        assert!(matches!(
            run_line("layer assign Control"),
            Err(CommandLineError::NothingSelected)
        ));
        assert!(matches!(
            run_line("layer release"),
            Err(CommandLineError::NothingSelected)
        ));
        assert!(matches!(
            run_line("layer new Control"),
            Err(CommandLineError::Refused(_))
        ));
    }
}
