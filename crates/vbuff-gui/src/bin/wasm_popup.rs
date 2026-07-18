#[cfg(target_arch = "wasm32")]
use std::sync::{Arc, Mutex};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::JsCast as _;
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(start)]
pub async fn start() -> Result<(), JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("window unavailable"))?;
    let document = window
        .document()
        .ok_or_else(|| JsValue::from_str("document unavailable"))?;
    let canvas = document
        .get_element_by_id("vbuff-demo")
        .ok_or_else(|| JsValue::from_str("demo canvas unavailable"))?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;
    let state = Arc::new(Mutex::new(vbuff_gui::demo_state()));
    eframe::WebRunner::new()
        .start(
            canvas,
            eframe::WebOptions::default(),
            Box::new(move |_| Ok(Box::new(vbuff_gui::PopupApp::new(state)))),
        )
        .await
}

fn main() {}
