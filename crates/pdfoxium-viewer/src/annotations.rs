// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Annotation snapshots and edits on the same document worker as forms/rendering.
use eframe::egui::{self, Rect};
use pdfoxium::{
    thread::{DocumentThread, Ticket},
    AnnotId, AnnotationKind, AnnotationSpec, AnnotationStyle, Color, Document, Point, RenderConfig,
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, ops::Range};

fn kind_label(kind: AnnotationKind) -> &'static str {
    match kind {
        AnnotationKind::FreeText => "Text box",
        AnnotationKind::Text => "Sticky note",
        AnnotationKind::Square => "Rectangle",
        AnnotationKind::Circle => "Ellipse",
        AnnotationKind::Highlight => "Highlight",
        AnnotationKind::Underline => "Underline",
        AnnotationKind::StrikeOut => "Strikeout",
        AnnotationKind::Squiggly => "Squiggly underline",
        AnnotationKind::Ink => "Ink",
        _ => "Annotation",
    }
}
fn editable(kind: AnnotationKind) -> bool {
    matches!(
        kind,
        AnnotationKind::Text
            | AnnotationKind::FreeText
            | AnnotationKind::Highlight
            | AnnotationKind::Underline
            | AnnotationKind::Squiggly
            | AnnotationKind::StrikeOut
            | AnnotationKind::Square
            | AnnotationKind::Circle
            | AnnotationKind::Ink
    )
}
#[derive(Clone, Debug)]
pub struct Hit {
    pub id: AnnotId,
    pub index: usize,
    pub kind: AnnotationKind,
    pub rect: Rect,
    pub regions: Vec<Rect>,
}
#[derive(Clone)]
struct Target {
    page: usize,
    hit: Hit,
    live: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Text,
    Rectangle,
    Ellipse,
}
impl Tool {
    #[cfg(test)]
    pub const ALL: [Self; 3] = [Self::Text, Self::Rectangle, Self::Ellipse];
    pub fn label(self) -> &'static str {
        match self {
            Self::Text => "Text",
            Self::Rectangle => "Rectangle",
            Self::Ellipse => "Ellipse",
        }
    }
    pub fn command(self) -> &'static str {
        match self {
            Self::Text => "annotation.text",
            Self::Rectangle => "annotation.rectangle",
            Self::Ellipse => "annotation.ellipse",
        }
    }
}
#[derive(Clone)]
struct Creation {
    page: usize,
    anchor: egui::Pos2,
    rect: Rect,
    min_size: egui::Vec2,
    tool: Tool,
    style: AnnotationStyle,
    epoch: u64,
}
struct Creating {
    ticket: Ticket<Hit>,
    creation: Creation,
}
fn pdf_bounds(inverse: pdfoxium::Matrix, rect: Rect) -> pdfoxium::Rect {
    let points = [
        rect.left_top(),
        rect.right_top(),
        rect.right_bottom(),
        rect.left_bottom(),
    ]
    .map(|p| inverse.transform(Point { x: p.x, y: p.y }));
    pdfoxium::Rect {
        left: points.iter().map(|p| p.x).fold(f32::INFINITY, f32::min),
        bottom: points.iter().map(|p| p.y).fold(f32::INFINITY, f32::min),
        right: points.iter().map(|p| p.x).fold(f32::NEG_INFINITY, f32::max),
        top: points.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max),
    }
}
pub fn resizable(kind: AnnotationKind) -> bool {
    matches!(
        kind,
        AnnotationKind::FreeText | AnnotationKind::Square | AnnotationKind::Circle
    )
}
#[derive(Clone, Copy, Debug)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomRight,
    BottomLeft,
}
impl Corner {
    pub const ALL: [Self; 4] = [
        Self::TopLeft,
        Self::TopRight,
        Self::BottomRight,
        Self::BottomLeft,
    ];
    pub fn point(self, r: Rect) -> egui::Pos2 {
        match self {
            Self::TopLeft => r.left_top(),
            Self::TopRight => r.right_top(),
            Self::BottomRight => r.right_bottom(),
            Self::BottomLeft => r.left_bottom(),
        }
    }
}
#[derive(Clone, Copy)]
enum Gesture {
    Move,
    Resize {
        corner: Corner,
        min_size: egui::Vec2,
    },
}
fn resized_rect(from: Rect, corner: Corner, delta: egui::Vec2, min_size: egui::Vec2) -> Rect {
    let min_size = min_size.min(from.size());
    let mut r = from;
    match corner {
        Corner::TopLeft | Corner::BottomLeft => {
            r.min.x = (from.min.x + delta.x).clamp(
                0.0_f32.min(from.min.x),
                (from.max.x - min_size.x).max(0.0_f32.min(from.min.x)),
            )
        }
        _ => {
            r.max.x = (from.max.x + delta.x).clamp(
                (from.min.x + min_size.x).min(1.0_f32.max(from.max.x)),
                1.0_f32.max(from.max.x),
            )
        }
    }
    match corner {
        Corner::TopLeft | Corner::TopRight => {
            r.min.y = (from.min.y + delta.y).clamp(
                0.0_f32.min(from.min.y),
                (from.max.y - min_size.y).max(0.0_f32.min(from.min.y)),
            )
        }
        _ => {
            r.max.y = (from.max.y + delta.y).clamp(
                (from.min.y + min_size.y).min(1.0_f32.max(from.max.y)),
                1.0_f32.max(from.max.y),
            )
        }
    }
    r
}
#[derive(Clone)]
struct Drag {
    gesture: Gesture,
    target: Target,
    anchor: egui::Pos2,
    from: Rect,
    to: Rect,
    epoch: u64,
}
type MoveTicket = Ticket<(AnnotId, u64, Rect)>;
struct MovePreview {
    page: usize,
    rect: Rect,
    epoch: u64,
    revision: u64,
}
fn translated_note(rect: Rect, delta: egui::Vec2) -> Rect {
    let axis = |min: f32, max: f32, d: f32| {
        if max - min <= 1.0 {
            d.clamp(-min, 1.0 - max)
        } else {
            0.0
        }
    };
    rect.translate(egui::vec2(
        axis(rect.min.x, rect.max.x, delta.x),
        axis(rect.min.y, rect.max.y, delta.y),
    ))
}
struct Row {
    id: AnnotId,
    index: usize,
    kind: AnnotationKind,
    contents: String,
    rect: Rect,
    regions: Vec<Rect>,
    flags: i32,
    style: Option<AnnotationStyle>,
}
fn rows(doc: &Document, index: usize) -> pdfoxium::Result<Vec<Row>> {
    let page = doc.page(index)?;
    // Most pages have no editable/static annotations. Inspect dictionaries first
    // so hit-test prefetch does not parse page content just for an empty result.
    if page.annotations()?.iter().all(|a| {
        matches!(
            a.subtype.as_slice(),
            b"Widget" | b"Link" | b"Popup" | b"Screen"
        )
    }) {
        return Ok(Vec::new());
    }
    let matrix = page.render_matrix(&RenderConfig::fit(1, 1))?;
    let result = (|| {
        page.annotation_ids()?
            .into_iter()
            .enumerate()
            .filter_map(|(index, id)| {
                let result = (|| {
                    let details = page.annotation(id)?;
                    if matches!(
                        details.kind,
                        AnnotationKind::Widget
                            | AnnotationKind::Link
                            | AnnotationKind::Popup
                            | AnnotationKind::Screen
                    ) {
                        return Ok(None);
                    }
                    let regions = if details.quads.is_empty() {
                        vec![crate::text::normalized(matrix, details.rect)]
                    } else {
                        details
                            .quads
                            .iter()
                            .map(|quad| {
                                let rect = pdfoxium::Rect {
                                    left: quad.iter().map(|p| p.x).fold(f32::INFINITY, f32::min),
                                    right: quad
                                        .iter()
                                        .map(|p| p.x)
                                        .fold(f32::NEG_INFINITY, f32::max),
                                    bottom: quad.iter().map(|p| p.y).fold(f32::INFINITY, f32::min),
                                    top: quad.iter().map(|p| p.y).fold(f32::NEG_INFINITY, f32::max),
                                };
                                crate::text::normalized(matrix, rect)
                            })
                            .collect()
                    };
                    Ok(Some(Row {
                        id,
                        index,
                        regions,
                        flags: details.flags,
                        style: page.annotation_style(id)?,
                        kind: details.kind,
                        contents: page.annotation_string(id, b"Contents")?,
                        rect: crate::text::normalized(matrix, details.rect),
                    }))
                })();
                match result {
                    Ok(None) => None,
                    Ok(Some(row)) => Some(Ok(row)),
                    Err(e) => Some(Err(e)),
                }
            })
            .collect()
    })();
    page.unload()?;
    result
}
pub fn hit_regions(doc: &Document, page: usize) -> pdfoxium::Result<Vec<Hit>> {
    let handle = doc.page(page)?;
    let mut visible = Vec::new();
    for row in rows(doc, page)? {
        if row.flags & 35 == 0 && handle.annotation_layer_visible(row.id)? {
            visible.push(row);
        }
    }
    Ok(visible
        .into_iter()
        .map(|row| Hit {
            id: row.id,
            index: row.index,
            kind: row.kind,
            rect: row.rect,
            regions: row.regions,
        })
        .collect())
}
#[derive(Default)]
pub struct State {
    defaults: crate::annotation_style::Defaults,
    selected_style: Option<AnnotationStyle>,
    pub open: bool,
    pub placing_note: bool,
    pub tool: Option<Tool>,
    creation: Option<Creation>,
    pending_creation: Option<Creation>,
    creating: Option<Creating>,
    focus_text: bool,
    pub error: Option<String>,
    page: Option<usize>,
    revision: u64,
    rows: Vec<Row>,
    query: Option<Ticket<(usize, u64, Vec<Row>)>>,
    edits: Vec<Ticket<()>>,
    selected: Option<AnnotId>,
    selected_page: Option<usize>,
    pending_select: Option<Target>,
    pub hits: BTreeMap<usize, Vec<Hit>>,
    hit_queries: BTreeMap<usize, Ticket<Vec<Hit>>>,
    hit_revision: Option<u64>,
    pub controls: Vec<(&'static str, &'static str, Rect)>,
    contents: String,
    note_text: String,
    selection_epoch: u64,
    drag: Option<Drag>,
    cancelled_drag: bool,
    pending_move: Option<Drag>,
    moving: Option<(MoveTicket, Drag)>,
    applied_move: Option<MovePreview>,
}
impl State {
    pub fn layers_changed(&mut self) {
        self.cancel_drag();
        self.deselect();
        self.hits.clear();
        self.hit_queries.clear();
        self.hit_revision = None;
    }
    pub fn dragging(&self) -> bool {
        self.drag.is_some() || self.creation.is_some()
    }
    pub fn is_moving(&self) -> bool {
        self.dragging()
            || self.pending_creation.is_some()
            || self.creating.is_some()
            || self.pending_move.is_some()
            || self.moving.is_some()
            || self.applied_move.is_some()
    }
    pub fn cancel_drag(&mut self) {
        let creating = self.creation.take().is_some();
        if self.drag.take().is_some() || creating {
            self.cancelled_drag = true;
        }
    }
    pub fn drag_cancelled(&self) -> bool {
        self.cancelled_drag
    }
    pub fn pointer_released(&mut self) {
        self.cancelled_drag = false;
    }
    pub fn abort_unstarted_moves(&mut self) {
        self.drag = None;
        self.pending_move = None;
        self.creation = None;
        self.pending_creation = None;
        self.applied_move = None;
    }
    pub fn resize_corner(&self) -> Option<Corner> {
        match self.drag.as_ref()?.gesture {
            Gesture::Resize { corner, .. } => Some(corner),
            Gesture::Move => None,
        }
    }
    pub fn drag_page(&self) -> Option<usize> {
        self.creation
            .as_ref()
            .map(|c| c.page)
            .or_else(|| self.drag.as_ref().map(|d| d.target.page))
    }
    pub fn start_tool(&mut self, tool: Tool) {
        self.deselect();
        self.placing_note = false;
        self.tool = Some(tool);
        self.open = true;
        self.error = None;
    }
    pub fn cancel_tools(&mut self) {
        self.cancel_drag();
        self.tool = None;
        self.placing_note = false;
    }
    pub fn begin_creation(
        &mut self,
        page: usize,
        point: egui::Pos2,
        min_size: egui::Vec2,
    ) -> Result<(), String> {
        if self.is_moving() {
            return Err("wait for the current annotation edit".into());
        }
        let tool = self.tool.ok_or("choose a drawing tool")?;
        if !point.is_finite() || !min_size.is_finite() || min_size.min_elem() <= 0. {
            return Err("invalid drawing coordinates".into());
        }
        self.deselect();
        self.creation = Some(Creation {
            page,
            anchor: point,
            rect: Rect::from_min_max(point, point),
            min_size,
            tool,
            style: if tool == Tool::Text {
                self.defaults.text
            } else {
                self.defaults.shape
            },
            epoch: self.selection_epoch,
        });
        Ok(())
    }
    fn poll_creation(&mut self, doc: &DocumentThread) {
        if let Some(result) = self.creating.as_mut().and_then(|c| c.ticket.try_take()) {
            let creation = self.creating.take().unwrap().creation;
            match result {
                Ok(hit) if creation.epoch == self.selection_epoch => {
                    self.pending_select = Some(Target {
                        page: creation.page,
                        hit,
                        live: true,
                    });
                    self.focus_text = creation.tool == Tool::Text;
                }
                Ok(_) => {}
                Err(e) => self.error = Some(e.to_string()),
            }
        }
        if self.creating.is_none() {
            if let Some(creation) = self.pending_creation.take() {
                let c = creation.clone();
                let ticket = doc.call(move |doc, form| {
                    let page = doc.page(c.page)?;
                    let matrix = page.render_matrix(&RenderConfig::fit(1, 1))?;
                    let inverse = crate::text::inverse(matrix).map_err(|_| {
                        pdfoxium::Error::InvalidArgument("page transform is not invertible")
                    })?;
                    let rect = pdf_bounds(inverse, c.rect);
                    let spec = match (c.tool, c.style) {
                        (Tool::Text, AnnotationStyle::FreeText { font_size, color }) => {
                            AnnotationSpec::free_text(rect, "Text", font_size).color(color)
                        }
                        (
                            Tool::Rectangle | Tool::Ellipse,
                            AnnotationStyle::Shape {
                                border_color,
                                border_width,
                                fill,
                            },
                        ) => {
                            let spec = if c.tool == Tool::Rectangle {
                                AnnotationSpec::square(rect)
                            } else {
                                AnnotationSpec::circle(rect)
                            };
                            let spec = spec.color(border_color).border_width(border_width);
                            if let Some(fill) = fill {
                                spec.fill(fill)
                            } else {
                                spec
                            }
                        }
                        _ => {
                            return Err(pdfoxium::Error::Internal(
                                "drawing style does not match tool",
                            ))
                        }
                    };
                    let id = form.add_annotation(c.page, spec)?;
                    let details = page.annotation(id)?;
                    let rect = crate::text::normalized(matrix, details.rect);
                    let index = page
                        .annotation_ids()?
                        .iter()
                        .position(|a| *a == id)
                        .ok_or(pdfoxium::Error::Stale)?;
                    let hit = Hit {
                        id,
                        index,
                        kind: details.kind,
                        rect,
                        regions: vec![rect],
                    };
                    page.unload()?;
                    Ok(hit)
                });
                self.creating = Some(Creating { ticket, creation });
            }
        }
    }
    pub fn selected_box_hit(&self) -> Option<(usize, Hit)> {
        let id = self.selected?;
        let row = self.rows.iter().find(|r| r.id == id && resizable(r.kind))?;
        Some((
            self.selected_page?,
            Hit {
                id: row.id,
                index: row.index,
                kind: row.kind,
                rect: row.rect,
                regions: row.regions.clone(),
            },
        ))
    }
    pub fn movable_hit(&self, hit: &Hit) -> bool {
        hit.kind == AnnotationKind::Text || (resizable(hit.kind) && self.selected == Some(hit.id))
    }
    pub fn begin_drag(
        &mut self,
        page: usize,
        hit: Hit,
        live: bool,
        point: egui::Pos2,
    ) -> Result<(), String> {
        if !self.movable_hit(&hit) {
            return Err("select a box before dragging it".into());
        }
        self.begin_gesture(page, hit, live, point, Gesture::Move)
    }
    pub fn resize_bounds(&self) -> Option<(usize, Rect)> {
        let id = self.selected?;
        let row = self.rows.iter().find(|r| {
            r.id == id
                && resizable(r.kind)
                && r.rect.is_finite()
                && r.rect.is_positive()
                && r.rect.size().is_finite()
        })?;
        Some(
            self.move_preview()
                .map(|(p, r)| (p, *r))
                .unwrap_or((self.selected_page?, row.rect)),
        )
    }
    pub fn begin_resize(
        &mut self,
        corner: Corner,
        point: egui::Pos2,
        min_size: egui::Vec2,
    ) -> Result<(), String> {
        let id = self
            .selected
            .ok_or("select an annotation before resizing")?;
        let row = self
            .rows
            .iter()
            .find(|r| r.id == id)
            .ok_or("selected annotation is no longer present")?;
        if !resizable(row.kind) {
            return Err("this annotation cannot be resized".into());
        }
        if !min_size.is_finite() || min_size.min_elem() <= 0. {
            return Err("resize requires finite positive dimensions".into());
        }
        let hit = Hit {
            id: row.id,
            index: row.index,
            kind: row.kind,
            rect: row.rect,
            regions: row.regions.clone(),
        };
        self.begin_gesture(
            self.selected_page.ok_or("no selected page")?,
            hit,
            true,
            point,
            Gesture::Resize { corner, min_size },
        )
    }
    fn begin_gesture(
        &mut self,
        page: usize,
        hit: Hit,
        live: bool,
        point: egui::Pos2,
        gesture: Gesture,
    ) -> Result<(), String> {
        if self.is_moving() {
            return Err("wait for the current annotation edit".into());
        }
        if !hit.rect.is_finite()
            || !hit.rect.is_positive()
            || !hit.rect.size().is_finite()
            || !point.is_finite()
        {
            return Err("invalid annotation bounds".into());
        }
        let open = self.open;
        self.select_hit(page, hit.clone(), live);
        self.open = open; // Avoid changing layout under the pointer mid-gesture.
        self.drag = Some(Drag {
            gesture,
            target: Target {
                page,
                hit: hit.clone(),
                live,
            },
            anchor: point,
            from: hit.rect,
            to: hit.rect,
            epoch: self.selection_epoch,
        });
        Ok(())
    }
    pub fn update_drag(&mut self, point: egui::Pos2) {
        if let Some(c) = &mut self.creation {
            if point.is_finite() {
                c.rect = Rect::from_two_pos(c.anchor, point);
            }
        }
        if let Some(drag) = &mut self.drag {
            if point.is_finite() {
                drag.to = match drag.gesture {
                    Gesture::Move => translated_note(drag.from, point - drag.anchor),
                    Gesture::Resize { corner, min_size } => {
                        resized_rect(drag.from, corner, point - drag.anchor, min_size)
                    }
                };
            }
        }
    }
    pub fn finish_drag(&mut self, doc: Option<&DocumentThread>) {
        if let Some(c) = self.creation.take() {
            if c.rect.width() >= c.min_size.x && c.rect.height() >= c.min_size.y {
                self.pending_creation = Some(c);
                self.tool = None;
            }
        }
        if let Some(drag) = self.drag.take() {
            if (drag.to.min - drag.from.min).length_sq() + (drag.to.max - drag.from.max).length_sq()
                > 1e-12
            {
                self.pending_move = Some(drag);
            }
        }
        if let Some(doc) = doc {
            self.poll_creation(doc);
            self.poll_move(doc);
        }
    }
    pub fn move_preview(&self) -> Option<(usize, &Rect)> {
        if let Some(c) = self
            .creation
            .as_ref()
            .or(self.pending_creation.as_ref())
            .or_else(|| self.creating.as_ref().map(|c| &c.creation))
            .filter(|c| c.epoch == self.selection_epoch)
        {
            return Some((c.page, &c.rect));
        }
        if let Some(d) = self
            .drag
            .as_ref()
            .or(self.pending_move.as_ref())
            .filter(|d| d.epoch == self.selection_epoch)
        {
            return Some((d.target.page, &d.to));
        }
        if let Some((_, d)) = self
            .moving
            .as_ref()
            .filter(|(_, d)| d.epoch == self.selection_epoch)
        {
            return Some((d.target.page, &d.to));
        }
        self.applied_move
            .as_ref()
            .filter(|m| m.epoch == self.selection_epoch)
            .map(|m| (m.page, &m.rect))
    }
    fn poll_move(&mut self, doc: &DocumentThread) {
        if let Some(result) = self.moving.as_mut().and_then(|(t, _)| t.try_take()) {
            let (_, drag) = self.moving.take().unwrap();
            match result {
                Ok((id, revision, rect)) if drag.epoch == self.selection_epoch => {
                    let mut hit = drag.target.hit;
                    hit.id = id;
                    hit.rect = rect;
                    hit.regions = vec![rect];
                    self.pending_select = Some(Target {
                        page: drag.target.page,
                        hit,
                        live: true,
                    });
                    self.applied_move = Some(MovePreview {
                        page: drag.target.page,
                        rect,
                        epoch: drag.epoch,
                        revision,
                    });
                }
                Ok(_) => {}
                Err(e) => {
                    self.error = Some(e.to_string());
                    self.applied_move = None;
                }
            }
        }
        if self.moving.is_none() {
            if let Some(drag) = self.pending_move.take() {
                if drag.epoch == self.selection_epoch {
                    self.pending_select = None;
                }
                let target = drag.target.clone();
                let from = drag.from;
                let delta = drag.to.min - drag.from.min;
                let to = drag.to;
                let gesture = drag.gesture;
                let ticket = doc.call(move |doc, form| {
                    form.commit()?;
                    let page = doc.page(target.page)?;
                    let id = if target.live {
                        target.hit.id
                    } else {
                        *page
                            .annotation_ids()?
                            .get(target.hit.index)
                            .ok_or(pdfoxium::Error::Stale)?
                    };
                    let details = page.annotation(id)?;
                    let matrix = page.render_matrix(&RenderConfig::fit(1, 1))?;
                    let current = crate::text::normalized(matrix, details.rect);
                    if details.kind != target.hit.kind
                        || (current.min - from.min).abs().max_elem() > 1e-5
                        || (current.max - from.max).abs().max_elem() > 1e-5
                    {
                        return Err(pdfoxium::Error::InvalidArgument(
                            "annotation changed during the drag",
                        ));
                    }
                    let inverse = crate::text::inverse(matrix).map_err(|_| {
                        pdfoxium::Error::InvalidArgument("page transform is not invertible")
                    })?;
                    match gesture {
                        Gesture::Move => {
                            let a = inverse.transform(Point { x: 0., y: 0. });
                            let b = inverse.transform(Point {
                                x: delta.x,
                                y: delta.y,
                            });
                            form.move_annotation(
                                target.page,
                                id,
                                Point {
                                    x: b.x - a.x,
                                    y: b.y - a.y,
                                },
                            )?;
                        }
                        Gesture::Resize { .. } => {
                            // Crop and intrinsic page rotation are in this matrix;
                            // transforming all corners avoids inverted PDF edges.
                            let rect = pdf_bounds(inverse, to);
                            form.resize_annotation(target.page, id, rect)?;
                        }
                    }
                    let rect = crate::text::normalized(matrix, page.annotation(id)?.rect);
                    page.unload()?;
                    Ok((id, doc.revision(), rect))
                });
                self.moving = Some((ticket, drag));
            }
        }
    }
    pub fn context_page(&self, current: usize) -> usize {
        if let Some((page, _)) = self.move_preview() {
            return page;
        }
        self.pending_select
            .as_ref()
            .map(|t| t.page)
            .or(self.selected_page)
            .unwrap_or(current)
    }
    pub fn has_selection(&self) -> bool {
        self.selected.is_some() || self.pending_select.is_some() || self.move_preview().is_some()
    }
    pub fn deselect(&mut self) {
        self.selection_epoch = self.selection_epoch.wrapping_add(1);
        self.cancel_drag();
        self.applied_move = None;
        self.selected = None;
        self.selected_page = None;
        self.pending_select = None;
        self.contents.clear();
        self.selected_style = None;
        self.focus_text = false;
    }
    pub fn select_hit(&mut self, page: usize, hit: Hit, live: bool) {
        self.error = None;
        self.deselect();
        if live && self.page == Some(page) {
            if let Some(row) = self.rows.iter().find(|r| r.id == hit.id) {
                self.selected = Some(row.id);
                self.selected_page = Some(page);
                self.contents = row.contents.clone();
                self.selected_style = row.style;
            } else {
                self.pending_select = Some(Target { page, hit, live });
            }
        } else {
            self.pending_select = Some(Target { page, hit, live });
        }
        self.open = true;
        self.placing_note = false;
        self.tool = None;
    }
    pub fn selected_regions(&self) -> Option<(usize, &[Rect])> {
        if let Some((page, rect)) = self.move_preview() {
            return Some((page, std::slice::from_ref(rect)));
        }
        let id = self.selected?;
        Some((
            self.selected_page?,
            &self.rows.iter().find(|r| r.id == id)?.regions,
        ))
    }
    pub fn delete_selected(&mut self, doc: &DocumentThread) -> Result<(), String> {
        let id = self.selected.ok_or("no annotation selected")?;
        let row = self
            .rows
            .iter()
            .find(|r| r.id == id)
            .ok_or("selected annotation is no longer present")?;
        if !editable(row.kind) {
            return Err("this annotation is read-only".into());
        }
        self.edit(doc, row.index, None)
    }
    pub fn poll_hits(&mut self, doc: Option<&DocumentThread>, visible: &[usize], revision: u64) {
        let Some(doc) = doc else {
            return;
        };
        if self.hit_revision != Some(revision) {
            self.hits.clear();
            self.hit_queries.clear();
            self.hit_revision = Some(revision);
        }
        self.hits.retain(|page, _| visible.contains(page));
        self.hit_queries.retain(|page, _| visible.contains(page));
        let mut finished = Vec::new();
        for (page, ticket) in &mut self.hit_queries {
            if let Some(result) = ticket.try_take() {
                match result {
                    Ok(hits) => {
                        self.hits.insert(*page, hits);
                    }
                    Err(e) => {
                        self.error = Some(e.to_string());
                        self.hits.insert(*page, Vec::new());
                    }
                }
                finished.push(*page);
            }
        }
        for page in finished {
            self.hit_queries.remove(&page);
        }
        for &page in visible {
            if !self.hits.contains_key(&page) && !self.hit_queries.contains_key(&page) {
                self.hit_queries
                    .insert(page, doc.call(move |doc, _| hit_regions(doc, page)));
            }
        }
    }
    pub fn has_page(&self, page: usize) -> bool {
        self.page == Some(page)
    }
    pub fn busy(&self) -> bool {
        self.pending_creation.is_some()
            || self.creating.is_some()
            || self.pending_move.is_some()
            || self.moving.is_some()
            || self.applied_move.is_some()
            || self.query.is_some()
            || !self.edits.is_empty()
            || !self.hit_queries.is_empty()
            || self.pending_select.is_some()
    }
    pub fn poll(&mut self, doc: Option<&DocumentThread>, page: usize, revision: u64) {
        let Some(doc) = doc else {
            return;
        };
        self.poll_creation(doc);
        self.poll_move(doc);
        let page = self.context_page(page);
        self.edits.retain_mut(|t| match t.try_take() {
            None => true,
            Some(result) => {
                if let Err(e) = result {
                    self.error = Some(e.to_string());
                }
                false
            }
        });
        if let Some(result) = self.query.as_mut().and_then(Ticket::try_take) {
            self.query = None;
            match result {
                Ok((page, revision, rows)) => {
                    self.page = Some(page);
                    self.revision = revision;
                    self.rows = rows;
                    if self
                        .applied_move
                        .as_ref()
                        .is_some_and(|m| m.page == page && revision >= m.revision)
                    {
                        self.applied_move = None;
                    }
                    if self.pending_select.as_ref().is_some_and(|t| t.page == page) {
                        let target = self.pending_select.take().unwrap();
                        let selected = self.rows.iter().find(|r| {
                            if target.live {
                                r.id == target.hit.id
                            } else {
                                r.index == target.hit.index
                                    && r.kind == target.hit.kind
                                    && r.rect == target.hit.rect
                            }
                        });
                        if let Some(row) = selected {
                            self.selected = Some(row.id);
                            self.selected_page = Some(page);
                            self.contents = row.contents.clone();
                            self.selected_style = row.style;
                        } else {
                            self.error =
                                Some("annotation changed before selection completed".into());
                        }
                    }
                    if self
                        .selected
                        .is_some_and(|id| !self.rows.iter().any(|r| r.id == id))
                    {
                        self.deselect();
                    }
                }
                Err(e) => {
                    self.error = Some(e.to_string());
                    self.deselect();
                    self.open = false;
                }
            }
        }
        if (self.open
            || self.pending_select.is_some()
            || self.drag.is_some()
            || self.selected.is_some()
            || self.applied_move.is_some())
            && self.query.is_none()
            && (self.page != Some(page)
                || self.revision != revision
                || self.pending_select.is_some()
                || self
                    .applied_move
                    .as_ref()
                    .is_some_and(|m| self.revision < m.revision))
        {
            self.query = Some(doc.call(move |doc, _| Ok((page, doc.revision(), rows(doc, page)?))));
        }
    }
    pub fn markup(
        &mut self,
        doc: &DocumentThread,
        kind: &str,
        selection: &crate::text::Selection,
    ) -> Result<(), String> {
        if selection.pages.is_empty() {
            return Err("select text first".into());
        }
        if !matches!(kind, "highlight" | "underline" | "strikeout") {
            return Err("unknown markup kind".into());
        }
        let kind = kind.to_owned();
        let highlight = self.defaults.highlight;
        let ranges: Vec<(usize, Range<usize>)> = selection
            .pages
            .iter()
            .map(|p| (p.page, p.range.clone()))
            .collect();
        self.error = None;
        self.edits.push(doc.call(move |doc, form| {
            for (index, range) in ranges {
                let page = doc.page(index)?;
                let rects = crate::annotation_geometry::markup_rects(&page.text()?, range)?;
                if !rects.is_empty() {
                    let spec = match kind.as_str() {
                        "underline" => {
                            AnnotationSpec::underline(rects).color(Color::rgb(200, 40, 40))
                        }
                        "strikeout" => {
                            AnnotationSpec::strikeout(rects).color(Color::rgb(200, 40, 40))
                        }
                        _ => {
                            let AnnotationStyle::Highlight { color } = highlight else {
                                unreachable!()
                            };
                            AnnotationSpec::highlight(rects).color(color)
                        }
                    };
                    form.add_annotation(index, spec)?;
                }
                page.unload()?;
            }
            Ok(())
        }));
        self.open = true;
        Ok(())
    }
    pub fn note(
        &mut self,
        doc: &DocumentThread,
        page_index: usize,
        x: f32,
        y: f32,
        text: String,
    ) -> Result<(), String> {
        if !x.is_finite()
            || !y.is_finite()
            || !(0.0..=1.0).contains(&x)
            || !(0.0..=1.0).contains(&y)
        {
            return Err("note position must be within the page".into());
        }
        self.error = None;
        self.edits.push(doc.call(move |doc, form| {
            let page = doc.page(page_index)?;
            let inverse = crate::text::inverse(page.render_matrix(&RenderConfig::fit(1, 1))?)
                .map_err(|_| {
                    pdfoxium::Error::InvalidArgument("page transform is not invertible")
                })?;
            let point = inverse.transform(Point { x, y });
            let rect = pdfoxium::Rect {
                left: point.x,
                right: point.x + 22.0,
                bottom: point.y - 22.0,
                top: point.y,
            };
            form.add_annotation(
                page_index,
                AnnotationSpec::text_note(rect, text).color(Color::rgb(255, 210, 40)),
            )?;
            page.unload()?;
            Ok(())
        }));
        self.open = true;
        self.placing_note = false;
        Ok(())
    }
    pub fn edit(
        &mut self,
        doc: &DocumentThread,
        index: usize,
        contents: Option<String>,
    ) -> Result<(), String> {
        let page = self.page.ok_or("annotation list is not loaded")?;
        let id = self
            .rows
            .iter()
            .find(|r| r.index == index)
            .ok_or("annotation is not in the current list")?
            .id;
        self.error = None;
        if self.selected == Some(id) {
            if let Some(text) = &contents {
                self.contents = text.clone();
            } else {
                self.deselect();
            }
        }
        self.edits.push(doc.call(move |doc, form| {
            let page_handle = doc.page(page)?;
            match contents {
                Some(text) => form.set_annotation_contents(page, id, &text)?,
                None => form.remove_annotation(page, id)?,
            }
            let page = page_handle;
            page.unload()?;
            Ok(())
        }));
        Ok(())
    }
    pub fn configure_defaults(&mut self, values: &Value) -> Result<(), String> {
        let target = match values["kind"].as_str() {
            Some("text") => &mut self.defaults.text,
            Some("shape") => &mut self.defaults.shape,
            Some("highlight") => &mut self.defaults.highlight,
            _ => return Err("kind must be text, shape or highlight".into()),
        };
        *target = crate::annotation_style::patch(*target, values)?;
        Ok(())
    }
    pub fn style_patch(
        &mut self,
        doc: &DocumentThread,
        index: usize,
        values: &Value,
    ) -> Result<(), String> {
        let style = self
            .rows
            .iter()
            .find(|r| r.index == index)
            .and_then(|r| r.style)
            .ok_or("this annotation has no editable style")?;
        let style = crate::annotation_style::patch(style, values)?;
        self.apply_style(doc, index, style)
    }
    fn apply_style(
        &mut self,
        doc: &DocumentThread,
        index: usize,
        style: AnnotationStyle,
    ) -> Result<(), String> {
        let page = self.page.ok_or("annotation list is not loaded")?;
        let row = self
            .rows
            .iter()
            .find(|r| r.index == index)
            .ok_or("annotation is not in the current list")?;
        let id = row.id;
        if self.selected == Some(id) {
            self.selected_style = Some(style);
        }
        self.error = None;
        self.edits
            .push(doc.call(move |_, form| form.set_annotation_style(page, id, style)));
        Ok(())
    }
    pub fn snapshot(&self) -> Value {
        json!({"defaults":{"text":crate::annotation_style::snapshot(self.defaults.text),"shape":crate::annotation_style::snapshot(self.defaults.shape),"highlight":crate::annotation_style::snapshot(self.defaults.highlight)},"open":self.open,"page":self.page.map(|p|p+1),"pending":self.busy(),"placing_note":self.placing_note,"tool":self.tool.map(Tool::label),"moving":self.is_moving(),"dragging":self.dragging(),"move_preview":self.move_preview().map(|(page,r)|json!({"page":page+1,"rect":[r.min.x,r.min.y,r.width(),r.height()]})),"error":self.error,"selected":self.selected.and_then(|id|self.rows.iter().find(|r|r.id==id).map(|r|json!({"page":self.selected_page.map(|p|p+1),"index":r.index,"kind":format!("{:?}",r.kind),"contents":self.contents,"style":self.selected_style.map(crate::annotation_style::snapshot),"regions":r.regions.iter().map(|r|vec![r.min.x,r.min.y,r.width(),r.height()]).collect::<Vec<_>>()}))),
            "items":self.rows.iter().map(|r|json!({"index":r.index,"kind":format!("{:?}",r.kind),"contents":r.contents,"style":r.style.map(crate::annotation_style::snapshot),"editable":editable(r.kind),"rect":[r.rect.min.x,r.rect.min.y,r.rect.width(),r.rect.height()]})).collect::<Vec<_>>()})
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        doc: Option<&DocumentThread>,
        selection: &crate::text::Selection,
        selecting: bool,
        current_page: usize,
    ) -> Option<Rect> {
        self.controls.clear();
        ui.spacing_mut().item_spacing.y = 6.;
        egui::ScrollArea::vertical()
            .id_salt("annotation-panel-content")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let reveal = self.show_contents(ui, doc, selection, selecting, current_page);
                if let Some(error) = &self.error {
                    ui.colored_label(egui::Color32::DARK_RED, error);
                }
                reveal
            })
            .inner
    }
    fn show_contents(
        &mut self,
        ui: &mut egui::Ui,
        doc: Option<&DocumentThread>,
        selection: &crate::text::Selection,
        selecting: bool,
        current_page: usize,
    ) -> Option<Rect> {
        use crate::chrome::{self, Icon};
        let current_page = self.context_page(current_page);
        ui.add_space(2.);
        ui.horizontal(|ui| {
            ui.heading("Annotations");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let close = chrome::icon(ui, Icon::Close, false, true, "Close annotations");
                self.controls
                    .push(("annotations.close", "Close annotations", close.rect));
                if close.clicked() {
                    self.open = false;
                    self.cancel_tools();
                }
            });
        });
        let Some(doc) = doc else {
            ui.spinner();
            ui.weak("Preparing editor…");
            return None;
        };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.;
            ui.weak("Add");
            for (tool, icon) in [
                (Tool::Text, Icon::Text),
                (Tool::Rectangle, Icon::Rectangle),
                (Tool::Ellipse, Icon::Ellipse),
            ] {
                let button = chrome::icon(ui, icon, self.tool == Some(tool), true, tool.label());
                self.controls
                    .push((tool.command(), tool.label(), button.rect));
                if button.clicked() {
                    if self.tool == Some(tool) {
                        self.cancel_tools();
                    } else {
                        self.start_tool(tool);
                    }
                }
            }
            let note = chrome::icon(ui, Icon::Note, self.placing_note, true, "Sticky note");
            self.controls
                .push(("annotation.place_note", "Sticky note", note.rect));
            if note.clicked() {
                if self.placing_note {
                    self.cancel_tools();
                } else {
                    self.deselect();
                    self.start_note(self.note_text.clone());
                }
            }
        });
        ui.add_space(2.);
        ui.separator();
        if let Some(tool) = self.tool {
            chrome::section(ui, tool.label());
            ui.weak("Drag on the page to draw. Esc cancels.");
            let style = if tool == Tool::Text {
                &mut self.defaults.text
            } else {
                &mut self.defaults.shape
            };
            ui.add_space(10.);
            crate::annotation_style::show(ui, style, &mut self.controls);
            return None;
        }
        if self.placing_note {
            chrome::section(ui, "New note");
            let note = ui.add(
                egui::TextEdit::multiline(&mut self.note_text)
                    .desired_rows(5)
                    .desired_width(f32::INFINITY)
                    .hint_text("Write a note…"),
            );
            self.controls
                .push(("annotation.note_text", "New note", note.rect));
            ui.weak("Click the page to place it. Esc cancels.");
            return None;
        }
        if self.page != Some(current_page) {
            ui.weak("Loading annotations…");
            return None;
        }
        if let Some(index) = self
            .selected
            .and_then(|id| self.rows.iter().find(|r| r.id == id).map(|r| r.index))
        {
            let back = ui.button("‹ All annotations");
            self.controls
                .push(("annotation.back", "All annotations", back.rect));
            if back.clicked() {
                self.deselect();
                return None;
            }
            let row = self.rows.iter().find(|r| r.index == index).unwrap();
            let kind = row.kind;
            ui.add_space(8.);
            let title = ui
                .horizontal(|ui| {
                    ui.strong(kind_label(kind));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.weak(format!("Page {}", current_page + 1));
                    });
                })
                .response
                .interact(egui::Sense::click())
                .on_hover_text("Reveal on page");
            self.controls
                .push(("annotation.selected_row", "Selected annotation", title.rect));
            let reveal = title.clicked().then_some(row.rect);
            if !editable(kind) {
                ui.weak("This annotation is read-only.");
                return None;
            }
            let free_text = kind == AnnotationKind::FreeText;
            let contents_changed = self.contents != row.contents;
            let style_changed = self.selected_style != row.style;
            let mut output = egui::TextEdit::multiline(&mut self.contents)
                .desired_width(f32::INFINITY)
                .desired_rows(if free_text { 6 } else { 4 })
                .hint_text(if free_text {
                    "Text…"
                } else {
                    "Add a comment…"
                })
                .show(ui);
            if self.focus_text && free_text {
                output.response.request_focus();
                output
                    .state
                    .cursor
                    .set_char_range(Some(egui::text::CCursorRange::two(
                        egui::text::CCursor::new(0),
                        egui::text::CCursor::new(self.contents.chars().count()),
                    )));
                output.state.store(ui.ctx(), output.response.id);
                self.focus_text = false;
            }
            self.controls.push((
                "annotation.comment",
                "Annotation text",
                output.response.rect,
            ));
            if let Some(style) = self.selected_style.as_mut() {
                chrome::section(ui, "Appearance");
                crate::annotation_style::show(ui, style, &mut self.controls);
            }
            ui.add_space(16.);
            let mut apply = false;
            let mut delete = false;
            ui.horizontal(|ui| {
                let changed = contents_changed || style_changed || output.response.changed();
                let button =
                    ui.add_enabled(changed && !self.busy(), egui::Button::new("Apply changes"));
                // Existing control IDs remain aliases for the single commit action.
                self.controls
                    .push(("annotation.apply_comment", "Apply changes", button.rect));
                self.controls
                    .push(("annotation.apply_style", "Apply changes", button.rect));
                apply = button.clicked();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let button =
                        chrome::icon(ui, Icon::Delete, false, !self.busy(), "Delete annotation");
                    self.controls.push((
                        "annotation.delete_selected",
                        "Delete annotation",
                        button.rect,
                    ));
                    delete = button.clicked();
                });
            });
            if apply {
                if let Err(e) = self.apply_draft(doc, index) {
                    self.error = Some(e);
                }
            } else if delete {
                if let Err(e) = self.edit(doc, index, None) {
                    self.error = Some(e);
                }
            }
            return reveal;
        }
        if !selecting && !selection.pages.is_empty() {
            chrome::section(ui, "Selected text");
            ui.horizontal_wrapped(|ui| {
                for (kind, label, id) in [
                    ("highlight", "Highlight", "annotation.markup.highlight"),
                    ("underline", "Underline", "annotation.markup.underline"),
                    ("strikeout", "Strike out", "annotation.markup.strikeout"),
                ] {
                    let button = ui.button(label);
                    self.controls.push((id, label, button.rect));
                    if button.clicked() {
                        if let Err(e) = self.markup(doc, kind, selection) {
                            self.error = Some(e);
                        }
                    }
                }
            });
            crate::annotation_style::show(ui, &mut self.defaults.highlight, &mut self.controls);
            ui.add_space(8.);
            ui.separator();
        }
        chrome::section(
            ui,
            &format!(
                "Page {} · {} annotations",
                current_page + 1,
                self.rows.len()
            ),
        );
        if self.rows.is_empty() {
            ui.add_space(16.);
            ui.weak("No annotations on this page.");
            ui.weak("Choose a tool above, or select text to mark it up.");
        }
        let mut picked = None;
        let mut reveal = None;
        for row in &self.rows {
            let label = kind_label(row.kind);
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), 50.), egui::Sense::click());
            let response = response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(&row.contents);
            if response.hovered() {
                ui.painter()
                    .rect_filled(rect, 5., ui.visuals().widgets.hovered.weak_bg_fill);
            }
            let title = egui::RichText::new(label).size(13.);
            let summary = row.contents.lines().next().unwrap_or("").trim();
            let text_rect = rect.shrink2(egui::vec2(10., 4.));
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(text_rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            child.set_clip_rect(text_rect.intersect(ui.clip_rect()));
            child.spacing_mut().item_spacing.y = 3.;
            child.add(egui::Label::new(title).truncate());
            child.add(
                egui::Label::new(
                    egui::RichText::new(if summary.is_empty() {
                        "No comment"
                    } else {
                        summary
                    })
                    .small()
                    .weak(),
                )
                .truncate(),
            );
            if response.clicked() {
                picked = Some(Hit {
                    id: row.id,
                    index: row.index,
                    kind: row.kind,
                    rect: row.rect,
                    regions: row.regions.clone(),
                });
                reveal = Some(row.rect);
            }
        }
        if let Some(hit) = picked {
            self.select_hit(current_page, hit, true);
        }
        reveal
    }
    fn apply_draft(&mut self, doc: &DocumentThread, index: usize) -> Result<(), String> {
        let page = self.page.ok_or("annotation list is not loaded")?;
        let row = self
            .rows
            .iter()
            .find(|r| r.index == index)
            .ok_or("annotation is not in the current list")?;
        let id = row.id;
        let contents = (row.contents != self.contents).then(|| self.contents.clone());
        let style = (row.style != self.selected_style)
            .then_some(self.selected_style)
            .flatten();
        if contents.is_none() && style.is_none() {
            return Ok(());
        }
        self.error = None;
        self.edits.push(doc.call(move |_, form| {
            if let Some(contents) = contents {
                form.set_annotation_contents(page, id, &contents)?;
            }
            if let Some(style) = style {
                form.set_annotation_style(page, id, style)?;
            }
            Ok(())
        }));
        Ok(())
    }
    pub fn start_note(&mut self, text: String) {
        self.deselect();
        self.cancel_tools();
        self.open = true;
        self.note_text = text;
        self.placing_note = true;
    }
    pub fn place(&mut self, doc: &DocumentThread, location: crate::text::Location) {
        if let Err(e) = self.note(
            doc,
            location.page,
            location.point.x,
            location.point.y,
            self.note_text.clone(),
        ) {
            self.error = Some(e);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdfoxium::{
        thread::{DocOptions, Source},
        Config,
    };
    fn settle(state: &mut State, doc: &DocumentThread, page: usize) {
        let revision = doc.call(|doc, _| Ok(doc.revision())).wait().unwrap();
        let until = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            state.poll(Some(doc), page, revision);
            if !state.busy() {
                break;
            }
            assert!(std::time::Instant::now() < until);
            std::thread::yield_now();
        }
        assert!(state.error.is_none(), "{:?}", state.error);
    }
    #[test]
    fn selection_markup_notes_comments_delete_and_save_use_live_document() {
        let doc = DocumentThread::spawn(
            Source::Bytes(crate::text::tests::fixture(3)),
            Config::default(),
            DocOptions::default(),
        )
        .unwrap();
        let mut state = State {
            open: true,
            ..Default::default()
        };
        settle(&mut state, &doc, 0);
        let before = doc
            .call(|doc, _| {
                Ok(doc
                    .page(0)?
                    .render(&RenderConfig::scale(2.0))?
                    .into_pixels())
            })
            .wait()
            .unwrap();
        let selected = crate::text::Selection {
            text: "banana".into(),
            pages: vec![
                crate::text::SelectedPage {
                    page: 0,
                    range: 6..12,
                    rects: vec![],
                },
                crate::text::SelectedPage {
                    page: 1,
                    range: 6..12,
                    rects: vec![],
                },
            ],
        };
        for kind in ["highlight", "underline", "strikeout"] {
            state.markup(&doc, kind, &selected).unwrap();
        }
        settle(&mut state, &doc, 0);
        assert_eq!(state.rows.len(), 3);
        let id = state.rows[0].id;
        state.edit(&doc, 0, Some("Comment é🙂".into())).unwrap();
        settle(&mut state, &doc, 0);
        assert_eq!(state.rows[0].id, id);
        assert_eq!(state.rows[0].contents, "Comment é🙂");
        let after = doc
            .call(|doc, form| {
                assert!(form.state()?.dirty);
                Ok(doc
                    .page(0)?
                    .render(&RenderConfig::scale(2.0))?
                    .into_pixels())
            })
            .wait()
            .unwrap();
        assert!(before.iter().zip(&after).filter(|(a, b)| a != b).count() > 100);
        state.edit(&doc, 1, None).unwrap();
        settle(&mut state, &doc, 0);
        assert_eq!(state.rows.len(), 2);
        assert_eq!(state.rows[0].id, id);
        state
            .note(&doc, 1, 0.4, 0.6, "Rotated note".into())
            .unwrap();
        settle(&mut state, &doc, 1);
        let note = state
            .rows
            .iter()
            .find(|r| r.kind == AnnotationKind::Text)
            .unwrap();
        assert_eq!(note.contents, "Rotated note");
        assert!((note.rect.min.x - 0.4).abs() < 0.12 && (note.rect.min.y - 0.6).abs() < 0.12);
        let saved = doc
            .call(|doc, _| {
                let mut bytes = Vec::new();
                doc.save_to(&mut bytes)?;
                Ok(bytes)
            })
            .wait()
            .unwrap();
        let reopened = Document::from_bytes(saved).unwrap();
        assert_eq!(rows(&reopened, 0).unwrap().len(), 2);
        assert_eq!(rows(&reopened, 1).unwrap().len(), 4);
        assert_eq!(rows(&reopened, 0).unwrap()[0].contents, "Comment é🙂");
        assert_eq!(
            reopened
                .page(0)
                .unwrap()
                .render(&RenderConfig::scale(2.0))
                .unwrap()
                .pixels(),
            doc.call(|doc, _| Ok(doc
                .page(0)?
                .render(&RenderConfig::scale(2.0))?
                .into_pixels()))
                .wait()
                .unwrap()
        );
        assert!(state.note(&doc, 0, f32::NAN, 0.0, String::new()).is_err());
        assert!(state.edit(&doc, 999, None).is_err());
        assert!(state.markup(&doc, "invalid", &selected).is_err());
    }
}

