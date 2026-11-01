// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use crate::{
    control::{self, Reply, Request},
    view::{self, Loaded, View},
};
use eframe::egui::{self, Color32, Rect, Vec2};
use serde_json::{json, Value};
use std::{collections::VecDeque, path::PathBuf, sync::mpsc};
use web_time::{Duration, Instant};

const ACTIVITY_DELAY: Duration = Duration::from_millis(150);
fn activity_visible(since: &mut Option<Instant>, working: bool, now: Instant) -> bool {
    if working {
        now.duration_since(*since.get_or_insert(now)) >= ACTIVITY_DELAY
    } else {
        *since = None;
        false
    }
}

const COMMANDS: &[(&str, &str)] = &[
    ("layers.set", "Set layer visibility (index, visible)"),
    ("layers.configuration", "Apply layer configuration (index)"),
    ("layers.reset", "Reset default layer state"),
    ("file.open", "Open PDF (path)"),
    ("demo.open", "Show sample launcher"),
    (
        "demo.choose",
        "Open embedded sample (sample: layers/editing/acroform/xfa/dynamic-xfa/animation)",
    ),
    (
        "document.discard",
        "Discard changes and complete pending open/close",
    ),
    ("document.cancel", "Cancel pending open/close"),
    ("document.save", "Save copy before pending open/close"),
    ("page.next", "Next page"),
    ("page.previous", "Previous page"),
    ("navigation.back", "Back to previous location"),
    ("navigation.forward", "Forward to next location"),
    (
        "navigation.outline",
        "Follow outline item (index, zero-based)",
    ),
    (
        "navigation.link",
        "Follow page link (page, one-based; index, annotation index)",
    ),
    ("edit.open", "Open page and image editing tools"),
    ("page.rotate", "Rotate the actual current page clockwise"),
    (
        "page.delete",
        "Delete current page, retaining at least one page",
    ),
    (
        "page.import",
        "Import PDF pages after current page (path, optional one-based pages)",
    ),
    (
        "page.image",
        "Insert PNG content (path, normalized x/y/width/height)",
    ),
    ("view.annotations", "Toggle annotations panel"),
    (
        "annotation.markup",
        "Annotate selected text (kind: highlight/underline/strikeout)",
    ),
    (
        "annotation.place_note",
        "Place a note with the next page click (text)",
    ),
    (
        "annotation.text",
        "Draw a free-text box with the next page drag",
    ),
    (
        "annotation.rectangle",
        "Draw a rectangle with the next page drag",
    ),
    (
        "annotation.ellipse",
        "Draw an ellipse with the next page drag",
    ),
    ("annotation.note", "Add note (text, normalized x/y)"),
    (
        "annotation.style",
        "Style annotation (index; font_size/color, border_color/border_width/fill, color/opacity)",
    ),
    (
        "annotation.defaults",
        "Configure new text/shape/highlight style (kind and style settings)",
    ),
    (
        "annotation.comment",
        "Edit current-page annotation comment (index, text)",
    ),
    (
        "annotation.delete",
        "Delete current-page annotation (index)",
    ),
    ("view.outline", "Toggle outline sidebar"),
    ("view.thumbnails", "Toggle thumbnail sidebar"),
    (
        "thumbnails.page",
        "Scroll thumbnails to page (page, one-based)",
    ),
    ("form.save", "Save copy (path, or show save UI)"),
    (
        "form.dialog.respond",
        "Answer a script dialog (dialog_id, response: ok/cancel/yes/no/text, text)",
    ),
    ("form.blur", "Commit and leave the focused form field"),
    ("view.label", "Go to printed page label (label)"),
    ("view.page", "Go to page (page, one-based)"),
    ("view.zoom", "Set zoom (zoom, 1.0 = 100%)"),
    ("view.zoom_in", "Zoom in"),
    ("view.zoom_out", "Zoom out"),
    ("view.fit", "Fit width"),
    ("view.rotate", "Rotate view clockwise"),
    ("search.open", "Open Find"),
    ("search.find", "Search document (query, match_case?)"),
    ("search.next", "Next match"),
    ("search.previous", "Previous match"),
    ("search.clear", "Close Find and clear highlights"),
    (
        "selection.set",
        "Select UTF-8 byte range (page, start, end)",
    ),
    ("selection.clear", "Clear selection"),
    ("selection.copy", "Copy selected text"),
];
#[derive(Clone)]
struct Widget {
    id: &'static str,
    label: &'static str,
    rect: Rect,
    enabled: bool,
}
struct Waiting {
    reply: Reply,
    since: Instant,
    revision: Option<u64>,
}
struct Capture {
    waiting: Waiting,
    path: PathBuf,
    issued: Option<u64>,
    settled: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Departure {
    Open(PathBuf),
    #[cfg(target_arch = "wasm32")]
    Bytes(PathBuf, std::sync::Arc<[u8]>),
    Demo(usize),
    Close,
}
pub struct App {
    demo_open: bool,
    server: Option<control::Server>,
    view: Option<View>,
    editing: Option<mpsc::Receiver<Result<(Loaded, usize), String>>>,
    edit_open: bool,
    #[cfg(not(target_arch = "wasm32"))]
    edit_pdf_path: String,
    #[cfg(not(target_arch = "wasm32"))]
    edit_image_path: String,
    loading: Option<mpsc::Receiver<Result<Loaded, String>>>,
    page_input: String,
    error: Option<String>,
    widgets: Vec<Widget>,
    layer_widgets: Vec<Value>,
    control_only_input: bool,
    events: Vec<egui::Event>,
    batches: VecDeque<Vec<egui::Event>>,
    outline_rows: std::collections::HashMap<usize, Rect>,
    departure: Option<Departure>,
    departure_confirming: bool,
    close_approved: bool,
    working_since: Option<Instant>,
    form_save_open: bool,
    form_save_path: String,
    thumbnails_open: bool,
    outline_open: bool,
    outline_collapsed: std::collections::HashSet<usize>,
    find_open: bool,
    find_query: String,
    find_case: bool,
    focus_find: bool,
    waits: Vec<Waiting>,
    capture: Option<Capture>,
    screenshot_tx: mpsc::Sender<(u64, std::sync::Arc<egui::ColorImage>)>,
    screenshot_rx: mpsc::Receiver<(u64, std::sync::Arc<egui::ColorImage>)>,
    #[cfg(target_arch = "wasm32")]
    dropped: Option<mpsc::Receiver<Result<(PathBuf, Vec<u8>), String>>>,
    frame: u64,
    revision: u64,
    settle_frame: u64,
    last_view: Value,
    window: Vec2,
}
impl App {
    pub fn new(
        ctx: &egui::Context,
        path: Option<PathBuf>,
        control: Option<PathBuf>,
    ) -> Result<Self, String> {
        crate::chrome::configure(ctx);
        // Viewer zoom changes the document, never the toolbar or window scale.
        ctx.options_mut(|o| o.zoom_with_keyboard = false);
        #[cfg(not(target_arch = "wasm32"))]
        let server = control
            .map(|path| control::Server::start(&path, ctx.clone()))
            .transpose()
            .map_err(|e| e.to_string())?;
        #[cfg(target_arch = "wasm32")]
        let server = {
            debug_assert!(control.is_none());
            None
        };
        let (screenshot_tx, screenshot_rx) = mpsc::channel();
        let mut app = Self {
            demo_open: path.is_none(),
            server,
            view: None,
            loading: None,
            editing: None,
            edit_open: false,
            #[cfg(not(target_arch = "wasm32"))]
            edit_pdf_path: String::new(),
            #[cfg(not(target_arch = "wasm32"))]
            edit_image_path: String::new(),
            page_input: "1".into(),
            error: None,
            widgets: Vec::new(),
            layer_widgets: Vec::new(),
            control_only_input: false,
            events: Vec::new(),
            batches: VecDeque::new(),
            outline_rows: Default::default(),
            departure: None,
            departure_confirming: false,
            close_approved: false,
            working_since: None,
            form_save_open: false,
            form_save_path: String::new(),
            thumbnails_open: false,
            outline_open: false,
            outline_collapsed: Default::default(),
            find_open: false,
            find_query: String::new(),
            find_case: false,
            focus_find: false,
            waits: Vec::new(),
            capture: None,
            screenshot_tx,
            screenshot_rx,
            #[cfg(target_arch = "wasm32")]
            dropped: None,
            frame: 0,
            revision: 0,
            settle_frame: 3,
            last_view: Value::Null,
            window: Vec2::ZERO,
        };
        if let Some(path) = path {
            app.open(path, ctx);
        }
        Ok(app)
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn set_control_only_input(&mut self, enabled: bool) {
        self.control_only_input = enabled;
    }
    fn changed(&mut self) {
        self.revision += 1;
        self.settle_frame = self.frame + 3;
    }
    fn request_departure(&mut self, action: Departure) {
        if self.departure.as_ref() != Some(&action) {
            self.departure_confirming = false;
        }
        self.departure = Some(action);
    }
    fn cancel_departure(&mut self) {
        self.departure = None;
        self.departure_confirming = false;
    }
    fn open(&mut self, path: PathBuf, ctx: &egui::Context) {
        self.demo_open = false;
        self.request_departure(Departure::Open(path));
        ctx.request_repaint();
    }
    fn depart(&mut self, ctx: &egui::Context) {
        let Some(action) = self.departure.take() else {
            return;
        };
        self.departure_confirming = false;
        self.form_save_open = false;
        match action {
            Departure::Open(path) => self.open_now(path, ctx),
            #[cfg(target_arch = "wasm32")]
            Departure::Bytes(path, bytes) => {
                self.loading = Some(view::load_bytes(path, bytes, ctx.clone()));
                self.error = None;
                self.changed();
            }
            Departure::Demo(index) => {
                let demo = &crate::demos::DEMOS[index];
                self.loading = Some(view::load_bytes(
                    demo.filename.into(),
                    demo.bytes.into(),
                    ctx.clone(),
                ));
                self.error = None;
                self.outline_collapsed.clear();
                self.changed();
            }
            Departure::Close => {
                self.close_approved = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }
    fn intercept_close(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.close_approved {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.request_departure(Departure::Close);
        }
    }
    fn departure_ui(&mut self, ctx: &egui::Context) {
        if self.departure.is_none() {
            return;
        }
        // An unreleased drag is only a preview. Accepted release edits still
        // finish before dirty-departure handling below.
        if let Some(view) = &mut self.view {
            view.annotations.cancel_drag();
        }
        // Run after View::show has routed this frame's input to the worker.
        if !self.departure_confirming
            && (self.editing.is_some()
                || !self.events.is_empty()
                || !self.batches.is_empty()
                || self.view.as_ref().is_some_and(|v| {
                    v.forms.busy() || v.annotations.busy() || v.preparing_editor()
                }))
        {
            ctx.request_repaint();
            return;
        }
        // Once mounted, keep the modal independent of animation-triggered
        // metadata queries and focus blur. Save/script dialogs still own input.
        if self
            .view
            .as_ref()
            .is_some_and(|v| v.forms.is_saving() || v.forms.dialog.is_some())
        {
            ctx.request_repaint();
            return;
        }
        if self.view.as_ref().is_none_or(|v| !v.forms.dirty()) {
            self.depart(ctx);
            return;
        }
        if self.form_save_open {
            return;
        }
        self.departure_confirming = true;
        let mut choice = None;
        egui::Modal::new(egui::Id::unique("unsaved-forms")).show(ctx, |ui| {
            ui.heading("Unsaved changes");
            ui.label("Save a copy before leaving this document?");
            if let Some(error) = self
                .view
                .as_ref()
                .and_then(|v| v.forms.save_error.as_ref().or(v.forms.error.as_ref()))
            {
                ui.colored_label(Color32::DARK_RED, error);
            }
            ui.horizontal(|ui| {
                if self.button(ui, "document.save", "Save copy", true) {
                    choice = Some(0);
                }
                if self.button(ui, "document.discard", "Discard", true) {
                    choice = Some(1);
                }
                if self.button(ui, "document.cancel", "Cancel", true) {
                    choice = Some(2);
                }
            });
        });
        match choice {
            Some(0) => self.run("form.save", json!({}), ctx),
            Some(1) => self.depart(ctx),
            Some(2) => {
                self.cancel_departure();
                self.changed();
            }
            _ => {}
        }
    }
    fn open_now(&mut self, path: PathBuf, ctx: &egui::Context) {
        self.loading = Some(view::load(path, ctx.clone()));
        self.error = None;
        self.outline_collapsed.clear();
        self.changed();
    }
    fn state(&self) -> Value {
        json!({"demo_open":self.demo_open,"revision":self.revision,"frame":self.frame,"loading":self.loading.is_some() || self.editing.is_some(),"error":self.error,"window":{"width":self.window.x,"height":self.window.y},"active":self.view.as_ref().map(View::state),"departure":self.departure.as_ref().map(|a|match a { #[cfg(target_arch = "wasm32")] Departure::Bytes(path,_)=>json!({"kind":"open","path":path}), Departure::Open(path)=>json!({"kind":"open","path":path}), Departure::Demo(index)=>json!({"kind":"demo","sample":crate::demos::DEMOS[*index].id}), Departure::Close=>json!({"kind":"close"}) }),"busy_indicator":self.loading.is_some() || self.editing.is_some() || self.working_since.is_some_and(|since|since.elapsed()>=ACTIVITY_DELAY),"settled":self.ready()})
    }
    fn ready(&self) -> bool {
        self.frame >= self.settle_frame
            && (self.loading.is_none() && self.editing.is_none())
            && self.events.is_empty()
            && self.batches.is_empty()
            && self
                .view
                .as_ref()
                .is_none_or(|v| v.complete && (!self.thumbnails_open || v.thumbnails.complete))
    }
    fn start_edit(
        &mut self,
        operation: crate::editing::Operation,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        if self.loading.is_some() || self.editing.is_some() {
            return Err("document operation is pending".into());
        }
        let view = self.view.as_mut().ok_or("no document open")?;
        if view.is_xfa() {
            return Err("PDF page/content editing is not available for XFA documents".into());
        }
        if view.forms.busy() || view.annotations.busy() || view.preparing_editor() {
            return Err("wait for document input or edits to finish".into());
        }
        view.annotations.cancel_tools();
        let snapshot = view
            .loaded
            .pool
            .document()
            .map(|doc| doc.call(|doc, _| doc.snapshot()));
        self.editing = Some(crate::editing::start(
            view.loaded.bytes.clone(),
            snapshot,
            view.loaded.path.clone(),
            view.page,
            operation,
            ctx.clone(),
        ));
        self.edit_open = false;
        self.error = None;
        self.changed();
        Ok(())
    }
    fn command(&mut self, id: &str, params: &Value, ctx: &egui::Context) -> Result<Value, String> {
        self.error = None;
        let id = if id == "document.save" {
            "form.save"
        } else {
            id
        };
        if self.editing.is_some()
            && !matches!(id, "file.open" | "document.cancel" | "form.dialog.respond")
        {
            return Err("wait for the page/content edit to finish".into());
        }
        if matches!(id, "demo.open" | "demo.choose") {
            if id == "demo.open" {
                self.demo_open = true;
            } else {
                let index =
                    crate::demos::find(string(params, "sample")?).ok_or("unknown demo sample")?;
                self.request_departure(Departure::Demo(index));
                self.demo_open = false;
            }
            self.changed();
            return Ok(json!({"accepted":true}));
        }
        if matches!(id, "layers.set" | "layers.reset" | "layers.configuration") {
            let view = self.view.as_mut().ok_or("no document open")?;
            if id == "layers.reset" {
                view.loaded.layers.reset();
            } else if id == "layers.configuration" {
                let index = params
                    .get("index")
                    .and_then(Value::as_u64)
                    .ok_or("missing configuration index")? as usize;
                let id = view
                    .loaded
                    .layers
                    .configurations()
                    .get(index)
                    .ok_or("invalid configuration index")?
                    .id;
                view.loaded
                    .layers
                    .apply_configuration(id)
                    .map_err(|e| e.to_string())?;
            } else {
                let index = params
                    .get("index")
                    .and_then(Value::as_u64)
                    .ok_or("missing layer index")? as usize;
                let on = params
                    .get("visible")
                    .and_then(Value::as_bool)
                    .ok_or("missing visibility")?;
                let id = view
                    .loaded
                    .layers
                    .groups()
                    .get(index)
                    .ok_or("invalid layer index")?
                    .id;
                view.loaded
                    .layers
                    .set_visible(id, on)
                    .map_err(|e| e.to_string())?;
            }
            view.layers_changed()?;
            self.changed();
            return Ok(json!({"accepted":true}));
        }
        if id == "edit.open" {
            self.edit_open = !self.edit_open;
            self.changed();
            return Ok(json!({"accepted":true}));
        }
        #[cfg(target_arch = "wasm32")]
        if matches!(id, "page.import" | "page.image") && params.get("path").is_none() {
            crate::web::pick_file(id);
            return Ok(json!({"accepted":true}));
        }
        if matches!(
            id,
            "page.rotate" | "page.delete" | "page.import" | "page.image"
        ) {
            let operation = match id {
                "page.rotate" => crate::editing::Operation::Rotate,
                "page.delete" => crate::editing::Operation::Delete,
                "page.import" => {
                    let pages = params
                        .get("pages")
                        .map(|v| {
                            v.as_array()
                                .ok_or("pages must be an array of one-based indices")
                                .and_then(|list| {
                                    list.iter()
                                        .map(|p| {
                                            p.as_u64()
                                                .and_then(|p| usize::try_from(p).ok())
                                                .and_then(|p| p.checked_sub(1))
                                                .ok_or("page indices must be positive integers")
                                        })
                                        .collect::<Result<Vec<_>, _>>()
                                })
                        })
                        .transpose()?;
                    crate::editing::Operation::Import {
                        source: crate::editing::Input::Path(PathBuf::from(string(params, "path")?)),
                        pages,
                    }
                }
                _ => {
                    let x = optional_number(params, "x", 0.25)?;
                    let y = optional_number(params, "y", 0.25)?;
                    let w = optional_number(params, "width", 0.5)?;
                    let h = optional_number(params, "height", 0.5)?;
                    crate::editing::Operation::Image {
                        source: crate::editing::Input::Path(PathBuf::from(string(params, "path")?)),
                        rect: Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h)),
                    }
                }
            };
            self.start_edit(operation, ctx)?;
            return Ok(json!({"accepted":true}));
        }
        if id == "file.open" {
            self.open(PathBuf::from(string(params, "path")?), ctx);
        } else if id == "document.cancel" {
            self.cancel_departure();
            self.form_save_open = false;
        } else if id == "document.discard" {
            if self.departure.is_none() {
                return Err("no pending open or close".into());
            }
            if self.view.as_ref().is_some_and(|v| {
                v.forms.is_saving()
                    || v.forms.dialog.is_some()
                    || (!self.departure_confirming
                        && (v.forms.busy() || v.annotations.busy() || v.preparing_editor()))
            }) {
                return Err("document input, edit or save is pending".into());
            }
            self.depart(ctx);
        } else {
            let view = self.view.as_mut().ok_or("no document open")?;
            if view.is_xfa()
                && (id.starts_with("annotation.")
                    || matches!(
                        id,
                        "view.annotations"
                            | "search.open"
                            | "search.find"
                            | "search.next"
                            | "search.previous"
                            | "selection.set"
                            | "navigation.link"
                            | "navigation.outline"
                    ))
            {
                return Err(
                    "this PDF editing/navigation tool is not available for XFA documents".into(),
                );
            }
            match id {
                "form.dialog.respond" => {
                    let dialog_id = params
                        .get("dialog_id")
                        .and_then(Value::as_u64)
                        .ok_or("missing dialog_id")?;
                    let answer = match string(params, "response")? {
                        "ok" => pdfoxium::FormDialogResponse::Ok,
                        "cancel" => pdfoxium::FormDialogResponse::Cancel,
                        "yes" => pdfoxium::FormDialogResponse::Yes,
                        "no" => pdfoxium::FormDialogResponse::No,
                        "text" => {
                            pdfoxium::FormDialogResponse::Text(string(params, "text")?.into())
                        }
                        _ => return Err("unknown dialog response".into()),
                    };
                    view.forms
                        .answer_dialog(view.loaded.pool.document(), dialog_id, answer)?;
                }
                "page.next" => {
                    view.go_to((view.page + 1).min(view.loaded.sizes.len().saturating_sub(1)))?
                }
                "page.previous" => view.go_to(view.page.saturating_sub(1))?,
                "view.page" => {
                    let page = params["page"]
                        .as_u64()
                        .and_then(|n| usize::try_from(n).ok())
                        .filter(|n| *n > 0)
                        .ok_or("page must be a positive integer")?;
                    view.go_to(page - 1)?;
                }
                "navigation.back" => view.history_step(true)?,
                "navigation.forward" => view.history_step(false)?,
                "navigation.outline" => view.follow_outline(index(params, "index")?, ctx)?,
                "navigation.link" => {
                    let page = index(params, "page")?
                        .checked_sub(1)
                        .ok_or("page must be positive")?;
                    view.follow_link(page, index(params, "index")?, ctx)?;
                }
                "form.save" => {
                    if params.get("path").is_some() {
                        view.forms.save(
                            PathBuf::from(string(params, "path")?),
                            view.loaded.pool.document(),
                        )?;
                    } else {
                        if view.loaded.pool.document().is_none() {
                            return Err("no interactive form document".into());
                        }
                        self.form_save_path = view
                            .loaded
                            .path
                            .with_file_name(format!(
                                "{}-edited.pdf",
                                view.loaded
                                    .path
                                    .file_stem()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                            ))
                            .to_string_lossy()
                            .into_owned();
                        self.form_save_open = true;
                    }
                }
                "form.blur" => view.forms.blur(view.loaded.pool.document()),
                "view.annotations" => {
                    view.annotations.open = !view.annotations.open;
                    view.annotations.cancel_tools();
                    if view.annotations.open {
                        view.ensure_annotation_editor(ctx);
                        view.forms.blur(view.loaded.pool.document());
                    }
                }
                "annotation.markup" => {
                    if view.text.selecting {
                        return Err("selection is still pending".into());
                    }
                    view.annotations.markup(
                        view.loaded.pool.document().ok_or("no document worker")?,
                        string(params, "kind")?,
                        &view.text.selection,
                    )?;
                }
                "annotation.text" | "annotation.rectangle" | "annotation.ellipse" => {
                    view.ensure_annotation_editor(ctx);
                    view.forms.blur(view.loaded.pool.document());
                    let tool = match id {
                        "annotation.text" => crate::annotations::Tool::Text,
                        "annotation.rectangle" => crate::annotations::Tool::Rectangle,
                        _ => crate::annotations::Tool::Ellipse,
                    };
                    view.annotations.start_tool(tool);
                }
                "annotation.place_note" => {
                    view.ensure_annotation_editor(ctx);
                    view.annotations
                        .start_note(string(params, "text")?.to_owned());
                }
                "annotation.note" => {
                    view.annotations.note(
                        view.loaded.pool.document().ok_or("no document worker")?,
                        view.page,
                        params["x"].as_f64().unwrap_or(0.5) as f32,
                        params["y"].as_f64().unwrap_or(0.5) as f32,
                        string(params, "text")?.to_owned(),
                    )?;
                }
                "annotation.defaults" => view.annotations.configure_defaults(params)?,
                "annotation.style" => view.annotations.style_patch(
                    view.loaded.pool.document().ok_or("no document worker")?,
                    index(params, "index")?,
                    params,
                )?,
                "annotation.comment" | "annotation.delete" => {
                    if !view
                        .annotations
                        .has_page(view.annotations.context_page(view.page))
                    {
                        return Err("wait for this page's annotation list".into());
                    }
                    let contents = if id == "annotation.comment" {
                        Some(string(params, "text")?.to_owned())
                    } else {
                        None
                    };
                    view.annotations.edit(
                        view.loaded.pool.document().ok_or("no document worker")?,
                        index(params, "index")?,
                        contents,
                    )?;
                }
                "view.label" => view.go_to_label(string(params, "label")?)?,
                "view.outline" => {
                    self.outline_open = !self.outline_open;
                    if self.outline_open {
                        self.thumbnails_open = false;
                    }
                }
                "view.thumbnails" => {
                    self.thumbnails_open = !self.thumbnails_open;
                    if self.thumbnails_open {
                        self.outline_open = false;
                        view.thumbnails.scroll_to = Some(view.page);
                    }
                }
                "thumbnails.page" => {
                    let page = index(params, "page")?
                        .checked_sub(1)
                        .filter(|p| *p < view.loaded.sizes.len())
                        .ok_or("page out of range")?;
                    self.thumbnails_open = true;
                    self.outline_open = false;
                    view.thumbnails.scroll_to = Some(page);
                }
                "view.zoom" => view.set_zoom(number(params, "zoom")?)?,
                "view.zoom_in" => view.set_zoom((view.zoom * 1.25).min(8.0))?,
                "view.zoom_out" => view.set_zoom((view.zoom / 1.25).max(0.1))?,
                "view.fit" => {
                    view.fit = true;
                    view.complete = false;
                }
                "view.rotate" => view.rotate(),
                "search.open" => {
                    self.find_open = true;
                    self.focus_find = true;
                }
                "search.find" => {
                    self.find_query = params["query"]
                        .as_str()
                        .ok_or("query must be a string")?
                        .to_owned();
                    self.find_case = params["match_case"].as_bool().unwrap_or(false);
                    self.find_open = true;
                    view.find(self.find_query.clone(), self.find_case);
                }
                "search.next" => view.find_next(false),
                "search.previous" => view.find_next(true),
                "search.clear" => {
                    self.find_open = false;
                    self.find_query.clear();
                    view.find(String::new(), false);
                }
                "selection.set" => {
                    let page = integer(params, "page")?
                        .checked_sub(1)
                        .filter(|p| *p < view.loaded.sizes.len())
                        .ok_or("page out of range")?;
                    let start = integer(params, "start")?;
                    let end = integer(params, "end")?;
                    if end < start {
                        return Err("end must be at least start".into());
                    }
                    view.text.select(page, start..end);
                    view.complete = false;
                }
                "selection.clear" => view.text.clear_selection(),
                "selection.copy" => {
                    if view.forms.focused() {
                        let text = view.forms.state.selected_text.clone();
                        ctx.copy_text(text.clone());
                        return Ok(json!({"text":text}));
                    }

                    if view.text.selecting {
                        return Err("selection is still pending".into());
                    }
                    ctx.copy_text(view.text.selection.text.clone());
                    ctx.request_repaint();
                    return Ok(json!({"text":view.text.selection.text}));
                }
                _ => return Err(format!("unknown command: {id}")),
            }
        }
        self.changed();
        ctx.request_repaint();
        Ok(json!({"accepted":true}))
    }
    fn handle(&mut self, request: Request, ctx: &egui::Context) {
        if request.since.elapsed() > Duration::from_secs(5) {
            let _ = request
                .reply
                .send(Err("request expired before the UI handled it".into()));
            return;
        }
        if request
            .params
            .get("revision")
            .and_then(Value::as_u64)
            .is_some_and(|r| r != self.revision)
        {
            let _ = request.reply.send(Err("view revision changed".into()));
            return;
        }
        let params = &request.params;
        let result = (|| -> Result<Value, String> {
            match request.method.as_str() {
                "ui.state" => Ok(self.state()),
                "ui.commands" => Ok(json!(COMMANDS
                    .iter()
                    .map(|(id, label)| json!({"id":id,"label":label}))
                    .collect::<Vec<_>>())),
                "ui.command" => self.command(string(params, "id")?, params, ctx),
                "ui.open" => self.command("file.open", params, ctx),
                "ui.inspect" => {
                    let query = params["query"].as_str().unwrap_or("").to_lowercase();
                    Ok(
                        json!({"revision":self.revision,"widgets":self.widgets.iter().map(|w|json!({"id":w.id,"label":w.label,"enabled":w.enabled,"rect":[w.rect.min.x,w.rect.min.y,w.rect.width(),w.rect.height()]})).chain(self.layer_widgets.iter().cloned()).filter(|w|format!("{} {}",w["id"],w["label"]).to_lowercase().contains(&query)).collect::<Vec<_>>()}),
                    )
                }
                "ui.outline" => {
                    let view = self.view.as_ref().ok_or("no document open")?;
                    let offset = params["offset"].as_u64().unwrap_or(0) as usize;
                    let limit = params["limit"].as_u64().unwrap_or(100).min(500) as usize;
                    Ok(
                        json!({"count":view.loaded.outline.len(),"open":self.outline_open,"items":view.loaded.outline.iter().enumerate().skip(offset).take(limit).map(|(index,item)|json!({"index":index,"depth":item.depth,"title":item.title,"target":view.target_label(&item.target),"destination_page":crate::navigation::destination_page(&item.target).map(|p|p+1),"page_label":crate::navigation::destination_page(&item.target).map(|p|view.page_label(p)),"actionable":item.target.is_some(),"collapsed":self.outline_collapsed.contains(&index),"rect":self.outline_rows.get(&index).map(|r|[r.min.x,r.min.y,r.width(),r.height()])})).collect::<Vec<_>>()}),
                    )
                }
                "ui.forms" => {
                    let view = self.view.as_ref().ok_or("no document open")?;
                    Ok(view.forms.snapshot(
                        view.loaded.pool.document(),
                        &view.page_rects(),
                        view.rotation,
                    ))
                }
                "ui.thumbnails" => Ok(self
                    .view
                    .as_ref()
                    .ok_or("no document open")?
                    .thumbnails
                    .state()),
                "ui.pages" => {
                    let view = self.view.as_ref().ok_or("no document open")?;
                    let offset = index_or(params, "offset", 0)?;
                    let limit = index_or(params, "limit", 100)?.min(500);
                    Ok(
                        json!({"count":view.loaded.sizes.len(),"pages":(offset..view.loaded.sizes.len()).take(limit).map(|page|json!({"page":page+1,"label":view.page_label(page)})).collect::<Vec<_>>()}),
                    )
                }
                "ui.annotations" => {
                    let view = self.view.as_mut().ok_or("no document open")?;
                    if !view.annotations.open {
                        view.forms.blur(view.loaded.pool.document());
                    }
                    view.annotations.open = true;
                    view.ensure_annotation_editor(ctx);
                    view.annotations.poll(
                        view.loaded.pool.document(),
                        view.page,
                        view.forms.document_revision,
                    );
                    Ok(view.annotations_state())
                }
                "ui.links" => {
                    let view = self.view.as_mut().ok_or("no document open")?;
                    let page = if params.get("page").is_some() {
                        index(params, "page")?
                    } else {
                        view.page + 1
                    };
                    let page = page.checked_sub(1).ok_or("page must be positive")?;
                    view.links_state(page)
                }
                "ui.selection" => Ok(self
                    .view
                    .as_ref()
                    .ok_or("no document open")?
                    .selection_state()),
                "ui.search" => {
                    let view = self.view.as_ref().ok_or("no document open")?;
                    let offset = params["offset"].as_u64().unwrap_or(0) as usize;
                    let limit = params["limit"].as_u64().unwrap_or(20).min(100) as usize;
                    Ok(
                        json!({"state":view.search_state(),"hits":view.text.hits.iter().skip(offset).take(limit).map(|h|view.hit_state(h)).collect::<Vec<_>>()}),
                    )
                }
                "ui.drag" => {
                    if !self.batches.is_empty() {
                        return Err("a drag is already pending".into());
                    }
                    let from = position(params, "from")?;
                    let to = position(params, "to")?;
                    if !self
                        .view
                        .as_ref()
                        .is_some_and(|v| v.canvas_rect.contains(from))
                    {
                        return Err("drag must start inside the document viewport".into());
                    }
                    let steps = params["steps"].as_u64().unwrap_or(8).clamp(1, 32);
                    self.batches.push_back(vec![
                        egui::Event::PointerMoved(from),
                        egui::Event::PointerButton {
                            pos: from,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: egui::Modifiers::NONE,
                        },
                    ]);
                    for i in 1..=steps {
                        self.batches.push_back(vec![egui::Event::PointerMoved(
                            from + (to - from) * (i as f32 / steps as f32),
                        )]);
                    }
                    self.batches.push_back(vec![egui::Event::PointerButton {
                        pos: to,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    }]);
                    self.settle_frame = self.frame + 3;
                    Ok(json!({"queued":true}))
                }
                "ui.pointer" => {
                    if !self.batches.is_empty() {
                        return Err("a gesture is already queued".into());
                    }
                    let point = position(params, "position")?;
                    let action = if params.get("action").is_some() {
                        string(params, "action")?
                    } else {
                        "move"
                    };
                    if !matches!(action, "down" | "move" | "up") {
                        return Err("pointer action must be down, move or up".into());
                    }
                    if action == "down"
                        && !Rect::from_min_size(egui::Pos2::ZERO, self.window).contains(point)
                    {
                        return Err("pointer down must start inside the window".into());
                    }
                    self.events.push(egui::Event::PointerMoved(point));
                    if action != "move" {
                        self.events.push(egui::Event::PointerButton {
                            pos: point,
                            button: egui::PointerButton::Primary,
                            pressed: action == "down",
                            modifiers: egui::Modifiers::NONE,
                        });
                    }
                    self.settle_frame = self.frame + 3;
                    Ok(json!({"queued":true}))
                }
                "ui.click" => {
                    let pos = if params.get("position").is_some() {
                        let pos = position(params, "position")?;
                        if !Rect::from_min_size(egui::Pos2::ZERO, self.window).contains(pos) {
                            return Err("click is outside the window".into());
                        }
                        pos
                    } else {
                        let id = string(params, "id")?;
                        let widget = self
                            .widgets
                            .iter()
                            .find(|w| w.id == id)
                            .ok_or("unknown widget id; use ui.inspect")?;
                        if !widget.enabled {
                            return Err("widget is disabled".into());
                        }
                        widget.rect.center()
                    };
                    let count = index_or(params, "count", 1)?;
                    if !(1..=2).contains(&count) {
                        return Err("click count must be 1 or 2".into());
                    }
                    for _ in 0..count {
                        let mut batch = vec![egui::Event::PointerMoved(pos)];
                        for pressed in [true, false] {
                            batch.push(egui::Event::PointerButton {
                                pos,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: egui::Modifiers::NONE,
                            });
                        }
                        self.batches.push_back(batch);
                    }
                    self.settle_frame = self.frame + 3;
                    Ok(json!({"queued":true}))
                }
                "ui.type" => {
                    self.events
                        .push(egui::Event::Text(string(params, "text")?.to_owned()));
                    self.settle_frame = self.frame + 3;
                    Ok(json!({"queued":true}))
                }
                "ui.key" => {
                    let key = egui::Key::from_name(string(params, "key")?).ok_or("unknown key")?;
                    let modifiers = modifiers(params)?;
                    if modifiers.command && key == egui::Key::C {
                        self.events.push(egui::Event::Copy);
                    }
                    for pressed in [true, false] {
                        self.events.push(egui::Event::Key {
                            key,
                            physical_key: None,
                            pressed,
                            repeat: false,
                            modifiers,
                        });
                    }
                    self.settle_frame = self.frame + 3;
                    Ok(json!({"queued":true}))
                }
                "ui.pinch" => {
                    let factor = number(params, "factor")?;
                    if !(0.1..=10.0).contains(&factor) {
                        return Err("factor must be between 0.1 and 10.0".into());
                    }
                    let view = self.view.as_ref().ok_or("no document open")?;
                    let point = egui::pos2(
                        optional_number(params, "x", view.canvas_rect.center().x)?,
                        optional_number(params, "y", view.canvas_rect.center().y)?,
                    );
                    if !view.canvas_rect.contains(point) {
                        return Err("pinch point must lie inside the document viewport".into());
                    }
                    self.events.push(egui::Event::PointerMoved(point));
                    self.events.push(egui::Event::Zoom(factor));
                    self.settle_frame = self.frame + 3;
                    Ok(json!({"queued":true}))
                }
                "ui.wheel" => {
                    let pos = position(params, "position")?;
                    if !self
                        .view
                        .as_ref()
                        .is_some_and(|v| v.canvas_rect.contains(pos))
                    {
                        return Err("wheel point must lie inside the document viewport".into());
                    }
                    let delta = egui::vec2(
                        optional_number(params, "dx", 0.0)?,
                        optional_number(params, "dy", 0.0)?,
                    );
                    self.events.push(egui::Event::PointerMoved(pos));
                    self.events.push(egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta,
                        modifiers: egui::Modifiers::NONE,
                        phase: egui::TouchPhase::Move,
                        source: egui::MouseWheelSource::Wheel,
                    });
                    self.settle_frame = self.frame + 3;
                    Ok(json!({"queued":true}))
                }
                "ui.scroll" => {
                    let dx = optional_number(params, "dx", 0.0)?;
                    let dy = optional_number(params, "dy", 0.0)?;
                    let view = self.view.as_mut().ok_or("no document open")?;
                    view.scroll = (view.scroll + egui::vec2(dx, dy)).max(Vec2::ZERO);
                    view.complete = false;
                    self.changed();
                    Ok(json!({"accepted":true}))
                }
                "ui.resize" => {
                    let width = number(params, "width")?;
                    let height = number(params, "height")?;
                    if !(480.0..=4096.0).contains(&width) || !(320.0..=4096.0).contains(&height) {
                        return Err(
                            "window dimensions must be 480..4096 by 320..4096 points".into()
                        );
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        width, height,
                    )));
                    self.changed();
                    Ok(json!({"accepted":true}))
                }
                "ui.quit" => {
                    self.request_departure(Departure::Close);
                    ctx.request_repaint();
                    Ok(json!({"accepted":true}))
                }
                _ => Err(format!("unknown method: {}", request.method)),
            }
        })();
        if matches!(request.method.as_str(), "ui.wait" | "ui.screenshot") {
            let waiting = Waiting {
                reply: request.reply,
                since: request.since,
                revision: params.get("revision").and_then(Value::as_u64),
            };
            if request.method == "ui.wait" {
                if self.waits.len() >= 16 {
                    let _ = waiting.reply.send(Err("too many pending waits".into()));
                } else {
                    self.waits.push(waiting);
                }
            } else if self.capture.is_some() {
                let _ = waiting.reply.send(Err("screenshot already pending".into()));
            } else {
                let options = (|| -> Result<_, String> {
                    let settled = params
                        .get("settled")
                        .map(|value| value.as_bool().ok_or("settled must be a boolean"))
                        .transpose()?
                        .unwrap_or(true);
                    Ok((string(params, "path")?, settled))
                })();
                match options {
                    Ok((path, settled)) => {
                        if self.control_only_input && settled {
                            // An occluded macOS window may skip framebuffer
                            // readback. Test captures explicitly bring it forward.
                            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                            self.settle_frame = self.settle_frame.max(self.frame + 3);
                        }
                        self.capture = Some(Capture {
                            waiting,
                            path: PathBuf::from(path),
                            issued: None,
                            settled,
                        })
                    }
                    Err(error) => {
                        let _ = waiting.reply.send(Err(error));
                    }
                }
            }
        } else {
            let _ = request.reply.send(result);
        }
        ctx.request_repaint();
    }
    fn button(
        &mut self,
        ui: &mut egui::Ui,
        id: &'static str,
        label: &'static str,
        enabled: bool,
    ) -> bool {
        let response = ui.add_enabled(enabled, egui::Button::new(label));
        self.widgets.push(Widget {
            id,
            label,
            rect: response.rect,
            enabled,
        });
        response.clicked()
    }
    fn icon(
        &mut self,
        ui: &mut egui::Ui,
        id: &'static str,
        label: &'static str,
        icon: crate::chrome::Icon,
        selected: bool,
        enabled: bool,
    ) -> bool {
        let response = crate::chrome::icon(ui, icon, selected, enabled, label);
        self.widgets.push(Widget {
            id,
            label,
            rect: response.rect,
            enabled,
        });
        response.clicked()
    }
    fn navigation_tabs(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, thumbnails: bool) {
        ui.add_space(2.);
        ui.horizontal(|ui| {
            let pages = ui.selectable_label(thumbnails, "Pages");
            self.widgets.push(Widget {
                id: "view.thumbnails",
                label: "Pages",
                rect: pages.rect,
                enabled: true,
            });
            if pages.clicked() && !thumbnails {
                self.run("view.thumbnails", json!({}), ctx);
            }
            let outline = ui.selectable_label(!thumbnails, "Outline");
            self.widgets.push(Widget {
                id: "view.outline",
                label: "Outline",
                rect: outline.rect,
                enabled: true,
            });
            if outline.clicked() && thumbnails {
                self.run("view.outline", json!({}), ctx);
            }
        });
        ui.add_space(2.);
        ui.separator();
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn open_picker(&mut self, ctx: &egui::Context) {
        if let Some(path) = rfd::FileDialog::new()
            .set_title("Open PDF")
            .add_filter("PDF documents", &["pdf"])
            .pick_file()
        {
            self.open(path, ctx);
        }
    }
    #[cfg(target_arch = "wasm32")]
    fn open_picker(&mut self, _ctx: &egui::Context) {
        crate::web::pick_file("open");
    }
    fn toolbar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        use crate::chrome::Icon;
        let has = self.view.is_some() && self.loading.is_none() && self.editing.is_none();
        let pdf = has && self.view.as_ref().is_some_and(|v| !v.is_xfa());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.;
            let menu = ui.menu_button("File", |ui| {
                ui.set_min_width(180.);
                if self.button(ui, "file.open", "Open…", true) {
                    ui.close();
                    self.open_picker(ctx);
                }
                let editable = has
                    && self
                        .view
                        .as_ref()
                        .is_some_and(|v| v.loaded.pool.document().is_some());
                if self.button(ui, "form.save", "Save a copy…", editable) {
                    ui.close();
                    self.run("form.save", json!({}), ctx);
                }
                ui.separator();
                if self.button(ui, "edit.open", "Edit pages…", pdf) {
                    ui.close();
                    self.run("edit.open", json!({}), ctx);
                }
                ui.menu_button("Navigation", |ui| {
                    let navigation = self.view.as_ref().map(View::navigation_state);
                    for (id, label, enabled) in [
                        (
                            "navigation.back",
                            "Previous location",
                            navigation.as_ref().is_some_and(|s| s["can_back"] == true),
                        ),
                        (
                            "navigation.forward",
                            "Next location",
                            navigation
                                .as_ref()
                                .is_some_and(|s| s["can_forward"] == true),
                        ),
                    ] {
                        if self.button(ui, id, label, enabled) {
                            ui.close();
                            self.run(id, json!({}), ctx);
                        }
                    }
                });
                if self.button(ui, "demo.open", "Sample documents…", true) {
                    ui.close();
                    self.run("demo.open", json!({}), ctx);
                }
            });
            self.widgets.push(Widget {
                id: "menu.file",
                label: "File",
                rect: menu.response.rect,
                enabled: true,
            });
            ui.separator();
            if self.icon(
                ui,
                "sidebar.toggle",
                "Toggle navigation sidebar",
                Icon::Sidebar,
                self.outline_open || self.thumbnails_open,
                has,
            ) {
                if self.outline_open || self.thumbnails_open {
                    self.outline_open = false;
                    self.thumbnails_open = false;
                } else {
                    self.run("view.thumbnails", json!({}), ctx);
                }
            }
            let page_index = self.view.as_ref().map_or(0, |v| v.page);
            let count = self.view.as_ref().map_or(0, |v| v.loaded.sizes.len());
            if self.icon(
                ui,
                "page.previous",
                "Previous page",
                Icon::Previous,
                false,
                has && page_index > 0,
            ) {
                self.run("page.previous", json!({}), ctx);
            }
            let page = ui.add_enabled(
                has,
                egui::TextEdit::singleline(&mut self.page_input)
                    .desired_width(34.)
                    .align(egui::Align2::CENTER_TOP),
            );
            self.widgets.push(Widget {
                id: "view.page",
                label: "Page number",
                rect: page.rect,
                enabled: has,
            });
            let page = page.on_hover_text(self.view.as_ref().map_or_else(
                || "Page number or label".into(),
                |v| {
                    format!(
                        "Page {} · Enter a number or page label",
                        v.page_caption(v.page)
                    )
                },
            ));
            if page.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let input = self.page_input.clone();
                if let Ok(page) = input.parse::<u64>() {
                    self.run("view.page", json!({"page":page}), ctx);
                } else {
                    self.run("view.label", json!({"label":input}), ctx);
                }
            }
            ui.weak(format!("/ {count}"));
            if self.icon(
                ui,
                "page.next",
                "Next page",
                Icon::Next,
                false,
                has && page_index + 1 < count,
            ) {
                self.run("page.next", json!({}), ctx);
            }
            ui.separator();
            ui.add_enabled_ui(has, |ui| {
                let zoom = self.view.as_ref().map_or(1., |v| v.zoom);
                let menu = ui.menu_button(format!("{:.0}%", zoom * 100.), |ui| {
                    ui.set_min_width(130.);
                    if self.button(ui, "view.fit", "Fit width", has) {
                        ui.close();
                        self.run("view.fit", json!({}), ctx);
                    }
                    for (percent, id) in [
                        (50, "view.zoom.50"),
                        (75, "view.zoom.75"),
                        (100, "view.zoom.100"),
                        (125, "view.zoom.125"),
                        (150, "view.zoom.150"),
                        (200, "view.zoom.200"),
                    ] {
                        let response = ui.selectable_label(
                            (zoom * 100. - percent as f32).abs() < 0.5,
                            format!("{percent}%"),
                        );
                        self.widgets.push(Widget {
                            id,
                            label: "Zoom preset",
                            rect: response.rect,
                            enabled: has,
                        });
                        if response.clicked() {
                            ui.close();
                            self.run("view.zoom", json!({"zoom":percent as f32/100.}), ctx);
                        }
                    }
                    ui.separator();
                    if self.button(ui, "view.zoom_out", "Zoom out", has) {
                        ui.close();
                        self.run("view.zoom_out", json!({}), ctx);
                    }
                    if self.button(ui, "view.zoom_in", "Zoom in", has) {
                        ui.close();
                        self.run("view.zoom_in", json!({}), ctx);
                    }
                    if self.button(ui, "view.rotate", "Rotate view", has) {
                        ui.close();
                        self.run("view.rotate", json!({}), ctx);
                    }
                });
                self.widgets.push(Widget {
                    id: "menu.zoom",
                    label: "Zoom",
                    rect: menu.response.rect,
                    enabled: has,
                });
                menu.response.on_hover_text("Zoom, fit and rotation");
            });
            if has
                && self
                    .view
                    .as_ref()
                    .is_some_and(|v| !v.loaded.layers.is_empty())
            {
                let mut action = None;
                let menu = ui.menu_button("Layers", |ui| {
                    ui.set_min_width(200.);
                    let view = self.view.as_ref().unwrap();
                    let groups = view.loaded.layers.groups();
                    layer_rows(
                        ui,
                        &view.layer_order(),
                        &groups,
                        &mut self.layer_widgets,
                        &mut action,
                    );
                    let configurations = view.loaded.layers.configurations();
                    if configurations.len() > 1 {
                        ui.separator();
                        ui.label("Configurations");
                        for (index, c) in configurations.iter().enumerate() {
                            let title = if c.name.is_empty() {
                                "Default"
                            } else {
                                &c.name
                            };
                            let response = ui.selectable_label(
                                view.loaded.layers.configuration() == Some(c.id),
                                title,
                            );
                            record_layer_widget(
                                &mut self.layer_widgets,
                                format!("layers.configuration.{index}"),
                                title,
                                &response,
                                true,
                            );
                            if response.clicked() {
                                action = Some(("layers.configuration", json!({"index":index})));
                            }
                        }
                    }
                    ui.separator();
                    let response = ui.button("Reset layers");
                    record_layer_widget(
                        &mut self.layer_widgets,
                        "layers.reset".into(),
                        "Reset layers",
                        &response,
                        true,
                    );
                    if response.clicked() {
                        action = Some(("layers.reset", json!({})));
                    }
                });
                self.widgets.push(Widget {
                    id: "menu.layers",
                    label: "Layers",
                    rect: menu.response.rect,
                    enabled: true,
                });
                if let Some((id, params)) = action {
                    self.run(id, params, ctx);
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let open = self.view.as_ref().is_some_and(|v| v.annotations.open);
                if self.icon(
                    ui,
                    "view.annotations",
                    "Annotations",
                    Icon::Annotate,
                    open,
                    pdf,
                ) {
                    self.run("view.annotations", json!({}), ctx);
                }
                if self.icon(
                    ui,
                    "search.open",
                    "Find in document (⌘F)",
                    Icon::Search,
                    self.find_open,
                    pdf,
                ) {
                    self.run("search.open", json!({}), ctx);
                }
            });
        });
    }
    fn run(&mut self, id: &str, params: Value, ctx: &egui::Context) {
        if let Err(error) = self.command(id, &params, ctx) {
            self.error = Some(error);
        }
    }
    fn finish_control(&mut self, ctx: &egui::Context) {
        let failed = self.error.clone().or_else(|| {
            self.view.as_ref().and_then(|v| {
                v.errors
                    .values()
                    .next()
                    .cloned()
                    .or_else(|| v.text.error.clone())
                    .or_else(|| v.annotations.error.clone())
                    .or_else(|| v.forms.save_error.clone().or_else(|| v.forms.error.clone()))
            })
        });
        let state = self.state();
        let ready = self.ready();
        let dialog_ready = self.frame >= self.settle_frame
            && self.view.as_ref().is_some_and(|v| v.forms.dialog.is_some());
        self.waits.retain(|wait| {
            let error = if wait.since.elapsed() > Duration::from_secs(30) {
                Some("view did not settle within 30 seconds".into())
            } else if wait.revision.is_some_and(|r| r != self.revision) {
                Some("view revision changed".into())
            } else {
                failed.clone()
            };
            if let Some(error) = error {
                let _ = wait.reply.send(Err(error));
                false
            } else if ready || dialog_ready {
                let _ = wait.reply.send(Ok(state.clone()));
                false
            } else {
                true
            }
        });
        if let Some(capture) = &self.capture {
            let error = if capture.waiting.since.elapsed() > Duration::from_secs(30) {
                Some("screenshot timed out; keep the window visible".into())
            } else if capture
                .waiting
                .revision
                .or(capture.issued.filter(|_| capture.settled))
                .is_some_and(|r| r != self.revision)
            {
                Some("view changed before screenshot completed".into())
            } else {
                failed
            };
            if let Some(error) = error {
                let capture = self.capture.take().unwrap();
                let _ = capture.waiting.reply.send(Err(error));
            } else if (!capture.settled || ready || dialog_ready) && capture.issued.is_none() {
                let capture = self.capture.as_mut().unwrap();
                capture.issued = Some(self.revision);
                let tx = self.screenshot_tx.clone();
                let revision = self.revision;
                let repaint = ctx.clone();
                ctx.request_screenshot(move |image| {
                    let _ = tx.send((revision, image));
                    repaint.request_repaint();
                });
            }
        }
        if !self.waits.is_empty() || self.capture.is_some() || self.frame < self.settle_frame {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }
}
impl eframe::App for App {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        // Explicit test mode: native keyboard/pointer events and focus changes
        // must not contaminate the same egui input stream used by control calls.
        if self.control_only_input {
            input.events.clear();
            input
                .events
                .push(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
            input.focused = true;
            for viewport in input.viewports.values_mut() {
                viewport.focused = Some(true);
            }
        }
        if let Some(egui::Event::Key { modifiers, .. }) = self
            .events
            .iter()
            .rev()
            .find(|event| matches!(event, egui::Event::Key { .. }))
        {
            input.events.push(egui::Event::ModifiersChanged(*modifiers));
        }
        input.events.append(&mut self.events);
        if let Some(mut batch) = self.batches.pop_front() {
            input.events.append(&mut batch);
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &ui.ctx().clone();
        #[cfg(all(target_arch = "wasm32", feature = "web-test"))]
        for request in crate::web::testing::take_requests() {
            self.handle(request, ctx);
        }
        self.frame += 1;
        self.window = ctx.content_rect().size();
        self.intercept_close(ctx);
        if let Some(result) = self.editing.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.editing = None;
            match result {
                Ok((loaded, page)) => {
                    let old = self.view.take();
                    let zoom = old.as_ref().map(|v| v.zoom).unwrap_or(1.);
                    let mut view = View::new(loaded, ctx.clone());
                    view.forms.mark_snapshot_dirty();
                    let _ = view.set_zoom(zoom);
                    let _ = view.go_to(page);
                    self.view = Some(view);
                    self.page_input = (page + 1).to_string();
                    self.find_query.clear();
                    self.find_open = false;
                    self.outline_collapsed.clear();
                    if let Some(old) = old {
                        crate::platform::discard(old);
                    }
                }
                Err(error) => self.error = Some(error),
            }
            self.changed();
        }
        if let Some(result) = self.loading.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.loading = None;
            match result {
                Ok(loaded) => {
                    self.outline_collapsed = loaded
                        .outline
                        .iter()
                        .enumerate()
                        .filter(|(_, item)| !item.open)
                        .map(|(index, _)| index)
                        .collect();
                    if let Some(old) = self.view.take() {
                        crate::platform::discard(old);
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Title(format!(
                        "{} — pdfoxium",
                        loaded
                            .path
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                    )));
                    self.view = Some(View::new(loaded, ctx.clone()));
                    self.page_input = "1".into();
                    self.find_open = false;
                    self.find_query.clear();
                }
                Err(error) => self.error = Some(error),
            }
            self.changed();
        }
        while let Some(request) = self
            .server
            .as_ref()
            .and_then(|s| s.requests.try_recv().ok())
        {
            self.handle(request, ctx);
        }
        if self.departure.is_none()
            && !self.form_save_open
            && !self.view.as_ref().is_some_and(|v| v.forms.dialog.is_some())
            && ctx.input_mut(|i| {
                i.consume_shortcut(&egui::KeyboardShortcut::new(
                    egui::Modifiers::COMMAND,
                    egui::Key::O,
                ))
            })
        {
            self.open_picker(ctx);
        }
        if self.view.is_some() {
            if ctx.input_mut(|i| {
                i.consume_shortcut(&egui::KeyboardShortcut::new(
                    egui::Modifiers::COMMAND,
                    egui::Key::F,
                ))
            }) {
                self.run("search.open", json!({}), ctx);
            }
            for (modifiers, id) in [
                (
                    egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                    "search.previous",
                ),
                (egui::Modifiers::COMMAND, "search.next"),
            ] {
                if ctx.input_mut(|i| {
                    i.consume_shortcut(&egui::KeyboardShortcut::new(modifiers, egui::Key::G))
                }) {
                    self.run(id, json!({}), ctx);
                }
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                if self.form_save_open {
                    self.form_save_open = false;
                } else if self.departure.is_some() {
                    self.cancel_departure();
                    self.changed();
                } else if self.view.as_ref().is_some_and(|v| {
                    v.annotations.dragging()
                        || v.annotations.tool.is_some()
                        || v.annotations.placing_note
                }) {
                    if let Some(view) = &mut self.view {
                        view.annotations.cancel_tools();
                        view.annotations.deselect();
                        view.text.clear_selection();
                    }
                    self.changed();
                } else if self.find_open {
                    self.run("search.clear", json!({}), ctx);
                } else if self
                    .view
                    .as_ref()
                    .is_some_and(|v| v.annotations.has_selection())
                {
                    if let Some(view) = &mut self.view {
                        view.annotations.deselect();
                        view.annotations.cancel_tools();
                        view.text.clear_selection();
                    }
                    self.changed();
                } else if self.view.as_ref().is_some_and(|v| v.forms.focused()) {
                    if let Some(view) = &mut self.view {
                        view.forms.escape(view.loaded.pool.document());
                    }
                } else {
                    self.run("selection.clear", json!({}), ctx);
                }
            }
        }
        while let Ok((revision, image)) = self.screenshot_rx.try_recv() {
            if self
                .capture
                .as_ref()
                .is_some_and(|c| c.issued == Some(revision))
            {
                let capture = self.capture.take().unwrap();
                let result = if (capture.settled && capture.issued != Some(self.revision))
                    || capture.waiting.revision.is_some_and(|r| r != self.revision)
                {
                    Err("view changed before screenshot completed".into())
                } else {
                    save_png(&capture.path,&image).map(|()|json!({"path":capture.path,"width":image.width(),"height":image.height(),"revision":capture.issued,"render_complete":capture.settled}))
                };
                let _ = capture.waiting.reply.send(result);
            }
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(file) = crate::web::take_file() {
            if matches!(file.purpose, crate::web::UploadPurpose::Open) {
                self.demo_open = false;
                self.request_departure(Departure::Bytes(file.name.into(), file.bytes));
            } else {
                let source = crate::editing::Input::Bytes(file.bytes);
                let operation = if matches!(file.purpose, crate::web::UploadPurpose::Import) {
                    crate::editing::Operation::Import {
                        source,
                        pages: None,
                    }
                } else {
                    crate::editing::Operation::Image {
                        source,
                        rect: Rect::from_min_max(egui::pos2(0.25, 0.25), egui::pos2(0.75, 0.75)),
                    }
                };
                if let Err(error) = self.start_edit(operation, ctx) {
                    self.error = Some(error);
                }
            }
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(result) = self.dropped.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.dropped = None;
            match result {
                Ok((path, bytes)) => self.request_departure(Departure::Bytes(path, bytes.into())),
                Err(error) => self.error = Some(error),
            }
        }
        if let Some(file) = ctx.input(|i| i.raw.dropped_files.first().cloned()) {
            self.demo_open = false;
            #[cfg(not(target_arch = "wasm32"))]
            self.open(file.path().to_path_buf(), ctx);
            #[cfg(target_arch = "wasm32")]
            {
                let (tx, rx) = mpsc::channel();
                self.dropped = Some(rx);
                let repaint = ctx.clone();
                wasm_bindgen_futures::spawn_local(async move {
                    let result = file
                        .bytes_async()
                        .await
                        .map(|bytes| (file.path().to_path_buf(), bytes));
                    let _ = tx.send(result);
                    repaint.request_repaint();
                });
            }
        }
        self.widgets.clear();
        self.layer_widgets.clear();
        self.outline_rows.clear();
        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 4.;
            self.toolbar(ui, ctx);
            if self.find_open {
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    let entry = ui.add(
                        egui::TextEdit::singleline(&mut self.find_query)
                            .id_salt("find-query")
                            .hint_text("Find in document")
                            .desired_width((ui.available_width() - 225.).clamp(120., 320.))
                            .return_key(None),
                    );
                    self.widgets.push(Widget {
                        id: "search.query",
                        label: "Find in document",
                        rect: entry.rect,
                        enabled: true,
                    });
                    if self.focus_find {
                        entry.request_focus();
                        self.focus_find = false;
                    }
                    let case = ui
                        .toggle_value(&mut self.find_case, "Aa")
                        .on_hover_text("Match case");
                    self.widgets.push(Widget {
                        id: "search.case",
                        label: "Match case",
                        rect: case.rect,
                        enabled: true,
                    });
                    if entry.changed() || case.changed() {
                        if let Some(view) = &mut self.view {
                            view.find(self.find_query.clone(), self.find_case);
                        }
                        self.changed();
                    }
                    let has_hits = self
                        .view
                        .as_ref()
                        .is_some_and(|v| !v.text.hits.is_empty() || v.text.searching);
                    if self.icon(
                        ui,
                        "search.previous",
                        "Previous match",
                        crate::chrome::Icon::Previous,
                        false,
                        has_hits,
                    ) {
                        self.run("search.previous", json!({}), ctx);
                    }
                    if self.icon(
                        ui,
                        "search.next",
                        "Next match",
                        crate::chrome::Icon::Next,
                        false,
                        has_hits,
                    ) {
                        self.run("search.next", json!({}), ctx);
                    }
                    if let Some(view) = &self.view {
                        let state = &view.text;
                        ui.label(format!(
                            "{} / {}{}",
                            state.active.map_or(0, |i| i + 1),
                            state.hits.len(),
                            if state.truncated { "+" } else { "" }
                        ));
                        if state.searching {
                            ui.spinner();
                        }
                    }
                    if self.icon(
                        ui,
                        "search.clear",
                        "Close search",
                        crate::chrome::Icon::Close,
                        false,
                        true,
                    ) {
                        self.run("search.clear", json!({}), ctx);
                    }
                    if entry.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        self.run(
                            if ui.input(|i| i.modifiers.shift) {
                                "search.previous"
                            } else {
                                "search.next"
                            },
                            json!({}),
                            ctx,
                        );
                    }
                });
            }
        });
        egui::Panel::bottom("status").show(ui, |ui| {
            ui.spacing_mut().interact_size.y = 16.;
            ui.horizontal(|ui| {
                if self.loading.is_some() || self.editing.is_some() {
                    self.working_since = None;
                    ui.spinner();
                    ui.label(if self.editing.is_some() {
                        "Editing document…"
                    } else {
                        "Opening document…"
                    });
                } else if let Some(error) = &self.error {
                    self.working_since = None;
                    ui.colored_label(Color32::DARK_RED, error);
                } else if let Some(view) = &self.view {
                    if let Some(error) = view
                        .text
                        .navigation_error
                        .as_ref()
                        .or(view.text.error.as_ref())
                        .or(view.annotations.error.as_ref())
                    {
                        ui.colored_label(Color32::DARK_RED, error);
                        ui.separator();
                    }
                    let filename = view
                        .loaded
                        .path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy();
                    ui.add(
                        egui::Label::new(egui::RichText::new(filename.as_ref()).small().weak())
                            .truncate(),
                    )
                    .on_hover_text(view.loaded.path.display().to_string());
                    if view.forms.dirty() {
                        ui.weak("• Unsaved");
                    }

                    if let Some(error) =
                        view.forms.save_error.as_ref().or(view.forms.error.as_ref())
                    {
                        ui.colored_label(Color32::DARK_RED, error);
                    }
                    let working = view.rendering
                        || view.text.busy()
                        || view.text.navigation_busy()
                        || view.forms.is_saving()
                        || view.annotations.busy()
                        || view.preparing_editor();
                    let now = Instant::now();
                    let show_busy = activity_visible(&mut self.working_since, working, now);
                    if working && !show_busy {
                        ctx.request_repaint_after(
                            ACTIVITY_DELAY - now.duration_since(self.working_since.unwrap()),
                        );
                    }
                    if show_busy {
                        ui.spinner();
                    }
                    if view.forms.is_saving()
                        || view.text.searching
                        || (view.rendering && show_busy)
                    {
                        ui.weak(if view.forms.is_saving() {
                            "Saving…"
                        } else if view.text.searching {
                            "Searching…"
                        } else {
                            "Rendering…"
                        });
                    }
                } else {
                    self.working_since = None;
                    ui.label("Open a PDF to begin");
                }
            });
        });
        if self.form_save_open {
            let mut save = false;
            egui::Window::new("Save copy")
                .collapsible(false)
                .show(ctx, |ui| {
                    #[cfg(not(target_arch = "wasm32"))]
                    ui.label("Write a new PDF file (existing files are never overwritten).");
                    #[cfg(target_arch = "wasm32")]
                    ui.label("Download the edited PDF with this file name.");
                    let path = ui.text_edit_singleline(&mut self.form_save_path);
                    self.widgets.push(Widget {
                        id: "form.save_path",
                        label: "Save path",
                        rect: path.rect,
                        enabled: true,
                    });
                    save = self.button(ui, "form.save_confirm", "Save", true);
                    if self.button(ui, "form.save_cancel", "Cancel", true) {
                        self.form_save_open = false;
                    }
                });
            if save {
                self.form_save_open = false;
                self.run("form.save", json!({"path":self.form_save_path}), ctx);
            }
        }
        if self.edit_open {
            let mut action = None;
            let mut open = true;
            egui::Window::new("Page and image demo tools")
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label("Edits stay in memory. Save copy writes a new PDF.");
                    if self.button(
                        ui,
                        "page.rotate",
                        "Rotate page clockwise",
                        self.editing.is_none(),
                    ) {
                        action = Some(("page.rotate", json!({})));
                    }
                    if self.button(
                        ui,
                        "page.delete",
                        "Delete current page",
                        self.editing.is_none(),
                    ) {
                        action = Some(("page.delete", json!({})));
                    }
                    ui.separator();
                    #[cfg(target_arch = "wasm32")]
                    {
                        if self.button(ui, "page.import", "Import PDF…", self.editing.is_none()) {
                            action = Some(("page.import", json!({})));
                        }
                        if self.button(
                            ui,
                            "page.image",
                            "Insert PNG image…",
                            self.editing.is_none(),
                        ) {
                            action = Some(("page.image", json!({})));
                        }
                    }
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        ui.label("Import PDF pages after current page");
                        let entry = ui.text_edit_singleline(&mut self.edit_pdf_path);
                        self.widgets.push(Widget {
                            id: "edit.import_path",
                            label: "PDF to import",
                            rect: entry.rect,
                            enabled: true,
                        });
                        if self.button(
                            ui,
                            "page.import",
                            "Import PDF",
                            !self.edit_pdf_path.is_empty(),
                        ) {
                            action = Some(("page.import", json!({"path":self.edit_pdf_path})));
                        }
                        ui.separator();
                        ui.label("Insert PNG image as page content (centered)");
                        let entry = ui.text_edit_singleline(&mut self.edit_image_path);
                        self.widgets.push(Widget {
                            id: "edit.image_path",
                            label: "PNG to insert",
                            rect: entry.rect,
                            enabled: true,
                        });
                        if self.button(
                            ui,
                            "page.image",
                            "Insert image",
                            !self.edit_image_path.is_empty(),
                        ) {
                            action = Some(("page.image", json!({"path":self.edit_image_path})));
                        }
                    }
                });
            self.edit_open &= open;
            if let Some((id, params)) = action {
                self.run(id, params, ctx);
            }
        }
        let annotation_editing_enabled = self.loading.is_none() && self.editing.is_none();
        if self.view.as_ref().is_some_and(|v| v.annotations.open) {
            // Resize entirely inside the sidebar, leaving the adjacent document
            // scrollbar its own hit area without an extra layout gutter.
            let panel_id = egui::Id::unique("annotations");
            let resize_id = panel_id.with("inside-resize");
            if let Some(response) = ctx.read_response(resize_id) {
                if response.dragged() {
                    if let Some(pointer) = response.interact_pointer_pos() {
                        if let Some(mut state) =
                            egui::containers::panel::PanelState::load(ctx, panel_id)
                        {
                            state.outer_rect.min.x = state.outer_rect.max.x
                                - (state.outer_rect.max.x - pointer.x).clamp(220., 420.);
                            ctx.data_mut(|data| data.insert_persisted(panel_id, state));
                        }
                    }
                }
            }
            egui::Panel::right("annotations")
                .resizable(false)
                .default_size(296.0)
                .size_range(220.0..=(ctx.content_rect().width() * 0.45).clamp(220., 420.))
                .show(ui, |ui| {
                    ui.add_enabled_ui(annotation_editing_enabled, |ui| {
                        if let Some(view) = &mut self.view {
                            if let Some(rect) = view.annotations.show(
                                ui,
                                view.loaded.pool.document(),
                                &view.text.selection,
                                view.text.selecting,
                                view.page,
                            ) {
                                view.reveal_annotation(rect);
                            }
                            self.widgets.extend(view.annotations.controls.iter().map(
                                |(id, label, rect)| Widget {
                                    id,
                                    label,
                                    rect: *rect,
                                    enabled: annotation_editing_enabled,
                                },
                            ));
                        }
                    });
                    let panel = ui.clip_rect();
                    let width = ui.style().interaction.resize_grab_radius_side.clamp(4., 8.);
                    let handle =
                        Rect::from_min_max(panel.min, egui::pos2(panel.min.x + width, panel.max.y));
                    ui.interact(handle, resize_id, egui::Sense::drag())
                        .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
                    self.widgets.push(Widget {
                        id: "annotations.resize",
                        label: "Resize annotations",
                        rect: handle,
                        enabled: true,
                    });
                });
        }
        if self.thumbnails_open {
            let mut follow = None;
            egui::Panel::left("thumbnails")
                .default_size(200.0)
                .size_range(180.0..=320.0)
                .show(ui, |ui| {
                    self.navigation_tabs(ui, ctx, true);
                    if let Some(view) = &mut self.view {
                        follow = view.thumbnails.show(
                            ui,
                            &view.loaded,
                            view.page,
                            view.rotation,
                            &view.text,
                        );
                    }
                });
            if let Some(page) = follow {
                self.run("view.page", json!({"page":page+1}), ctx);
            }
        } else if let Some(view) = &mut self.view {
            view.thumbnails.hide(&view.text);
        }
        if self.outline_open {
            let mut follow = None;
            egui::Panel::left("outline")
                .default_size(240.0)
                .size_range(160.0..=480.0)
                .show(ui, |ui| {
                    self.navigation_tabs(ui, ctx, false);
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        if let Some(view) = &self.view {
                            if view.loaded.outline.is_empty() {
                                ui.label("No outline in this document");
                            }
                            let mut hidden_depth = None;
                            for (index, item) in view.loaded.outline.iter().enumerate() {
                                if hidden_depth.is_some_and(|depth| item.depth > depth) {
                                    continue;
                                }
                                hidden_depth = None;
                                let children = view
                                    .loaded
                                    .outline
                                    .get(index + 1)
                                    .is_some_and(|next| next.depth > item.depth);
                                let target = view.target_label(&item.target);
                                let page_label = crate::navigation::destination_page(&item.target)
                                    .map(|page| view.page_label(page));
                                let row = ui
                                    .push_id(index, |ui| {
                                        crate::outline::row(
                                            ui,
                                            item,
                                            children,
                                            self.outline_collapsed.contains(&index),
                                            &target,
                                            page_label.as_deref(),
                                        )
                                    })
                                    .inner;
                                if row.toggle && !self.outline_collapsed.remove(&index) {
                                    self.outline_collapsed.insert(index);
                                }
                                self.outline_rows.insert(index, row.title.rect);
                                if row.title.clicked() {
                                    follow = Some(index);
                                }
                                if self.outline_collapsed.contains(&index) {
                                    hidden_depth = Some(item.depth);
                                }
                            }
                        }
                    });
                });
            if let Some(index) = follow {
                self.run("navigation.outline", json!({"index":index}), ctx);
            }
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(crate::chrome::CANVAS))
            .show(ui, |ui| {
                if let Some(view) = &mut self.view {
                    ui.add_enabled_ui(self.loading.is_none() && self.editing.is_none(), |ui| {
                        view.show(ui)
                    });
                } else {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            egui::RichText::new("Drop a PDF here\nor choose File → Open")
                                .size(24.0)
                                .color(crate::chrome::MUTED),
                        );
                    });
                }
            });
        if self.demo_open {
            let mut open = true;
            let mut selected = None;
            egui::Window::new("Demo PDFs")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(360.)
                .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.vertical_centered_justified(|ui| {
                        for (index, demo) in crate::demos::DEMOS.iter().enumerate() {
                            if self.button(
                                ui,
                                demo.widget,
                                demo.title,
                                self.loading.is_none() && self.editing.is_none(),
                            ) {
                                selected = Some(index);
                            }
                        }
                    });
                });
            self.demo_open = open;
            if let Some(index) = selected {
                self.run(
                    "demo.choose",
                    json!({"sample":crate::demos::DEMOS[index].id}),
                    ctx,
                );
            }
        }
        if !ctx.egui_wants_keyboard_input()
            && !self.view.as_ref().is_some_and(|v| v.forms.dialog.is_some())
        {
            if self.view.is_some()
                && ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Copy)))
            {
                self.run("selection.copy", json!({}), ctx);
            }
            for (key, id) in [
                (egui::Key::PageDown, "page.next"),
                (egui::Key::PageUp, "page.previous"),
                (egui::Key::Plus, "view.zoom_in"),
                (egui::Key::Minus, "view.zoom_out"),
            ] {
                if ctx.input(|i| i.key_pressed(key)) {
                    self.run(id, json!({}), ctx);
                }
            }
            if ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Equals)) {
                self.run("view.zoom_in", json!({}), ctx);
            }
            if ctx.input(|i| i.modifiers.alt && i.key_pressed(egui::Key::ArrowLeft))
                && self
                    .view
                    .as_ref()
                    .is_some_and(|v| v.navigation_state()["can_back"] == true)
            {
                self.run("navigation.back", json!({}), ctx);
            }
            if ctx.input(|i| i.modifiers.alt && i.key_pressed(egui::Key::ArrowRight))
                && self
                    .view
                    .as_ref()
                    .is_some_and(|v| v.navigation_state()["can_forward"] == true)
            {
                self.run("navigation.forward", json!({}), ctx);
            }
            if let Some(view) = &mut self.view {
                let delta = ctx.input(|i| {
                    if i.key_pressed(egui::Key::ArrowDown) {
                        80.0
                    } else if i.key_pressed(egui::Key::ArrowUp) {
                        -80.0
                    } else {
                        0.0
                    }
                });
                if delta != 0.0 {
                    view.scroll.y = (view.scroll.y + delta).max(0.0);
                    ctx.request_repaint();
                }
            }
        }
        if let Some(view) = &mut self.view {
            view.forms.dialog_ui(ctx, view.loaded.pool.document());
        }
        let current_view = json!({"window":[self.window.x,self.window.y],"view":self.view.as_ref().map(|v|json!([v.zoom,v.rotation as i32,v.scroll.x,v.scroll.y,v.text.revision,v.render_revision,v.thumbnails.revision,v.forms.revision,v.canvas_rect.min.x,v.canvas_rect.min.y,v.canvas_rect.width(),v.canvas_rect.height()]))});
        if current_view != self.last_view {
            self.last_view = current_view;
            self.changed();
        }
        if !ctx.egui_wants_keyboard_input()
            && !self.view.as_ref().is_some_and(|v| v.forms.dialog.is_some())
        {
            if let Some(view) = &self.view {
                self.page_input = (view.page + 1).to_string();
            }
        }
        self.departure_ui(ctx);
        self.finish_control(ctx);
    }
}
fn string<'a>(params: &'a Value, key: &str) -> Result<&'a str, String> {
    params[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| format!("{key} must be a nonempty string"))
}
fn number(params: &Value, key: &str) -> Result<f32, String> {
    let value = params[key]
        .as_f64()
        .ok_or_else(|| format!("missing numeric {key}"))? as f32;
    if !value.is_finite() {
        Err(format!("{key} must be finite"))
    } else {
        Ok(value)
    }
}
fn optional_number(params: &Value, key: &str, default: f32) -> Result<f32, String> {
    if params.get(key).is_some() {
        number(params, key)
    } else {
        Ok(default)
    }
}
fn save_png(path: &std::path::Path, image: &egui::ColorImage) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut encoder = png::Encoder::new(
        std::io::BufWriter::new(file),
        image.width() as u32,
        image.height() as u32,
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| {
            writer.write_image_data(image.as_raw())?;
            writer.finish()
        })
        .map_err(|e| e.to_string())
}

