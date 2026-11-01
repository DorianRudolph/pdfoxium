// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use crate::{
    layout::{Anchor, Layout, GAP},
    navigation,
    policy::{Mode, Screen},
    text::{self, Location},
    texture,
};
use eframe::egui::{self, Color32, Pos2, Rect, TextureHandle, Vec2};
use pdfoxium::{
    thread::{PoolBitmap, Priority, RenderPool, Source, Ticket},
    view::{RenderRequest, TileGrid, TileGridOptions, TileId},
    Config, DeviceRect, Document, PixelFormat, RenderConfig, Rotation, Size,
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{mpsc, Arc},
};
use web_time::{Duration, Instant};

const CACHE_LIMIT: usize = 128 * 1024 * 1024;
const MAX_PENDING: usize = 16;

fn intersection(a: DeviceRect, b: DeviceRect) -> Option<DeviceRect> {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let right = a.x.saturating_add(a.width).min(b.x.saturating_add(b.width));
    let bottom =
        a.y.saturating_add(a.height)
            .min(b.y.saturating_add(b.height));
    (x < right && y < bottom).then(|| DeviceRect::new(x, y, right - x, bottom - y))
}
fn damage_pixels(rect: Rect, canvas: (u32, u32), rotation: Rotation) -> Option<DeviceRect> {
    let canvas_rect = Rect::from_min_size(Pos2::ZERO, egui::vec2(canvas.0 as f32, canvas.1 as f32));
    // Round outwards and include one pixel for rasterized edge coverage.
    let r = text_screen_rect(canvas_rect, rect, rotation)
        .expand(1.0)
        .intersect(canvas_rect);
    if !r.is_positive() {
        return None;
    }
    let x = r.min.x.floor() as u32;
    let y = r.min.y.floor() as u32;
    Some(DeviceRect::new(
        x,
        y,
        r.max.x.ceil() as u32 - x,
        r.max.y.ceil() as u32 - y,
    ))
}
#[derive(Clone, Copy)]
struct Damage {
    page: usize,
    rect: DeviceRect,
    canvas: (u32, u32),
}
struct Patch {
    damage: Damage,
    ticket: Ticket<PoolBitmap>,
    requested: Instant,
}
fn merge_damage(regions: &mut Vec<Damage>, mut damage: Damage) {
    let mut i = 0;
    while i < regions.len() {
        let other = regions[i];
        if other.page == damage.page
            && other.canvas == damage.canvas
            && intersection(other.rect, damage.rect).is_some()
        {
            let x = other.rect.x.min(damage.rect.x);
            let y = other.rect.y.min(damage.rect.y);
            let right = (other.rect.x + other.rect.width).max(damage.rect.x + damage.rect.width);
            let bottom = (other.rect.y + other.rect.height).max(damage.rect.y + damage.rect.height);
            damage.rect = DeviceRect::new(x, y, right - x, bottom - y);
            regions.swap_remove(i);
            i = 0;
        } else {
            i += 1;
        }
    }
    regions.push(damage);
}

#[derive(Clone)]
pub(crate) struct PaintTarget {
    view_revision: u64,
    page: usize,
    source: DeviceRect,
    canvas: (u32, u32),
    matrix: Option<pdfoxium::Matrix>,
    rotation: Rotation,
    config: RenderConfig,
}
pub(crate) struct InputPaint {
    pub generation: u64,
    pub work_ms: Option<(f64, f64)>,
    view_revision: u64,
    patches: Vec<(Damage, PoolBitmap)>,
    fallback: Vec<(usize, pdfoxium::Rect)>,
    full: bool,
}
pub(crate) fn paint_input(
    form: &pdfoxium::FormSession,
    input: &pdfoxium::FormInputResult,
    generation: u64,
    targets: &[PaintTarget],
) -> pdfoxium::Result<InputPaint> {
    let view_revision = targets.first().map_or(0, |target| target.view_revision);
    // Reflow invalidates the target geometry itself, including equal-sized pages.
    // Do not paint new XFA controls through a pre-reflow matrix.
    if input
        .events
        .iter()
        .any(|e| matches!(e, pdfoxium::FormEvent::PagesChanged))
    {
        return Ok(InputPaint {
            generation,
            work_ms: None,
            view_revision,
            patches: vec![],
            fallback: vec![],
            full: true,
        });
    }
    let mut regions = Vec::new();
    let mut fallback = Vec::new();
    let mut invalidated = false;
    for event in &input.events {
        let pdfoxium::FormEvent::Invalidate { page, rect } = event else {
            continue;
        };
        invalidated = true;
        for target in targets.iter().filter(|t| t.page == *page) {
            let Some(matrix) = target.matrix else {
                fallback.push((*page, *rect));
                continue;
            };
            if let Some(rect) = damage_pixels(
                crate::text::normalized(matrix, *rect),
                target.canvas,
                target.rotation,
            )
            .and_then(|r| intersection(r, target.source))
            {
                merge_damage(
                    &mut regions,
                    Damage {
                        page: *page,
                        rect,
                        canvas: target.canvas,
                    },
                );
            }
        }
    }
    let full = !invalidated
        && input.events.iter().any(|e| {
            matches!(
                e,
                pdfoxium::FormEvent::FocusChanged
                    | pdfoxium::FormEvent::TextFocus { .. }
                    | pdfoxium::FormEvent::Changed
            )
        });
    if regions.len() > 64
        || regions
            .iter()
            .map(|r| u64::from(r.rect.width) * u64::from(r.rect.height))
            .sum::<u64>()
            > 1_048_576
    {
        return Ok(InputPaint {
            generation,
            work_ms: None,
            view_revision,
            patches: vec![],
            fallback: input
                .events
                .iter()
                .filter_map(|e| match e {
                    pdfoxium::FormEvent::Invalidate { page, rect } => Some((*page, *rect)),
                    _ => None,
                })
                .collect(),
            full,
        });
    }
    let mut patches = Vec::new();
    for region in regions {
        let target = targets
            .iter()
            .find(|t| t.page == region.page && t.canvas == region.canvas)
            .unwrap();
        let bitmap = form.render(region.page, &target.config.clone().device_clip(region.rect))?;
        patches.push((
            region,
            PoolBitmap {
                bitmap,
                source: pdfoxium::thread::PoolSource {
                    page_index: region.page,
                    generation,
                },
            },
        ));
    }
    Ok(InputPaint {
        generation,
        work_ms: None,
        view_revision,
        patches,
        fallback,
        full,
    })
}

type Key = (usize, TileId);
struct Tile {
    page: usize,
    texture: TextureHandle,
    source: DeviceRect,
    canvas: (u32, u32),
    bytes: usize,
    used: u64,
}
impl Tile {
    fn region_key(&self) -> (usize, (u32, u32), u32, u32, u32, u32) {
        (
            self.page,
            self.canvas,
            self.source.x,
            self.source.y,
            self.source.width,
            self.source.height,
        )
    }
}
struct Pending {
    page: usize,
    request: RenderRequest,
    ticket: Ticket<PoolBitmap>,
    canvas: (u32, u32),
    requested: Instant,
}
pub enum Renderer {
    Static(Box<RenderPool>),
    Forms(pdfoxium::thread::DocumentThread),
}
impl From<RenderPool> for Renderer {
    fn from(pool: RenderPool) -> Self {
        Self::Static(Box::new(pool))
    }
}
impl Renderer {
    pub fn has_layers(&self) -> bool {
        match self {
            Self::Static(pool) => pool.has_optional_content(),
            Self::Forms(_) => true,
        }
    }
    pub fn document(&self) -> Option<&pdfoxium::thread::DocumentThread> {
        match self {
            Self::Forms(doc) => Some(doc),
            _ => None,
        }
    }
    pub fn render(
        &self,
        page: usize,
        config: RenderConfig,
        priority: Priority,
    ) -> Ticket<PoolBitmap> {
        match self {
            Self::Static(pool) => pool.render(page, config, priority),
            Self::Forms(doc) => doc.render(page, config, priority),
        }
    }
    pub fn generation(&self) -> u64 {
        match self {
            Self::Static(pool) => pool.generation(),
            Self::Forms(doc) => doc.generation(),
        }
    }
    pub fn preview_generation(&self) -> u64 {
        match self {
            Self::Static(pool) => pool.generation(),
            Self::Forms(doc) => doc.preview_generation(),
        }
    }
    pub fn worker_count(&self) -> usize {
        match self {
            Self::Static(pool) => pool.worker_count(),
            Self::Forms(_) => 1,
        }
    }
}
pub struct Loaded {
    pub path: PathBuf,
    pub sizes: Vec<Size>,
    pub labels: Vec<Option<String>>,
    pub pool: Renderer,
    pub bytes: Arc<[u8]>,
    pub outline: Vec<pdfoxium::OutlineItem>,
    pub layers: pdfoxium::OptionalContentState,
}
fn document_worker(
    bytes: Arc<[u8]>,
    ctx: crate::platform::Repaint,
) -> Result<pdfoxium::thread::DocumentThread, String> {
    let doc = pdfoxium::thread::DocumentThread::spawn(
        Source::Bytes(bytes),
        Config::default(),
        pdfoxium::thread::DocOptions {
            forms: pdfoxium::FormOptions {
                javascript: true,
                javascript_loop_limit: Some(1_000_000),
                highlight: Some(pdfoxium::Color::rgba(80, 140, 255, 25)),
            },
            ..Default::default()
        },
    )
    .map_err(|e| e.to_string())?;
    let wake = ctx.clone();
    doc.set_observer(move || wake.request_repaint());
    Ok(doc)
}
pub fn load(path: PathBuf, ctx: egui::Context) -> mpsc::Receiver<Result<Loaded, String>> {
    load_input(path, None, ctx)
}
pub fn load_bytes(
    path: PathBuf,
    bytes: Arc<[u8]>,
    ctx: egui::Context,
) -> mpsc::Receiver<Result<Loaded, String>> {
    load_input(path, Some(bytes), ctx)
}
fn load_input(
    path: PathBuf,
    bytes: Option<Arc<[u8]>>,
    ctx: egui::Context,
) -> mpsc::Receiver<Result<Loaded, String>> {
    let (tx, rx) = mpsc::channel();
    let ctx = crate::platform::repaint(&ctx);
    crate::platform::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let bytes: Arc<[u8]> = match bytes {
                Some(bytes) => bytes,
                None => std::fs::read(&path).map_err(|e| e.to_string())?.into(),
            };
            loaded_from_bytes(path, bytes, ctx.clone(), false)
        }))
        .unwrap_or_else(|_| Err("document loader panicked".into()));
        let _ = tx.send(result);
        ctx.request_repaint();
    });
    rx
}

pub fn loaded_from_bytes(
    path: PathBuf,
    bytes: Arc<[u8]>,
    ctx: crate::platform::Repaint,
    force_worker: bool,
) -> Result<Loaded, String> {
    let document = Document::from_bytes(bytes.clone()).map_err(|e| e.to_string())?;
    let layouts = document.page_layouts().map_err(|e| e.to_string())?;
    let sizes = layouts.iter().map(|p| p.size).collect();
    let labels = layouts.into_iter().map(|p| p.label).collect();
    let mut outline = document.outline().map_err(|e| e.to_string())?;
    for item in &mut outline {
        item.title = crate::outline::title(&item.title);
    }
    let layers = document
        .optional_content_state()
        .map_err(|e| e.to_string())?;
    let form_type = document.form_type().map_err(|e| e.to_string())?;
    drop(document);
    let pool = if force_worker || form_type != pdfoxium::FormType::None {
        let doc = document_worker(bytes.clone(), ctx.clone())?;
        Renderer::Forms(doc)
    } else {
        RenderPool::new(Source::Bytes(bytes.clone()), Config::default(), 2)
            .map_err(|e| e.to_string())?
            .into()
    };
    Ok(Loaded {
        path,
        sizes,
        labels,
        outline,
        layers,
        pool,
        bytes,
    })
}

