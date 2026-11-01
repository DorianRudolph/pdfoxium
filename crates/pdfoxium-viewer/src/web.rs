// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Browser UI boundary. PDF bytes and worker queues live in shared WASM memory;
//! all DOM/graphics work remains on the browser's UI thread.
use std::{cell::RefCell, sync::Arc};
use wasm_bindgen::{prelude::*, JsCast};

thread_local! {
    static RUNNER: RefCell<Option<eframe::WebRunner>> = const { RefCell::new(None) };
    static CONTEXT: RefCell<Option<eframe::egui::Context>> = const { RefCell::new(None) };
    static FILES: RefCell<Vec<Upload>> = const { RefCell::new(Vec::new()) };
}

#[wasm_bindgen]
pub async fn start(
    canvas: web_sys::HtmlCanvasElement,
    shim_source: JsValue,
) -> Result<(), JsValue> {
    wasm_bindgen_spawn::init_bg_web(shim_source, wasm_bindgen::module())
        .create_dispatcher()
        .await?;
    let runner = eframe::WebRunner::new();
    // eframe's hook owns UI-only state. Worker failures must be logged without
    // touching that state or disabling the browser UI that reports the error.
    let ui_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if web_sys::window().is_some() {
            ui_hook(info);
        } else {
            web_sys::console::error_1(&info.to_string().into());
        }
    }));
    runner
        .start(
            canvas,
            eframe::WebOptions::default(),
            Box::new(|cc| {
                CONTEXT.with_borrow_mut(|ctx| *ctx = Some(cc.egui_ctx.clone()));
                let app = crate::app::App::new(&cc.egui_ctx, None, None)
                    .map_err(std::io::Error::other)?;
                Ok(Box::new(app))
            }),
        )
        .await?;
    RUNNER.with_borrow_mut(|r| *r = Some(runner));
    Ok(())
}

/// Called by the main event loop; worker callbacks only set an atomic flag.
#[wasm_bindgen]
pub fn repaint() {
    if crate::platform::needs_repaint() {
        CONTEXT.with_borrow(|ctx| {
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        });
    }
}

pub enum UploadPurpose {
    Open,
    Import,
    Image,
}
pub struct Upload {
    pub purpose: UploadPurpose,
    pub name: String,
    pub bytes: Arc<[u8]>,
}
#[wasm_bindgen]
pub fn upload_file(purpose: &str, name: String, bytes: Vec<u8>) -> Result<(), JsValue> {
    let purpose = match purpose {
        "open" => UploadPurpose::Open,
        "page.import" => UploadPurpose::Import,
        "page.image" => UploadPurpose::Image,
        _ => return Err(JsValue::from_str("unknown file operation")),
    };
    FILES.with_borrow_mut(|files| {
        files.clear();
        files.push(Upload {
            purpose,
            name,
            bytes: bytes.into(),
        });
    });
    CONTEXT.with_borrow(|ctx| {
        if let Some(ctx) = ctx {
            ctx.request_repaint();
        }
    });
    Ok(())
}
pub fn take_file() -> Option<Upload> {
    FILES.with_borrow_mut(Vec::pop)
}
pub fn pick_file(purpose: &str) {
    let document = web_sys::window().unwrap().document().unwrap();
    let input: web_sys::HtmlInputElement = document
        .get_element_by_id("open-pdf")
        .unwrap()
        .dyn_into()
        .unwrap();
    input.set_value("");
    input.set_accept(if purpose == "page.image" {
        ".png,image/png"
    } else {
        ".pdf,application/pdf"
    });
    input.set_attribute("data-purpose", purpose).unwrap();
    input.click();
}
pub fn download(name: &str, bytes: &[u8]) -> Result<(), String> {
    let document = web_sys::window()
        .ok_or("browser window unavailable")?
        .document()
        .ok_or("browser document unavailable")?;
    let data = js_sys::Uint8Array::from(bytes);
    let parts = js_sys::Array::new();
    parts.push(&data);
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("application/pdf");
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)
        .map_err(|e| format!("cannot create PDF download: {e:?}"))?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(|e| format!("{e:?}"))?;
    let result = (|| -> Result<(), JsValue> {
        let link: web_sys::HtmlAnchorElement = document.create_element("a")?.dyn_into()?;
        link.set_href(&url);
        link.set_download(name);
        link.click();
        Ok(())
    })();
    // Delay revocation until the browser has consumed the click.
    let release = Closure::once_into_js(move || {
        let _ = web_sys::Url::revoke_object_url(&url);
    });
    let _ = web_sys::window()
        .unwrap()
        .set_timeout_with_callback_and_timeout_and_arguments_0(release.unchecked_ref(), 1000);
    result.map_err(|e| format!("cannot download PDF: {e:?}"))
}

#[cfg(feature = "web-test")]
#[path = "web_testing.rs"]
pub mod testing;
