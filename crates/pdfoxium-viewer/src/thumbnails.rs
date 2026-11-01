// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Lazy, memory-only previews. Embedded /Thumb is read; no PDF entries are written.
use crate::{text, texture};
use eframe::egui::{self, Color32, Pos2, Rect, TextureHandle, Vec2};
use pdfoxium::{
    thread::{PoolBitmap, Priority, Ticket},
    Bitmap, DeviceRect, PixelFormat, RenderConfig, Rotation, Size,
};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
const BUDGET: usize = 16 * 1024 * 1024;
const MAX_ENTRIES: usize = 64;
const MAX_PENDING: usize = 2;
const ROW: f32 = 184.0;
struct Entry {
    texture: Option<TextureHandle>,
    pixels: (u32, u32),
    bytes: usize,
    embedded: bool,
    used: u64,
    error: Option<String>,
}
enum Pending {
    Embedded(u64),
    Render(Ticket<PoolBitmap>),
}
#[derive(Default)]
pub struct State {
    cache: HashMap<usize, Entry>,
    stale: HashSet<usize>,
    pending: HashMap<usize, Pending>,
    wanted: HashSet<usize>,
    side: u32,
    limit: usize,
    max_entry_bytes: usize,
    generation: u64,
    next_id: u64,
    frame: u64,
    pub revision: u64,
    pub complete: bool,
    pub open: bool,
    pub scroll_to: Option<usize>,
    rows: Vec<(usize, Rect)>,
}
fn fit(size: Size, side: u32) -> (u32, u32) {
    let scale = f64::from(side) / f64::from(size.width.max(size.height));
    (
        (f64::from(size.width) * scale)
            .ceil()
            .clamp(1.0, f64::from(side)) as u32,
        (f64::from(size.height) * scale)
            .ceil()
            .clamp(1.0, f64::from(side)) as u32,
    )
}
fn adequate(bitmap: &Bitmap, size: Size, side: u32, limit: usize) -> bool {
    let required = fit(size, side);
    let ratio = f64::from(size.width) / f64::from(size.height);
    bitmap.width() >= required.0
        && bitmap.height() >= required.1
        && bitmap.width() as usize <= limit
        && bitmap.height() as usize <= limit
        && bitmap.pixels().len() <= BUDGET
        && ((f64::from(bitmap.width()) / f64::from(bitmap.height())) / ratio - 1.0).abs() <= 0.02
}
impl State {
    pub fn invalidate(&mut self) {
        self.pending.clear();
        self.stale.extend(self.cache.keys().copied());
        self.complete = false;
        self.revision += 1;
    }

