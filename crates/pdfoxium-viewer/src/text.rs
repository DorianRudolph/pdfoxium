// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Static-document reader work. Only detached text/navigation data cross threads.
use crate::navigation;
use eframe::egui::{self, Pos2, Rect, Vec2};
use pdfoxium::thread::runtime::{self as thread, JoinHandle};
use pdfoxium::{Document, Matrix, Point, RenderConfig, SearchOptions, Size, TextPage};
#[cfg(not(target_arch = "wasm32"))]
use std::sync::{Condvar, Mutex};
use std::{
    collections::VecDeque,
    ops::Range,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc,
    },
};
#[cfg(target_arch = "wasm32")]
use wasm_sync::{Condvar, Mutex};

const MAX_SELECTION_BYTES: usize = 4 * 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Location {
    pub page: usize,
    pub point: Pos2,
    pub tolerance: Vec2,
}
#[derive(Clone, Debug)]
pub struct Hit {
    pub page: usize,
    pub range: Range<usize>,
    pub text: String,
    pub rects: Vec<Rect>,
}
#[derive(Clone, Debug)]
pub struct SelectedPage {
    pub page: usize,
    pub range: Range<usize>,
    pub rects: Vec<Rect>,
}
#[derive(Clone, Debug, Default)]
pub struct Selection {
    pub text: String,
    pub pages: Vec<SelectedPage>,
}
struct SearchRequest {
    id: u64,
    query: String,
    match_case: bool,
    start_page: usize,
}
pub enum Select {
    Points(Location, Location),
    Range(usize, Range<usize>),
}
struct SelectionRequest {
    id: u64,
    select: Select,
}
#[derive(Default)]
struct Inbox {
    search: Option<SearchRequest>,
    selection: Option<SelectionRequest>,
    layers: Option<(u64, pdfoxium::OptionalContentState)>,
    links: VecDeque<usize>,
    annotations: VecDeque<usize>,
    thumbnails: VecDeque<(u64, usize)>,
    destination: Option<(u64, pdfoxium::Destination, usize)>,
    stop: bool,
}
#[derive(Default)]
struct Shared {
    inbox: Mutex<Inbox>,
    wake: Condvar,
    search_id: AtomicU64,
    selection_id: AtomicU64,
    layers_epoch: AtomicU64,
}
pub enum Event {
    Annotations {
        epoch: u64,
        page: usize,
        result: Result<Vec<crate::annotations::Hit>, String>,
    },
    Search {
        id: u64,
        hits: Vec<Hit>,
        scanned: usize,
        done: bool,
        truncated: bool,
        error: Option<String>,
    },
    Selection {
        id: u64,
        result: Result<Selection, String>,
    },
    Links {
        groups: Option<Vec<usize>>,
        epoch: u64,
        page: usize,
        result: Result<Vec<navigation::Link>, String>,
    },
    Destination {
        id: u64,
        result: Result<navigation::Resolved, String>,
    },
    Thumbnail {
        id: u64,
        page: usize,
        result: Result<Option<pdfoxium::Bitmap>, String>,
    },
    Failed(String),
}
pub struct Worker {
    shared: Arc<Shared>,
    pub events: mpsc::Receiver<Event>,
    thread: Option<JoinHandle<()>>,
}
impl Worker {
    pub fn new(bytes: Arc<[u8]>, ctx: egui::Context) -> Self {
        let shared = Arc::new(Shared::default());
        let ctx = crate::platform::repaint(&ctx);
        let state = shared.clone();
        let (tx, events) = mpsc::channel();
        let worker = thread::spawn(move || {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run(bytes, &state, &tx, &ctx)
            }));
            let error = match result {
                Ok(Ok(())) => None,
                Ok(Err(e)) => Some(e),
                Err(_) => Some("text worker panicked".into()),
            };
            if let Some(error) = error {
                let _ = tx.send(Event::Failed(error));
                ctx.request_repaint();
            }
        });
        Self {
            shared,
            events,
            thread: Some(worker),
        }
    }
    pub fn annotations(&self, page: usize) {
        self.shared
            .inbox
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .annotations
            .push_back(page);
        self.shared.wake.notify_one();
    }
    pub fn search(&self, query: String, match_case: bool, start_page: usize) -> u64 {
        let id = self.shared.search_id.fetch_add(1, Ordering::Relaxed) + 1;
        self.shared
            .inbox
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .search = Some(SearchRequest {
            id,
            query,
            match_case,
            start_page,
        });
        self.shared.wake.notify_one();
        id
    }
    pub fn select(&self, select: Select) -> u64 {
        let id = self.shared.selection_id.fetch_add(1, Ordering::Relaxed) + 1;
        self.shared
            .inbox
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .selection = Some(SelectionRequest { id, select });
        self.shared.wake.notify_one();
        id
    }
    pub fn set_layers(&self, state: pdfoxium::OptionalContentState) {
        let epoch = self.shared.layers_epoch.fetch_add(1, Ordering::AcqRel) + 1;
        self.shared
            .inbox
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .layers = Some((epoch, state));
        self.shared.wake.notify_one();
    }
    pub fn links(&self, page: usize) {
        let mut inbox = self.shared.inbox.lock().unwrap_or_else(|p| p.into_inner());
        if !inbox.links.contains(&page) {
            inbox.links.push_back(page);
        }
        self.shared.wake.notify_one();
    }
    pub fn thumbnail(&self, id: u64, page: usize) {
        self.shared
            .inbox
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .thumbnails
            .push_back((id, page));
        self.shared.wake.notify_one();
    }
    pub fn cancel_thumbnails(&self) {
        self.shared
            .inbox
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .thumbnails
            .clear();
    }
    pub fn destination(&self, id: u64, destination: pdfoxium::Destination, current_page: usize) {
        self.shared
            .inbox
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .destination = Some((id, destination, current_page));
        self.shared.wake.notify_one();
    }
    pub fn cancel_selection(&self) -> u64 {
        self.shared.selection_id.fetch_add(1, Ordering::Relaxed) + 1
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.shared
            .inbox
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .stop = true;
        self.shared.search_id.fetch_add(1, Ordering::Relaxed);
        self.shared.selection_id.fetch_add(1, Ordering::Relaxed);
        self.shared.wake.notify_one();
        if let Some(worker) = self.thread.take() {
            thread::join(worker);
        }
    }
}
#[derive(Clone)]
struct Cached {
    page: usize,
    text: TextPage,
    matrix: Matrix,
}
struct Engine {
    document: Document,
    cache: VecDeque<Cached>,
    search_page: Option<Cached>,
}
impl Engine {
    fn page(&mut self, index: usize) -> Result<&Cached, String> {
        if self.search_page.as_ref().is_some_and(|p| p.page == index) {
            return Ok(self.search_page.as_ref().unwrap());
        }
        if let Some(i) = self.cache.iter().position(|p| p.page == index) {
            let page = self.cache.remove(i).unwrap();
            self.cache.push_back(page);
        } else {
            let page = self.document.page(index).map_err(|e| e.to_string())?;
            let prepared = (|| -> pdfoxium::Result<_> {
                let text = page.text()?;
                // A 1x1 full-canvas matrix normalizes crop and intrinsic rotation.
                let matrix = page.render_matrix(&RenderConfig::fit(1, 1))?;
                Ok((text, matrix))
            })();
            // Also release parsed content on text/geometry errors.
            let unloaded = page.unload();
            let (text, matrix) = prepared.map_err(|e| e.to_string())?;
            unloaded.map_err(|e| e.to_string())?;
            self.cache.push_back(Cached {
                page: index,
                text,
                matrix,
            });
            if self.cache.len() > 8 {
                self.cache.pop_front();
            }
        }
        Ok(self.cache.back().unwrap())
    }
    fn endpoint(
        &mut self,
        location: Location,
        nearest: bool,
    ) -> Result<Option<Range<usize>>, String> {
        let page = self.page(location.page)?;
        let inverse = inverse(page.matrix)?;
        let point = inverse.transform(Point {
            x: location.point.x,
            y: location.point.y,
        });
        let tolerance = if nearest {
            Size {
                width: 2.0 * (inverse.a.abs() + inverse.c.abs()),
                height: 2.0 * (inverse.b.abs() + inverse.d.abs()),
            }
        } else {
            Size {
                width: inverse.a.abs() * location.tolerance.x
                    + inverse.c.abs() * location.tolerance.y,
                height: inverse.b.abs() * location.tolerance.x
                    + inverse.d.abs() * location.tolerance.y,
            }
        };
        let pos = page
            .text
            .hit_test(point, tolerance)
            .map_err(|e| e.to_string())?;
        pos.map(|pos| {
            page.text
                .select_between(&pos, &pos)
                .map(|s| s.byte_range())
                .map_err(|e| e.to_string())
        })
        .transpose()
    }
    fn selection(
        &mut self,
        request: &SelectionRequest,
        shared: &Shared,
    ) -> Result<Selection, String> {
        let (start_page, start, end_page, end) = match &request.select {
            Select::Range(page, range) => (*page, range.start, *page, range.end),
            Select::Points(anchor, focus) => {
                let Some(a) = self.endpoint(*anchor, false)? else {
                    return Ok(Selection::default());
                };
                let focus_span = match self.endpoint(*focus, false)? {
                    Some(span) => Some(span),
                    None => self.endpoint(*focus, true)?,
                };
                let Some(f) = focus_span else {
                    return Ok(Selection::default());
                };
                if (anchor.page, a.start) <= (focus.page, f.start) {
                    (anchor.page, a.start, focus.page, f.end)
                } else {
                    (focus.page, f.start, anchor.page, a.end)
                }
            }
        };
        let mut selected = Selection::default();
        for index in start_page..=end_page {
            if shared.selection_id.load(Ordering::Relaxed) != request.id {
                return Err("selection superseded".into());
            }
            let page = self.page(index)?;
            let start = if index == start_page { start } else { 0 };
            let end = if index == end_page {
                end
            } else {
                page.text.as_str().len()
            };
            let selection = page.text.select(start..end).map_err(|e| e.to_string())?;
            if selected.text.len() + selection.text().len() + 1 > MAX_SELECTION_BYTES {
                return Err("selection exceeds 4 MiB; select a smaller range".into());
            }
            if !selected.text.is_empty() {
                selected.text.push('\n');
            }
            selected.text.push_str(selection.text());
            selected.pages.push(SelectedPage {
                page: index,
                range: selection.byte_range(),
                rects: selection
                    .display_rects()
                    .into_iter()
                    .map(|r| normalized(page.matrix, r))
                    .collect(),
            });
        }
        Ok(selected)
    }
}
fn run(
    bytes: Arc<[u8]>,
    shared: &Shared,
    tx: &mpsc::Sender<Event>,
    ctx: &crate::platform::Repaint,
) -> Result<(), String> {
    let document = Document::from_bytes(bytes).map_err(|e| e.to_string())?;
    let count = document.page_count().map_err(|e| e.to_string())?;
    let mut engine = Engine {
        document,
        cache: VecDeque::new(),
        search_page: None,
    };
    let mut search: Option<(SearchRequest, usize)> = None;
    let mut epoch = 0;
    loop {
        let (new_search, selection, links, destination, thumbnail, annotations, layers) = {
            let mut inbox = shared.inbox.lock().unwrap_or_else(|p| p.into_inner());
            while !inbox.stop
                && inbox.layers.is_none()
                && inbox.search.is_none()
                && inbox.selection.is_none()
                && inbox.thumbnails.is_empty()
                && inbox.links.is_empty()
                && inbox.annotations.is_empty()
                && inbox.destination.is_none()
                && search.is_none()
            {
                inbox = shared.wake.wait(inbox).unwrap_or_else(|p| p.into_inner());
            }
            if inbox.stop {
                return Ok(());
            }
            (
                inbox.search.take(),
                inbox.selection.take(),
                inbox.links.pop_front(),
                inbox.destination.take(),
                inbox.thumbnails.pop_front(),
                inbox.annotations.pop_front(),
                inbox.layers.take(),
            )
        };
        if let Some((next, state)) = layers {
            engine
                .document
                .set_optional_content_state(&state)
                .map_err(|e| e.to_string())?;
            epoch = next;
        }
        if let Some((id, destination, current_page)) = destination {
            let result = navigation::resolve(&engine.document, destination, current_page);
            let _ = tx.send(Event::Destination { id, result });
            ctx.request_repaint();
        }
        if let Some(page) = annotations {
            let result =
                crate::annotations::hit_regions(&engine.document, page).map_err(|e| e.to_string());
            let _ = tx.send(Event::Annotations {
                epoch,
                page,
                result,
            });
            ctx.request_repaint();
        }
        if let Some(page) = links {
            let result = navigation::links(&engine.document, page);
            let layers = engine
                .document
                .optional_content_state()
                .map_err(|e| e.to_string())?;
            let groups = if layers.lists_all_pages() {
                None
            } else {
                let handle = engine.document.page(page).map_err(|e| e.to_string())?;
                let used = handle
                    .optional_content_groups()
                    .map_err(|e| e.to_string())?;
                handle.unload().map_err(|e| e.to_string())?;
                Some(
                    layers
                        .groups()
                        .iter()
                        .enumerate()
                        .filter_map(|(i, g)| used.contains(&g.id).then_some(i))
                        .collect(),
                )
            };
            let _ = tx.send(Event::Links {
                groups,
                epoch,
                page,
                result,
            });
            ctx.request_repaint();
        }
        if let Some(mut job) = new_search {
            if job.query.is_empty() {
                let _ = tx.send(Event::Search {
                    id: job.id,
                    hits: Vec::new(),
                    scanned: 0,
                    done: true,
                    truncated: false,
                    error: None,
                });
                ctx.request_repaint();
                search = None;
            } else {
                job.start_page = job.start_page.min(count.saturating_sub(1));
                // Keep the query's starting page warm while the bounded LRU
                // scans the rest of the document. Refining a query need not
                // reload the visible page after every background scan.
                if !engine
                    .search_page
                    .as_ref()
                    .is_some_and(|p| p.page == job.start_page)
                {
                    engine.search_page = None;
                }
                search = Some((job, 0));
            }
        }
        if let Some(job) = selection {
            let result = engine.selection(&job, shared);
            if job.id == shared.selection_id.load(Ordering::Relaxed) {
                let _ = tx.send(Event::Selection { id: job.id, result });
                ctx.request_repaint();
            }
        }
        if let Some((id, page)) = thumbnail {
            let result = (|| -> Result<_, String> {
                let page = engine.document.page(page).map_err(|e| e.to_string())?;
                let bitmap = page.embedded_thumbnail(pdfoxium::PixelFormat::Rgba8);
                let unloaded = page.unload();
                let bitmap = bitmap.map_err(|e| e.to_string())?;
                unloaded.map_err(|e| e.to_string())?;
                Ok(bitmap)
            })();
            let _ = tx.send(Event::Thumbnail { id, page, result });
            ctx.request_repaint();
        }

        if let Some((job, index)) = search.as_mut() {
            if job.id != shared.search_id.load(Ordering::Relaxed) {
                search = None;
                continue;
            }
            let mut hits = Vec::new();
            let result = (|| -> Result<(), String> {
                if *index >= count {
                    return Ok(());
                }
                let tail = count - job.start_page;
                let page_index = if *index < tail {
                    job.start_page + *index
                } else {
                    *index - tail
                };
                let pin_page = *index == 0 && engine.search_page.is_none();
                let page = engine.page(page_index)?;
                for hit in page
                    .text
                    .search_with_rects(
                        &job.query,
                        SearchOptions {
                            match_case: job.match_case,
                            ..Default::default()
                        },
                    )
                    .map_err(|e| e.to_string())?
                {
                    if job.id != shared.search_id.load(Ordering::Relaxed) {
                        break;
                    }
                    let hit = hit.map_err(|e| e.to_string())?;
                    hits.push(Hit {
                        page: page_index,
                        range: hit.byte_range(),
                        text: hit.text().into(),
                        rects: hit
                            .rects
                            .into_iter()
                            .map(|r| normalized(page.matrix, r))
                            .collect(),
                    });
                }
                if pin_page {
                    engine.search_page = Some(page.clone());
                }
                Ok(())
            })();
            *index += 1;
            let done = *index >= count || result.is_err();
            let _ = tx.send(Event::Search {
                id: job.id,
                hits,
                scanned: (*index).min(count),
                done,
                truncated: false,
                error: result.err(),
            });
            ctx.request_repaint();
            if done {
                search = None;
            }
        }
    }
}
pub(crate) fn inverse(m: Matrix) -> Result<Matrix, String> {
    let determinant = f64::from(m.a) * f64::from(m.d) - f64::from(m.b) * f64::from(m.c);
    if !determinant.is_finite() || determinant == 0.0 {
        return Err("page text transform is singular".into());
    }
    let [a, b, c, d, e, f] = [m.a, m.b, m.c, m.d, m.e, m.f].map(f64::from);
    let inverse = Matrix {
        a: (d / determinant) as f32,
        b: (-b / determinant) as f32,
        c: (-c / determinant) as f32,
        d: (a / determinant) as f32,
        e: ((c * f - d * e) / determinant) as f32,
        f: ((b * e - a * f) / determinant) as f32,
    };
    if [
        inverse.a, inverse.b, inverse.c, inverse.d, inverse.e, inverse.f,
    ]
    .into_iter()
    .any(|v| !v.is_finite())
    {
        return Err("page text transform exceeds coordinate limits".into());
    }
    Ok(inverse)
}
pub(crate) fn normalized(matrix: Matrix, r: pdfoxium::Rect) -> Rect {
    let corners = [
        (r.left, r.bottom),
        (r.right, r.bottom),
        (r.left, r.top),
        (r.right, r.top),
    ]
    .map(|(x, y)| {
        let p = matrix.transform(Point { x, y });
        Pos2::new(p.x, p.y)
    });
    Rect::from_points(&corners)
}

