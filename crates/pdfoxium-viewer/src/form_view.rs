// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Detached UI snapshots and ordered input for the owning form document thread.
use eframe::egui::{self, Pos2, Rect};
use pdfoxium::{
    thread::{DocEvent, DocumentThread, Ticket},
    FormField, FormInput, FormInputResult, FormKey, FormModifiers, FormState, Matrix, Point,
    RenderConfig, Rotation,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
struct Page {
    fields: Vec<FormField>,
    matrix: Matrix,
    inverse: Matrix,
}
enum InputTicket {
    Plain(Ticket<FormInputResult>),
    Paint(Ticket<(FormInputResult, crate::view::InputPaint)>),
}
impl InputTicket {
    fn take(
        &mut self,
    ) -> Option<pdfoxium::Result<(FormInputResult, Option<crate::view::InputPaint>)>> {
        match self {
            Self::Plain(t) => t.try_take().map(|r| r.map(|r| (r, None))),
            Self::Paint(t) => t.try_take().map(|r| r.map(|(r, p)| (r, Some(p)))),
        }
    }
}
#[derive(Default)]
pub struct State {
    pub dialog: Option<(u64, pdfoxium::FormDialog)>,
    dialog_text: String,
    dialog_focus: bool,
    visible_pages: Vec<usize>,
    current_page: usize,
    actions: Vec<Ticket<()>>,
    pub state: FormState,
    snapshot_dirty: bool,
    pub error: Option<String>,
    pub save_error: Option<String>,
    pub revision: u64,
    pages: BTreeMap<usize, Page>,
    queries: BTreeMap<usize, Ticket<(Vec<FormField>, Matrix)>>,
    inputs: Vec<(InputTicket, Option<u64>)>,
    pub paint_targets: Vec<crate::view::PaintTarget>,
    pub ready_paints: Vec<crate::view::InputPaint>,
    pub full_damage: bool,
    pub layers: Option<pdfoxium::OptionalContentState>,
    pub layout_changed: bool,
    pub document_revision: u64,
    pub typing: crate::typing_trace::Trace,
    metadata_queries: u64,
    capture: Option<usize>,
    last_click: Option<(usize, Pos2, f64)>,
    optimistic: bool,
    target: Option<usize>,
    blurring: bool,
    pub notifications: Vec<pdfoxium::FormEvent>,
    pub saved_path: Option<String>,
    #[cfg(not(target_arch = "wasm32"))]
    saving: Option<Ticket<()>>,
    #[cfg(target_arch = "wasm32")]
    saving: Option<Ticket<Vec<u8>>>,
}
fn modifiers(m: egui::Modifiers, left: bool) -> FormModifiers {
    FormModifiers {
        shift: m.shift,
        control: m.ctrl,
        alt: m.alt,
        meta: m.mac_cmd,
        left_button: left,
    }
}
impl State {
    pub fn answer_dialog(
        &mut self,
        doc: Option<&DocumentThread>,
        id: u64,
        answer: pdfoxium::FormDialogResponse,
    ) -> Result<(), String> {
        if self.dialog.as_ref().map(|d| d.0) != Some(id) {
            return Err("dialog is no longer pending".into());
        }
        doc.ok_or("no form document")?
            .answer_dialog(id, answer)
            .map_err(|e| e.to_string())?;
        self.dialog = None;
        self.dialog_text.clear();
        self.revision += 1;
        Ok(())
    }
    pub fn dialog_ui(&mut self, ctx: &egui::Context, doc: Option<&DocumentThread>) {
        use pdfoxium::{FormDialog as D, FormDialogResponse as R};
        let Some((id, dialog)) = self.dialog.clone() else {
            return;
        };
        let mut answer = None;
        egui::Modal::new(egui::Id::unique("script-dialog")).show(ctx, |ui| {
            ui.set_max_width(480.);
            match &dialog {
                D::Alert {
                    title,
                    message,
                    buttons,
                    ..
                } => {
                    ui.heading(title);
                    ui.label(message);
                    ui.horizontal(|ui| {
                        let choices: &[(&str, R)] = match buttons {
                            0 => &[("OK", R::Ok)],
                            1 => &[("OK", R::Ok), ("Cancel", R::Cancel)],
                            2 => &[("Yes", R::Yes), ("No", R::No)],
                            3 => &[("Yes", R::Yes), ("No", R::No), ("Cancel", R::Cancel)],
                            _ => &[("Cancel", R::Cancel)],
                        };
                        for (index, (label, response)) in choices.iter().enumerate() {
                            let button = ui.button(*label);
                            if self.dialog_focus && index == 0 {
                                button.request_focus();
                            }
                            if button.clicked() {
                                answer = Some(response.clone());
                            }
                        }
                    });
                    if ctx.input(|i| i.key_pressed(egui::Key::Enter)) {
                        answer = Some(if *buttons < 2 { R::Ok } else { R::Yes });
                    } else if *buttons == 0 && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                        answer = Some(R::Ok);
                    }
                }
                D::Response {
                    title,
                    question,
                    label,
                    password,
                    ..
                } => {
                    ui.heading(title);
                    ui.label(question);
                    if !label.is_empty() {
                        ui.label(label);
                    }
                    let edit = ui
                        .add(egui::TextEdit::singleline(&mut self.dialog_text).password(*password));
                    if self.dialog_focus {
                        edit.request_focus();
                    }
                    ui.horizontal(|ui| {
                        if ui.button("OK").clicked()
                            || (edit.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter)))
                        {
                            answer = Some(R::Text(self.dialog_text.clone()));
                        }
                        if ui.button("Cancel").clicked() {
                            answer = Some(R::Cancel);
                        }
                    });
                }
            }
            if dialog.accepts(&R::Cancel) && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                answer = Some(R::Cancel);
            }
        });
        self.dialog_focus = false;
        if let Some(answer) = answer {
            if let Err(error) = self.answer_dialog(doc, id, answer) {
                self.error = Some(error);
            }
            ctx.request_repaint();
        }
    }
    pub fn set_visible_pages(
        &mut self,
        pages: &[usize],
        current: usize,
        doc: Option<&DocumentThread>,
    ) {
        let Some(doc) = doc else {
            return;
        };
        if self.visible_pages == pages && self.current_page == current {
            return;
        }
        self.current_page = current;
        let old = std::mem::replace(&mut self.visible_pages, pages.to_vec());
        let new = pages.to_vec();
        self.actions.push(doc.call(move |document, form| {
            let count = document.page_count()?;
            if current < count {
                form.set_current_page(current)?;
            }
            for page in &old {
                if *page < count && !new.contains(page) {
                    form.page_action(*page, false)?;
                }
            }
            for page in &new {
                if *page < count && !old.contains(page) {
                    form.page_action(*page, true)?;
                }
            }
            Ok(())
        }));
    }
    pub fn page_matrix(&self, page: usize) -> Option<Matrix> {
        self.pages.get(&page).map(|p| p.matrix)
    }
    pub fn normalized_damage(&self, page: usize, rect: pdfoxium::Rect) -> Option<Rect> {
        Some(crate::text::normalized(self.pages.get(&page)?.matrix, rect))
    }
    pub fn focused(&self) -> bool {
        self.state.focus.is_some() || self.optimistic || self.capture.is_some()
    }
    pub fn dirty(&self) -> bool {
        self.state.dirty || self.snapshot_dirty || self.save_error.is_some()
    }
    pub fn mark_snapshot_dirty(&mut self) {
        self.snapshot_dirty = true;
    }
    pub fn is_saving(&self) -> bool {
        self.saving.is_some()
    }
    pub fn busy(&self) -> bool {
        self.dialog.is_some()
            || !self.actions.is_empty()
            || !self.inputs.is_empty()
            || !self.queries.is_empty()
            || self.saving.is_some()
    }
    pub fn request(&mut self, page: usize, doc: Option<&DocumentThread>) {
        let Some(doc) = doc else {
            return;
        };
        if self.pages.contains_key(&page) || self.queries.contains_key(&page) {
            return;
        }
        self.metadata_queries += 1;
        self.queries.insert(
            page,
            doc.call(move |document, form| {
                let fields = form.fields(page)?;
                let matrix = form.page_matrix(page, &RenderConfig::fit(1, 1))?;
                if !form.is_full_xfa() {
                    document.page(page)?.unload()?;
                }
                Ok((fields, matrix))
            }),
        );
    }
    pub fn send(&mut self, page: usize, input: FormInput, doc: &DocumentThread) {
        self.error = None;
        let kind = match &input {
            FormInput::Text(_, _) | FormInput::ReplaceSelection(_) => Some("text"),
            FormInput::MouseDown(_, _) => Some("mouse-down"),
            FormInput::MouseUp(_, _) => Some("mouse-up"),
            _ => None,
        };
        let trace = kind.and_then(|kind| self.typing.begin(kind));
        // Pointer interactions need the same short input-to-paint path as
        // typing. Hover with no invalidation returns without rendering.
        let paint = !self.typing.roundtrip && !self.paint_targets.is_empty();
        let ticket = if paint {
            let targets = self.paint_targets.clone();
            let submitted = trace.map(|_| web_time::Instant::now());
            if let Some(trace) = trace {
                self.typing.queued_input(trace);
            }
            InputTicket::Paint(doc.input_with_paint(
                page,
                input,
                move |_, form, input, generation| {
                    let started = submitted.map(|_| web_time::Instant::now());
                    let mut paint = crate::view::paint_input(form, input, generation, &targets)?;
                    paint.work_ms = started.zip(submitted).map(|(start, submitted)| {
                        (
                            start.duration_since(submitted).as_secs_f64() * 1000.,
                            start.elapsed().as_secs_f64() * 1000.,
                        )
                    });
                    Ok(paint)
                },
            ))
        } else {
            InputTicket::Plain(doc.input(page, input))
        };
        self.inputs.push((ticket, trace));
    }
    pub fn blur(&mut self, doc: Option<&DocumentThread>) {
        if !self.blurring {
            if let (Some(doc), Some(page)) = (
                doc,
                self.state.focus.as_ref().map(|f| f.page).or(self.capture),
            ) {
                self.send(page, FormInput::Blur, doc);
                self.blurring = true;
                self.optimistic = false;
                self.capture = None;
            }
        }
    }
    pub fn escape(&mut self, doc: Option<&DocumentThread>) {
        if let (Some(doc), Some(page)) = (doc, self.state.focus.as_ref().map(|f| f.page)) {
            self.send(
                page,
                FormInput::Text("\u{1b}".into(), Default::default()),
                doc,
            );
            self.send(page, FormInput::Blur, doc);
        }
    }
    pub fn save(
        &mut self,
        path: std::path::PathBuf,
        doc: Option<&DocumentThread>,
    ) -> Result<(), String> {
        let doc = doc.ok_or("no interactive form document")?;
        if self.saving.is_some() {
            return Err("save is already pending".into());
        }
        self.error = None;
        self.save_error = None;
        self.saved_path = Some(path.to_string_lossy().into_owned());
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.saving = Some(doc.save(path));
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.saving = Some(doc.call(|doc, form| {
                form.document_action(pdfoxium::FormDocumentAction::WillSave)?;
                let mut bytes = Vec::new();
                doc.save_to(&mut bytes)?;
                form.document_action(pdfoxium::FormDocumentAction::DidSave)?;
                Ok(bytes)
            }));
        }
        Ok(())
    }
    pub fn poll(&mut self, doc: Option<&DocumentThread>, ctx: &egui::Context) -> bool {
        let Some(doc) = doc else {
            return false;
        };
        self.typing.frame = ctx.cumulative_frame_nr();
        self.actions.retain_mut(|ticket| match ticket.try_take() {
            None => true,
            Some(result) => {
                if let Err(error) = result {
                    self.error = Some(error.to_string());
                }
                false
            }
        });
        let mut visual = false;
        let mut metadata_changed = false;
        for event in doc.take_events() {
            match event {
                DocEvent::OptionalContent(state) => {
                    self.layers = Some(state);
                    self.pages.clear();
                    self.queries.clear();
                    visual = true;
                    self.full_damage = true;
                }
                DocEvent::Dialog { id, dialog } => {
                    self.dialog_focus = true;
                    self.dialog_text = match &dialog {
                        pdfoxium::FormDialog::Response { default, .. } => default.clone(),
                        _ => String::new(),
                    };
                    self.dialog = Some((id, dialog));
                    self.revision += 1;
                }
                DocEvent::Failed(error) => {
                    self.error = Some(error);
                    self.inputs.clear();
                    self.queries.clear();
                }
                DocEvent::Form {
                    visual: changed,
                    events,
                    state,
                    revision,
                    damage_handled,
                    ..
                } => {
                    metadata_changed |= revision != self.document_revision;
                    self.document_revision = revision;
                    visual |= changed && !damage_handled;
                    self.state = state;
                    if self.state.focus.is_none() {
                        self.blurring = false;
                    }
                    if events
                        .iter()
                        .any(|e| matches!(e, pdfoxium::FormEvent::PagesChanged))
                    {
                        self.layout_changed = true;
                        self.full_damage = true;
                        self.pages.clear();
                        self.queries.clear();
                    }
                    self.notifications
                        .extend(events.into_iter().filter(|event| {
                            !damage_handled
                                || !matches!(event, pdfoxium::FormEvent::Invalidate { .. })
                        }));
                    self.revision += 1;
                }
            }
        }
        let mut i = 0;
        while i < self.inputs.len() {
            if let Some(result) = self.inputs[i].0.take() {
                let (_, trace) = self.inputs.swap_remove(i);
                match result {
                    Ok((_, paint)) => {
                        let generation = paint
                            .as_ref()
                            .map_or_else(|| doc.generation(), |p| p.generation);
                        if let Some(trace) = trace {
                            self.typing.ack(
                                trace,
                                generation,
                                paint.as_ref().and_then(|p| p.work_ms),
                            );
                        }
                        if let Some(paint) = paint {
                            self.ready_paints.push(paint);
                        }
                    }
                    Err(error) => {
                        // Input may already have succeeded before a paint
                        // failed. Its event delegated damage to this ticket.
                        visual = true;
                        self.full_damage = true;
                        self.error = Some(error.to_string());
                    }
                }
            } else {
                i += 1;
            }
        }
        let ready: Vec<_> = self
            .queries
            .iter_mut()
            .filter_map(|(&page, ticket)| ticket.try_take().map(|r| (page, r)))
            .collect();
        for (page, result) in ready {
            self.queries.remove(&page);
            match result {
                Ok((fields, matrix)) => match crate::text::inverse(matrix) {
                    Ok(inverse) => {
                        self.pages.insert(
                            page,
                            Page {
                                fields,
                                matrix,
                                inverse,
                            },
                        );
                    }
                    Err(error) => self.error = Some(error),
                },
                Err(error) => self.error = Some(error.to_string()),
            }
            self.revision += 1;
        }
        if let Some(ticket) = self.saving.as_mut() {
            if let Some(result) = ticket.try_take() {
                self.saving = None;
                let result = result.map_err(|e| e.to_string());
                #[cfg(target_arch = "wasm32")]
                let result = result.and_then(|bytes| {
                    crate::web::download(
                        self.saved_path.as_deref().unwrap_or("document.pdf"),
                        &bytes,
                    )
                });
                if result.is_ok() {
                    self.snapshot_dirty = false;
                }
                self.save_error = result.err();
                if let Some(error) = &self.save_error {
                    self.error = Some(error.clone());
                }
                self.revision += 1;
            }
        }
        if self.inputs.is_empty() {
            self.optimistic = false;
        }
        if metadata_changed {
            for (&page, model) in &self.pages {
                let matrix = model.matrix;
                if !self.queries.contains_key(&page) {
                    self.metadata_queries += 1;
                }
                self.queries
                    .entry(page)
                    .or_insert_with(|| doc.call(move |_, form| Ok((form.fields(page)?, matrix))));
            }
        }
        visual
    }
    fn point(
        &self,
        page: usize,
        pos: Pos2,
        rects: &[(usize, Rect)],
        rotation: Rotation,
    ) -> Option<Point> {
        let rect = rects.iter().find(|(p, _)| *p == page)?.1;
        let p = (pos - rect.min) / rect.size();
        let inverse = match rotation {
            Rotation::Cw90 => Rotation::Cw270,
            Rotation::Cw270 => Rotation::Cw90,
            r => r,
        };
        let p = crate::view::rotate_unit(p.to_pos2(), inverse);
        Some(
            self.pages
                .get(&page)?
                .inverse
                .transform(Point { x: p.x, y: p.y }),
        )
    }
    pub fn capturing_pointer(&self) -> bool {
        self.capture.is_some()
    }
    pub fn pointer_on_field(
        &self,
        point: Pos2,
        rects: &[(usize, Rect)],
        rotation: Rotation,
    ) -> bool {
        rects.iter().any(|(page, rect)| {
            rect.contains(point)
                && self.point(*page, point, rects, rotation).is_some_and(|p| {
                    self.pages.get(page).is_some_and(|data| {
                        data.fields.iter().any(|f| {
                            f.visible
                                && p.x >= f.view_rect.left
                                && p.x <= f.view_rect.right
                                && p.y >= f.view_rect.bottom
                                && p.y <= f.view_rect.top
                        })
                    })
                })
        })
    }
    pub fn route(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        rects: &[(usize, Rect)],
        rotation: Rotation,
        doc: Option<&DocumentThread>,
    ) -> bool {
        let Some(doc) = doc.filter(|_| ui.is_enabled()) else {
            return false;
        };
        if self.dialog.is_some() {
            return true;
        }
        if !ui
            .ctx()
            .memory(|m| m.is_above_modal_layer(response.layer_id))
        {
            self.blur(Some(doc));
            return false;
        }
        let events = ui.input(|i| i.events.clone());
        let space_text = events
            .iter()
            .any(|e| matches!(e,egui::Event::Text(s) if s==" "));
        let mut consumed = self.capture.is_some()
            || (self.focused()
                && response.has_focus()
                && events.iter().any(|event| {
                    matches!(
                        event,
                        egui::Event::Key { .. }
                            | egui::Event::Text(_)
                            | egui::Event::Paste(_)
                            | egui::Event::Copy
                            | egui::Event::Cut
                            | egui::Event::Ime(_)
                    )
                }));
        if ui.ctx().egui_wants_keyboard_input() && !response.has_focus() {
            self.blur(Some(doc));
        }
        if self
            .state
            .focus
            .as_ref()
            .is_some_and(|f| !rects.iter().any(|(page, _)| *page == f.page))
        {
            self.blur(Some(doc));
        }
        // ScrollArea consumes smooth deltas after this contents callback. Keep
        // choice-wheel input (including smoothing tails) out of document scrolling.
        let wheel_target = self.state.focus.as_ref().and_then(|f| {
            if !matches!(
                f.kind,
                pdfoxium::WidgetFieldType::ComboBox | pdfoxium::WidgetFieldType::ListBox
            ) {
                return None;
            }
            let pos = ui.input(|i| i.pointer.hover_pos())?;
            if !ui.clip_rect().contains(pos) || ui.ctx().layer_id_at(pos) != Some(response.layer_id)
            {
                return None;
            }
            let point = self.point(f.page, pos, rects, rotation)?;
            (point.x >= f.view_rect.left
                && point.x <= f.view_rect.right
                && point.y >= f.view_rect.bottom
                && point.y <= f.view_rect.top)
                .then_some((f.page, point))
        });
        if wheel_target.is_some() && !ui.input(|i| i.modifiers.ctrl || i.modifiers.command) {
            ui.ctx().input_mut(|i| {
                i.smooth_scroll_delta = egui::Vec2::ZERO;
            });
        }
        for event in events {
            match event {
                egui::Event::MouseWheel {
                    delta,
                    modifiers: m,
                    ..
                } if delta.y != 0.0 && !m.ctrl && !m.command => {
                    if let Some((page, point)) = wheel_target {
                        self.send(
                            page,
                            FormInput::MouseWheel(
                                point,
                                Point {
                                    x: delta.x,
                                    y: delta.y,
                                },
                                modifiers(m, false),
                            ),
                            doc,
                        );
                        consumed = true;
                    }
                }
                egui::Event::PointerMoved(pos) => {
                    if self.capture.is_none()
                        && ui.ctx().layer_id_at(pos) != Some(response.layer_id)
                    {
                        continue;
                    }
                    let page = self.capture.or_else(|| {
                        rects
                            .iter()
                            .find(|(_, r)| r.contains(pos))
                            .map(|(page, _)| *page)
                    });
                    if let Some(page) = page {
                        if let Some(point) = self.point(page, pos, rects, rotation) {
                            let hit = self.pages.get(&page).is_some_and(|p| {
                                p.fields.iter().any(|f| {
                                    f.visible
                                        && point.x >= f.rect.left
                                        && point.x <= f.rect.right
                                        && point.y >= f.rect.bottom
                                        && point.y <= f.rect.top
                                })
                            });
                            if hit || self.capture.is_some() {
                                self.send(
                                    page,
                                    FormInput::MouseMove(
                                        point,
                                        modifiers(
                                            ui.input(|i| i.modifiers),
                                            self.capture.is_some(),
                                        ),
                                    ),
                                    doc,
                                );
                            }
                        }
                    }
                }
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: m,
                } => {
                    if pressed
                        && (!ui.clip_rect().contains(pos)
                            || ui.ctx().layer_id_at(pos) != Some(response.layer_id))
                    {
                        self.blur(Some(doc));
                        continue;
                    }
                    let page = self.capture.or_else(|| {
                        rects
                            .iter()
                            .find(|(_, r)| r.contains(pos))
                            .map(|(page, _)| *page)
                    });
                    if let Some(page) = page {
                        if let Some(point) = self.point(page, pos, rects, rotation) {
                            let hit = self.pages.get(&page).is_some_and(|p| {
                                p.fields.iter().any(|f| {
                                    f.visible
                                        && point.x >= f.rect.left
                                        && point.x <= f.rect.right
                                        && point.y >= f.rect.bottom
                                        && point.y <= f.rect.top
                                })
                            });
                            if pressed {
                                self.send(
                                    page,
                                    FormInput::MouseMove(point, modifiers(m, false)),
                                    doc,
                                );
                                self.send(
                                    page,
                                    FormInput::MouseDown(point, modifiers(m, true)),
                                    doc,
                                );
                                if hit || self.focused() {
                                    self.capture = Some(page);
                                    self.target = Some(page);
                                    self.optimistic = true;
                                    self.blurring = false;
                                    consumed = true;
                                    response.request_focus();
                                }
                            } else if self.capture.is_some() {
                                self.send(
                                    page,
                                    FormInput::MouseUp(point, modifiers(m, false)),
                                    doc,
                                );
                                let (time, clicked) = ui.input(|i| {
                                    (
                                        i.time,
                                        i.pointer.button_clicked(egui::PointerButton::Primary),
                                    )
                                });
                                let options = ui.ctx().options(|o| o.input_options);
                                let double = clicked
                                    && self.last_click.is_some_and(|(p, at, t)| {
                                        p == page
                                            && pos.distance(at) <= options.max_click_dist
                                            && time - t <= options.max_double_click_delay
                                    });
                                self.last_click = if clicked && !double {
                                    Some((page, pos, time))
                                } else {
                                    None
                                };
                                if double {
                                    self.send(
                                        page,
                                        if doc.is_xfa() {
                                            FormInput::DoubleClick(point, modifiers(m, false))
                                        } else {
                                            FormInput::SelectWord
                                        },
                                        doc,
                                    );
                                }
                                self.capture = None;
                                consumed = true;
                            }
                        }
                    }
                }
                egui::Event::Text(value) if self.focused() && response.has_focus() => {
                    if let Some(page) = self
                        .state
                        .focus
                        .as_ref()
                        .map(|f| f.page)
                        .or(self.target.filter(|_| self.optimistic))
                    {
                        self.send(
                            page,
                            FormInput::Text(value, modifiers(ui.input(|i| i.modifiers), false)),
                            doc,
                        );
                    }
                }
                egui::Event::Paste(value) if self.focused() && response.has_focus() => {
                    if let Some(page) = self
                        .state
                        .focus
                        .as_ref()
                        .map(|f| f.page)
                        .or(self.target.filter(|_| self.optimistic))
                    {
                        self.send(page, FormInput::ReplaceSelection(value), doc);
                    }
                }
                egui::Event::Copy if self.focused() && response.has_focus() => {
                    ui.ctx().copy_text(self.state.selected_text.clone())
                }
                egui::Event::Cut if self.focused() && response.has_focus() => {
                    ui.ctx().copy_text(self.state.selected_text.clone());
                    if let Some(page) = self
                        .state
                        .focus
                        .as_ref()
                        .map(|f| f.page)
                        .or(self.target.filter(|_| self.optimistic))
                    {
                        self.send(page, FormInput::ReplaceSelection(String::new()), doc);
                    }
                }
                egui::Event::Ime(egui::ImeEvent::Commit(value))
                    if self.focused() && response.has_focus() =>
                {
                    if let Some(page) = self
                        .state
                        .focus
                        .as_ref()
                        .map(|f| f.page)
                        .or(self.target.filter(|_| self.optimistic))
                    {
                        self.send(page, FormInput::Text(value, Default::default()), doc);
                    }
                }
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers: m,
                    ..
                } if self.focused() && response.has_focus() => {
                    let Some(page) = self
                        .state
                        .focus
                        .as_ref()
                        .map(|f| f.page)
                        .or(self.target.filter(|_| self.optimistic))
                    else {
                        continue;
                    };
                    let mods = modifiers(m, false);
                    if m.command && key == egui::Key::A {
                        self.send(page, FormInput::SelectAll, doc);
                        continue;
                    }
                    if m.command && key == egui::Key::Z {
                        self.send(
                            page,
                            if m.shift {
                                FormInput::Redo
                            } else {
                                FormInput::Undo
                            },
                            doc,
                        );
                        continue;
                    }
                    let key = match key {
                        egui::Key::Space if !space_text => {
                            self.send(page, FormInput::Text(" ".into(), mods), doc);
                            continue;
                        }
                        egui::Key::Tab => FormKey::Tab,
                        egui::Key::ArrowLeft => FormKey::Left,
                        egui::Key::ArrowRight => FormKey::Right,
                        egui::Key::ArrowUp => FormKey::Up,
                        egui::Key::ArrowDown => FormKey::Down,
                        egui::Key::Home => FormKey::Home,
                        egui::Key::End => FormKey::End,
                        egui::Key::Delete => FormKey::Delete,
                        egui::Key::Backspace => {
                            self.send(page, FormInput::Text("\u{8}".into(), mods), doc);
                            continue;
                        }
                        egui::Key::Enter => {
                            self.send(page, FormInput::Text("\r".into(), mods), doc);
                            continue;
                        }
                        _ => continue,
                    };
                    self.send(page, FormInput::KeyDown(key, mods), doc);
                }
                _ => {}
            }
        }
        if self.focused() && response.has_focus() {
            ui.ctx().memory_mut(|m| {
                m.set_focus_lock_filter(
                    response.id,
                    egui::EventFilter {
                        tab: true,
                        horizontal_arrows: true,
                        vertical_arrows: true,
                        escape: true,
                    },
                )
            });
            if let Some(focus) = &self.state.focus {
                if let Some((_, page)) = rects.iter().find(|(p, _)| *p == focus.page) {
                    if let Some(model) = self.pages.get(&focus.page) {
                        let r = crate::view::text_screen_rect(
                            *page,
                            crate::text::normalized(model.matrix, focus.rect),
                            rotation,
                        );
                        ui.ctx().output_mut(|o| {
                            o.ime = Some(egui::output::IMEOutput {
                                rect: r,
                                cursor_rect: r,
                                purpose: egui::IMEPurpose::Normal,
                                should_interrupt_composition: false,
                            })
                        });
                    }
                }
            }
        }
        consumed
    }
    pub fn snapshot(
        &self,
        doc: Option<&DocumentThread>,
        rects: &[(usize, Rect)],
        rotation: Rotation,
    ) -> Value {
        let field = |f: &FormField| {
            let screen = self.pages.get(&f.page).and_then(|model| {
                rects
                    .iter()
                    .find(|(page, _)| *page == f.page)
                    .map(|(_, page)| {
                        crate::view::text_screen_rect(
                            *page,
                            crate::text::normalized(model.matrix, f.rect),
                            rotation,
                        )
                    })
            });
            json!({"page":f.page+1,"index":f.index,"name":f.name,"kind":format!("{:?}",f.kind),"value":f.value,"checked":f.checked,"readonly":f.flags&1!=0,"visible":f.visible,"rect":[f.rect.left,f.rect.bottom,f.rect.right,f.rect.top],"screen_rect":screen.map(|r|[r.min.x,r.min.y,r.width(),r.height()])})
        };
        let dialog = self.dialog.as_ref().map(|(id, dialog)| match dialog {
            pdfoxium::FormDialog::Alert { message, title, buttons, icon } => json!({"id":id,"kind":"alert","message":message,"title":title,"buttons":buttons,"icon":icon}),
            pdfoxium::FormDialog::Response { question, title, default, label, password } => json!({"id":id,"kind":"response","question":question,"title":title,"default":default,"label":label,"password":password}),
        });
        json!({"enabled":doc.is_some(),"typing_latency":self.typing.snapshot(),"metadata_queries":self.metadata_queries,"javascript":doc.is_some_and(DocumentThread::javascript_enabled),"dialog":dialog,"pending":self.busy(),"dirty":self.dirty(),"save_error":self.save_error,"focus":self.state.focus.as_ref().map(field),"text":self.state.text,"selected_text":self.state.selected_text,"fields":self.pages.values().flat_map(|p|p.fields.iter()).map(field).collect::<Vec<_>>(),"error":self.error,"saved_path":self.saved_path,"saving":self.saving.is_some()})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_edits_keep_their_dirty_state_separate_from_live_form_events() {
        let mut forms = State::default();
        assert!(!forms.dirty());
        forms.mark_snapshot_dirty();
        assert!(forms.dirty());
        forms.state = FormState::default();
        assert!(forms.dirty());
        forms.snapshot_dirty = false;
        assert!(!forms.dirty());
        forms.state.dirty = true; // A post-save script can modify the document.
        assert!(forms.dirty());
    }
}