#[derive(Default)]
struct PipelineStats {
    results: u64,
    textures: u64,
    partial_updates: u64,
    bytes: u64,
    conversion_ms: f64,
    enqueue_ms: f64,
    request_to_collect_ms: f64,
}
#[derive(Clone, PartialEq)]
struct Position {
    scroll: Vec2,
    zoom: f32,
    rotation: Rotation,
    fit: bool,
    page: usize,
}
pub struct View {
    pub loaded: Loaded,
    pub text: text::State,
    pub render_revision: u64,
    paint_revision: u64,
    pub thumbnails: crate::thumbnails::State,
    pub forms: crate::form_view::State,
    pub annotations: crate::annotations::State,
    layer_update: Option<Ticket<()>>,
    layer_zoom: Option<f32>,
    focus_annotation: bool,
    xfa_layout: Option<pdfoxium::thread::Ticket<Vec<Size>>>,
    editor_loading: Option<mpsc::Receiver<Result<pdfoxium::thread::DocumentThread, String>>>,
    history: navigation::History<Position>,
    destination_point: Option<(usize, Pos2)>,
    pub last_external_uri: Option<String>,
    modes: Vec<Mode>,
    grids: Vec<Option<TileGrid>>,
    screen: Screen,
    policy_pages: Vec<Value>,
    pipeline: PipelineStats,
    pub zoom: f32,
    pub rotation: Rotation,
    pub scroll: Vec2,
    pub page: usize,
    pub jump: Option<usize>,
    pub fit: bool,
    pub visible: Vec<usize>,
    pub errors: HashMap<usize, String>,
    pub complete: bool,
    pub rendering: bool,
    pub canvas_rect: Rect,
    page_rects: Vec<(usize, Rect)>,
    tiles: HashMap<Key, Tile>,
    backdrop: Vec<Tile>,
    pending: Vec<Pending>,
    dirty: Vec<Damage>,
    patches: Vec<Patch>,
    pixels_per_point: f32,
    frame: u64,
    layout: Option<Layout>,
    zoom_anchor: Option<Anchor>,
    zoom_until: Option<Instant>,
    zoom_frame: Option<u64>,
    reveal: Option<(usize, Rect)>,
}
impl View {
    pub fn new(loaded: Loaded, ctx: egui::Context) -> Self {
        let screen = Screen::read(&ctx);
        let modes = vec![Mode::Whole; loaded.sizes.len()];
        let grids = vec![None; loaded.sizes.len()];
        let text = text::State::new(loaded.bytes.clone(), ctx);
        let xfa_layout = loaded
            .pool
            .document()
            .filter(|d| d.is_xfa())
            .map(|d| d.call(|_, form| form.page_sizes()));
        let mut view = Self {
            xfa_layout,
            text,
            render_revision: 0,
            paint_revision: 0,
            thumbnails: Default::default(),
            forms: Default::default(),
            annotations: Default::default(),
            layer_update: None,
            layer_zoom: None,
            focus_annotation: false,
            editor_loading: None,
            history: navigation::History::default(),
            destination_point: None,
            last_external_uri: None,
            modes,
            grids,
            screen,
            policy_pages: Vec::new(),
            pipeline: PipelineStats::default(),
            loaded,
            zoom: 1.0,
            rotation: Rotation::None,
            scroll: Vec2::ZERO,
            page: 0,
            jump: None,
            fit: true,
            visible: Vec::new(),
            errors: HashMap::new(),
            complete: false,
            rendering: true,
            canvas_rect: Rect::NOTHING,
            page_rects: Vec::new(),
            tiles: HashMap::new(),
            backdrop: Vec::new(),
            pending: Vec::new(),
            dirty: Vec::new(),
            patches: Vec::new(),
            pixels_per_point: 1.0,
            frame: 0,
            layout: None,
            zoom_anchor: None,
            zoom_until: None,
            zoom_frame: None,
            reveal: None,
        };
        if !view.loaded.layers.is_empty() {
            let mut environment = view.loaded.layers.environment();
            environment.language = crate::platform::language();
            if let Err(error) = view
                .loaded
                .layers
                .set_environment(environment)
                .map_err(|e| e.to_string())
                .and_then(|_| view.layers_changed())
            {
                view.forms.error = Some(error);
            }
        }
        view
    }
    pub fn is_xfa(&self) -> bool {
        self.loaded.pool.document().is_some_and(|d| d.is_xfa())
    }
    pub fn set_zoom(&mut self, zoom: f32) -> Result<(), String> {
        self.set_zoom_at(zoom, self.canvas_rect.center())
    }
    fn set_zoom_at(&mut self, zoom: f32, point: Pos2) -> Result<(), String> {
        if !zoom.is_finite() || !(0.1..=8.0).contains(&zoom) {
            return Err("zoom must be between 0.1 and 8.0".into());
        }
        if (self.zoom - zoom).abs() > f32::EPSILON {
            self.annotations.cancel_drag();
            if self.zoom_anchor.is_none() && self.jump.is_none() && self.canvas_rect.is_positive() {
                self.zoom_anchor = self.layout.as_ref().and_then(|layout| {
                    layout.anchor(
                        self.scroll,
                        (point - self.canvas_rect.min).clamp(Vec2::ZERO, self.canvas_rect.size()),
                    )
                });
            }
            self.zoom = zoom;
            self.invalidate(true);
        }
        self.zoom_until = None;
        self.fit = false;
        Ok(())
    }
    pub fn rotate(&mut self) {
        self.annotations.cancel_drag();
        self.zoom_anchor = None;
        self.zoom_until = None;
        self.rotation = match self.rotation {
            Rotation::None => Rotation::Cw90,
            Rotation::Cw90 => Rotation::Cw180,
            Rotation::Cw180 => Rotation::Cw270,
            Rotation::Cw270 => Rotation::None,
        };
        self.invalidate(false);
        self.jump = Some(self.page);
    }
    fn invalidate(&mut self, preserve: bool) {
        self.paint_revision = self.paint_revision.wrapping_add(1);
        self.pending.clear();
        self.forms.paint_targets.clear();
        self.dirty.clear();
        self.patches.clear();
        if preserve && !self.tiles.is_empty() {
            // A second change can arrive before all replacement tiles do.
            // Keep older coverage for those gaps, with the newest pixels on top.
            self.retire_backdrop(&HashSet::new());
            self.backdrop
                .extend(self.tiles.drain().map(|(_, tile)| tile));
        } else if !preserve {
            self.tiles.clear();
            self.backdrop.clear();
        }
        self.errors.clear();
        self.complete = false;
    }
    fn retire_backdrop(&mut self, ready_pages: &HashSet<usize>) {
        self.backdrop
            .retain(|tile| !ready_pages.contains(&tile.page));
        if !self.backdrop.is_empty() && !self.tiles.is_empty() {
            // Release each old tile as its replacement arrives, rather than
            // briefly doubling the cache until the entire page is ready.
            let replaced: HashSet<_> = self.tiles.values().map(Tile::region_key).collect();
            self.backdrop
                .retain(|tile| !replaced.contains(&tile.region_key()));
        }
    }
    fn invalidate_regions(&mut self, damage: &[(usize, Rect)]) {
        // Full requests may contain pixels from the previous document generation.
        self.pending.clear();
        for patch in self.patches.drain(..) {
            merge_damage(&mut self.dirty, patch.damage);
        }
        for tile in self.tiles.values() {
            for (page, rect) in damage.iter().filter(|(page, _)| *page == tile.page) {
                if let Some(rect) = damage_pixels(*rect, tile.canvas, self.rotation)
                    .and_then(|rect| intersection(rect, tile.source))
                {
                    merge_damage(
                        &mut self.dirty,
                        Damage {
                            page: *page,
                            rect,
                            canvas: tile.canvas,
                        },
                    );
                }
            }
        }
        if self.dirty.len() > 64 {
            self.invalidate(true);
        }
        self.errors
            .retain(|page, _| !damage.iter().any(|(p, _)| p == page));
        self.complete = false;
    }
    fn collect_patches(&mut self, ctx: &egui::Context) {
        let mut i = 0;
        while i < self.patches.len() {
            let Some(result) = self.patches[i].ticket.try_take() else {
                i += 1;
                continue;
            };
            let patch = self.patches.swap_remove(i);
            let damage = patch.damage;
            self.pipeline.request_to_collect_ms += patch.requested.elapsed().as_secs_f64() * 1000.;
            match result {
                Ok(result) if result.source.generation == self.loaded.pool.generation() => {
                    self.upload_patch(damage, &result, ctx);
                    self.forms.typing.uploaded(result.source.generation);
                    self.render_revision += 1;
                    ctx.request_repaint();
                }
                Ok(_) | Err(pdfoxium::Error::Cancelled) => merge_damage(&mut self.dirty, damage),
                Err(error) => {
                    self.errors.insert(damage.page, error.to_string());
                }
            }
        }
    }
    fn upload_patch(&mut self, damage: Damage, result: &PoolBitmap, ctx: &egui::Context) {
        let start = Instant::now();
        let converter = texture::Converter::new(&result.bitmap);
        self.pipeline.conversion_ms += start.elapsed().as_secs_f64() * 1000.;
        self.pipeline.results += 1;
        for tile in self
            .tiles
            .values_mut()
            .filter(|t| t.page == damage.page && t.canvas == damage.canvas)
        {
            let Some(rect) = intersection(tile.source, damage.rect) else {
                continue;
            };
            let start = Instant::now();
            let image = converter.image(DeviceRect::new(
                rect.x - damage.rect.x,
                rect.y - damage.rect.y,
                rect.width,
                rect.height,
            ));
            self.pipeline.conversion_ms += start.elapsed().as_secs_f64() * 1000.;
            let start = Instant::now();
            tile.texture.set_partial(
                [
                    (rect.x - tile.source.x) as usize,
                    (rect.y - tile.source.y) as usize,
                ],
                image,
                egui::TextureOptions::LINEAR,
            );
            self.pipeline.enqueue_ms += start.elapsed().as_secs_f64() * 1000.;
            self.pipeline.partial_updates += 1;
            self.pipeline.bytes += u64::from(rect.width) * u64::from(rect.height) * 4;
        }
        ctx.request_repaint();
    }
    fn collect_input_paints(&mut self, ctx: &egui::Context) {
        let mut fresh = Vec::new();
        let mut stale = Vec::new();
        for paint in std::mem::take(&mut self.forms.ready_paints) {
            // A zoom/rotation/configuration change already scheduled a fresh
            // view. Its old input paint must not touch the new textures.
            if paint.view_revision != self.paint_revision {
                continue;
            }
            if paint.full {
                self.invalidate(true);
                continue;
            }
            for (damage, result) in paint.patches {
                if result.source.generation == self.loaded.pool.generation() {
                    self.upload_patch(damage, &result, ctx);
                    self.forms.typing.uploaded(result.source.generation);
                    self.render_revision += 1;
                    fresh.push(damage);
                } else {
                    stale.push(damage);
                }
            }
            if !paint.fallback.is_empty() {
                let damage: Option<Vec<_>> = paint
                    .fallback
                    .into_iter()
                    .map(|(p, r)| self.forms.normalized_damage(p, r).map(|r| (p, r)))
                    .collect();
                if let Some(damage) = damage {
                    self.invalidate_regions(&damage);
                } else {
                    self.invalidate(true);
                }
            }
        }
        let covered = |damage: &Damage| {
            fresh.iter().any(|f| {
                f.page == damage.page
                    && f.canvas == damage.canvas
                    && intersection(f.rect, damage.rect) == Some(damage.rect)
            })
        };
        self.dirty.retain(|damage| !covered(damage));
        self.patches.retain(|patch| !covered(&patch.damage));
        for damage in stale {
            if !covered(&damage) {
                merge_damage(&mut self.dirty, damage);
            }
        }
    }
    fn schedule_patches(&mut self) {
        while self.pending.len() + self.patches.len() < MAX_PENDING {
            let Some(damage) = self.dirty.pop() else {
                break;
            };
            if !self.tiles.values().any(|tile| {
                tile.page == damage.page
                    && tile.canvas == damage.canvas
                    && intersection(tile.source, damage.rect).is_some()
            }) {
                continue;
            }
            let ticket = self.loaded.pool.render(
                damage.page,
                self.config().device_clip(damage.rect),
                Priority::Visible,
            );
            self.forms.typing.queued(self.loaded.pool.generation());
            self.patches.push(Patch {
                damage,
                ticket,
                requested: Instant::now(),
            });
        }
    }
    fn position(&self) -> Position {
        Position {
            scroll: self.scroll,
            zoom: self.zoom,
            rotation: self.rotation,
            fit: self.fit,
            page: self.page,
        }
    }
    pub fn history_step(&mut self, backwards: bool) -> Result<(), String> {
        self.annotations.cancel_drag();
        self.forms.blur(self.loaded.pool.document());
        let current = self.position();
        let position = self
            .history
            .step(current, backwards)
            .ok_or("no navigation history in that direction")?;
        let rotated = self.rotation != position.rotation;
        self.text.cancel_navigation();
        self.text.navigation_error = None;
        self.zoom_anchor = None;
        self.jump = None;
        self.reveal = None;
        self.destination_point = None;
        self.zoom = position.zoom;
        self.rotation = position.rotation;
        self.fit = position.fit;
        self.page = position.page;
        if self.thumbnails.open {
            self.thumbnails.scroll_to = Some(self.page);
        }
        self.scroll = position.scroll;
        self.invalidate(!rotated);
        Ok(())
    }
    pub fn page_label(&self, page: usize) -> String {
        navigation::page_label(&self.loaded.labels, page)
    }
    pub fn page_caption(&self, page: usize) -> String {
        navigation::page_caption(&self.loaded.labels, page)
    }
    pub fn go_to_label(&mut self, label: &str) -> Result<(), String> {
        let page = self
            .loaded
            .labels
            .iter()
            .position(|candidate| candidate.as_deref() == Some(label))
            .ok_or("page label not found")?;
        self.go_to(page)
    }
    pub fn target_label(&self, target: &Option<pdfoxium::LinkTarget>) -> String {
        match target {
            Some(pdfoxium::LinkTarget::Destination(d))
            | Some(pdfoxium::LinkTarget::NamedDestination {
                resolved: Some(d), ..
            }) => format!("Page {}", self.page_caption(d.page_index)),
            _ => navigation::target_label(target),
        }
    }
    pub fn navigation_state(&self) -> Value {
        json!({"can_back":!self.history.back.is_empty(),"can_forward":!self.history.forward.is_empty(),"pending":self.text.navigation_busy(),"error":self.text.navigation_error,"last_external_uri":self.last_external_uri,"outline_count":self.loaded.outline.len()})
    }
    pub fn links_state(&mut self, page: usize) -> Result<Value, String> {
        if page >= self.loaded.sizes.len() {
            return Err("page out of range".into());
        }
        self.text.request_links(page);
        self.complete = false;
        Ok(
            json!({"page":page+1,"pending":!self.text.links.contains_key(&page),"links":self.text.links.get(&page).into_iter().flatten().map(|link|json!({"index":link.index,"rect":rect_json(link.rect),"screen_rects":self.screen_rects(page,&[link.rect]),"target":self.target_label(&link.target),"actionable":link.target.is_some()})).collect::<Vec<_>>()}),
        )
    }
    pub fn follow_link(
        &mut self,
        page: usize,
        index: usize,
        ctx: &egui::Context,
    ) -> Result<(), String> {
        let target = self
            .text
            .links
            .get(&page)
            .ok_or("page links are not loaded; request ui.links and wait first")?
            .iter()
            .find(|link| link.index == index)
            .ok_or("link index out of range")?
            .target
            .clone()
            .ok_or("link has no supported target")?;
        self.follow(target, ctx)
    }
    pub fn follow_outline(&mut self, index: usize, ctx: &egui::Context) -> Result<(), String> {
        let target = self
            .loaded
            .outline
            .get(index)
            .ok_or("outline index out of range")?
            .target
            .clone()
            .ok_or("outline item has no supported target")?;
        self.follow(target, ctx)
    }
    fn follow(&mut self, target: pdfoxium::LinkTarget, ctx: &egui::Context) -> Result<(), String> {
        use pdfoxium::LinkTarget;
        match target {
            LinkTarget::Destination(destination)
            | LinkTarget::NamedDestination {
                resolved: Some(destination),
                ..
            } => {
                if destination.page_index >= self.loaded.sizes.len() {
                    return Err("destination page out of range".into());
                }
                self.text.navigate(destination, self.page);
                self.complete = false;
            }
            LinkTarget::OptionalContent(action) => {
                self.loaded
                    .layers
                    .apply_action(&action)
                    .map_err(|e| e.to_string())?;
                self.layers_changed()?;
            }
            LinkTarget::Uri(bytes) => {
                let uri = navigation::uri(&bytes)?;
                ctx.open_url(egui::OpenUrl::new_tab(uri.clone()));
                self.last_external_uri = Some(uri);
            }
            LinkTarget::NamedAction(name) => match name.as_slice() {
                b"NextPage" => {
                    self.go_to((self.page + 1).min(self.loaded.sizes.len().saturating_sub(1)))?
                }
                b"PrevPage" => self.go_to(self.page.saturating_sub(1))?,
                b"FirstPage" => self.go_to(0)?,
                b"LastPage" => self.go_to(self.loaded.sizes.len().saturating_sub(1))?,
                b"GoBack" => self.history_step(true)?,
                b"GoForward" => self.history_step(false)?,
                _ => return Err("unsupported named navigation action".into()),
            },
            _ => return Err("unsupported or unresolved link target".into()),
        }
        Ok(())
    }
    fn apply_destination(&mut self, resolved: navigation::Resolved) -> Result<(), String> {
        use pdfoxium::DestinationView;
        let destination = resolved.destination;
        let page = destination.page_index;
        let size = self.size(page);
        let viewport = (self.canvas_rect.size() - Vec2::splat(16.0)).max(Vec2::splat(1.0));
        // Null XYZ fields preserve the current page-local viewport coordinate.
        let current_unit = self
            .page_rects
            .iter()
            .find(|(p, _)| *p == self.page)
            .map(|(_, r)| {
                let p = (self.canvas_rect.min - r.min) / r.size();
                rotate_unit(
                    p.to_pos2(),
                    match self.rotation {
                        Rotation::Cw90 => Rotation::Cw270,
                        Rotation::Cw270 => Rotation::Cw90,
                        r => r,
                    },
                )
            })
            .unwrap_or(Pos2::ZERO);
        let pdf = crate::text::inverse(resolved.current_matrix)?.transform(pdfoxium::Point {
            x: current_unit.x,
            y: current_unit.y,
        });
        let point;
        let zoom = match destination.view {
            DestinationView::Xyz { left, top, zoom } => {
                point =
                    navigation::point(resolved.matrix, left.unwrap_or(pdf.x), top.unwrap_or(pdf.y));
                zoom.filter(|z| z.is_finite() && *z > 0.0)
                    .unwrap_or(self.zoom)
            }
            DestinationView::Fit => {
                point = rotate_unit(
                    Pos2::ZERO,
                    match self.rotation {
                        Rotation::Cw90 => Rotation::Cw270,
                        Rotation::Cw270 => Rotation::Cw90,
                        r => r,
                    },
                );
                (viewport.x / size.x).min(viewport.y / size.y)
            }
            DestinationView::FitH { top } => {
                point =
                    navigation::point(resolved.matrix, resolved.crop.left, top.unwrap_or(pdf.y));
                viewport.x / size.x
            }
            DestinationView::FitV { left } => {
                point =
                    navigation::point(resolved.matrix, left.unwrap_or(pdf.x), resolved.crop.top);
                viewport.y / size.y
            }
            DestinationView::FitR {
                left: Some(left),
                bottom: Some(bottom),
                right: Some(right),
                top: Some(top),
            } if right > left && top > bottom => {
                let unit = text::normalized(
                    resolved.matrix,
                    pdfoxium::Rect {
                        left,
                        bottom,
                        right,
                        top,
                    },
                );
                let rotated = text_screen_rect(
                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                    unit,
                    self.rotation,
                );
                // The top-left after view rotation may be a different corner.
                let inverse = match self.rotation {
                    Rotation::Cw90 => Rotation::Cw270,
                    Rotation::Cw270 => Rotation::Cw90,
                    r => r,
                };
                point = rotate_unit(rotated.min, inverse);
                (viewport.x / (rotated.width() * size.x))
                    .min(viewport.y / (rotated.height() * size.y))
            }
            _ => {
                return Err(
                    "unsupported destination view (content-bounds fits are not implemented)".into(),
                )
            }
        };
        if !zoom.is_finite() || !point.is_finite() {
            return Err("invalid destination coordinates".into());
        }
        self.annotations.cancel_drag();
        self.history.record(self.position());
        self.set_zoom(zoom.clamp(0.1, 8.0))?;
        self.zoom_anchor = None;
        self.reveal = None;
        self.jump = None;
        self.destination_point = Some((page, rotate_unit(point, self.rotation)));
        self.page = page;
        if self.thumbnails.open {
            self.thumbnails.scroll_to = Some(page);
        }
        self.complete = false;
        Ok(())
    }
    pub fn go_to(&mut self, page: usize) -> Result<(), String> {
        self.annotations.cancel_drag();
        if page >= self.loaded.sizes.len() {
            return Err("page out of range".into());
        }
        self.forms.blur(self.loaded.pool.document());
        self.history.record(self.position());
        self.text.cancel_navigation();
        self.text.navigation_error = None;
        self.destination_point = None;
        self.reveal = None;
        self.zoom_anchor = None;
        self.jump = Some(page);
        if self.thumbnails.open {
            self.thumbnails.scroll_to = Some(page);
        }
        self.page = page;
        self.complete = false;
        Ok(())
    }
    pub fn layers_changed(&mut self) -> Result<(), String> {
        self.invalidate(true);
        self.thumbnails.invalidate();
        self.text.cancel_thumbnails();
        self.text.set_layers(self.loaded.layers.clone());
        self.annotations.layers_changed();
        match &self.loaded.pool {
            Renderer::Static(pool) => pool
                .set_optional_content(self.loaded.layers.snapshot())
                .map_err(|e| e.to_string())?,
            Renderer::Forms(doc) => {
                let state = self.loaded.layers.clone();
                self.layer_update =
                    Some(doc.call(move |document, _| document.set_optional_content_state(&state)));
            }
        }
        Ok(())
    }
    pub fn layer_order(&self) -> Vec<pdfoxium::OptionalContentOrder> {
        use pdfoxium::OptionalContentOrder as Order;
        fn filter(
            order: Vec<Order>,
            groups: &[pdfoxium::OptionalContentGroup],
            used: &std::collections::BTreeSet<usize>,
        ) -> Vec<Order> {
            order
                .into_iter()
                .filter_map(|n| match n {
                    Order::Label { text, children } => {
                        let children = filter(children, groups, used);
                        (!children.is_empty()).then_some(Order::Label { text, children })
                    }
                    Order::Group { id, children } => {
                        let children = filter(children, groups, used);
                        let index = groups.iter().position(|g| g.id == id)?;
                        if used.contains(&index) {
                            Some(Order::Group { id, children })
                        } else {
                            (!children.is_empty()).then(|| Order::Label {
                                text: groups[index].name.clone(),
                                children,
                            })
                        }
                    }
                })
                .collect()
        }
        let order = self.loaded.layers.order();
        if self.loaded.layers.lists_all_pages() {
            return order;
        }
        let used = self
            .visible
            .iter()
            .filter_map(|p| self.text.page_layers.get(p))
            .flatten()
            .copied()
            .collect();
        filter(order, &self.loaded.layers.groups(), &used)
    }
    pub fn layers_state(&self) -> Value {
        json!({"revision":self.loaded.layers.snapshot().revision(),"groups":self.loaded.layers.groups().iter().enumerate().map(|(index,g)|json!({"index":index,"name":g.name,"visible":g.visible,"locked":g.locked})).collect::<Vec<_>>(),"configurations":self.loaded.layers.configurations().iter().enumerate().map(|(index,c)|json!({"index":index,"name":c.name})).collect::<Vec<_>>()})
    }
    fn config(&self) -> RenderConfig {
        RenderConfig::scale(self.zoom * self.pixels_per_point)
            .rotation(self.rotation)
            .format(PixelFormat::Rgba8)
            .optional_content(self.loaded.layers.snapshot())
    }
    fn size(&self, page: usize) -> Vec2 {
        let size = self.loaded.sizes[page];
        match self.rotation {
            Rotation::None | Rotation::Cw180 => egui::vec2(size.width, size.height),
            _ => egui::vec2(size.height, size.width),
        }
    }
    fn grid_for(
        &mut self,
        page: usize,
        dimensions: (u32, u32),
        visible_area: u64,
    ) -> pdfoxium::Result<TileGrid> {
        self.modes[page] = self.modes[page].update(
            dimensions,
            visible_area,
            self.screen.area(),
            self.screen.max_texture_side,
        );
        let grid = TileGrid::new(
            dimensions.0,
            dimensions.1,
            TileGridOptions {
                tile_size: 512.min(self.screen.max_texture_side),
                whole_page_max_pixels: if self.modes[page] == Mode::Whole {
                    u64::MAX
                } else {
                    0
                },
            },
        )?;
        if self.grids[page] != Some(grid) {
            // Tile IDs are local to a grid. Never satisfy a new grid with old
            // (column,row) keys; retain their correctly positioned backdrop.
            self.pending.retain(|request| request.page != page);
            let keys: Vec<_> = self
                .tiles
                .keys()
                .copied()
                .filter(|(p, _)| *p == page)
                .collect();
            if !keys.is_empty() {
                self.backdrop.retain(|tile| tile.page != page);
                for key in keys {
                    self.backdrop.push(self.tiles.remove(&key).unwrap());
                }
            }
            self.errors.remove(&page);
            self.grids[page] = Some(grid);
            self.render_revision += 1;
        }
        Ok(grid)
    }
    fn collect_page(&mut self, ctx: &egui::Context, page: usize) {
        let mut index = 0;
        while index < self.pending.len() {
            if self.pending[index].page != page {
                index += 1;
                continue;
            }
            let Some(result) = self.pending[index].ticket.try_take() else {
                index += 1;
                continue;
            };
            let pending = self.pending.swap_remove(index);
            self.pipeline.request_to_collect_ms +=
                pending.requested.elapsed().as_secs_f64() * 1000.0;
            match result {
                Ok(result) if result.source.generation == self.loaded.pool.generation() => {
                    let started = Instant::now();
                    let converter = texture::Converter::new(&result.bitmap);
                    self.pipeline.conversion_ms += started.elapsed().as_secs_f64() * 1000.0;
                    self.pipeline.results += 1;
                    for (id, source) in pending.request.tiles() {
                        let started = Instant::now();
                        let image = converter.image(source);
                        self.pipeline.conversion_ms += started.elapsed().as_secs_f64() * 1000.0;
                        // Also defend against a backend limit change while a
                        // request was in flight; grid_for normally cancels it.
                        if image
                            .size
                            .iter()
                            .any(|&side| side > self.screen.max_texture_side as usize)
                        {
                            self.errors.insert(
                                page,
                                "rendered texture exceeds the GPU dimension limit".into(),
                            );
                            continue;
                        }
                        let bytes = image.pixels.len() * 4;
                        let started = Instant::now();
                        let texture = ctx.load_texture(
                            format!("page-{page}-{}-{}", id.column, id.row),
                            image,
                            egui::TextureOptions::LINEAR,
                        );
                        self.pipeline.enqueue_ms += started.elapsed().as_secs_f64() * 1000.0;
                        self.pipeline.textures += 1;
                        self.pipeline.bytes += bytes as u64;
                        let region = pending.request.rect();
                        self.tiles.insert(
                            (page, id),
                            Tile {
                                page,
                                texture,
                                source: DeviceRect::new(
                                    region.x + source.x,
                                    region.y + source.y,
                                    source.width,
                                    source.height,
                                ),
                                canvas: pending.canvas,
                                bytes,
                                used: self.frame,
                            },
                        );
                    }
                    self.forms.typing.uploaded(result.source.generation);
                }
                Ok(_) | Err(pdfoxium::Error::Cancelled) => {}
                Err(error) => {
                    self.errors.insert(page, error.to_string());
                }
            }
        }
    }
    pub fn preparing_editor(&self) -> bool {
        self.editor_loading.is_some()
    }
    pub fn ensure_annotation_editor(&mut self, ctx: &egui::Context) {
        if self.loaded.pool.document().is_some() || self.editor_loading.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        let bytes = self.loaded.bytes.clone();
        let ctx = crate::platform::repaint(ctx);
        self.editor_loading = Some(rx);
        self.complete = false;
        crate::platform::spawn(move || {
            let result = document_worker(bytes, ctx.clone());
            let _ = tx.send(result);
            ctx.request_repaint();
        });
    }
    pub fn show(&mut self, ui: &mut egui::Ui) {
        if let Some(result) = self.layer_update.as_mut().and_then(|t| t.try_take()) {
            self.layer_update = None;
            if let Err(error) = result {
                self.forms.error = Some(error.to_string());
            }
        }
        if self.layer_zoom != Some(self.zoom) {
            self.layer_zoom = Some(self.zoom);
            let mut environment = self.loaded.layers.environment();
            environment.zoom = self.zoom;
            match self.loaded.layers.set_environment(environment) {
                Ok(true) => {
                    if let Err(e) = self.layers_changed() {
                        self.forms.error = Some(e);
                    }
                }
                Ok(false) => {}
                Err(e) => self.forms.error = Some(e.to_string()),
            }
        }
        if let Some(result) = self
            .editor_loading
            .as_ref()
            .and_then(|rx| match rx.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    Some(Err("annotation editor worker stopped".into()))
                }
            })
        {
            self.editor_loading = None;
            match result {
                Ok(doc) => {
                    self.loaded.pool = Renderer::Forms(doc);
                    if let Err(error) = self.layers_changed() {
                        self.forms.error = Some(error);
                    }
                }
                Err(error) => {
                    self.annotations.error = Some(error);
                    self.annotations.deselect();
                    self.annotations.abort_unstarted_moves();
                    self.annotations.open = false;
                }
            }
        }
        if !ui.input(|i| i.focused) {
            self.annotations.cancel_drag();
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.annotations.cancel_tools();
            self.annotations.deselect();
        }
        let visual = self.forms.poll(self.loaded.pool.document(), ui.ctx());
        if let Some(layers) = self.forms.layers.take() {
            // Owner-thread actions arrive in queue order. Update the UI and
            // render snapshots without echoing another command to the worker.
            self.loaded.layers = layers;
            self.layer_zoom = None;
            self.invalidate(true);
            self.thumbnails.invalidate();
            self.text.cancel_thumbnails();
            self.text.set_layers(self.loaded.layers.clone());
            self.annotations.layers_changed();
        }
        if self.is_xfa() {
            if std::mem::take(&mut self.forms.layout_changed) && self.xfa_layout.is_none() {
                self.xfa_layout = self
                    .loaded
                    .pool
                    .document()
                    .map(|d| d.call(|_, form| form.page_sizes()));
            }
            if let Some(result) = self.xfa_layout.as_mut().and_then(|t| t.try_take()) {
                self.xfa_layout = None;
                match result {
                    Ok(sizes) if !sizes.is_empty() && sizes != self.loaded.sizes => {
                        self.loaded.sizes = sizes;
                        self.loaded.labels = vec![None; self.loaded.sizes.len()];
                        self.modes = vec![Mode::Whole; self.loaded.sizes.len()];
                        self.grids = vec![None; self.loaded.sizes.len()];
                        self.page = self.page.min(self.loaded.sizes.len() - 1);
                        self.invalidate(false);
                    }
                    Ok(_) => {}
                    Err(e) => self.forms.error = Some(e.to_string()),
                }
            }
        }
        self.annotations.poll(
            self.loaded.pool.document(),
            self.page,
            self.forms.document_revision,
        );
        let notifications = std::mem::take(&mut self.forms.notifications);
        if std::mem::take(&mut self.forms.full_damage) {
            self.invalidate(true);
        } else if visual {
            let damage: Option<Vec<_>> = notifications
                .iter()
                .filter_map(|event| match event {
                    pdfoxium::FormEvent::Invalidate { page, rect } => Some(
                        self.forms
                            .normalized_damage(*page, *rect)
                            .map(|rect| (*page, rect)),
                    ),
                    _ => None,
                })
                .collect();
            if let Some(damage) = damage.filter(|rects| !rects.is_empty()) {
                self.invalidate_regions(&damage);
            } else {
                self.invalidate(true);
            }
        }
        for notification in notifications {
            match notification {
                pdfoxium::FormEvent::NavigatePage(page) => {
                    if let Err(error) = self.go_to(page) {
                        self.forms.error = Some(error);
                    }
                }
                pdfoxium::FormEvent::Uri(uri) => {
                    if let Err(error) = self.follow(pdfoxium::LinkTarget::Uri(uri), ui.ctx()) {
                        self.forms.error = Some(error);
                    }
                }
                pdfoxium::FormEvent::Named(name) => {
                    if let Err(error) =
                        self.follow(pdfoxium::LinkTarget::NamedAction(name), ui.ctx())
                    {
                        self.forms.error = Some(error);
                    }
                }
                pdfoxium::FormEvent::Cursor(cursor) => ui.ctx().set_cursor_icon(match cursor {
                    3 | 4 => egui::CursorIcon::Text,
                    5 => egui::CursorIcon::PointingHand,
                    _ => egui::CursorIcon::Default,
                }),
                pdfoxium::FormEvent::ScriptError { message, .. }
                | pdfoxium::FormEvent::UnsupportedAction(message) => {
                    self.forms.error = Some(message)
                }
                _ => {}
            }
        }
        self.collect_input_paints(ui.ctx());
        self.collect_patches(ui.ctx());
        if self.xfa_layout.is_none() {
            self.schedule_patches();
        }
        self.frame += 1;
        self.screen = self.screen.refresh(ui.ctx());
        self.policy_pages.clear();
        if let Some(hit) = self.text.poll() {
            self.reveal_hit(hit);
        }
        if let Some((zoom, point)) =
            pinch_zoom(ui.ctx(), self.canvas_rect, self.zoom, &mut self.zoom_frame)
        {
            let _ = self.set_zoom_at(zoom, point);
            self.zoom_until = Some(Instant::now() + Duration::from_millis(150));
        }
        self.thumbnails.collect(
            &mut self.text,
            &self.loaded.pool,
            ui.ctx(),
            &self.loaded.sizes,
        );
        let gesturing = self.zoom_until.is_some_and(|until| until > Instant::now());
        if let Some(until) = self.zoom_until.filter(|_| gesturing) {
            ui.ctx()
                .request_repaint_after(until.saturating_duration_since(Instant::now()));
        }
        if self.fit && !self.loaded.sizes.is_empty() {
            // Use the widest page so scrolling through mixed sizes never changes
            // zoom or pushes a fitted page outside the horizontal viewport.
            let widest = (0..self.loaded.sizes.len())
                .map(|page| self.size(page).x)
                .fold(0.0, f32::max);
            let target = ((ui.available_width() - GAP * 2.0 - ui.spacing().scroll.bar_width)
                / widest)
                .clamp(0.1, 8.0);
            let _ = self.set_zoom(target);
            self.fit = true;
        }
        let ppp = ui.ctx().pixels_per_point();
        if ppp != self.pixels_per_point {
            self.pixels_per_point = ppp;
            self.invalidate(true);
        }
        if let Some(destination) = self.text.destination.take() {
            if let Err(error) = self.apply_destination(destination) {
                self.text.navigation_error = Some(error);
            }
        }
        let sizes: Vec<Vec2> = (0..self.loaded.sizes.len())
            .map(|page| self.size(page))
            .collect();
        let mut layout = Layout::new(&sizes, self.zoom, ui.available_width());
        let scale = self.zoom * self.pixels_per_point;
        // Match RenderConfig::scale's f64 ceil, including rotated page sizes.
        let dimensions: Vec<_> = sizes
            .iter()
            .map(|base| {
                (
                    (base.x as f64 * scale as f64).ceil() as u32,
                    (base.y as f64 * scale as f64).ceil() as u32,
                )
            })
            .collect();
        if let Some(anchor) = self.zoom_anchor.take() {
            self.scroll = layout.anchored_scroll(anchor);
        }
        if let Some(page) = self.jump.take() {
            self.scroll.y = layout.pages[page].min.y;
        }
        if let Some((page, point)) = self.destination_point.take() {
            self.scroll = (layout.pages[page].min.to_vec2()
                + point.to_vec2() * layout.pages[page].size()
                - Vec2::splat(8.0))
            .max(Vec2::ZERO);
        }
        if let Some((page, unit)) = self.reveal.take() {
            let rect = text_screen_rect(layout.pages[page], unit, self.rotation);
            self.scroll = (rect.center().to_vec2() - ui.available_size() * 0.5).max(Vec2::ZERO);
        }
        // ScrollArea clamps at the end of its frame, after painting. Clamp
        // before painting too, so destinations near an edge never flash a
        // page at an invalid horizontal or vertical offset.
        self.scroll = self.scroll.clamp(
            Vec2::ZERO,
            (layout.size - ui.available_size()).max(Vec2::ZERO),
        );
        let mut auto_scroll = 0.0;
        let mut visible = Vec::new();
        self.page_rects.clear();
        let mut wanted: HashSet<Key> = HashSet::new();
        let mut nearest = (f32::INFINITY, self.page);
        let output = egui::ScrollArea::both()
            .id_salt("document")
            .scroll_offset(self.scroll)
            .scroll_source(egui::scroll_area::ScrollSource {
                drag: egui::scroll_area::DragScroll::Never,
                ..Default::default()
            })
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, viewport| {
                let (canvas, response) =
                    ui.allocate_exact_size(layout.size, egui::Sense::click_and_drag());
                let response = response.on_hover_cursor(egui::CursorIcon::Text);
                if std::mem::take(&mut self.focus_annotation) { response.request_focus(); }
                // Keep layout/scroll anchors continuous, but snap this frame's
                // displayed geometry after applying the scroll translation.
                // Hit testing and highlights use the same displayed rectangles.
                for (page, local) in layout.pages.iter_mut().enumerate() {
                    *local = pixel_page_rect(
                        local.translate(canvas.min.to_vec2()), dimensions[page], ppp,
                    ).translate(-canvas.min.to_vec2());
                }
                for (page, local) in layout.pages.iter().copied().enumerate() {
                    if !local.intersects(viewport) {
                        continue;
                    }
                    visible.push(page);
                    if !self.is_xfa() {self.text.request_links(page);}
                    if self.xfa_layout.is_none() {self.forms.request(page,self.loaded.pool.document());}
                    let distance = (local.center().y - viewport.center().y).abs();
                    if distance < nearest.0 {
                        nearest = (distance, page);
                    }
                    let dimensions = dimensions[page];
                    let rect = pixel_page_rect(local.translate(canvas.min.to_vec2()), dimensions, ppp);
                    self.page_rects.push((page, rect));
                    ui.painter().rect_filled(
                        rect.expand(2.0).translate(egui::vec2(0.0, 2.0)),
                        2.0,
                        Color32::from_black_alpha(24),
                    );
                    ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
                    let clip = local.intersect(viewport);
                    let x = ((clip.min.x - local.min.x) * self.pixels_per_point)
                        .floor()
                        .max(0.0) as u32;
                    let y = ((clip.min.y - local.min.y) * self.pixels_per_point)
                        .floor()
                        .max(0.0) as u32;
                    let right = ((clip.max.x - local.min.x) * self.pixels_per_point)
                        .ceil()
                        .max(0.0) as u32;
                    let bottom = ((clip.max.y - local.min.y) * self.pixels_per_point)
                        .ceil()
                        .max(0.0) as u32;
                    let viewport =
                        DeviceRect::new(x, y, right.saturating_sub(x), bottom.saturating_sub(y));
                    let visible_width=viewport.width.min(dimensions.0.saturating_sub(viewport.x));
                    let visible_height=viewport.height.min(dimensions.1.saturating_sub(viewport.y));
                    let visible_area=u64::from(visible_width)*u64::from(visible_height);
                    let grid=match self.grid_for(page,dimensions,visible_area) {
                        Ok(grid)=>grid,Err(error)=>{self.errors.insert(page,error.to_string());continue;}
                    };
                    self.policy_pages.push(json!({"page":page+1,"mode":self.modes[page].name(),"full_pixels":[dimensions.0,dimensions.1],"visible_fraction":visible_area as f64/(u64::from(dimensions.0)*u64::from(dimensions.1)) as f64,"grid":[grid.columns(),grid.rows()]}));
                    if self.xfa_layout.is_none() {self.collect_page(ui.ctx(),page);}
                    wanted.extend(grid.visible_tiles(viewport).map(|id| (page, id)));
                    for tile in self.backdrop.iter_mut().filter(|tile| tile.page == page) {
                        paint_tile(ui, tile, rect, dimensions, ppp, self.frame);
                    }
                    for (id, _) in grid.visible_tiles(viewport).map(|id| (id, ())) {
                        if let Some(tile) = self.tiles.get_mut(&(page, id)) {
                            paint_tile(ui, tile, rect, dimensions, ppp, self.frame);
                        }
                    }
                    let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
                    // Search results are ordered from the starting page to
                    // the end, then wrap to page zero. Use that same order
                    // for binary lookup of each visible page's highlights.
                    let key = |p| (p < self.text.start_page, p);
                    let first = self.text.hits.partition_point(|hit| key(hit.page) < key(page));
                    let end = self.text.hits.partition_point(|hit| key(hit.page) <= key(page));
                    for index in first..end {
                        let active = self.text.active == Some(index);
                        let color = if active {
                            Color32::from_rgba_unmultiplied(255, 157, 0, 105)
                        } else {
                            Color32::from_rgba_unmultiplied(255, 217, 45, 75)
                        };
                        for unit in &self.text.hits[index].rects {
                            let highlight = text_screen_rect(rect, *unit, self.rotation);
                            painter.rect_filled(highlight, 1.0, color);
                            if active {
                                painter.rect_stroke(
                                    highlight,
                                    1.0,
                                    egui::Stroke::new(1.0_f32, Color32::from_rgb(215, 130, 0)),
                                    egui::StrokeKind::Inside,
                                );
                            }
                        }
                    }
                    for selected in self
                        .text
                        .selection
                        .pages
                        .iter()
                        .filter(|selected| selected.page == page)
                    {
                        for unit in &selected.rects {
                            painter.rect_filled(
                                text_screen_rect(rect, *unit, self.rotation),
                                1.0,
                                crate::chrome::TEXT_SELECTION,
                            );
                        }
                    }
                    let in_flight: HashSet<_> = self
                        .pending
                        .iter()
                        .filter(|p| p.page == page)
                        .flat_map(|p| p.request.tiles().map(|(id, _)| id))
                        .collect();
                    let plan = grid.plan(viewport, |id| {
                        self.tiles.contains_key(&(page, id)) || in_flight.contains(&id)
                    });
                    if !gesturing && !self.errors.contains_key(&page) {
                        for request in plan.requests() {
                            if self.pending.len()+self.patches.len() >= MAX_PENDING {
                                break;
                            }
                            let requested=Instant::now();
                            let ticket = self.loaded.pool.render(
                                page,
                                request.config(&self.config()),
                                Priority::Visible,
                            );
                            self.pending.push(Pending {
                                page,
                                request: request.clone(),
                                ticket,
                                canvas:dimensions,requested,
                            });
                        }
                    }
                    if let Some(error) = self.errors.get(&page) {
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            error,
                            egui::FontId::proportional(16.0),
                            Color32::DARK_RED,
                        );
                    }
                }
                if self.loaded.pool.document().is_some() && ui.input(|i|i.events.iter().any(|e|matches!(e,egui::Event::Key{..}|egui::Event::Text(_)|egui::Event::Paste(_)|egui::Event::Cut|egui::Event::Ime(_)|egui::Event::PointerButton{..}|egui::Event::PointerMoved(_)|egui::Event::MouseWheel{..}))) {
                    self.forms.paint_targets.clear();
                    let config=self.config();
                    for tile in self.tiles.values() {
                        let matrix=self.forms.page_matrix(tile.page);
                        self.forms.paint_targets.push(PaintTarget{view_revision:self.paint_revision,page:tile.page,source:tile.source,canvas:tile.canvas,matrix,rotation:self.rotation,config:config.clone()});
                    }
                }
                let placing_note = self.annotations.placing_note;
                let drawing=self.annotations.tool.is_some();
                if drawing {ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);}
                if placing_note {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                    if response.clicked() {
                        if let Some(location) = response.interact_pointer_pos().and_then(|p| location_at(&layout,canvas,p,self.rotation,true)) {
                            if let Some(doc) = self.loaded.pool.document() { self.annotations.place(doc,location); }
                        }
                    }
                }
                let hovered_link = ui.input(|i|i.pointer.hover_pos()).and_then(|point| {
                    self.page_rects.iter().rev().find_map(|(page,rect)| {
                        self.text.links.get(page)?.iter().rev().find(|link| text_screen_rect(*rect,link.rect,self.rotation).contains(point) && ui.clip_rect().contains(point)).map(|link|(*page,link.index,link.target.clone()))
                    })
                });
                let hovered_annotation = ui.input(|i|i.pointer.hover_pos()).filter(|p|ui.clip_rect().contains(*p))
                    .and_then(|p|self.annotation_at(p)).map(|(page,hit)|(page,hit.index));
                let hovered_move=ui.input(|i|i.pointer.hover_pos()).filter(|p|ui.clip_rect().contains(*p)).and_then(|p|self.box_move_handle().filter(|(_,r)|r.contains(p)).map(|(page,_)|page));
                let hovered_resize=ui.input(|i|i.pointer.hover_pos()).filter(|p|ui.clip_rect().contains(*p)).and_then(|p|self.resize_handle_at(p));
                let can_drag_annotation=ui.is_enabled() && self.forms.dialog.is_none() && ui.ctx().memory(|m|m.is_above_modal_layer(response.layer_id));
                if !can_drag_annotation {self.annotations.cancel_drag();}
                let mut annotation_drag_input=self.annotations.dragging() || self.annotations.drag_cancelled();
                if can_drag_annotation && drawing && !self.annotations.is_moving() && !self.annotations.drag_cancelled() && response.drag_started_by(egui::PointerButton::Primary) {
                    if let Some(press)=ui.input(|i|i.pointer.press_origin()).filter(|p|ui.clip_rect().contains(*p)) {
                        if let Some(location)=location_at(&layout,canvas,press,self.rotation,true) {
                            if let Some(rect)=layout.pages.get(location.page) {
                                let size=if matches!(self.rotation,Rotation::Cw90|Rotation::Cw270) {egui::vec2(rect.height(),rect.width())} else {rect.size()};
                                match self.annotations.begin_creation(location.page,location.point,Vec2::splat(8.)/size) {
                                    Ok(())=>{self.forms.blur(self.loaded.pool.document());self.text.clear_selection();self.text.anchor=None;response.request_focus();}
                                    Err(error)=>self.annotations.error=Some(error),
                                }
                                annotation_drag_input=true;
                            }
                        }
                    }
                }
                if can_drag_annotation && !placing_note && !drawing && !self.forms.capturing_pointer() && !self.annotations.is_moving() && !self.annotations.drag_cancelled() && response.drag_started_by(egui::PointerButton::Primary) {
                    if let Some(press)=ui.input(|i|i.pointer.press_origin()).filter(|p|ui.clip_rect().contains(*p)) {
                        let on_link=self.page_rects.iter().any(|(page,rect)|self.text.links.get(page).is_some_and(|links|links.iter().any(|link|text_screen_rect(*rect,link.rect,self.rotation).contains(press))));
                        if !on_link && !self.forms.pointer_on_field(press,&self.page_rects,self.rotation) {
                            let resize=self.resize_handle_at(press);
                            let note=if resize.is_none() {
                                if self.box_move_handle().is_some_and(|(_,r)|r.contains(press)) {self.annotations.selected_box_hit()}
                                else {self.annotation_at(press).filter(|(_,h)|self.annotations.movable_hit(h)).map(|(page,hit)|(page,hit.clone()))}
                            } else {None};
                            if let Some(page)=resize.map(|(page,_)|page).or_else(||note.as_ref().map(|(page,_)|*page)) {
                                if let Some(rect)=layout.pages.get(page).map(|r|r.translate(canvas.min.to_vec2())) {
                                    if let Some(point)=page_unit_point_unbounded(rect,press,self.rotation) {
                                        let result=if let Some((_,corner))=resize {
                                            let size=if matches!(self.rotation,Rotation::Cw90|Rotation::Cw270) {egui::vec2(rect.height(),rect.width())} else {rect.size()};
                                            self.annotations.begin_resize(corner,point,Vec2::splat(8.)/size)
                                        } else {
                                            let (_,hit)=note.unwrap();
                                            self.annotations.begin_drag(page,hit,self.loaded.pool.document().is_some(),point)
                                        };
                                        match result {
                                            Ok(())=>{
                                                self.forms.blur(self.loaded.pool.document());
                                                self.ensure_annotation_editor(ui.ctx());
                                                self.text.clear_selection();self.text.anchor=None;
                                                response.request_focus();
                                            }
                                            Err(error)=>self.annotations.error=Some(error),
                                        }
                                        annotation_drag_input=true;
                                    }
                                }
                            }
                        }
                    }
                }
                if let Some(page)=self.annotations.drag_page() {
                    if let Some(point)=ui.input(|i|i.pointer.latest_pos()).and_then(|point|layout.pages.get(page).and_then(|r|if drawing {page_unit_point(r.translate(canvas.min.to_vec2()),point,self.rotation)} else {page_unit_point_unbounded(r.translate(canvas.min.to_vec2()),point,self.rotation)})) {
                        self.annotations.update_drag(point);
                    }
                    ui.ctx().set_cursor_icon(if drawing {egui::CursorIcon::Crosshair} else {egui::CursorIcon::Grabbing});
                    if !ui.input(|i|i.pointer.primary_down()) {self.annotations.finish_drag(self.loaded.pool.document());}
                } else if can_drag_annotation && !placing_note && !drawing && hovered_link.is_none() && hovered_annotation.is_some_and(|(page,index)|self.annotation_hits(page).iter().any(|h|h.index==index && self.annotations.movable_hit(h))) {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                }
                let pointer_event = ui.input(|i|i.pointer.any_pressed() || i.pointer.any_released());
                let annotation_pointer = !placing_note && !drawing && hovered_link.is_none() && (hovered_annotation.is_some() || hovered_resize.is_some() || hovered_move.is_some())
                    && pointer_event && !self.forms.capturing_pointer()
                    && !ui.input(|i|i.pointer.hover_pos()).is_some_and(|p|self.forms.pointer_on_field(p,&self.page_rects,self.rotation));
                let delete_annotation = ui.is_enabled() && self.forms.dialog.is_none() && ui.ctx().memory(|m|m.is_above_modal_layer(response.layer_id)) && !drawing && !self.annotations.is_moving() && !self.forms.focused() && (response.has_focus() || !ui.ctx().egui_wants_keyboard_input()) && self.annotations.selected_regions().is_some()
                    && ui.input(|i|i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace));
                if delete_annotation {
                    if let Some(doc) = self.loaded.pool.document() {
                        if let Err(error) = self.annotations.delete_selected(doc) {self.annotations.error=Some(error);}
                    }
                }
                let form_consumed=annotation_drag_input || drawing || placing_note || annotation_pointer || delete_annotation || self.xfa_layout.is_some() || self.forms.route(ui,&response,&self.page_rects,self.rotation,self.loaded.pool.document());
                if form_consumed && pointer_event && !annotation_drag_input && !annotation_pointer && !placing_note && !delete_annotation {
                    self.annotations.deselect();
                }
                if response.clicked() && annotation_pointer && hovered_resize.is_none() && hovered_move.is_none() {
                    if let Some((page,hit))=hovered_annotation.and_then(|(page,index)|self.annotation_hits(page).iter().find(|h|h.index==index).cloned().map(|hit|(page,hit))) {
                        self.forms.blur(self.loaded.pool.document());
                        let live = self.loaded.pool.document().is_some();
                        self.annotations.select_hit(page,hit,live);
                        self.ensure_annotation_editor(ui.ctx());
                        self.text.clear_selection();
                        response.request_focus();
                        self.complete=false;
                    }
                }
                if let Some((_,_,target))=hovered_link.as_ref().filter(|_|!drawing && !placing_note && !annotation_drag_input) {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    response.clone().on_hover_text(self.target_label(target));
                }
                if response.clicked() && !form_consumed {
                    if let Some((page,index,_))=hovered_link {
                        self.annotations.deselect();
                        if let Err(error)=self.follow_link(page,index,ui.ctx()) {self.text.navigation_error=Some(error);}
                    }
                }
                if response.clicked() && !form_consumed {
                    self.annotations.deselect();
                    self.text.clear_selection();
                    response.request_focus();
                    response.surrender_focus();
                }
                if !self.is_xfa() && !form_consumed && response.drag_started_by(egui::PointerButton::Primary) {
                    self.annotations.deselect();
                    self.text.clear_selection();
                    response.request_focus();
                    response.surrender_focus();
                    self.text.anchor = ui
                        .input(|i| i.pointer.press_origin())
                        .and_then(|p| location_at(&layout, canvas, p, self.rotation, true));
                }
                if let Some(anchor) = self.text.anchor.filter(|_|!form_consumed) {
                    if let Some(point) = ui.input(|i| i.pointer.latest_pos()) {
                        if let Some(focus) =
                            location_at(&layout, canvas, point, self.rotation, false)
                        {
                            self.text.drag(anchor, focus);
                        }
                        if ui.input(|i| i.pointer.primary_down()) {
                            let screen = viewport.translate(canvas.min.to_vec2());
                            let speed = if point.y < screen.min.y + 20.0 {
                                -600.0
                            } else if point.y > screen.max.y - 20.0 {
                                600.0
                            } else {
                                0.0
                            };
                            auto_scroll = speed * ui.input(|i| i.stable_dt.min(0.05));
                            if speed != 0.0 {
                                ui.ctx().request_repaint();
                            }
                        }
                    }
                    if !ui.input(|i| i.pointer.primary_down()) {
                        self.text.anchor = None;
                    }
                }
                if !ui.input(|i|i.pointer.primary_down()) {self.annotations.pointer_released();}
                if can_drag_annotation && hovered_link.is_none() && !self.forms.capturing_pointer() {
                    if let Some((page,corner))=self.annotations.resize_corner().and_then(|corner|self.annotations.drag_page().map(|page|(page,corner))).or(hovered_resize) {
                        if let Some((_,bounds))=self.annotations.resize_bounds().filter(|(p,_)|*p==page) {
                            let p=rotate_unit(corner.point(bounds),self.rotation);
                            let c=rotate_unit(bounds.center(),self.rotation);
                            ui.ctx().set_cursor_icon(if (p.x-c.x)*(p.y-c.y)>=0. {egui::CursorIcon::ResizeNwSe} else {egui::CursorIcon::ResizeNeSw});
                        }
                    }
                }
                if let Some((page,regions)) = self.annotations.selected_regions() {
                    if let Some((_,rect)) = self.page_rects.iter().find(|(p,_)|*p==page) {
                        let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
                        for unit in regions {
                            let bounds = text_screen_rect(*rect,*unit,self.rotation).expand(2.0);
                            if bounds.is_finite() {
                                if self.annotations.move_preview().is_some() {painter.rect_filled(text_screen_rect(*rect,*unit,self.rotation),1.0,crate::chrome::TEXT_SELECTION);}
                                painter.rect_stroke(bounds,0.0,egui::Stroke::new(1.5_f32,crate::chrome::ACCENT),egui::StrokeKind::Outside); }
                        }
                    }
                }
                if let Some((_,handle))=self.box_move_handle().filter(|(_,r)|ui.clip_rect().contains_rect(*r)) {
                    let painter=ui.painter();
                    painter.rect_filled(handle,2.,Color32::WHITE);
                    painter.rect_stroke(handle,2.,egui::Stroke::new(1.0_f32,crate::chrome::ACCENT),egui::StrokeKind::Inside);
                    for direction in [egui::vec2(1.,0.),egui::vec2(-1.,0.),egui::vec2(0.,1.),egui::vec2(0.,-1.)] {
                        painter.arrow(handle.center(),direction*5.,egui::Stroke::new(0.75_f32,crate::chrome::ACCENT));
                    }
                    if hovered_move.is_some() && can_drag_annotation && !annotation_drag_input && !drawing && !placing_note {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
                        response.clone().on_hover_text("Move annotation");
                    }
                }
                if let Some((_,handles))=self.resize_handles() {
                    for (_,point) in handles {
                        if ui.clip_rect().contains(point) {
                            let r=Rect::from_center_size(point,Vec2::splat(8.));
                            ui.painter().rect_filled(r,1.,Color32::WHITE);
                            ui.painter().rect_stroke(r,1.,egui::Stroke::new(1.0_f32,crate::chrome::ACCENT),egui::StrokeKind::Inside);
                        }
                    }
                }
            });
        self.canvas_rect = output.inner_rect;
        self.layout = Some(layout);
        self.scroll = output.state.clamped_offset();
        self.scroll.y = (self.scroll.y + auto_scroll).max(0.0);
        self.page = nearest.1;
        self.visible = visible;
        if !self.is_xfa() && self.loaded.pool.document().is_some() {
            self.annotations.poll_hits(
                self.loaded.pool.document(),
                &self.visible,
                self.forms.document_revision,
            );
        } else if !self.is_xfa() {
            for &page in &self.visible {
                self.text.request_annotations(page);
            }
        }
        if self.xfa_layout.is_none() {
            self.forms
                .set_visible_pages(&self.visible, self.page, self.loaded.pool.document());
        }
        self.pending.retain(|p| {
            p.request
                .tiles()
                .any(|(id, _)| wanted.contains(&(p.page, id)))
        });
        self.rendering = gesturing
            || !self.pending.is_empty()
            || !self.patches.is_empty()
            || !self.dirty.is_empty()
            || !wanted.iter().all(|key| self.tiles.contains_key(key));
        self.complete = !self.rendering
            && self.xfa_layout.is_none()
            && !self.forms.busy()
            && !self.annotations.busy()
            && !self.preparing_editor()
            && !self.text.busy()
            && !self.text.navigation_busy()
            && !self.text.annotations_busy()
            && self.errors.is_empty();
        let ready_pages: HashSet<_> = self
            .visible
            .iter()
            .copied()
            .filter(|page| {
                wanted
                    .iter()
                    .filter(|(p, _)| p == page)
                    .all(|key| self.tiles.contains_key(key))
            })
            .collect();
        self.retire_backdrop(&ready_pages);
        // Keep visible tiles even when one viewport exceeds the soft cache budget.
        let mut bytes = self.cache_bytes();
        if bytes > CACHE_LIMIT {
            self.backdrop.clear();
            bytes = self.cache_bytes();
            let mut candidates: Vec<_> = self
                .tiles
                .iter()
                .filter(|(key, _)| !wanted.contains(key))
                .map(|(key, tile)| (*key, tile.used, tile.bytes))
                .collect();
            candidates.sort_unstable_by_key(|(_, used, _)| *used);
            for (key, _, size) in candidates {
                if bytes <= CACHE_LIMIT {
                    break;
                }
                self.tiles.remove(&key);
                bytes -= size;
            }
        }
        // The owning document worker wakes the UI on every completion/timer.
        // Static pools retain polling until they have the same wake integration.
        if self.loaded.pool.document().is_none() && !self.pending.is_empty() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }
    }
    pub fn reveal_annotation(&mut self, rect: Rect) {
        self.annotations.cancel_drag();
        let page = self.annotations.context_page(self.page);
        self.focus_annotation = true;
        self.forms.blur(self.loaded.pool.document());
        self.history.record(self.position());
        self.text.cancel_navigation();
        self.destination_point = None;
        self.zoom_anchor = None;
        self.jump = None;
        self.reveal = Some((page, rect));
        self.page = page;
        self.complete = false;
    }
    pub fn find(&mut self, query: String, match_case: bool) {
        if self.is_xfa() {
            self.text.error = Some("Text search is not available for XFA documents".into());
            return;
        }
        self.text.search(query, match_case, self.page);
        self.complete = false;
    }
    pub fn find_next(&mut self, backwards: bool) {
        if let Some(hit) = self.text.next(backwards) {
            self.reveal_hit(hit);
        }
    }
    fn reveal_hit(&mut self, hit: text::Hit) {
        self.annotations.cancel_drag();
        if let Some(rect) = hit.rects.first() {
            self.forms.blur(self.loaded.pool.document());
            self.history.record(self.position());
            self.text.cancel_navigation();
            self.destination_point = None;
            self.reveal = Some((hit.page, *rect));
            self.zoom_anchor = None;
            self.jump = None;
            self.page = hit.page;
            if self.thumbnails.open {
                self.thumbnails.scroll_to = Some(hit.page);
            }
            self.complete = false;
        }
    }
    pub fn page_rects(&self) -> Vec<(usize, Rect)> {
        self.page_rects.clone()
    }
    pub fn selection_state(&self) -> Value {
        json!({"text":self.text.selection.text,"pending":self.text.selecting,"pages":self.text.selection.pages.iter().map(|p|json!({"page":p.page+1,"start":p.range.start,"end":p.range.end,"rects":p.rects.iter().map(|r|rect_json(*r)).collect::<Vec<_>>(),"screen_rects":self.screen_rects(p.page,&p.rects)})).collect::<Vec<_>>()})
    }
    fn annotation_hits(&self, page: usize) -> &[crate::annotations::Hit] {
        if self.loaded.pool.document().is_some() {
            self.annotations.hits.get(&page)
        } else {
            self.text.annotations.get(&page)
        }
        .map_or(&[], Vec::as_slice)
    }
    fn box_move_handle(&self) -> Option<(usize, Rect)> {
        let (page, bounds) = self.annotations.resize_bounds()?;
        let (_, rect) = self.page_rects.iter().find(|(p, _)| *p == page)?;
        let screen = text_screen_rect(*rect, bounds, self.rotation);
        let handle =
            Rect::from_center_size(screen.left_top() - egui::vec2(12., 12.), Vec2::splat(14.));
        self.canvas_rect
            .contains_rect(handle)
            .then_some((page, handle))
    }
    fn resize_handles(&self) -> Option<(usize, [(crate::annotations::Corner, Pos2); 4])> {
        let (page, bounds) = self.annotations.resize_bounds()?;
        let (_, rect) = self.page_rects.iter().find(|(p, _)| *p == page)?;
        Some((
            page,
            crate::annotations::Corner::ALL.map(|corner| {
                (
                    corner,
                    rect.min
                        + rotate_unit(corner.point(bounds), self.rotation).to_vec2() * rect.size(),
                )
            }),
        ))
    }
    fn resize_handle_at(&self, point: Pos2) -> Option<(usize, crate::annotations::Corner)> {
        let (page, handles) = self.resize_handles()?;
        handles
            .into_iter()
            .find(|(_, p)| Rect::from_center_size(*p, Vec2::splat(12.)).contains(point))
            .map(|(corner, _)| (page, corner))
    }
    fn annotation_at(&self, point: Pos2) -> Option<(usize, &crate::annotations::Hit)> {
        self.page_rects.iter().rev().find_map(|(page, rect)| {
            if !rect.contains(point) {
                return None;
            }
            self.annotation_hits(*page)
                .iter()
                .rev()
                .find(|hit| {
                    hit.regions.iter().any(|unit| {
                        text_screen_rect(*rect, *unit, self.rotation)
                            .expand(3.0)
                            .contains(point)
                    })
                })
                .map(|hit| (*page, hit))
        })
    }
    pub fn annotations_state(&self) -> Value {
        let mut state = self.annotations.snapshot();
        state["pending"] = json!(
            self.preparing_editor() || self.annotations.busy() || self.text.annotations_busy()
        );
        state["visible"] = json!(self.visible.iter().map(|page|json!({"page":page+1,"items":self.annotation_hits(*page).iter().map(|hit|json!({"index":hit.index,"kind":format!("{:?}",hit.kind),"rect":[hit.rect.min.x,hit.rect.min.y,hit.rect.width(),hit.rect.height()],"screen_rects":self.screen_rects(*page,&hit.regions)})).collect::<Vec<_>>()})).collect::<Vec<_>>());
        if let Some((page, rect)) = self.annotations.move_preview() {
            state["move_preview"]["screen_rects"] =
                json!(self.screen_rects(page, std::slice::from_ref(rect)));
        }
        state["move_handle"] = json!(self
            .box_move_handle()
            .map(|(page, r)| json!({"page":page+1,"rect":[r.min.x,r.min.y,r.width(),r.height()]})));
        state["resize_handles"]=json!(self.resize_handles().map(|(page,handles)|json!({"page":page+1,"corners":handles.map(|(corner,p)|json!({"corner":format!("{:?}",corner),"position":[p.x,p.y]}))})));
        if state["selected"].is_object() {
            if let Some((page, regions)) = self.annotations.selected_regions() {
                state["selected"]["screen_rects"] = json!(self.screen_rects(page, regions));
            }
        }
        state
    }
    pub fn search_state(&self) -> Value {
        json!({"query":self.text.query,"start_page":self.text.start_page+1,"pending":self.text.searching,"count":self.text.hits.len(),"current":self.text.active.map(|i|i+1),"scanned_pages":self.text.scanned,"truncated":self.text.truncated,"error":self.text.error,"active":self.text.active.map(|i|self.hit_state(&self.text.hits[i]))})
    }
    pub fn hit_state(&self, hit: &text::Hit) -> Value {
        json!({"page":hit.page+1,"text":hit.text,"start":hit.range.start,"end":hit.range.end,"rects":hit.rects.iter().map(|r|rect_json(*r)).collect::<Vec<_>>(),"screen_rects":self.screen_rects(hit.page,&hit.rects)})
    }
    fn screen_rects(&self, page: usize, rects: &[Rect]) -> Vec<Value> {
        self.page_rects
            .iter()
            .find(|(p, _)| *p == page)
            .map(|(_, page_rect)| {
                rects
                    .iter()
                    .map(|r| rect_json(text_screen_rect(*page_rect, *r, self.rotation)))
                    .collect()
            })
            .unwrap_or_default()
    }
    fn cache_bytes(&self) -> usize {
        self.tiles
            .values()
            .chain(&self.backdrop)
            .map(|t| t.bytes)
            .sum()
    }
    pub fn state(&self) -> Value {
        json!({"layers":self.layers_state(),"path":self.loaded.path,"xfa":self.is_xfa(),"page_label":self.page_label(self.page),"annotations":self.annotations_state(),"forms":self.forms.snapshot(self.loaded.pool.document(),&self.page_rects,self.rotation),"thumbnails":self.thumbnails.state(),"page_count":self.loaded.sizes.len(),"page":self.page+1,"zoom":self.zoom,"rotation":self.rotation as i32*90,"scroll":[self.scroll.x,self.scroll.y],"visible_pages":self.visible.iter().map(|p|p+1).collect::<Vec<_>>(),"pending_renders":self.pending.len()+self.patches.len(),"dirty_regions":self.dirty.len(),"cached_tiles":self.tiles.len(),"cache_bytes":self.cache_bytes(),"render_complete":self.complete,"rendering":self.rendering,"errors":self.errors.iter().map(|(p,e)|json!({"page":p+1,"message":e})).collect::<Vec<_>>(),"workers":self.loaded.pool.worker_count(),"navigation":self.navigation_state(),"render_policy":{"monitor_pixels":[self.screen.pixels.0,self.screen.pixels.1],"screen_source":if self.screen.monitor {"monitor"} else {"window-fallback"},"max_texture_side":self.screen.max_texture_side,"pixels_per_point":self.pixels_per_point,"pages":self.policy_pages},"pipeline":{"results":self.pipeline.results,"textures":self.pipeline.textures,"partial_updates":self.pipeline.partial_updates,"bytes":self.pipeline.bytes,"conversion_ms":self.pipeline.conversion_ms,"texture_enqueue_ms":self.pipeline.enqueue_ms,"request_to_collect_ms":self.pipeline.request_to_collect_ms},"search":self.search_state(),"selection":{"text":self.text.selection.text.chars().take(200).collect::<String>(),"bytes":self.text.selection.text.len(),"pending":self.text.selecting},"text_error":self.text.error,"zooming":self.zoom_until.is_some_and(|until|until>Instant::now()),"viewport":[self.canvas_rect.min.x,self.canvas_rect.min.y,self.canvas_rect.width(),self.canvas_rect.height()],"pages_on_screen":self.page_rects.iter().map(|(p,r)|json!({"page":p+1,"rect":[r.min.x,r.min.y,r.width(),r.height()]})).collect::<Vec<_>>()})
    }
}
fn rect_json(r: Rect) -> Value {
    json!([r.min.x, r.min.y, r.width(), r.height()])
}
pub(crate) fn rotate_unit(p: Pos2, rotation: Rotation) -> Pos2 {
    match rotation {
        Rotation::None => p,
        Rotation::Cw90 => Pos2::new(1.0 - p.y, p.x),
        Rotation::Cw180 => Pos2::new(1.0 - p.x, 1.0 - p.y),
        Rotation::Cw270 => Pos2::new(p.y, 1.0 - p.x),
    }
}
pub(crate) fn text_screen_rect(page: Rect, unit: Rect, rotation: Rotation) -> Rect {
    Rect::from_points(
        &[
            unit.min,
            unit.max,
            Pos2::new(unit.min.x, unit.max.y),
            Pos2::new(unit.max.x, unit.min.y),
        ]
        .map(|p| page.min + rotate_unit(p, rotation).to_vec2() * page.size()),
    )
}
fn page_unit_point(rect: Rect, point: Pos2, rotation: Rotation) -> Option<Pos2> {
    page_unit_point_impl(rect, point, rotation, true)
}
fn page_unit_point_unbounded(rect: Rect, point: Pos2, rotation: Rotation) -> Option<Pos2> {
    page_unit_point_impl(rect, point, rotation, false)
}
fn page_unit_point_impl(
    rect: Rect,
    point: Pos2,
    rotation: Rotation,
    bounded: bool,
) -> Option<Pos2> {
    if !rect.is_finite() || !rect.is_positive() || !point.is_finite() {
        return None;
    }
    let unit = (point - rect.min) / rect.size();
    if !unit.is_finite() {
        return None;
    }
    let unit = if bounded {
        unit.clamp(Vec2::ZERO, Vec2::splat(1.))
    } else {
        unit
    };
    let inverse = match rotation {
        Rotation::None => Rotation::None,
        Rotation::Cw90 => Rotation::Cw270,
        Rotation::Cw180 => Rotation::Cw180,
        Rotation::Cw270 => Rotation::Cw90,
    };
    Some(rotate_unit(Pos2::new(unit.x, unit.y), inverse))
}
fn location_at(
    layout: &Layout,
    canvas: Rect,
    point: Pos2,
    rotation: Rotation,
    strict: bool,
) -> Option<Location> {
    if layout.pages.is_empty() {
        return None;
    }
    let local = point - canvas.min;
    let page = layout
        .pages
        .partition_point(|r| r.max.y < local.y)
        .min(layout.pages.len() - 1);
    let rect = layout.pages[page];
    if strict && !rect.contains(Pos2::new(local.x, local.y)) {
        return None;
    }
    let unit = ((local - rect.min.to_vec2()) / rect.size()).clamp(Vec2::ZERO, Vec2::splat(1.0));
    let inverse = match rotation {
        Rotation::None => Rotation::None,
        Rotation::Cw90 => Rotation::Cw270,
        Rotation::Cw180 => Rotation::Cw180,
        Rotation::Cw270 => Rotation::Cw90,
    };
    let tolerance = match rotation {
        Rotation::None | Rotation::Cw180 => Vec2::splat(8.0) / rect.size(),
        _ => egui::vec2(8.0 / rect.height(), 8.0 / rect.width()),
    };
    Some(Location {
        page,
        point: rotate_unit(Pos2::new(unit.x, unit.y), inverse),
        tolerance,
    })
}

