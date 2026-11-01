// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Device-pixel tile grids and pure request planning, independent of a UI.
//!
//! A grid describes one page at one render configuration. Cache namespaces must
//! also include document identity/revision (or pool generation and page index),
//! the complete render configuration, and grid settings. A [`TileId`] alone is
//! only a position, not a globally unique cache key. Clear or switch the cache
//! namespace after zoom, rotation, edits, source reloads or appearance changes.
//!
//! ```
//! use pdfoxium::{DeviceRect, RenderConfig};
//! use pdfoxium::view::{TileGrid, TileGridOptions};
//! # fn main() -> pdfoxium::Result<()> {
//! let grid = TileGrid::new(2000, 3000, TileGridOptions {
//!     tile_size: 512,
//!     whole_page_max_pixels: 1024 * 1024, // caller's texture policy
//! })?;
//! let plan = grid.plan(DeviceRect::new(0, 0, 1000, 800), |_| false);
//! assert_eq!(plan.requests().len(), 1);
//! for request in plan.requests() {
//!     let config = request.config(&RenderConfig::default());
//!     // Submit config to Page::render or RenderPool::render, then upload
//!     // request.tiles() from the returned bitmap using its stride.
//!     assert_eq!(request.tiles().count(), 4);
//! }
//! # Ok(()) }
//! ```
//!
//! See `docs/tile-planning.md` for use with the render pool.

use std::collections::BTreeMap;

use crate::{DeviceRect, Error, RenderConfig, Result};

/// Grid policy. Thresholds are explicit because the useful texture size depends
/// on the viewer. Set `whole_page_max_pixels` to zero to always use tiles.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TileGridOptions {
    /// Edge length of square tiles; edge tiles are clipped to the page.
    pub tile_size: u32,
    /// Pages with at most this many device pixels become a single tile.
    pub whole_page_max_pixels: u64,
}

/// Position within a particular grid/cache namespace.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct TileId {
    pub column: u32,
    pub row: u32,
}

/// A page's tile layout in full-canvas device pixels, after rotation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TileGrid {
    width: u32,
    height: u32,
    tile_width: u32,
    tile_height: u32,
}

