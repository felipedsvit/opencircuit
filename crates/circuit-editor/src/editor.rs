//! The editor: tools, selection and gestures, driven by grid points.
//!
//! The editor never changes the document. Pointer and key events go in, and
//! an [`Outcome`] comes out with at most one [`Transaction`] to apply through
//! a `History`. After any change to the document (a commit, an undo, a redo)
//! the caller passes the new document to [`Editor::retain_existing`], so the
//! selection never names something that is gone.

use std::collections::BTreeSet;

use circuit_commands::{Anchor, CommandError, Transaction, ops};
use circuit_model::{
    ComponentCatalog, GridPoint, GridRect, LayoutIndex, ModelRef, QuarterTurn, Schematic, Subject,
    SymbolId, SymbolKey, orthogonal_route,
};

use crate::hit::{Hit, hit_test};

/// What a click does.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Tool {
    /// Select, drag to move, drag on empty space to select a box.
    Select,
    /// Place new devices of a model, one per click.
    Place {
        /// The model.
        model: ModelRef,
        /// The symbol drawn for the new device.
        symbol: SymbolKey,
        /// The rotation of the next placement.
        rotation: QuarterTurn,
    },
    /// Draw wires: click where the wire starts (a pin, a junction or a
    /// wire), click on empty space for each bend, click where it ends.
    Wire,
}

/// Keyboard state that changes what a click does.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Modifiers {
    /// Add to or remove from the selection instead of replacing it.
    pub shift: bool,
}

/// Keys the editor understands.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Key {
    /// Delete the selection.
    Delete,
    /// Turn the selected symbols, or the next placement, a quarter turn.
    Rotate,
    /// Cancel the gesture in progress, then leave the tool, then clear the
    /// selection.
    Escape,
}

/// The result of an event.
#[must_use]
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Outcome {
    /// The change to apply to the document, if any.
    pub commit: Option<Transaction>,
    /// Why an action was refused, to show to the user.
    pub error: Option<CommandError>,
}

impl Outcome {
    fn from_result(result: Result<Transaction, CommandError>) -> Self {
        match result {
            Ok(tx) if tx.is_empty() => Self::default(),
            Ok(tx) => Self {
                commit: Some(tx),
                error: None,
            },
            Err(error) => Self {
                commit: None,
                error: Some(error),
            },
        }
    }
}

/// What to draw on top of the document while a gesture is in progress.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Preview {
    /// The document as it would be: the selection being dragged, or the
    /// device about to be placed. `None` to draw the document itself.
    pub document: Option<Schematic>,
    /// The path of the wire being drawn, from its start to the pointer.
    pub guide: Vec<GridPoint>,
    /// The rectangle of a box selection.
    pub selection_box: Option<GridRect>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Gesture {
    Idle,
    /// The button went down on a selected element; a move turns it into a
    /// drag.
    Pressed {
        at: GridPoint,
    },
    Dragging {
        from: GridPoint,
        to: GridPoint,
    },
    BoxSelect {
        from: GridPoint,
        to: GridPoint,
        additive: bool,
    },
    Wiring {
        start: Anchor,
        start_at: GridPoint,
        waypoints: Vec<GridPoint>,
        cursor: GridPoint,
    },
}

/// The state of the editor of one document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Editor {
    tool: Tool,
    selection: BTreeSet<Subject>,
    gesture: Gesture,
    cursor: Option<GridPoint>,
    hover: Option<Hit>,
}

impl Default for Editor {
    fn default() -> Self {
        Self {
            tool: Tool::Select,
            selection: BTreeSet::new(),
            gesture: Gesture::Idle,
            cursor: None,
            hover: None,
        }
    }
}

fn difference(a: GridPoint, b: GridPoint) -> Option<GridPoint> {
    Some(GridPoint::new(a.x.checked_sub(b.x)?, a.y.checked_sub(b.y)?))
}

/// The last point of a wire being drawn.
fn last_point(start_at: GridPoint, waypoints: &[GridPoint]) -> GridPoint {
    waypoints.last().copied().unwrap_or(start_at)
}

impl Editor {
    /// An editor with the select tool and nothing selected.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The current tool.
    #[must_use]
    pub const fn tool(&self) -> &Tool {
        &self.tool
    }