/// egui-winit maps macOS PinchGesture to Event::Zoom. Consume its factor
/// only over the document; the toolbar's scale stays unchanged.
fn pinch_zoom(
    ctx: &egui::Context,
    viewport: Rect,
    zoom: f32,
    last_frame: &mut Option<u64>,
) -> Option<(f32, Pos2)> {
    // egui may discard a layout pass and call the UI again with the same input.
    // A native pinch delta must be applied only once per input frame.
    let frame = ctx.cumulative_frame_nr();
    if *last_frame == Some(frame) {
        return None;
    }
    *last_frame = Some(frame);
    let (factor, point) = ctx.input(|i| (i.zoom_delta(), i.pointer.hover_pos()));
    let point = point.filter(|point| viewport.contains(*point))?;
    if !factor.is_finite() || factor <= 0.0 || factor == 1.0 {
        return None;
    }
    let target = (zoom * factor).clamp(0.1, 8.0);
    (target != zoom).then_some((target, point))
}

/// Round the translated origin in physical pixels and keep the raster's exact
/// integer dimensions, rather than shrinking its ceil-rounded bitmap to layout.
fn pixel_page_rect(page: Rect, pixels: (u32, u32), ppp: f32) -> Rect {
    let origin = (page.min.to_vec2() * ppp).round();
    Rect::from_min_max(
        (origin / ppp).to_pos2(),
        ((origin + egui::vec2(pixels.0 as f32, pixels.1 as f32)) / ppp).to_pos2(),
    )
}
fn tile_screen_rect(
    page: Rect,
    source: DeviceRect,
    canvas: (u32, u32),
    current: (u32, u32),
    ppp: f32,
) -> Rect {
    if canvas == current {
        // Compute both edges from the shared physical origin: one texel per
        // screen pixel, including partial edge tiles. Never round tiles alone.
        let origin = (page.min.to_vec2() * ppp).round();
        let edge = |x, y| ((origin + egui::vec2(x as f32, y as f32)) / ppp).to_pos2();
        Rect::from_min_max(
            edge(source.x, source.y),
            edge(source.x + source.width, source.y + source.height),
        )
    } else {
        // Old zoom backdrops intentionally scale while replacements render.
        let edge = |x, y| {
            page.min
                + egui::vec2(
                    x as f32 / canvas.0 as f32 * page.width(),
                    y as f32 / canvas.1 as f32 * page.height(),
                )
        };
        Rect::from_min_max(
            edge(source.x, source.y),
            edge(source.x + source.width, source.y + source.height),
        )
    }
}
fn paint_tile(
    ui: &egui::Ui,
    tile: &mut Tile,
    page: Rect,
    current: (u32, u32),
    ppp: f32,
    frame: u64,
) {
    ui.painter().image(
        tile.texture.id(),
        tile_screen_rect(page, tile.source, tile.canvas, current, ppp),
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE,
    );
    tile.used = frame;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layer_changes_keep_painted_coverage_during_partial_and_repeated_updates() {
        for owner in [false, true] {
            let ctx = egui::Context::default();
            let loaded = loaded_from_bytes(
                "layers.pdf".into(),
                Arc::from(&include_bytes!("../assets/demos/layers-demo.pdf")[..]),
                crate::platform::repaint(&ctx),
                owner,
            )
            .unwrap();
            let mut view = View::new(loaded, ctx.clone());
            let canvas = (1024, 512);
            let make_tile = |x, color| Tile {
                page: 0,
                texture: ctx.load_texture(
                    "layer pixels",
                    egui::ColorImage::filled([1, 1], color),
                    egui::TextureOptions::LINEAR,
                ),
                source: DeviceRect::new(x, 0, 512, 512),
                canvas,
                bytes: CACHE_LIMIT / 2,
                used: 0,
            };
            let left = TileId { column: 0, row: 0 };
            let right = TileId { column: 1, row: 0 };
            let old_left = make_tile(0, Color32::RED);
            let old_left_id = old_left.texture.id();
            let old_right = make_tile(512, Color32::BLUE);
            let old_right_id = old_right.texture.id();
            view.tiles.insert((0, left), old_left);
            view.tiles.insert((0, right), old_right);
            let group = view.loaded.layers.groups()[0].id;
            view.loaded.layers.set_visible(group, false).unwrap();
            view.layers_changed().unwrap();
            assert!(view.tiles.is_empty());
            assert_eq!(view.backdrop.len(), 2);

            // Only half the page has arrived before the next toggle. Both
            // halves must still be drawn, without retaining duplicate tiles.
            let latest_left = make_tile(0, Color32::GREEN);
            let latest_left_id = latest_left.texture.id();
            view.tiles.insert((0, left), latest_left);
            assert!(view.cache_bytes() > CACHE_LIMIT);
            view.retire_backdrop(&HashSet::new());
            assert_eq!(view.backdrop.len(), 1);
            assert_eq!(view.cache_bytes(), CACHE_LIMIT);
            for visible in [true, false, true] {
                view.loaded.layers.set_visible(group, visible).unwrap();
                view.layers_changed().unwrap();
                assert!(view.tiles.is_empty());
                assert_eq!(view.backdrop.len(), 2);
                assert_eq!(view.cache_bytes(), CACHE_LIMIT);
            }
            assert!(!view
                .backdrop
                .iter()
                .any(|tile| tile.texture.id() == old_left_id));
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1200., 800.))),
                    ..Default::default()
                },
                |ui| {
                    for tile in &mut view.backdrop {
                        paint_tile(
                            ui,
                            tile,
                            Rect::from_min_size(Pos2::ZERO, egui::vec2(1024., 512.)),
                            canvas,
                            1.,
                            0,
                        );
                    }
                },
            );
            let painted: HashSet<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Mesh(mesh) => Some(mesh.texture_id),
                    _ => None,
                })
                .collect();
            assert!(painted.contains(&latest_left_id));
            assert!(painted.contains(&old_right_id));
            output.drop_without_applying_deltas();
            view.retire_backdrop(&HashSet::from([0]));
            assert!(view.backdrop.is_empty());
        }
    }

    #[test]
    fn geometry_handles_preserve_pointer_offsets_outside_the_page() {
        let rect = Rect::from_min_size(egui::pos2(40., 80.), egui::vec2(200., 300.));
        let start = rect.min - egui::vec2(12., 12.);
        let end = start + egui::vec2(20., 30.);
        for rotation in [
            Rotation::None,
            Rotation::Cw90,
            Rotation::Cw180,
            Rotation::Cw270,
        ] {
            let a = page_unit_point_unbounded(rect, start, rotation).unwrap();
            let b = page_unit_point_unbounded(rect, end, rotation).unwrap();
            assert!(((b - a).length() - egui::vec2(0.1, 0.1).length()).abs() < 1e-6);
        }
    }
    #[test]
    fn note_pointer_coordinates_invert_every_view_rotation() {
        let rect = Rect::from_min_size(egui::pos2(45., 90.), egui::vec2(300., 500.));
        let unit = egui::pos2(0.2, 0.7);
        for rotation in [
            Rotation::None,
            Rotation::Cw90,
            Rotation::Cw180,
            Rotation::Cw270,
        ] {
            let screen = rect.min + rotate_unit(unit, rotation).to_vec2() * rect.size();
            assert!((page_unit_point(rect, screen, rotation).unwrap() - unit).length() < 1e-6);
        }
        assert!(page_unit_point(Rect::NOTHING, rect.min, Rotation::None).is_none());
    }
    #[test]
    fn dirty_pixels_cover_rotated_bounds_and_merge_only_overlapping_regions() {
        let rect = Rect::from_min_max(egui::pos2(0.1, 0.1), egui::pos2(0.2, 0.2));
        for rotation in [
            Rotation::None,
            Rotation::Cw90,
            Rotation::Cw180,
            Rotation::Cw270,
        ] {
            let pixels = damage_pixels(rect, (1000, 1000), rotation).unwrap();
            assert!(pixels.width <= 103 && pixels.height <= 103);
            assert!(intersection(pixels, DeviceRect::new(400, 400, 100, 100)).is_none());
        }
        let mut regions = vec![];
        for (page, x) in [(0, 10), (0, 15), (0, 100), (1, 10)] {
            merge_damage(
                &mut regions,
                Damage {
                    page,
                    rect: DeviceRect::new(x, 10, 10, 10),
                    canvas: (1000, 1000),
                },
            );
        }
        assert_eq!(regions.len(), 3);
        assert!(regions
            .iter()
            .any(|d| d.page == 0 && d.rect == DeviceRect::new(10, 10, 15, 10)));
    }
    #[test]
    fn current_rasters_use_integer_screen_edges_and_exact_texel_sizes() {
        for ppp in [1.0, 1.25, 1.5, 2.0] {
            for origin in [egui::vec2(33.3, -107.7), egui::vec2(-17.25, 229.5)] {
                let pixels = (1031, 777);
                let page = pixel_page_rect(
                    Rect::from_min_size(origin.to_pos2(), egui::vec2(1030.2, 776.7) / ppp),
                    pixels,
                    ppp,
                );
                let physical = page.min.to_vec2() * ppp;
                assert!((physical - physical.round()).abs().max_elem() < 0.001);
                assert!(
                    (page.size() * ppp - egui::vec2(1031.0, 777.0))
                        .abs()
                        .max_elem()
                        < 0.001
                );
                let grid = TileGrid::new(
                    pixels.0,
                    pixels.1,
                    TileGridOptions {
                        tile_size: 512,
                        whole_page_max_pixels: 0,
                    },
                )
                .unwrap();
                for id in grid.visible_tiles(DeviceRect::new(0, 0, pixels.0, pixels.1)) {
                    let source = grid.tile_rect(id).unwrap();
                    let tile = tile_screen_rect(page, source, pixels, pixels, ppp);
                    for edge in [tile.min, tile.max] {
                        let edge = edge.to_vec2() * ppp;
                        assert!((edge - edge.round()).abs().max_elem() < 0.001);
                    }
                    assert!(
                        (tile.size() * ppp - egui::vec2(source.width as f32, source.height as f32))
                            .abs()
                            .max_elem()
                            < 0.001
                    );
                    assert!(
                        (tile.min.to_vec2() * ppp
                            - physical
                            - egui::vec2(source.x as f32, source.y as f32))
                        .abs()
                        .max_elem()
                            < 0.001
                    );
                }
                let whole = tile_screen_rect(
                    page,
                    DeviceRect::new(0, 0, pixels.0, pixels.1),
                    pixels,
                    pixels,
                    ppp,
                );
                assert_eq!(whole, page);
                // A different-resolution backdrop still fills the new page.
                let backdrop = tile_screen_rect(
                    page,
                    DeviceRect::new(0, 0, 400, 300),
                    (400, 300),
                    pixels,
                    ppp,
                );
                assert!((backdrop.min - page.min).abs().max_elem() < 0.001);
                assert!((backdrop.max - page.max).abs().max_elem() < 0.001);
            }
        }
    }
    fn navigation_view() -> View {
        let ctx = egui::Context::default();
        let bytes: Arc<[u8]> = Arc::from(
            &include_bytes!("../../../tests/render_corpus/pdfs/rectangles_multi_pages.pdf")[..],
        );
        let doc = Document::from_bytes(bytes.clone()).unwrap();
        let sizes = doc
            .page_layouts()
            .unwrap()
            .into_iter()
            .map(|p| p.size)
            .collect();
        let pool = RenderPool::new(Source::Bytes(bytes.clone()), Config::default(), 1).unwrap();
        let mut view = View::new(
            Loaded {
                path: "test.pdf".into(),
                labels: vec![None; pool.page_count()],
                sizes,
                layers: pool.optional_content_state(),
                pool: pool.into(),
                bytes,
                outline: Vec::new(),
            },
            ctx,
        );
        view.canvas_rect = Rect::from_min_size(Pos2::new(0.0, 80.0), egui::vec2(800.0, 600.0));
        view
    }
    #[test]
    fn annotation_pick_uses_visible_quads_and_topmost_order_at_every_rotation() {
        let doc =
            Document::from_bytes(crate::annotations::picking_tests::annotated_bytes()).unwrap();
        let hits = crate::annotations::hit_regions(&doc, 0).unwrap();
        let mut view = navigation_view();
        let page_rect = Rect::from_min_size(Pos2::new(40., 60.), egui::vec2(600., 400.));
        view.page_rects = vec![(0, page_rect)];
        view.text.annotations.insert(0, hits.clone());
        let gap = Pos2::new(
            hits[0].regions[0].center().x,
            (hits[0].regions[0].max.y + hits[0].regions[1].min.y) * 0.5,
        );
        for rotation in [
            Rotation::None,
            Rotation::Cw90,
            Rotation::Cw180,
            Rotation::Cw270,
        ] {
            view.rotation = rotation;
            let screen = |unit: Pos2| {
                text_screen_rect(
                    page_rect,
                    Rect::from_center_size(unit, egui::vec2(0.001, 0.001)),
                    rotation,
                )
                .center()
            };
            assert_eq!(
                view.annotation_at(screen(hits[0].regions[1].center()))
                    .unwrap()
                    .1
                    .index,
                0
            );
            assert_eq!(
                view.annotation_at(screen(hits[1].regions[0].center()))
                    .unwrap()
                    .1
                    .index,
                1
            );
            assert!(view.annotation_at(screen(gap)).is_none());
            assert!(view
                .annotation_at(screen(Pos2::new(0.755, 0.445)))
                .is_none());
            assert!(view.annotation_at(Pos2::ZERO).is_none());
        }
    }
    #[test]
    fn destination_clamps_both_axes_before_first_paint() {
        let mut view = navigation_view();
        view.fit = false;
        view.zoom = 0.5;
        let ctx = egui::Context::default();
        let frame = |view: &mut View| {
            let _ = crate::test_frame(
                &ctx,
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800., 600.))),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| view.show(ui));
                },
            );
        };
        frame(&mut view);
        frame(&mut view);
        let last = view.loaded.sizes.len() - 1;
        view.destination_point = Some((last, Pos2::new(1., 1.)));
        frame(&mut view);
        let first = view.page_rects.clone();
        frame(&mut view);
        assert_eq!(first.len(), view.page_rects.len());
        for ((page, a), (next, b)) in first.iter().zip(&view.page_rects) {
            assert_eq!(page, next);
            assert!(
                (a.min - b.min).abs().max_elem() <= 1.,
                "navigation flashed: {a:?} then {b:?}"
            );
        }
        assert_eq!(view.scroll.x, 0.);
    }
    #[test]
    fn destination_fit_rotation_xyz_and_history_restore_actual_view() {
        let mut view = navigation_view();
        view.zoom = 1.5;
        view.scroll = egui::vec2(20.0, 300.25);
        view.fit = false;
        let before = view.position();
        let crop = pdfoxium::Rect {
            left: 10.0,
            bottom: 20.0,
            right: 210.0,
            top: 270.0,
        };
        let matrix = pdfoxium::Matrix {
            a: 0.005,
            b: 0.0,
            c: 0.0,
            d: -0.004,
            e: -0.05,
            f: 1.08,
        };
        for rotation in [
            Rotation::None,
            Rotation::Cw90,
            Rotation::Cw180,
            Rotation::Cw270,
        ] {
            view.rotation = rotation;
            view.apply_destination(navigation::Resolved {
                destination: pdfoxium::Destination {
                    page_index: 0,
                    view: pdfoxium::DestinationView::Fit,
                },
                matrix,
                current_matrix: matrix,
                crop,
            })
            .unwrap();
            assert!((view.destination_point.unwrap().1 - Pos2::ZERO).length() < 0.001);
            assert!(view.zoom.is_finite());
        }
        view.history = navigation::History::default();
        view.rotation = before.rotation;
        view.zoom = before.zoom;
        view.scroll = before.scroll;
        view.apply_destination(navigation::Resolved {
            destination: pdfoxium::Destination {
                page_index: 0,
                view: pdfoxium::DestinationView::Xyz {
                    left: Some(110.0),
                    top: Some(145.0),
                    zoom: Some(2.0),
                },
            },
            matrix,
            current_matrix: matrix,
            crop,
        })
        .unwrap();
        assert_eq!(view.zoom, 2.0);
        assert!((view.destination_point.unwrap().1 - Pos2::new(0.5, 0.5)).length() < 0.001);
        view.scroll = egui::vec2(90.0, 200.0);
        let destination = view.position();
        view.history_step(true).unwrap();
        assert!(view.position() == before);
        assert!(view.destination_point.is_none() && view.jump.is_none());
        view.history_step(false).unwrap();
        assert!(view.position() == destination);
    }
    #[test]
    fn null_xyz_coordinates_preserve_pdf_coordinates_between_different_pages() {
        let mut view = navigation_view();
        view.zoom = 1.5;
        view.page_rects = vec![(
            0,
            Rect::from_min_size(Pos2::new(-100.0, -45.0), egui::vec2(200.0, 250.0)),
        )];
        let current_matrix = pdfoxium::Matrix {
            a: 0.005,
            b: 0.0,
            c: 0.0,
            d: -0.004,
            e: -0.05,
            f: 1.08,
        };
        let matrix = pdfoxium::Matrix {
            a: 0.0025,
            b: 0.0,
            c: 0.0,
            d: -0.002,
            e: -0.025,
            f: 1.04,
        };
        view.apply_destination(navigation::Resolved {
            destination: pdfoxium::Destination {
                page_index: 0,
                view: pdfoxium::DestinationView::Xyz {
                    left: None,
                    top: None,
                    zoom: None,
                },
            },
            matrix,
            current_matrix,
            crop: pdfoxium::Rect {
                left: 10.0,
                bottom: 20.0,
                right: 410.0,
                top: 520.0,
            },
        })
        .unwrap();
        assert_eq!(view.zoom, 1.5);
        assert!((view.destination_point.unwrap().1 - Pos2::new(0.25, 0.75)).length() < 0.001);
    }
    #[test]
    fn external_navigation_emits_platform_url_without_document_history() {
        let mut view = navigation_view();
        let ctx = egui::Context::default();
        let output = crate::test_frame(&ctx, egui::RawInput::default(), |ctx| {
            view.follow(
                pdfoxium::LinkTarget::Uri(b"https://example.org".to_vec()),
                ctx,
            )
            .unwrap();
        });
        assert!(output.platform_output.commands.iter().any(|command|matches!(command,egui::OutputCommand::OpenUrl(url) if url.url=="https://example.org")));
        assert!(view.history.back.is_empty());
        assert!(view
            .follow(
                pdfoxium::LinkTarget::Uri(b"javascript:alert(1)".to_vec()),
                &ctx
            )
            .is_err());
        assert!(view
            .follow(
                pdfoxium::LinkTarget::UnsupportedAction {
                    kind: b"Launch".to_vec(),
                    file: Some("x".into())
                },
                &ctx
            )
            .is_err());
    }
    #[test]
    fn grid_changes_cancel_pending_work_and_do_not_reuse_old_tile_ids() {
        let ctx = egui::Context::default();
        let bytes: Arc<[u8]> = Arc::from(
            &include_bytes!("../../../tests/render_corpus/pdfs/rectangles_multi_pages.pdf")[..],
        );
        let pool = RenderPool::new(Source::Bytes(bytes.clone()), Config::default(), 1).unwrap();
        let count = pool.page_count();
        let loaded = Loaded {
            path: "test.pdf".into(),
            labels: vec![None; count],
            sizes: vec![
                Size {
                    width: 200.0,
                    height: 250.0
                };
                count
            ],
            layers: pool.optional_content_state(),
            pool: pool.into(),
            bytes,
            outline: Vec::new(),
        };
        let mut view = View::new(loaded, ctx.clone());
        view.screen = Screen {
            pixels: (1000, 1000),
            monitor: true,
            max_texture_side: 2048,
        };
        let grid = view.grid_for(0, (1000, 1000), 1_000_000).unwrap();
        let id = TileId { column: 0, row: 0 };
        let source = grid.tile_rect(id).unwrap();
        let texture = ctx.load_texture(
            "old whole",
            egui::ColorImage::filled([1, 1], Color32::RED),
            egui::TextureOptions::LINEAR,
        );
        view.tiles.insert(
            (0, id),
            Tile {
                page: 0,
                texture,
                source,
                canvas: (1000, 1000),
                bytes: 4,
                used: 0,
            },
        );
        let request = grid.plan(source, |_| false).requests()[0].clone();
        let ticket = view
            .loaded
            .pool
            .render(count, RenderConfig::default(), Priority::Visible);
        view.pending.push(Pending {
            page: 0,
            request,
            ticket,
            canvas: (1000, 1000),
            requested: Instant::now(),
        });
        view.screen.max_texture_side = 64;
        let tiled = view.grid_for(0, (1000, 1000), 1_000_000).unwrap();
        assert_eq!(view.modes[0], Mode::Tiled);
        assert_eq!(tiled.tile_rect(id).unwrap().width, 64);
        assert!(view.tiles.is_empty() && view.pending.is_empty());
        assert_eq!(view.backdrop[0].source.width, 1000);
        view.screen.max_texture_side = 2048;
        let whole = view.grid_for(0, (1000, 1000), 1_000_000).unwrap();
        assert_eq!(view.modes[0], Mode::Whole);
        assert_eq!((whole.columns(), whole.rows()), (1, 1));
    }
    #[test]
    fn selection_screen_mapping_round_trips_zoom_rotation_and_scroll() {
        let unit = Rect::from_min_max(Pos2::new(0.2, 0.3), Pos2::new(0.5, 0.4));
        for rotation in [
            Rotation::None,
            Rotation::Cw90,
            Rotation::Cw180,
            Rotation::Cw270,
        ] {
            let size = match rotation {
                Rotation::None | Rotation::Cw180 => egui::vec2(200.0, 300.0),
                _ => egui::vec2(300.0, 200.0),
            };
            for zoom in [0.5, 1.0, 3.0] {
                let layout = Layout::new(&[size], zoom, 800.0);
                let canvas = Rect::from_min_size(Pos2::new(30.0, -120.0), layout.size);
                let page = layout.pages[0].translate(canvas.min.to_vec2());
                let screen = text_screen_rect(page, unit, rotation);
                let location =
                    location_at(&layout, canvas, screen.center(), rotation, true).unwrap();
                assert!((location.point - unit.center()).length() < 0.0001);
            }
        }
    }
    #[test]
    fn native_zoom_events_change_document_zoom_only_inside_its_viewport() {
        let ctx = egui::Context::default();
        let mut last_frame = None;
        let viewport = Rect::from_min_max(Pos2::new(0.0, 70.0), Pos2::new(1000.0, 800.0));
        for (point, factor, zoom, expected) in [
            (Pos2::new(400.0, 300.0), 1.25, 2.0, Some(2.5)),
            (Pos2::new(400.0, 300.0), 0.8, 2.5, Some(2.0)),
            (Pos2::new(400.0, 30.0), 1.25, 2.0, None),
            (Pos2::new(400.0, 300.0), 2.0, 7.0, Some(8.0)),
        ] {
            let _ = crate::test_frame(
                &ctx,
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(1000.0, 850.0))),
                    events: vec![egui::Event::PointerMoved(point), egui::Event::Zoom(factor)],
                    ..Default::default()
                },
                |ctx| {
                    assert_eq!(
                        pinch_zoom(ctx, viewport, zoom, &mut last_frame).map(|(zoom, _)| zoom),
                        expected
                    );
                    assert!(
                        pinch_zoom(ctx, viewport, zoom, &mut last_frame).is_none(),
                        "input cannot replay in another layout pass"
                    );
                    assert_eq!(ctx.zoom_factor(), 1.0);
                },
            );
        }
    }
}