impl TileGrid {
    /// Use the dimensions returned by `Page::render_size` for the same config
    /// later passed to rendering. Dimensions and tile size must be nonzero.
    pub fn new(width: u32, height: u32, options: TileGridOptions) -> Result<Self> {
        if width == 0 || height == 0 || options.tile_size == 0 {
            return Err(Error::InvalidArgument(
                "page and tile dimensions must be nonzero",
            ));
        }
        let whole = u64::from(width) * u64::from(height) <= options.whole_page_max_pixels;
        Ok(Self {
            width,
            height,
            tile_width: if whole { width } else { options.tile_size },
            tile_height: if whole { height } else { options.tile_size },
        })
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    pub fn columns(&self) -> u32 {
        self.width.div_ceil(self.tile_width)
    }
    pub fn rows(&self) -> u32 {
        self.height.div_ceil(self.tile_height)
    }

    /// Full-page pixel rectangle of a tile; `None` for an invalid position.
    pub fn tile_rect(&self, tile: TileId) -> Option<DeviceRect> {
        if tile.column >= self.columns() || tile.row >= self.rows() {
            return None;
        }
        let x = tile.column * self.tile_width;
        let y = tile.row * self.tile_height;
        Some(DeviceRect::new(
            x,
            y,
            self.tile_width.min(self.width - x),
            self.tile_height.min(self.height - y),
        ))
    }

    /// Tiles intersecting a half-open viewport, in row order. The viewport is
    /// clipped to the page; empty/off-page viewports yield no tiles. Round a
    /// fractional UI viewport outward before converting to device pixels.
    pub fn visible_tiles(&self, viewport: DeviceRect) -> impl Iterator<Item = TileId> + use<> {
        let right =
            (u64::from(viewport.x) + u64::from(viewport.width)).min(u64::from(self.width)) as u32;
        let bottom =
            (u64::from(viewport.y) + u64::from(viewport.height)).min(u64::from(self.height)) as u32;
        let left = viewport.x.min(self.width);
        let top = viewport.y.min(self.height);
        let (first_col, end_col, first_row, end_row) = if left >= right || top >= bottom {
            (0, 0, 0, 0)
        } else {
            (
                left / self.tile_width,
                right.div_ceil(self.tile_width),
                top / self.tile_height,
                bottom.div_ceil(self.tile_height),
            )
        };
        (first_row..end_row)
            .flat_map(move |row| (first_col..end_col).map(move |column| TileId { column, row }))
    }

    /// Plan missing visible tiles. `available` may include both cached tiles and
    /// tiles already in flight, to avoid submitting duplicates. It is called
    /// exactly once per visible tile. Failed/cancelled work must be removed from
    /// the caller's in-flight set before replanning.
    ///
    /// Merge horizontal runs, then identical runs in adjacent rows. Requests
    /// never overlap or include available tiles. No rendering or cache mutation
    /// happens here; all requests have visible priority when submitted by a UI.
    pub fn plan(
        &self,
        viewport: DeviceRect,
        mut available: impl FnMut(TileId) -> bool,
    ) -> RenderPlan {
        let mut requests: Vec<RenderRequest> = Vec::new();
        let mut previous = BTreeMap::new();
        let mut current = BTreeMap::new();
        let mut row = None;
        let mut run: Option<(u32, u32)> = None;
        let mut flush = |run: &mut Option<(u32, u32)>,
                         row: u32,
                         previous: &BTreeMap<(u32, u32), usize>,
                         current: &mut BTreeMap<(u32, u32), usize>| {
            if let Some((start, end)) = run.take() {
                let key = (start, end);
                let first = self.tile_rect(TileId { column: start, row }).unwrap();
                let last = self
                    .tile_rect(TileId {
                        column: end - 1,
                        row,
                    })
                    .unwrap();
                let index = if let Some(&index) = previous.get(&key) {
                    requests[index].rect.height += first.height;
                    requests[index].end_row = row + 1;
                    index
                } else {
                    requests.push(RenderRequest {
                        grid: *self,
                        rect: DeviceRect::new(
                            first.x,
                            first.y,
                            last.x + last.width - first.x,
                            first.height,
                        ),
                        first_column: start,
                        end_column: end,
                        first_row: row,
                        end_row: row + 1,
                    });
                    requests.len() - 1
                };
                current.insert(key, index);
            }
        };
        for tile in self.visible_tiles(viewport) {
            if row != Some(tile.row) {
                if let Some(old_row) = row {
                    flush(&mut run, old_row, &previous, &mut current);
                    previous = std::mem::take(&mut current);
                }
                row = Some(tile.row);
            }
            if available(tile) {
                flush(&mut run, tile.row, &previous, &mut current);
            } else if let Some((_, end)) = &mut run {
                *end = tile.column + 1;
            } else {
                run = Some((tile.column, tile.column + 1));
            }
        }
        if let Some(row) = row {
            flush(&mut run, row, &previous, &mut current);
        }
        RenderPlan { requests }
    }
}

/// Missing visible regions in deterministic first-encounter order.
#[derive(Clone, Debug)]
pub struct RenderPlan {
    requests: Vec<RenderRequest>,
}
impl RenderPlan {
    pub fn requests(&self) -> &[RenderRequest] {
        &self.requests
    }
    pub fn is_empty(&self) -> bool {
        self.requests.is_empty()
    }
}

/// One merged rectangle to render; use [`Self::tiles`] to upload/copy its tiles.
#[derive(Clone, Debug)]
pub struct RenderRequest {
    grid: TileGrid,
    rect: DeviceRect,
    first_column: u32,
    end_column: u32,
    first_row: u32,
    end_row: u32,
}
impl RenderRequest {
    /// Full-page device rectangle.
    pub fn rect(&self) -> DeviceRect {
        self.rect
    }
    /// Keep the full page's scale/rotation/settings and replace its clip.
    /// Do not fit the page to the request size: that would change its scale.
    pub fn config(&self, base: &RenderConfig) -> RenderConfig {
        base.clone().device_clip(self.rect)
    }
    /// Tile IDs and source rectangles relative to the returned region bitmap.
    /// Honor the bitmap's stride when copying rows or uploading textures.
    pub fn tiles(&self) -> impl Iterator<Item = (TileId, DeviceRect)> + '_ {
        (self.first_row..self.end_row).flat_map(move |row| {
            (self.first_column..self.end_column).map(move |column| {
                let id = TileId { column, row };
                let rect = self.grid.tile_rect(id).unwrap();
                (
                    id,
                    DeviceRect::new(
                        rect.x - self.rect.x,
                        rect.y - self.rect.y,
                        rect.width,
                        rect.height,
                    ),
                )
            })
        })
    }
}

#[cfg(test)]
mod tests;