    /// Change the tool, cancelling the gesture in progress.
    pub fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
        self.gesture = Gesture::Idle;
    }

    /// The selected elements.
    #[must_use]
    pub const fn selection(&self) -> &BTreeSet<Subject> {
        &self.selection
    }

    /// Replace the selection.
    pub fn set_selection(&mut self, selection: BTreeSet<Subject>) {
        self.selection = selection;
    }

    /// The element under the pointer, as of the last pointer event.
    #[must_use]
    pub const fn hover(&self) -> Option<&Hit> {
        self.hover.as_ref()
    }

    /// Whether a gesture (drag, box selection, wire) is in progress.
    #[must_use]
    pub fn is_busy(&self) -> bool {
        self.gesture != Gesture::Idle
    }

    /// Forget selected elements the document no longer has, and cancel a
    /// gesture that started from one. Call after every change to the
    /// document.
    pub fn retain_existing(&mut self, doc: &Schematic) {
        let exists = |subject: &Subject| match subject {
            Subject::Device(id) => doc.devices.iter().any(|d| d.id == *id),
            Subject::Symbol(id) => doc.symbols.iter().any(|s| s.id == *id),
            Subject::Junction(id) => doc.junctions.iter().any(|j| j.id == *id),
            Subject::Wire(id) => doc.wires.iter().any(|w| w.id == *id),
            Subject::Supply(id) => doc.supplies.iter().any(|s| s.id == *id),
            Subject::Pin(pin) => doc.symbols.iter().any(|s| s.id == pin.symbol_id),
            Subject::Document => true,
        };
        let selected = self.selection.len();
        self.selection.retain(exists);
        let lost_selection = self.selection.len() != selected;
        if let Gesture::Wiring { start, .. } = &self.gesture {
            let still_there = match start {
                Anchor::Endpoint(endpoint) => exists(&match endpoint {
                    circuit_model::Endpoint::Terminal(pin) => Subject::Symbol(pin.symbol_id),
                    circuit_model::Endpoint::Junction(id) => Subject::Junction(*id),
                }),
                Anchor::OnWire { wire, .. } => exists(&Subject::Wire(*wire)),
            };
            if !still_there {
                self.gesture = Gesture::Idle;
            }
        }
        // A drag moves the selection; once part of it is gone, stop.
        if lost_selection
            && matches!(
                self.gesture,
                Gesture::Pressed { .. } | Gesture::Dragging { .. }
            )
        {
            self.gesture = Gesture::Idle;
        }
    }

    /// The pointer moved to `at`.
    pub fn pointer_move(&mut self, doc: &Schematic, catalog: &ComponentCatalog, at: GridPoint) {
        self.cursor = Some(at);
        self.hover = hit_test(doc, catalog, at);
        match &mut self.gesture {
            Gesture::Pressed { at: from } => {
                if *from != at {
                    self.gesture = Gesture::Dragging {
                        from: *from,
                        to: at,
                    };
                }
            }
            Gesture::Dragging { to, .. } | Gesture::BoxSelect { to, .. } => *to = at,
            Gesture::Wiring { cursor, .. } => *cursor = at,
            Gesture::Idle => {}
        }
    }

    /// The primary button went down at `at`.
    pub fn pointer_down(
        &mut self,
        doc: &Schematic,
        catalog: &ComponentCatalog,
        at: GridPoint,
        modifiers: Modifiers,
    ) -> Outcome {
        self.cursor = Some(at);
        let hit = hit_test(doc, catalog, at);
        self.hover = hit.clone();
        match &self.tool {
            Tool::Select => {
                self.select_at(hit, at, modifiers);
                Outcome::default()
            }
            Tool::Place {
                model,
                symbol,
                rotation,
            } => {
                let result = ops::place(doc, catalog, model, symbol, at, *rotation);
                if let Ok(tx) = &result {
                    self.selection = tx.inserted().into_iter().collect();
                }
                Outcome::from_result(result)
            }
            Tool::Wire => self.wire_at(doc, catalog, hit, at),
        }
    }

    fn select_at(&mut self, hit: Option<Hit>, at: GridPoint, modifiers: Modifiers) {
        match (hit, modifiers.shift) {
            (Some(hit), true) => {
                let subject = hit.subject();
                if !self.selection.remove(&subject) {
                    self.selection.insert(subject);
                }
                self.gesture = Gesture::Idle;
            }
            (Some(hit), false) => {
                let subject = hit.subject();
                if !self.selection.contains(&subject) {
                    self.selection = BTreeSet::from([subject]);
                }
                self.gesture = Gesture::Pressed { at };
            }
            (None, additive) => {
                if !additive {
                    self.selection.clear();
                }
                self.gesture = Gesture::BoxSelect {
                    from: at,
                    to: at,
                    additive,
                };
            }
        }
    }

    fn wire_at(
        &mut self,
        doc: &Schematic,
        catalog: &ComponentCatalog,
        hit: Option<Hit>,
        at: GridPoint,
    ) -> Outcome {
        let anchor = hit.as_ref().and_then(Hit::anchor);
        match std::mem::replace(&mut self.gesture, Gesture::Idle) {
            Gesture::Wiring {
                start,
                start_at,
                mut waypoints,
                ..
            } => match anchor {
                Some(end) => {
                    let mut via = waypoints;
                    via.extend(orthogonal_route(last_point(start_at, &via), at));
                    Outcome::from_result(ops::connect(doc, catalog, &start, &end, &via))
                }
                None => {
                    let last = last_point(start_at, &waypoints);
                    if last != at {
                        waypoints.extend(orthogonal_route(last, at));
                        waypoints.push(at);
                    }
                    self.gesture = Gesture::Wiring {
                        start,
                        start_at,
                        waypoints,
                        cursor: at,
                    };
                    Outcome::default()
                }
            },
            Gesture::Idle
            | Gesture::Pressed { .. }
            | Gesture::Dragging { .. }
            | Gesture::BoxSelect { .. } => {
                if let Some(start) = anchor {
                    self.gesture = Gesture::Wiring {
                        start,
                        start_at: at,
                        waypoints: Vec::new(),
                        cursor: at,
                    };
                }
                Outcome::default()
            }
        }
    }

    /// The primary button was released at `at`.
    pub fn pointer_up(
        &mut self,
        doc: &Schematic,
        catalog: &ComponentCatalog,
        at: GridPoint,
    ) -> Outcome {
        self.cursor = Some(at);
        match std::mem::replace(&mut self.gesture, Gesture::Idle) {
            Gesture::Dragging { from, .. } => match difference(at, from) {
                Some(delta) => {
                    Outcome::from_result(ops::move_by(doc, catalog, &self.selection, delta))
                }
                None => Outcome {
                    commit: None,
                    error: Some(CommandError::OutOfRange),
                },
            },
            Gesture::BoxSelect { from, additive, .. } => {
                let inside = Self::inside(doc, catalog, GridRect::from_corners(from, at));
                if additive {
                    self.selection.extend(inside);
                } else {
                    self.selection = inside;
                }
                Outcome::default()
            }
            wiring @ Gesture::Wiring { .. } => {
                // A wire is drawn with clicks; releasing the button between
                // them changes nothing.
                self.gesture = wiring;
                Outcome::default()
            }
            Gesture::Pressed { .. } | Gesture::Idle => Outcome::default(),
        }
    }

    /// The symbols, junctions and wires entirely inside a rectangle.
    fn inside(doc: &Schematic, catalog: &ComponentCatalog, area: GridRect) -> BTreeSet<Subject> {
        let layout = LayoutIndex::new(doc, catalog);
        let mut found = BTreeSet::new();
        for symbol in &doc.symbols {
            let within = layout
                .symbol_bounds(symbol.id)
                .is_some_and(|b| area.contains(b.min) && area.contains(b.max));
            if within {
                found.insert(Subject::Symbol(symbol.id));
            }
        }
        for junction in &doc.junctions {
            if area.contains(junction.position) {
                found.insert(Subject::Junction(junction.id));
            }
        }
        for wire in &doc.wires {
            let within = layout
                .wire_polyline(wire)
                .is_some_and(|path| path.iter().all(|p| area.contains(*p)));
            if within {
                found.insert(Subject::Wire(wire.id));
            }
        }
        found
    }

    /// A key was pressed.
    pub fn key(&mut self, doc: &Schematic, catalog: &ComponentCatalog, key: Key) -> Outcome {
        match key {
            Key::Escape => {
                if self.is_busy() {
                    self.gesture = Gesture::Idle;
                } else if self.tool != Tool::Select {
                    self.tool = Tool::Select;
                } else {
                    self.selection.clear();
                }
                Outcome::default()
            }
            Key::Rotate => {
                if let Tool::Place { rotation, .. } = &mut self.tool {
                    *rotation = rotation.next();
                    return Outcome::default();
                }
                let symbols: BTreeSet<SymbolId> = self
                    .selection
                    .iter()
                    .filter_map(|subject| match subject {
                        Subject::Symbol(id) => Some(*id),
                        _ => None,
                    })
                    .collect();
                Outcome::from_result(ops::rotate(doc, catalog, &symbols))
            }
            Key::Delete => {
                if self.selection.is_empty() || self.is_busy() {
                    return Outcome::default();
                }
                Outcome::from_result(ops::delete(doc, &self.selection))
            }
        }
    }

    /// What to draw for the gesture in progress.
    #[must_use]
    pub fn preview(&self, doc: &Schematic, catalog: &ComponentCatalog) -> Preview {
        let applied = |result: Result<Transaction, CommandError>| {
            result
                .and_then(|tx| tx.apply_to(doc))
                .ok()
                .map(|(edited, _)| edited)
        };
        match (&self.gesture, &self.tool) {
            (Gesture::Dragging { from, to }, _) => Preview {
                document: difference(*to, *from)
                    .and_then(|delta| applied(ops::move_by(doc, catalog, &self.selection, delta))),
                ..Preview::default()
            },
            (Gesture::BoxSelect { from, to, .. }, _) => Preview {
                selection_box: Some(GridRect::from_corners(*from, *to)),
                ..Preview::default()
            },
            (
                Gesture::Wiring {
                    start_at,
                    waypoints,
                    cursor,
                    ..
                },
                _,
            ) => {
                let mut guide = vec![*start_at];
                guide.extend(waypoints.iter().copied());
                guide.extend(orthogonal_route(last_point(*start_at, waypoints), *cursor));
                guide.push(*cursor);
                Preview {
                    guide,
                    ..Preview::default()
                }
            }
            (
                Gesture::Idle,
                Tool::Place {
                    model,
                    symbol,
                    rotation,
                },
            ) => Preview {
                document: self
                    .cursor
                    .and_then(|at| applied(ops::place(doc, catalog, model, symbol, at, *rotation))),
                ..Preview::default()
            },
            (Gesture::Idle | Gesture::Pressed { .. }, _) => Preview::default(),
        }
    }
}
