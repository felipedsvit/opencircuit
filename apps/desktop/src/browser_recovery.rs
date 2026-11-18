//! Autosave in the browser (M9.1, issue #118): the copy of a project with
//! unsaved changes kept in browser storage, and offered when the page is
//! opened again after it was closed or crashed without saving.
//!
//! The logic is the same on every target and works on a [`Slot`], one place
//! that holds one text: `localStorage` in the browser (the `web` module of the browser build), and a
//! [`MemorySlot`] in tests. A page holds one copy: the project shown.

use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use serde::{Deserialize, Serialize};

use crate::session::Session;

/// Seconds between two copies, as on the desktop.
pub const DEFAULT_INTERVAL_SECONDS: f64 = 30.0;

/// Largest copy read back, in bytes: the limit of a project file.
const MAX_COPY_BYTES: usize = circuit_project::MAX_PROJECT_BYTES;

/// One place that holds one text.
pub trait Slot: fmt::Debug {
    /// The text held, if any.
    fn get(&self) -> Option<String>;
    /// Hold `text` instead.
    ///
    /// # Errors
    ///
    /// Why the storage refused it (it is full, or turned off).
    fn set(&self, text: &str) -> Result<(), String>;
    /// Hold nothing.
    fn remove(&self);
}

/// A slot in memory, shared by its clones: for tests, and for a page whose
/// browser storage is turned off.
#[derive(Clone, Debug, Default)]
pub struct MemorySlot(Rc<RefCell<Option<String>>>);

impl Slot for MemorySlot {
    fn get(&self) -> Option<String> {
        self.0.borrow().clone()
    }

    fn set(&self, text: &str) -> Result<(), String> {
        *self.0.borrow_mut() = Some(text.to_owned());
        Ok(())
    }

    fn remove(&self) {
        *self.0.borrow_mut() = None;
    }
}

/// What the slot holds.
#[derive(Debug, Deserialize, Serialize)]
struct Copy {
    format: u32,
    /// The name of the project.
    name: String,
    /// The file it came from, as the page names it.
    origin: Option<String>,
    /// The project text.
    text: String,
}

/// A copy found in the slot when the page started.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Found {
    /// The name of the project.
    pub name: String,
    /// The file it came from, as the page names it.
    pub origin: Option<String>,
    /// The project text.
    pub text: String,
}

/// The autosave of a page.
#[derive(Debug)]
pub struct SlotAutosave {
    slot: Box<dyn Slot>,
    interval: f64,
    last_write: f64,
    /// The revision of the copy held, if one is.
    written: Option<u64>,
    failure: Option<String>,
}

impl SlotAutosave {
    /// The autosave into `slot`, writing at most once per `interval`
    /// seconds of the window's clock.
    #[must_use]
    pub fn new(slot: Box<dyn Slot>, interval: f64) -> Self {
        Self {
            slot,
            interval: interval.max(0.0),
            last_write: f64::NEG_INFINITY,
            written: None,
            failure: None,
        }
    }

    /// The copy the slot holds from an earlier page; `None` when it holds
    /// nothing readable.
    #[must_use]
    pub fn found(&self) -> Option<Found> {
        let text = self.slot.get()?;
        if text.len() > MAX_COPY_BYTES {
            return None;
        }
        let copy: Copy = serde_json::from_str(&text).ok()?;
        (copy.format == 1).then_some(Found {
            name: copy.name,
            origin: copy.origin,
            text: copy.text,
        })
    }

    /// Called once per frame: hold a copy of a project with unsaved changes
    /// when the interval has passed, and hold nothing for a project without.
    pub fn tick(&mut self, now: f64, session: Option<&Session>) {
        let Some(session) = session.filter(|session| session.is_dirty()) else {
            self.clear();
            return;
        };
        let revision = session.history().revision();
        if self.written == Some(revision) || now - self.last_write < self.interval {
            return;
        }
        self.last_write = now;
        let copy = session
            .text()
            .map_err(|error| error.to_string())
            .and_then(|text| {
                serde_json::to_string(&Copy {
                    format: 1,
                    name: session.document().project.name.clone(),
                    origin: session.path().map(|path| path.display().to_string()),
                    text,
                })
                .map_err(|error| error.to_string())
            });
        self.failure = match copy.and_then(|copy| self.slot.set(&copy)) {
            Ok(()) => {
                self.written = Some(revision);
                None
            }
            Err(error) => Some(error),
        };
    }

    /// Hold nothing: the project was saved, replaced or closed on purpose.
    pub fn clear(&mut self) {
        if self.written.take().is_some() {
            self.slot.remove();
        }
        self.failure = None;
    }

    /// Hold nothing, whatever a page held before: the copy found was
    /// recovered or discarded.
    pub fn discard_found(&mut self) {
        self.slot.remove();
        self.written = None;
    }

    /// Whether the copy held is the project as it is now.
    #[must_use]
    pub fn is_current(&self, session: Option<&Session>) -> bool {
        session.is_some_and(|session| self.written == Some(session.history().revision()))
    }

    /// Why the last copy could not be held, until one is.
    #[must_use]
    pub fn failure(&self) -> Option<&str> {
        self.failure.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edited() -> Session {
        let mut session = Session::untitled();
        let tx = circuit_commands::ops::add_text(
            session.document(),
            circuit_model::GridPoint::new(0, 0),
            "x",
            circuit_model::TextAnchor::Left,
        )
        .unwrap();
        session.edit(tx).unwrap();
        session
    }

    #[test]
    fn a_dirty_project_is_held_at_the_interval_and_found_again() {
        let slot = MemorySlot::default();
        let mut autosave = SlotAutosave::new(Box::new(slot.clone()), 30.0);
        let session = edited();
        autosave.tick(0.0, Some(&session));
        assert!(autosave.is_current(Some(&session)));
        let found = SlotAutosave::new(Box::new(slot.clone()), 30.0)
            .found()
            .expect("a copy");
        assert_eq!(found.text, session.text().unwrap());
        assert_eq!(found.name, session.document().project.name);

        // A project with no change holds nothing.
        autosave.tick(40.0, Some(&Session::untitled()));
        assert_eq!(slot.get(), None);
    }

    #[test]
    fn damaged_or_foreign_text_is_not_a_copy() {
        let slot = MemorySlot::default();
        let autosave = SlotAutosave::new(Box::new(slot.clone()), 30.0);
        slot.set("not json").unwrap();
        assert_eq!(autosave.found(), None);
        slot.set(r#"{"format": 2, "name": "x", "origin": null, "text": ""}"#)
            .unwrap();
        assert_eq!(autosave.found(), None);
    }

    #[test]
    fn a_full_storage_is_a_failure_until_a_copy_is_held() {
        #[derive(Debug)]
        struct Full;
        impl Slot for Full {
            fn get(&self) -> Option<String> {
                None
            }
            fn set(&self, _: &str) -> Result<(), String> {
                Err("the storage is full".to_owned())
            }
            fn remove(&self) {}
        }
        let mut autosave = SlotAutosave::new(Box::new(Full), 0.0);
        autosave.tick(0.0, Some(&edited()));
        assert_eq!(autosave.failure(), Some("the storage is full"));
    }
}