pub struct State {
    worker: Worker,
    search_id: u64,
    pending_step: Option<bool>,
    pub start_page: usize,
    selection_id: u64,
    pub query: String,
    pub hits: Vec<Hit>,
    pub active: Option<usize>,
    pub searching: bool,
    pub scanned: usize,
    pub truncated: bool,
    pub error: Option<String>,
    pub selection: Selection,
    pub selecting: bool,
    pub anchor: Option<Location>,
    last_focus: Option<Location>,
    pub revision: u64,
    pub links: std::collections::BTreeMap<usize, Vec<navigation::Link>>,
    pub page_layers: std::collections::BTreeMap<usize, Vec<usize>>,
    link_pending: std::collections::HashSet<usize>,
    pub annotations: std::collections::BTreeMap<usize, Vec<crate::annotations::Hit>>,
    annotation_pending: std::collections::HashSet<usize>,
    pub navigation_error: Option<String>,
    pub destination: Option<navigation::Resolved>,
    destination_id: u64,
    pub navigating: bool,
    pub thumbnail_results: VecDeque<(u64, usize, Result<Option<pdfoxium::Bitmap>, String>)>,
}
impl State {
    pub fn new(bytes: Arc<[u8]>, ctx: egui::Context) -> Self {
        Self {
            worker: Worker::new(bytes, ctx),
            search_id: 0,
            pending_step: None,
            start_page: 0,
            selection_id: 0,
            query: String::new(),
            hits: Vec::new(),
            active: None,
            searching: false,
            scanned: 0,
            truncated: false,
            error: None,
            selection: Selection::default(),
            selecting: false,
            anchor: None,
            last_focus: None,
            revision: 0,
            links: Default::default(),
            page_layers: Default::default(),
            link_pending: Default::default(),
            annotations: Default::default(),
            annotation_pending: Default::default(),
            navigation_error: None,
            destination: None,
            destination_id: 0,
            navigating: false,
            thumbnail_results: VecDeque::new(),
        }
    }
    pub fn request_thumbnail(&self, id: u64, page: usize) {
        self.worker.thumbnail(id, page);
    }
    pub fn cancel_thumbnails(&self) {
        self.worker.cancel_thumbnails();
    }
    pub fn stopped(&self) -> bool {
        self.worker
            .thread
            .as_ref()
            .is_none_or(|thread| thread.is_finished())
    }
    pub fn set_layers(&mut self, state: pdfoxium::OptionalContentState) {
        self.links.clear();
        self.page_layers.clear();
        self.link_pending.clear();
        self.annotations.clear();
        self.annotation_pending.clear();
        self.worker.set_layers(state);
    }
    pub fn request_annotations(&mut self, page: usize) {
        if !self.annotations.contains_key(&page) && self.annotation_pending.insert(page) {
            self.worker.annotations(page);
        }
    }
    pub fn annotations_busy(&self) -> bool {
        !self.annotation_pending.is_empty()
    }
    pub fn request_links(&mut self, page: usize) {
        if !self.links.contains_key(&page) && self.link_pending.insert(page) {
            self.worker.links(page);
        }
    }
    pub fn navigate(&mut self, destination: pdfoxium::Destination, current_page: usize) {
        self.cancel_navigation();
        self.navigation_error = None;
        self.navigating = true;
        self.worker
            .destination(self.destination_id, destination, current_page);
        self.revision += 1;
    }
    pub fn cancel_navigation(&mut self) {
        self.pending_step = None;
        self.destination_id += 1;
        self.navigating = false;
        self.destination = None;
    }
    pub fn navigation_busy(&self) -> bool {
        self.navigating || !self.link_pending.is_empty()
    }
    pub fn search(&mut self, query: String, match_case: bool, start_page: usize) {
        self.search_id = self.worker.search(query.clone(), match_case, start_page);
        self.query = query;
        self.start_page = start_page;
        self.pending_step = None;
        self.hits.clear();
        self.active = None;
        self.searching = !self.query.is_empty();
        self.scanned = 0;
        self.truncated = false;
        self.error = None;
        self.revision += 1;
    }
    pub fn select(&mut self, page: usize, range: Range<usize>) {
        self.selection_id = self.worker.select(Select::Range(page, range));
        self.selecting = true;
        self.error = None;
        self.revision += 1;
    }
    pub fn clear_selection(&mut self) {
        self.selection_id = self.worker.cancel_selection();
        self.selection = Selection::default();
        self.selecting = false;
        self.error = None;
        self.anchor = None;
        self.last_focus = None;
        self.revision += 1;
    }
    pub fn drag(&mut self, anchor: Location, focus: Location) {
        if self.last_focus == Some(focus) {
            return;
        }
        self.selection_id = self.worker.select(Select::Points(anchor, focus));
        self.selecting = true;
        self.last_focus = Some(focus);
        self.error = None;
    }
    /// Returns a hit only in response to explicit pending search navigation.
    pub fn poll(&mut self) -> Option<Hit> {
        let mut reveal = None;
        while let Ok(event) = self.worker.events.try_recv() {
            match event {
                Event::Search {
                    id,
                    hits,
                    scanned,
                    done,
                    truncated,
                    error,
                } if id == self.search_id => {
                    self.hits.extend(hits);
                    self.scanned = scanned;
                    self.searching = !done;
                    self.truncated = truncated;
                    self.error = error;
                    if self.pending_step.is_some() {
                        reveal = self.activate_pending().or(reveal);
                    }
                    self.revision += 1;
                }
                Event::Selection { id, result } if id == self.selection_id => {
                    self.selecting = false;
                    match result {
                        Ok(selection) => self.selection = selection,
                        Err(error) => self.error = Some(error),
                    };
                    self.revision += 1;
                }
                Event::Annotations {
                    epoch,
                    page,
                    result,
                } if epoch == self.worker.shared.layers_epoch.load(Ordering::Acquire) => {
                    self.annotation_pending.remove(&page);
                    match result {
                        Ok(hits) => {
                            self.annotations.insert(page, hits);
                        }
                        Err(e) => {
                            self.error = Some(e);
                            self.annotations.insert(page, Vec::new());
                        }
                    }
                    self.revision += 1;
                }
                Event::Links {
                    groups,
                    epoch,
                    page,
                    result,
                } if epoch == self.worker.shared.layers_epoch.load(Ordering::Acquire) => {
                    self.link_pending.remove(&page);
                    if let Some(groups) = groups {
                        self.page_layers.insert(page, groups);
                    }
                    match result {
                        Ok(links) => {
                            self.links.insert(page, links);
                        }
                        Err(error) => {
                            self.navigation_error = Some(error);
                            self.links.insert(page, Vec::new());
                        }
                    }
                    self.revision += 1;
                }
                Event::Destination { id, result } if id == self.destination_id => {
                    self.navigating = false;
                    match result {
                        Ok(destination) => self.destination = Some(destination),
                        Err(error) => self.navigation_error = Some(error),
                    }
                    self.revision += 1;
                }
                Event::Thumbnail { id, page, result } => {
                    self.thumbnail_results.push_back((id, page, result))
                }
                Event::Failed(error) => {
                    self.navigation_error = Some(error.clone());
                    self.navigating = false;
                    self.link_pending.clear();
                    self.annotation_pending.clear();
                    self.error = Some(error);
                    self.searching = false;
                    self.selecting = false;
                    self.revision += 1;
                }
                _ => {}
            }
        }
        if (self.busy() || self.navigation_busy())
            && self
                .worker
                .thread
                .as_ref()
                .is_some_and(|thread| thread.is_finished())
        {
            self.searching = false;
            self.selecting = false;
            self.navigating = false;
            self.link_pending.clear();
            self.navigation_error = Some("reader worker stopped".into());
            self.error = Some("text worker stopped".into());
            self.revision += 1;
        }
        reveal
    }
    pub fn next(&mut self, backwards: bool) -> Option<Hit> {
        self.pending_step = Some(backwards);
        self.activate_pending()
    }
    fn activate_pending(&mut self) -> Option<Hit> {
        let backwards = self.pending_step?;
        let len = self.hits.len();
        if len == 0 {
            if !self.searching {
                self.pending_step = None;
            }
            return None;
        }
        let index = match self.active {
            None if backwards => {
                if let Some(index) = self
                    .hits
                    .iter()
                    .rposition(|hit| hit.page == self.start_page)
                {
                    index
                } else if self.searching {
                    return None;
                } else {
                    len - 1
                }
            }
            None => 0,
            Some(0) if backwards && self.searching => return None,
            Some(i) if !backwards && i + 1 == len && self.searching => return None,
            Some(i) if backwards => (i + len - 1) % len,
            Some(i) => (i + 1) % len,
        };
        self.pending_step = None;
        self.active = Some(index);
        self.revision += 1;
        Some(self.hits[index].clone())
    }
    pub fn busy(&self) -> bool {
        self.searching || self.selecting
    }
}

#[cfg(test)]
pub(crate) mod tests;