fn integer(params: &Value, key: &str) -> Result<usize, String> {
    params[key]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| format!("{key} must be a nonnegative integer"))
}
fn position(params: &Value, key: &str) -> Result<egui::Pos2, String> {
    let values = params[key]
        .as_array()
        .filter(|a| a.len() == 2)
        .ok_or_else(|| format!("{key} must be [x,y]"))?;
    let x = values[0].as_f64().ok_or("invalid x")? as f32;
    let y = values[1].as_f64().ok_or("invalid y")? as f32;
    if !x.is_finite() || !y.is_finite() {
        return Err("position must be finite".into());
    }
    Ok(egui::pos2(x, y))
}

fn modifiers(params: &Value) -> Result<egui::Modifiers, String> {
    let mut result = egui::Modifiers::NONE;
    if let Some(values) = params.get("modifiers") {
        for value in values.as_array().ok_or("modifiers must be an array")? {
            match value.as_str() {
                Some("command") => {
                    result.command = true;
                    if cfg!(target_os = "macos") {
                        result.mac_cmd = true;
                    } else {
                        result.ctrl = true;
                    }
                }
                Some("shift") => result.shift = true,
                Some("ctrl") => result.ctrl = true,
                Some("alt") => result.alt = true,
                _ => return Err("unknown key modifier".into()),
            }
        }
    }
    Ok(result)
}