    fn refresh_generation(&mut self, generation: u64, pages: usize) -> bool {
        if self.generation == generation {
            return false;
        }
        self.generation = generation;
        self.pending.clear();
        self.cache.retain(|page, _| *page < pages);
        self.stale = self.cache.keys().copied().collect();
        self.revision += 1;
        true
    }
    pub fn hide(&mut self, reader: &text::State) {
        if self.open {
            self.pending.clear();
            reader.cancel_thumbnails();
        }
        self.open = false;
        self.wanted.clear();
        self.rows.clear();
        self.complete = true;
    }
    fn fallback(&mut self, page: usize, pool: &crate::view::Renderer) {
        self.pending.insert(
            page,
            Pending::Render(pool.render(
                page,
                RenderConfig::fit(self.side, self.side).format(PixelFormat::Rgba8),
                Priority::Thumbnail,
            )),
        );
    }
    fn install(&mut self, page: usize, bitmap: Bitmap, embedded: bool, ctx: &egui::Context) {
        self.stale.remove(&page);
        let pixels = (bitmap.width(), bitmap.height());
        let bytes = bitmap.pixels().len();
        let image =
            texture::Converter::new(&bitmap).image(DeviceRect::new(0, 0, pixels.0, pixels.1));
        let texture = ctx.load_texture(
            format!("thumbnail-{page}"),
            image,
            egui::TextureOptions::LINEAR,
        );
        self.cache.insert(
            page,
            Entry {
                texture: Some(texture),
                pixels,
                bytes,
                embedded,
                used: self.frame,
                error: None,
            },
        );
        self.revision += 1;
        ctx.request_repaint();
    }
    pub fn collect(
        &mut self,
        reader: &mut text::State,
        pool: &crate::view::Renderer,
        ctx: &egui::Context,
        sizes: &[Size],
    ) {
        while let Some((id, page, result)) = reader.thumbnail_results.pop_front() {
            if !matches!(self.pending.get(&page),Some(Pending::Embedded(current)) if *current==id) {
                continue;
            }
            self.pending.remove(&page);
            if !self.wanted.contains(&page) {
                continue;
            }
            match result {
                Ok(Some(bitmap))
                    if adequate(&bitmap, sizes[page], self.side, self.limit)
                        && bitmap.pixels().len() <= self.max_entry_bytes =>
                {
                    self.install(page, bitmap, true, ctx)
                }
                _ => self.fallback(page, pool),
            }
        }
        let ready: Vec<_> = self
            .pending
            .iter_mut()
            .filter_map(|(&page, pending)| match pending {
                Pending::Render(ticket) => ticket.try_take().map(|result| (page, result)),
                _ => None,
            })
            .collect();
        for (page, result) in ready {
            self.pending.remove(&page);
            if !self.wanted.contains(&page) {
                continue;
            }
            match result {
                Ok(result) if result.source.generation == pool.generation() => {
                    self.install(page, result.bitmap, false, ctx)
                }
                Err(pdfoxium::Error::Busy | pdfoxium::Error::Cancelled) | Ok(_) => {}
                Err(error) => {
                    self.stale.remove(&page);
                    self.cache.insert(
                        page,
                        Entry {
                            texture: None,
                            pixels: (0, 0),
                            bytes: 0,
                            embedded: false,
                            used: self.frame,
                            error: Some(error.to_string()),
                        },
                    );
                    self.revision += 1;
                    ctx.request_repaint();
                }
            }
        }
        if reader.stopped() {
            let pages: Vec<_> = self
                .pending
                .iter()
                .filter_map(|(&page, p)| matches!(p, Pending::Embedded(_)).then_some(page))
                .collect();
            for page in pages {
                self.fallback(page, pool);
            }
        }
        self.evict();
    }
    fn evict(&mut self) {
        let mut bytes: usize = self.cache.values().map(|e| e.bytes).sum();
        let mut old: Vec<_> = self
            .cache
            .iter()
            .filter(|(page, _)| !self.wanted.contains(page))
            .map(|(&page, e)| (e.used, page))
            .collect();
        old.sort_unstable();
        for (_, page) in old {
            if bytes <= BUDGET && self.cache.len() <= MAX_ENTRIES {
                break;
            }
            if let Some(entry) = self.cache.remove(&page) {
                self.stale.remove(&page);
                bytes -= entry.bytes;
            }
        }
    }
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        loaded: &crate::view::Loaded,
        current: usize,
        rotation: Rotation,
        reader: &text::State,
    ) -> Option<usize> {
        let sizes = &loaded.sizes;
        let pool = &loaded.pool;
        let captions = |page| crate::navigation::page_caption(&loaded.labels, page);
        self.open = true;
        self.frame += 1;
        let limit = ui.input(|i| i.max_texture_side);
        let visible_bound =
            (ui.available_height() / (ROW + ui.spacing().item_spacing.y)).ceil() as usize + 2;
        let max_entry_bytes = BUDGET / visible_bound.max(1);
        let budget_side = ((max_entry_bytes / 4) as f64).sqrt().floor() as u32;
        let side = ((160.0 * ui.ctx().pixels_per_point()).ceil() as u32)
            .min(limit as u32)
            .min(budget_side)
            .max(1);
        let generation_changed = self.refresh_generation(pool.preview_generation(), sizes.len());
        if side != self.side || max_entry_bytes != self.max_entry_bytes || limit != self.limit {
            self.cache.clear();
            self.stale.clear();
            self.pending.clear();
            reader.cancel_thumbnails();
            self.side = side;
            self.generation = pool.preview_generation();
            self.max_entry_bytes = max_entry_bytes;
            self.revision += 1;
        } else if generation_changed {
            // Keep the last preview visible while an edit or animation frame
            // is rendered. Clearing it here flashes a white page every tick.
            reader.cancel_thumbnails();
        }
        self.limit = limit;
        self.rows.clear();
        self.wanted.clear();
        let mut selected = None;
        let mut scroll = egui::ScrollArea::vertical().id_salt("thumbnail-list");
        if let Some(page) = self.scroll_to.take() {
            scroll =
                scroll.vertical_scroll_offset(page as f32 * (ROW + ui.spacing().item_spacing.y));
        }
        scroll.show_rows(ui, ROW, sizes.len(), |ui, range| {
            for page in range {
                self.wanted.insert(page);
                let (row, response) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), ROW),
                    egui::Sense::click(),
                );
                self.rows.push((page, row));
                let bound = Rect::from_center_size(
                    egui::pos2(row.center().x, row.min.y + 80.0),
                    Vec2::splat(160.0).min(egui::vec2(row.width() - 8.0, 160.0)),
                );
                let size = sizes[page];
                let size = if matches!(rotation, Rotation::Cw90 | Rotation::Cw270) {
                    egui::vec2(size.height, size.width)
                } else {
                    egui::vec2(size.width, size.height)
                };
                let rect = Rect::from_center_size(
                    bound.center(),
                    size * (bound.width() / size.x).min(bound.height() / size.y),
                );
                ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
                if let Some(entry) = self.cache.get_mut(&page) {
                    entry.used = self.frame;
                    if let Some(texture) = &entry.texture {
                        let mut mesh = egui::Mesh::with_texture(texture.id());
                        mesh.add_rect_with_uv(
                            rect,
                            Rect::from_min_max(Pos2::ZERO, egui::pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                        for vertex in &mut mesh.vertices {
                            let uv = vertex.uv;
                            vertex.uv = match rotation {
                                Rotation::None => uv,
                                Rotation::Cw90 => egui::pos2(uv.y, 1.0 - uv.x),
                                Rotation::Cw180 => egui::pos2(1.0 - uv.x, 1.0 - uv.y),
                                Rotation::Cw270 => egui::pos2(1.0 - uv.y, uv.x),
                            };
                        }
                        ui.painter().add(egui::Shape::mesh(mesh));
                    } else {
                        ui.painter().text(
                            rect.center(),
                            egui::Align2::CENTER_CENTER,
                            "Preview unavailable",
                            egui::FontId::proportional(11.0),
                            crate::chrome::MUTED,
                        );
                    }
                }
                if page == current {
                    ui.painter().rect_stroke(
                        rect.expand(2.0),
                        0.0,
                        egui::Stroke::new(2.0_f32, crate::chrome::ACCENT),
                        egui::StrokeKind::Outside,
                    );
                }
                ui.painter().text(
                    egui::pos2(row.center().x, row.max.y - 12.0),
                    egui::Align2::CENTER_CENTER,
                    captions(page),
                    egui::FontId::proportional(12.0),
                    crate::chrome::MUTED,
                );
                if response
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    selected = Some(page);
                }
            }
        });
        if self
            .pending
            .iter()
            .any(|(page, p)| !self.wanted.contains(page) && matches!(p, Pending::Embedded(_)))
        {
            reader.cancel_thumbnails();
            self.pending.retain(|_, p| matches!(p, Pending::Render(_)));
        }
        self.pending.retain(|page, _| self.wanted.contains(page));
        for page in self.wanted.iter().copied().collect::<Vec<_>>() {
            if self.pending.len() >= MAX_PENDING {
                break;
            }
            if (!self.cache.contains_key(&page) || self.stale.contains(&page))
                && !self.pending.contains_key(&page)
            {
                if pool.document().is_some() || pool.has_layers() {
                    self.fallback(page, pool);
                    continue;
                }
                self.next_id += 1;
                reader.request_thumbnail(self.next_id, page);
                self.pending.insert(page, Pending::Embedded(self.next_id));
            }
        }
        self.evict();
        self.complete = self
            .wanted
            .iter()
            .all(|page| self.cache.contains_key(page) && !self.stale.contains(page));
        if !self.complete {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(16));
        }
        selected
    }
    pub fn state(&self) -> Value {
        json!({"open":self.open,"complete":self.complete,"pending":self.pending.len(),"cached":self.cache.len(),"cache_bytes":self.cache.values().map(|e|e.bytes).sum::<usize>(),"budget":BUDGET,"target_side":self.side,"rows":self.rows.iter().map(|(page,rect)|json!({"page":page+1,"rect":[rect.min.x,rect.min.y,rect.width(),rect.height()],"ready":self.cache.contains_key(page),"source":self.cache.get(page).map(|e|if e.embedded{"embedded"}else{"rendered"}),"pixels":self.cache.get(page).map(|e|[e.pixels.0,e.pixels.1]),"error":self.cache.get(page).and_then(|e|e.error.as_ref())})).collect::<Vec<_>>()})
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layer_changes_refresh_previews_without_clearing_visible_textures_or_closing_sidebar() {
        let ctx = egui::Context::default();
        let mut state = State::default();
        state.refresh_generation(1, 1);
        state.install(
            0,
            Bitmap::new(20, 30, PixelFormat::Rgba8).unwrap(),
            false,
            &ctx,
        );
        state.open = true;
        state.complete = true;
        let previous = state.cache[&0].texture.as_ref().unwrap().id();
        state.invalidate();
        assert!(state.open);
        assert!(!state.complete);
        assert_eq!(state.cache[&0].texture.as_ref().unwrap().id(), previous);
        assert_eq!(state.stale, HashSet::from([0]));
        // Static render pools keep the same document generation for a toggle.
        assert!(!state.refresh_generation(1, 1));
        assert_eq!(state.stale, HashSet::from([0]));
        state.install(
            0,
            Bitmap::new(20, 30, PixelFormat::Rgba8).unwrap(),
            false,
            &ctx,
        );
        assert!(state.stale.is_empty());
    }

    #[test]
    fn edited_previews_keep_the_old_texture_until_replacement_and_drop_removed_pages() {
        let ctx = egui::Context::default();
        let mut state = State::default();
        state.refresh_generation(1, 2);
        state.install(
            0,
            Bitmap::new(20, 30, PixelFormat::Rgba8).unwrap(),
            false,
            &ctx,
        );
        state.install(
            1,
            Bitmap::new(20, 30, PixelFormat::Rgba8).unwrap(),
            false,
            &ctx,
        );
        let previous = state.cache[&0].texture.as_ref().unwrap().id();
        assert!(state.refresh_generation(2, 1));
        assert_eq!(state.cache[&0].texture.as_ref().unwrap().id(), previous);
        assert!(!state.cache.contains_key(&1));
        assert_eq!(state.stale, HashSet::from([0]));
        assert!(!state.refresh_generation(2, 1));
        state.install(
            0,
            Bitmap::new(20, 30, PixelFormat::Rgba8).unwrap(),
            false,
            &ctx,
        );
        assert!(state.stale.is_empty());
        assert!(state.cache[&0].texture.is_some());
    }
    #[test]
    fn eviction_bounds_bytes_and_count_without_discarding_visible_entries() {
        let mut state = State::default();
        state.wanted.extend([0, 1]);
        for page in 0..100 {
            state.cache.insert(
                page,
                Entry {
                    texture: None,
                    pixels: (512, 512),
                    bytes: 1024 * 1024,
                    embedded: true,
                    used: page as u64,
                    error: None,
                },
            );
        }
        state.evict();
        assert!(state.cache.values().map(|e| e.bytes).sum::<usize>() <= BUDGET);
        assert!(state.cache.len() <= MAX_ENTRIES);
        assert!(state.cache.contains_key(&0) && state.cache.contains_key(&1));
        assert!(state.cache.contains_key(&99));
        assert!(!state.cache.contains_key(&2));
    }
    #[test]
    fn embedded_previews_require_physical_resolution_aspect_and_gpu_fit() {
        let size = Size {
            width: 600.0,
            height: 800.0,
        };
        let good = Bitmap::new(240, 320, PixelFormat::Rgba8).unwrap();
        assert!(adequate(&good, size, 160, 8192));
        assert!(!adequate(&good, size, 640, 8192));
        assert!(!adequate(&good, size, 160, 256));
        assert!(!adequate(
            &good,
            Size {
                width: 800.0,
                height: 600.0
            },
            160,
            8192
        ));
        assert!(!adequate(
            &Bitmap::new(2, 2, PixelFormat::Rgba8).unwrap(),
            size,
            160,
            8192
        ));
    }
}
