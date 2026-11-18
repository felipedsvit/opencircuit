//! The browser side of the application (`wasm32`): start on the page's
//! canvas, read dropped files asynchronously and hand files to the user as
//! downloads. Everything else is the same application as on the desktop.

use std::cell::RefCell;
use std::rc::Rc;

use eframe::egui::DroppedFileHandle;
use eframe::wasm_bindgen::{JsCast as _, JsValue};

use crate::DesktopApp;

/// Identifier of the canvas element in `index.html`.
const CANVAS_ID: &str = "opencircuit_canvas";

/// Files read from the browser, waiting for the next frame: name and bytes.
pub type Inbox = Rc<RefCell<Vec<(String, Vec<u8>)>>>;

/// Start the application on the canvas of the page.
pub fn start() {
    wasm_bindgen_futures::spawn_local(async {
        let Some(canvas) = web_sys::window()
            .and_then(|window| window.document())
            .and_then(|document| document.get_element_by_id(CANVAS_ID))
            .and_then(|element| element.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        else {
            return;
        };
        let _ = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|_creation| {
                    let mut app = DesktopApp::default();
                    // Autosave into the browser storage of the page (M9.1).
                    app.enable_slot_autosave(
                        Box::new(LocalStorage),
                        crate::browser_recovery::DEFAULT_INTERVAL_SECONDS,
                    );
                    Ok(Box::new(app))
                }),
            )
            .await;
    });
}

/// The browser storage of the page (`localStorage`), under one key: the
/// slot of the autosave. A page whose storage is turned off holds nothing.
#[derive(Debug)]
pub struct LocalStorage;

/// The key of the copy in the storage of the page.
const COPY_KEY: &str = "opencircuit.recovery";

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

impl crate::browser_recovery::Slot for LocalStorage {
    fn get(&self) -> Option<String> {
        storage()?.get_item(COPY_KEY).ok()?
    }

    fn set(&self, text: &str) -> Result<(), String> {
        storage()
            .ok_or_else(|| "the browser storage is turned off".to_owned())?
            .set_item(COPY_KEY, text)
            .map_err(|value| format!("{value:?}"))
    }

    fn remove(&self) {
        if let Some(storage) = storage() {
            // Best effort: a copy left behind is offered once and discarded.
            let _ = storage.remove_item(COPY_KEY);
        }
    }
}

/// Read a dropped file in the background and put it in `inbox`.
pub fn read_dropped(file: DroppedFileHandle, inbox: Inbox) {
    let name = file.path().to_string_lossy().into_owned();
    wasm_bindgen_futures::spawn_local(async move {
        if let Ok(bytes) = file.bytes_async().await {
            inbox.borrow_mut().push((name, bytes));
        }
    });
}

/// Offer `bytes` to the user as a download named `name`.
///
/// # Errors
///
/// Returns a message when the browser refuses one of the steps.
pub fn download(name: &str, bytes: &[u8], mime: &str) -> Result<(), String> {
    let describe = |value: JsValue| format!("{value:?}");
    let parts = js_sys::Array::new();
    parts.push(&js_sys::Uint8Array::from(bytes).into());
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime);
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)
        .map_err(describe)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(describe)?;
    let document = web_sys::window()
        .and_then(|window| window.document())
        .ok_or("the page has no document")?;
    let anchor = document
        .create_element("a")
        .map_err(describe)?
        .dyn_into::<web_sys::HtmlAnchorElement>()
        .map_err(|_| "the page cannot create a link".to_owned())?;
    anchor.set_href(&url);
    anchor.set_download(name);
    anchor.click();
    web_sys::Url::revoke_object_url(&url).map_err(describe)
}