#[cfg(test)]
pub(crate) mod picking_tests {
    use super::*;
    use pdfoxium::{
        thread::{DocOptions, Source},
        Config,
    };
    pub fn annotated_bytes() -> std::sync::Arc<[u8]> {
        let doc = Document::from_bytes(crate::text::tests::fixture(3)).unwrap();
        let page = doc.page(0).unwrap();
        page.add_annotation(
            AnnotationSpec::highlight(vec![
                pdfoxium::Rect {
                    left: 30.,
                    bottom: 120.,
                    right: 100.,
                    top: 135.,
                },
                pdfoxium::Rect {
                    left: 30.,
                    bottom: 80.,
                    right: 100.,
                    top: 95.,
                },
            ])
            .contents("Multiline"),
        )
        .unwrap();
        page.add_annotation(AnnotationSpec::text_note(
            pdfoxium::Rect {
                left: 30.,
                bottom: 120.,
                right: 52.,
                top: 142.,
            },
            "Note",
        ))
        .unwrap();
        let hidden = page
            .add_annotation(AnnotationSpec::text_note(
                pdfoxium::Rect {
                    left: 150.,
                    bottom: 120.,
                    right: 172.,
                    top: 142.,
                },
                "Hidden",
            ))
            .unwrap();
        page.set_annotation_flags(hidden, 2).unwrap();
        let mut bytes = Vec::new();
        doc.save_to(&mut bytes).unwrap();
        bytes.into()
    }
    #[test]
    fn hit_metadata_preserves_quads_and_hides_invisible_annotations() {
        let doc = Document::from_bytes(annotated_bytes()).unwrap();
        let hits = hit_regions(&doc, 0).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].regions.len(), 2);
        assert_eq!(hits[1].regions.len(), 1);
        let gap = egui::pos2(
            hits[0].regions[0].center().x,
            (hits[0].regions[0].max.y + hits[0].regions[1].min.y) * 0.5,
        );
        assert!(hits[0].rect.contains(gap));
        assert!(!hits[0].regions.iter().any(|r| r.contains(gap)));
    }
    #[test]
    fn readonly_pick_rebinds_to_edit_document_and_identity_survives_index_shift() {
        let bytes = annotated_bytes();
        let reader = Document::from_bytes(bytes.clone()).unwrap();
        let hits = hit_regions(&reader, 0).unwrap();
        let doc = DocumentThread::spawn(
            Source::Bytes(bytes),
            Config::default(),
            DocOptions::default(),
        )
        .unwrap();
        let mut state = State::default();
        state.select_hit(0, hits[1].clone(), false);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            state.poll(Some(&doc), 0, 0);
            if !state.busy() {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(state.error.is_none(), "{:?}", state.error);
        let selected = state.selected.unwrap();
        assert_ne!(selected, hits[1].id);
        assert_eq!(state.contents, "Note");
        state.edit(&doc, 0, None).unwrap();
        let revision = doc.call(|doc, _| Ok(doc.revision())).wait().unwrap();
        loop {
            state.poll(Some(&doc), 1, revision); // Panel remains on the selected page.
            if !state.busy() {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert_eq!(state.selected, Some(selected));
        assert_eq!(state.rows[0].id, selected);
        assert_eq!(state.context_page(1), 0);
        state.delete_selected(&doc).unwrap();
        assert!(state.selected_regions().is_none());
        assert!(state.delete_selected(&doc).is_err());
        doc.call(|doc, _| {
            assert_eq!(hit_regions(doc, 0)?.len(), 0);
            Ok(())
        })
        .wait()
        .unwrap();
    }
    fn settle_move(state: &mut State, doc: &DocumentThread) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let revision = doc.call(|doc, _| Ok(doc.revision())).wait().unwrap();
            state.poll(Some(doc), 0, revision);
            if !state.busy() {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "{:?}", state.error);
            std::thread::yield_now();
        }
        assert!(state.error.is_none(), "{:?}", state.error);
    }
    #[test]
    fn note_drag_previews_without_editing_then_commits_once_with_closed_panel() {
        let bytes = annotated_bytes();
        let reader = Document::from_bytes(bytes.clone()).unwrap();
        let hit = hit_regions(&reader, 0).unwrap()[1].clone();
        let doc = DocumentThread::spawn(
            Source::Bytes(bytes),
            Config::default(),
            DocOptions::default(),
        )
        .unwrap();
        let mut state = State::default();
        state
            .begin_drag(0, hit.clone(), false, hit.rect.center())
            .unwrap();
        state.update_drag(hit.rect.center() + egui::vec2(0.2, 0.1));
        settle_move(&mut state, &doc);
        assert!(!state.open);
        assert!(state.dragging());
        assert!(!doc.call(|doc, _| Ok(doc.is_dirty())).wait().unwrap());
        let preview = *state.move_preview().unwrap().1;
        assert!((preview.min - hit.rect.min - egui::vec2(0.2, 0.1)).length() < 1e-6);
        assert_eq!(state.rows[1].rect, hit.rect);
        state.finish_drag(Some(&doc));
        settle_move(&mut state, &doc);
        assert!(!state.open && !state.dragging());
        assert!((state.rows[1].rect.min - preview.min).length() < 1e-6);
        assert!((state.rows[1].rect.size() - hit.rect.size()).length() < 1e-6);
        assert!(state.selected.is_some());
        assert!(doc.call(|doc, _| Ok(doc.is_dirty())).wait().unwrap());
    }
    #[test]
    fn cancelling_unreleased_drag_and_deselecting_released_drag_have_distinct_semantics() {
        let bytes = annotated_bytes();
        let reader = Document::from_bytes(bytes.clone()).unwrap();
        let hit = hit_regions(&reader, 0).unwrap()[1].clone();
        let doc = DocumentThread::spawn(
            Source::Bytes(bytes),
            Config::default(),
            DocOptions::default(),
        )
        .unwrap();
        let mut state = State::default();
        state
            .begin_drag(0, hit.clone(), false, hit.rect.center())
            .unwrap();
        state.update_drag(hit.rect.center() + egui::vec2(0.2, 0.1));
        state.deselect();
        state.finish_drag(Some(&doc));
        settle_move(&mut state, &doc);
        assert!(state.drag_cancelled());
        assert!(!doc.call(|doc, _| Ok(doc.is_dirty())).wait().unwrap());
        state.pointer_released();
        state
            .begin_drag(0, hit.clone(), false, hit.rect.center())
            .unwrap();
        state.update_drag(hit.rect.center() + egui::vec2(0.2, 0.1));
        state.finish_drag(None); // Release while editing worker is still starting.
        state.deselect();
        settle_move(&mut state, &doc);
        assert!(state.selected.is_none() && state.pending_select.is_none());
        assert!(doc.call(|doc, _| Ok(doc.is_dirty())).wait().unwrap());
    }
    #[test]
    fn note_translation_clamps_without_resizing_or_panicking_on_oversized_notes() {
        let r = Rect::from_min_max(egui::pos2(0.2, 0.3), egui::pos2(0.4, 0.5));
        let moved = translated_note(r, egui::vec2(10., -10.));
        assert!((moved.min - egui::pos2(0.8, 0.)).length() < 1e-6);
        assert!((moved.size() - r.size()).length() < 1e-6);
        let large = Rect::from_min_max(egui::pos2(-0.5, 0.3), egui::pos2(1.5, 0.5));
        assert_eq!(
            translated_note(large, egui::vec2(10., 0.)).min.x,
            large.min.x
        );
    }
    #[test]
    fn changed_target_rejects_released_move_instead_of_overwriting_new_position() {
        let bytes = annotated_bytes();
        let reader = Document::from_bytes(bytes.clone()).unwrap();
        let hit = hit_regions(&reader, 0).unwrap()[1].clone();
        let doc = DocumentThread::spawn(
            Source::Bytes(bytes),
            Config::default(),
            DocOptions::default(),
        )
        .unwrap();
        let mut state = State::default();
        state
            .begin_drag(0, hit.clone(), false, hit.rect.center())
            .unwrap();
        state.update_drag(hit.rect.center() + egui::vec2(0.2, 0.1));
        let changed = doc
            .call(|doc, form| {
                let page = doc.page(0)?;
                let id = page.annotation_ids()?[1];
                form.move_note(0, id, Point { x: 5., y: 0. })?;
                Ok(page.annotation(id)?.rect)
            })
            .wait()
            .unwrap();
        state.finish_drag(Some(&doc));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while state.busy() {
            let revision = doc.call(|doc, _| Ok(doc.revision())).wait().unwrap();
            state.poll(Some(&doc), 0, revision);
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(state
            .error
            .as_ref()
            .unwrap()
            .contains("changed during the drag"));
        assert_eq!(
            doc.call(|doc, _| {
                let page = doc.page(0)?;
                Ok(page.annotation(page.annotation_ids()?[1])?.rect)
            })
            .wait()
            .unwrap(),
            changed
        );
    }
    #[test]
    fn resize_keeps_opposite_corner_and_clamps_minimum_and_page_edges() {
        let r = Rect::from_min_max(egui::pos2(0.2, 0.3), egui::pos2(0.6, 0.7));
        for corner in Corner::ALL {
            for delta in [
                egui::vec2(-10., -10.),
                egui::vec2(10., 10.),
                egui::vec2(0.1, -0.1),
            ] {
                let resized = resized_rect(r, corner, delta, egui::vec2(0.02, 0.03));
                assert!(resized.is_positive());
                assert!(resized.width() >= 0.01999 && resized.height() >= 0.02999);
                assert!(
                    resized.min.x >= 0.
                        && resized.min.y >= 0.
                        && resized.max.x <= 1.
                        && resized.max.y <= 1.
                );
                let opposite = match corner {
                    Corner::TopLeft => Corner::BottomRight,
                    Corner::TopRight => Corner::BottomLeft,
                    Corner::BottomRight => Corner::TopLeft,
                    Corner::BottomLeft => Corner::TopRight,
                };
                assert_eq!(opposite.point(resized), opposite.point(r));
            }
        }
        let offpage = Rect::from_min_max(egui::pos2(-0.1, -0.2), egui::pos2(1.3, 1.4));
        for corner in Corner::ALL {
            assert!(resized_rect(
                offpage,
                corner,
                egui::vec2(10., -10.),
                egui::Vec2::splat(0.02)
            )
            .is_positive());
        }
    }
    #[test]
    fn shape_resize_only_edits_on_release_and_rebuilds_appearance() {
        let doc = DocumentThread::spawn(
            Source::Bytes(crate::text::tests::fixture(1)),
            Config::default(),
            DocOptions::default(),
        )
        .unwrap();
        let (id, hit) = doc
            .call(|doc, form| {
                let id = form.add_annotation(
                    0,
                    AnnotationSpec::circle(pdfoxium::Rect {
                        left: 40.,
                        bottom: 40.,
                        right: 100.,
                        top: 100.,
                    }),
                )?;
                Ok((id, hit_regions(doc, 0)?.remove(0)))
            })
            .wait()
            .unwrap();
        let mut state = State::default();
        state.select_hit(0, hit.clone(), true);
        settle_move(&mut state, &doc);
        let revision = doc.call(|doc, _| Ok(doc.revision())).wait().unwrap();
        state
            .begin_resize(
                Corner::BottomRight,
                hit.rect.right_bottom(),
                egui::Vec2::splat(0.02),
            )
            .unwrap();
        state.update_drag(hit.rect.right_bottom() + egui::vec2(0.1, 0.2));
        let preview = *state.move_preview().unwrap().1;
        assert_eq!(state.rows[0].rect, hit.rect);
        assert_eq!(
            doc.call(|doc, _| Ok(doc.revision())).wait().unwrap(),
            revision
        );
        state.finish_drag(Some(&doc));
        settle_move(&mut state, &doc);
        assert_eq!(state.selected, Some(id));
        assert!((state.rows[0].rect.max - preview.max).length() < 1e-5);
        assert_eq!(state.rows[0].rect.min, hit.rect.min);
        assert!(doc.call(|doc, _| Ok(doc.revision())).wait().unwrap() > revision);
    }
    #[test]
    fn drawing_tools_create_on_release_select_live_identity_and_edit_free_text() {
        for tool in Tool::ALL {
            let doc = DocumentThread::spawn(
                Source::Bytes(crate::text::tests::fixture(1)),
                Config::default(),
                DocOptions::default(),
            )
            .unwrap();
            let mut state = State::default();
            state.start_tool(tool);
            state
                .begin_creation(0, egui::pos2(0.2, 0.2), egui::Vec2::splat(0.02))
                .unwrap();
            state.update_drag(egui::pos2(0.6, 0.5));
            assert_eq!(
                doc.call(|doc, _| Ok(doc.page(0)?.annotation_ids()?.len()))
                    .wait()
                    .unwrap(),
                0
            );
            assert!(!doc.call(|doc, _| Ok(doc.is_dirty())).wait().unwrap());
            state.finish_drag(Some(&doc));
            settle_move(&mut state, &doc);
            assert!(state.tool.is_none());
            assert!(state.selected.is_some());
            assert_eq!(state.rows.len(), 1);
            let expected = match tool {
                Tool::Text => AnnotationKind::FreeText,
                Tool::Rectangle => AnnotationKind::Square,
                Tool::Ellipse => AnnotationKind::Circle,
            };
            assert_eq!(state.rows[0].kind, expected);
            assert!((state.rows[0].rect.min - egui::pos2(0.2, 0.2)).length() < 1e-5);
            assert!((state.rows[0].rect.max - egui::pos2(0.6, 0.5)).length() < 1e-5);
            if tool == Tool::Text {
                let id = state.selected.unwrap();
                state.edit(&doc, 0, Some("Editable text".into())).unwrap();
                settle_move(&mut state, &doc);
                assert_eq!(state.selected, Some(id));
                assert_eq!(state.contents, "Editable text");
            }
            assert!(doc.call(|doc, _| Ok(doc.is_dirty())).wait().unwrap());
        }
    }
    #[test]
    fn tiny_or_cancelled_draw_does_not_edit_but_accepted_creation_survives_deselection() {
        let doc = DocumentThread::spawn(
            Source::Bytes(crate::text::tests::fixture(1)),
            Config::default(),
            DocOptions::default(),
        )
        .unwrap();
        let mut state = State::default();
        state.start_tool(Tool::Rectangle);
        state
            .begin_creation(0, egui::pos2(0.2, 0.2), egui::Vec2::splat(0.02))
            .unwrap();
        state.update_drag(egui::pos2(0.21, 0.21));
        state.finish_drag(Some(&doc));
        assert_eq!(state.tool, Some(Tool::Rectangle));
        state
            .begin_creation(0, egui::pos2(0.2, 0.2), egui::Vec2::splat(0.02))
            .unwrap();
        state.update_drag(egui::pos2(0.6, 0.5));
        state.cancel_tools();
        state.finish_drag(Some(&doc));
        assert!(!doc.call(|doc, _| Ok(doc.is_dirty())).wait().unwrap());
        state.start_tool(Tool::Ellipse);
        state.pointer_released();
        state
            .begin_creation(0, egui::pos2(0.6, 0.5), egui::Vec2::splat(0.02))
            .unwrap();
        state.update_drag(egui::pos2(0.2, 0.2));
        state.finish_drag(None);
        state.deselect();
        settle_move(&mut state, &doc);
        assert!(state.selected.is_none());
        assert_eq!(
            doc.call(|doc, _| Ok(doc.page(0)?.annotation_ids()?.len()))
                .wait()
                .unwrap(),
            1
        );
    }
    #[test]
    fn selected_boxes_move_without_reflow_and_unselected_boxes_leave_text_dragging_available() {
        for kind in [
            AnnotationKind::FreeText,
            AnnotationKind::Square,
            AnnotationKind::Circle,
        ] {
            let doc = DocumentThread::spawn(
                Source::Bytes(crate::text::tests::fixture(1)),
                Config::default(),
                DocOptions::default(),
            )
            .unwrap();
            let hit = doc
                .call(move |doc, form| {
                    let r = pdfoxium::Rect {
                        left: 40.,
                        bottom: 40.,
                        right: 100.,
                        top: 100.,
                    };
                    let spec = match kind {
                        AnnotationKind::FreeText => {
                            AnnotationSpec::free_text(r, "First\nSecond", 12.)
                        }
                        AnnotationKind::Square => AnnotationSpec::square(r),
                        _ => AnnotationSpec::circle(r),
                    };
                    form.add_annotation(0, spec)?;
                    Ok(hit_regions(doc, 0)?.remove(0))
                })
                .wait()
                .unwrap();
            let mut state = State::default();
            assert!(!state.movable_hit(&hit));
            assert!(state
                .begin_drag(0, hit.clone(), true, hit.rect.center())
                .is_err());
            state.select_hit(0, hit.clone(), true);
            settle_move(&mut state, &doc);
            let id = state.selected.unwrap();
            let before = doc
                .call(move |doc, _| {
                    doc.page(0)?
                        .annotation_appearance(id, pdfoxium::AppearanceMode::Normal)
                })
                .wait()
                .unwrap();
            state
                .begin_drag(0, hit.clone(), true, hit.rect.center())
                .unwrap();
            state.update_drag(hit.rect.center() + egui::vec2(0.1, 0.08));
            state.finish_drag(Some(&doc));
            settle_move(&mut state, &doc);
            assert_eq!(state.selected, Some(id));
            assert!(
                (state.rows[0].rect.min - hit.rect.min - egui::vec2(0.1, 0.08)).length() < 1e-6
            );
            assert!((state.rows[0].rect.size() - hit.rect.size()).length() < 1e-6);
            assert_eq!(
                doc.call(move |doc, _| doc
                    .page(0)?
                    .annotation_appearance(id, pdfoxium::AppearanceMode::Normal))
                    .wait()
                    .unwrap(),
                before
            );
        }
    }
}