fn index_or(params: &Value, key: &str, default: usize) -> Result<usize, String> {
    if params.get(key).is_some() {
        index(params, key)
    } else {
        Ok(default)
    }
}
fn index(params: &Value, key: &str) -> Result<usize, String> {
    params[key]
        .as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .ok_or_else(|| format!("{key} must be a nonnegative integer"))
}

// /Order is presentation structure; a child's visibility is independent of its
// parent. Do not offer unlisted groups or turn a heading into a visibility gate.
fn layer_rows(
    ui: &mut egui::Ui,
    order: &[pdfoxium::OptionalContentOrder],
    groups: &[pdfoxium::OptionalContentGroup],
    widgets: &mut Vec<Value>,
    action: &mut Option<(&'static str, Value)>,
) {
    for (row, node) in order.iter().enumerate() {
        let children = match node {
            pdfoxium::OptionalContentOrder::Group { id, children } => {
                if let Some((index, group)) = groups.iter().enumerate().find(|(_, g)| g.id == *id) {
                    let mut visible = group.visible;
                    let response = ui.add_enabled(
                        !group.locked,
                        egui::Checkbox::new(&mut visible, &group.name),
                    );
                    let r = response.rect;
                    widgets.push(json!({"id":format!("layers.{index}"),"label":group.name,"enabled":!group.locked,"rect":[r.min.x,r.min.y,r.width(),r.height()]}));
                    if response.changed() {
                        *action = Some(("layers.set", json!({"index":index,"visible":visible})));
                    }
                    if group.locked {
                        response.on_hover_text("This layer is locked by the document");
                    }
                }
                children
            }
            pdfoxium::OptionalContentOrder::Label { text, children } => {
                ui.strong(text);
                children
            }
        };
        if !children.is_empty() {
            ui.indent(row, |ui| layer_rows(ui, children, groups, widgets, action));
        }
    }
}

fn record_layer_widget(
    widgets: &mut Vec<Value>,
    id: String,
    label: &str,
    response: &egui::Response,
    enabled: bool,
) {
    let r = response.rect;
    widgets.push(json!({"id":id,"label":label,"enabled":enabled,"rect":[r.min.x,r.min.y,r.width(),r.height()]}));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(
        app: &mut App,
        method: &str,
        params: Value,
        since: Instant,
    ) -> Result<Value, String> {
        let (reply, rx) = mpsc::channel();
        app.handle(
            Request {
                method: method.into(),
                params,
                reply,
                since,
            },
            &egui::Context::default(),
        );
        rx.try_recv().unwrap()
    }
    fn dirty_app(ctx: &egui::Context) -> App {
        use pdfoxium::thread::{RenderPool, Source};
        use pdfoxium::{Config, Document, Size};
        let document = Document::create().unwrap();
        document
            .insert_blank_page(
                0,
                Size {
                    width: 100.,
                    height: 100.,
                },
            )
            .unwrap();
        let mut bytes = Vec::new();
        document.save_to(&mut bytes).unwrap();
        let bytes: std::sync::Arc<[u8]> = bytes.into();
        let pool = RenderPool::new(Source::Bytes(bytes.clone()), Config::default(), 1).unwrap();
        let mut app = App::new(ctx, None, None).unwrap();
        app.view = Some(View::new(
            Loaded {
                path: "original.pdf".into(),
                sizes: vec![Size {
                    width: 100.,
                    height: 100.,
                }],
                labels: vec![None],
                layers: pool.optional_content_state(),
                pool: pool.into(),
                bytes,
                outline: vec![],
            },
            ctx.clone(),
        ));
        app.view.as_mut().unwrap().forms.state.dirty = true;
        app
    }
    #[test]
    fn activity_indicator_waits_for_sustained_work_and_resets_between_short_updates() {
        let now = Instant::now();
        let mut since = None;
        assert!(!activity_visible(&mut since, true, now));
        assert!(!activity_visible(
            &mut since,
            true,
            now + Duration::from_millis(149)
        ));
        assert!(activity_visible(
            &mut since,
            true,
            now + Duration::from_millis(150)
        ));
        assert!(!activity_visible(
            &mut since,
            false,
            now + Duration::from_millis(151)
        ));
        assert!(since.is_none());
        assert!(!activity_visible(
            &mut since,
            true,
            now + Duration::from_secs(1)
        ));
    }
    #[test]
    fn native_close_is_cancelled_until_dirty_document_is_resolved() {
        let ctx = egui::Context::default();
        let mut app = dirty_app(&ctx);
        let mut input = egui::RawInput::default();
        input
            .viewports
            .get_mut(&egui::ViewportId::ROOT)
            .unwrap()
            .events
            .push(egui::ViewportEvent::Close);
        let output = crate::test_frame(&ctx, input, |ctx| app.intercept_close(ctx));
        assert!(matches!(app.departure, Some(Departure::Close)));
        assert!(!app.close_approved);
        assert!(output.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .iter()
            .any(|cmd| matches!(cmd, egui::ViewportCommand::CancelClose)));
        app.command("document.cancel", &json!({}), &ctx).unwrap();
        assert!(app.departure.is_none() && app.view.as_ref().unwrap().forms.state.dirty);
    }
    #[test]
    fn departure_waits_for_input_then_cancel_preserves_dirty_document() {
        let ctx = egui::Context::default();
        let mut app = dirty_app(&ctx);
        app.events.push(egui::Event::Text("queued".into()));
        app.open("replacement.pdf".into(), &ctx);
        app.departure_ui(&ctx);
        assert!(app.loading.is_none());
        app.events.clear();
        let _ = crate::test_frame(&ctx, Default::default(), |ctx| app.departure_ui(ctx));
        assert!(app.loading.is_none());
        assert!(app.widgets.iter().any(|w| w.id == "document.cancel"));
        app.command("document.cancel", &json!({}), &ctx).unwrap();
        assert!(app.departure.is_none());
        assert!(app.view.as_ref().unwrap().forms.state.dirty);
        assert_eq!(
            app.view.as_ref().unwrap().loaded.path,
            PathBuf::from("original.pdf")
        );
    }
    #[test]
    fn confirmed_departure_stays_visible_during_background_metadata_queries() {
        use pdfoxium::thread::{DocOptions, DocumentThread, Source};
        let ctx = egui::Context::default();
        let mut app = dirty_app(&ctx);
        app.request_departure(Departure::Close);
        let _ = crate::test_frame(&ctx, Default::default(), |ctx| app.departure_ui(ctx));
        assert!(app.departure_confirming);
        let document = DocumentThread::spawn(
            Source::Bytes(app.view.as_ref().unwrap().loaded.bytes.clone()),
            pdfoxium::Config::default(),
            DocOptions::default(),
        )
        .unwrap();
        app.view.as_mut().unwrap().forms.request(0, Some(&document));
        assert!(app.view.as_ref().unwrap().forms.busy());
        // Repeated native close events must not restart the initial wait.
        app.request_departure(Departure::Close);
        app.widgets.clear();
        let _ = crate::test_frame(&ctx, Default::default(), |ctx| app.departure_ui(ctx));
        assert!(app.widgets.iter().any(|w| w.id == "document.cancel"));
        assert!(app
            .widgets
            .iter()
            .any(|w| w.id == "document.discard" && w.enabled));
        app.command("document.discard", &json!({}), &ctx).unwrap();
        assert!(app.close_approved && !app.departure_confirming);
    }

    #[test]
    fn save_error_does_not_complete_pending_close_even_if_dirty_was_cleared() {
        let ctx = egui::Context::default();
        let mut app = dirty_app(&ctx);
        app.departure = Some(Departure::Close);
        let forms = &mut app.view.as_mut().unwrap().forms;
        forms.state.dirty = false;
        forms.save_error = Some("save failed".into());
        forms.error = None; // unrelated pointer input may clear transient errors.
        let _ = crate::test_frame(&ctx, Default::default(), |ctx| app.departure_ui(ctx));
        assert!(matches!(app.departure, Some(Departure::Close)));
        assert!(!app.close_approved);
    }
    #[test]
    fn pending_open_proceeds_only_after_save_or_explicit_discard() {
        let ctx = egui::Context::default();
        let mut app = dirty_app(&ctx);
        app.open("replacement.pdf".into(), &ctx);
        let _ = crate::test_frame(&ctx, Default::default(), |ctx| app.departure_ui(ctx));
        assert!(app.loading.is_none());
        app.view.as_mut().unwrap().forms.state.dirty = false;
        let _ = crate::test_frame(&ctx, Default::default(), |ctx| app.departure_ui(ctx));
        assert!(app.departure.is_none() && app.loading.is_some());
        let mut app = dirty_app(&ctx);
        app.open("replacement.pdf".into(), &ctx);
        app.command("document.discard", &json!({}), &ctx).unwrap();
        assert!(app.departure.is_none() && app.loading.is_some());
    }
    #[test]
    fn control_only_input_is_opt_in_and_excludes_native_events() {
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx, None, None).unwrap();
        let mut input = egui::RawInput {
            events: vec![egui::Event::Text("native".into())],
            focused: false,
            ..Default::default()
        };
        eframe::App::raw_input_hook(&mut app, &ctx, &mut input);
        assert!(matches!(&input.events[0],egui::Event::Text(text) if text=="native"));
        app.set_control_only_input(true);
        app.events.push(egui::Event::Text("control".into()));
        eframe::App::raw_input_hook(&mut app, &ctx, &mut input);
        assert_eq!(input.events.len(), 2);
        assert!(matches!(
            &input.events[0],
            egui::Event::ModifiersChanged(egui::Modifiers::NONE)
        ));
        assert!(matches!(&input.events[1],egui::Event::Text(text) if text=="control"));
        assert!(input.focused);
    }
    #[test]
    fn stale_or_expired_input_is_rejected_before_mutating_the_app() {
        let mut app = App::new(&egui::Context::default(), None, None).unwrap();
        assert!(request(
            &mut app,
            "ui.type",
            json!({"text":"stale","revision":99}),
            Instant::now()
        )
        .is_err());
        assert!(request(
            &mut app,
            "ui.type",
            json!({"text":"expired"}),
            Instant::now() - Duration::from_secs(6)
        )
        .is_err());
        assert!(app.events.is_empty());
        assert!(request(
            &mut app,
            "ui.command",
            json!({"id":"view.zoom","zoom":2}),
            Instant::now()
        )
        .is_err());
        assert_eq!(app.revision, 0);
    }
    #[test]
    fn waits_and_captures_reject_errors_and_changed_revisions() {
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx, None, None).unwrap();
        app.frame = 10;
        app.error = Some("opening failed".into());
        let (reply, rx) = mpsc::channel();
        app.waits.push(Waiting {
            reply,
            since: Instant::now(),
            revision: None,
        });
        app.finish_control(&ctx);
        assert_eq!(rx.try_recv().unwrap().unwrap_err(), "opening failed");
        app.error = None;
        let (reply, rx) = mpsc::channel();
        app.capture = Some(Capture {
            waiting: Waiting {
                reply,
                since: Instant::now(),
                revision: Some(99),
            },
            path: "unused.png".into(),
            issued: None,
            settled: true,
        });
        app.finish_control(&ctx);
        assert!(rx.try_recv().unwrap().is_err());
        assert!(app.capture.is_none());
    }
    #[test]
    fn live_frame_capture_allows_motion_but_keeps_explicit_revision_pins() {
        let ctx = egui::Context::default();
        let mut app = App::new(&ctx, None, None).unwrap();
        app.frame = 10;
        app.revision = 2;
        let (reply, rx) = mpsc::channel();
        app.capture = Some(Capture {
            waiting: Waiting {
                reply,
                since: Instant::now(),
                revision: None,
            },
            path: "unused.png".into(),
            issued: Some(1),
            settled: false,
        });
        app.finish_control(&ctx);
        assert!(app.capture.is_some() && rx.try_recv().is_err());
        app.capture.as_mut().unwrap().waiting.revision = Some(1);
        app.finish_control(&ctx);
        assert!(app.capture.is_none() && rx.try_recv().unwrap().is_err());
    }
    #[test]
    fn startup_picker_is_shown_only_without_a_cli_pdf() {
        let ctx = egui::Context::default();
        assert!(App::new(&ctx, None, None).unwrap().demo_open);
        assert!(
            !App::new(&ctx, Some("example.pdf".into()), None)
                .unwrap()
                .demo_open
        );
    }
}
