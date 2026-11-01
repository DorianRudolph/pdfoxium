// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Page placement and zoom anchors, independent of rendering and GPU resources.
use eframe::egui::{pos2, vec2, Rect, Vec2};

pub const GAP: f32 = 24.0;
pub struct Layout {
    pub pages: Vec<Rect>,
    pub size: Vec2,
    zoom: f32,
}
#[derive(Clone, Copy)]
pub struct Anchor {
    page: usize,
    point: Vec2,
    gap_y: f32,
    offset: Vec2,
}
impl Layout {
    pub fn new(sizes: &[Vec2], zoom: f32, viewport_width: f32) -> Self {
        let width = sizes.iter().map(|s| s.x * zoom).fold(0.0, f32::max) + GAP * 2.0;
        let width = width.max(viewport_width);
        // Accumulate in f64 so page spacing/height rounding does not drift over
        // hundreds of pages. Convert only individual UI coordinates to f32.
        let mut y = f64::from(GAP);
        let pages = sizes
            .iter()
            .map(|s| {
                let size = *s * zoom;
                let rect = Rect::from_min_size(pos2((width - size.x) * 0.5, y as f32), size);
                y += f64::from(s.y) * f64::from(zoom) + f64::from(GAP);
                rect
            })
            .collect();
        Self {
            pages,
            size: vec2(width, y as f32),
            zoom,
        }
    }
    /// Preserve the page-local point at this viewport offset. A point in a page
    /// gap keeps its distance to the adjacent edge in logical points.
    pub fn anchor(&self, scroll: Vec2, offset: Vec2) -> Option<Anchor> {
        if self.pages.is_empty() {
            return None;
        }
        let point = scroll + offset;
        let page = self
            .pages
            .partition_point(|r| r.max.y < point.y)
            .min(self.pages.len() - 1);
        let rect = self.pages[page];
        let page_y = point.y.clamp(rect.min.y, rect.max.y);
        Some(Anchor {
            page,
            point: vec2(
                (point.x - rect.min.x) / self.zoom,
                (page_y - rect.min.y) / self.zoom,
            ),
            gap_y: point.y - page_y,
            offset,
        })
    }
    pub fn anchored_scroll(&self, anchor: Anchor) -> Vec2 {
        let rect = self.pages[anchor.page];
        (rect.min.to_vec2() + anchor.point * self.zoom + vec2(0.0, anchor.gap_y) - anchor.offset)
            .max(Vec2::ZERO)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn close(a: Vec2, b: Vec2) {
        assert!((a - b).abs().max_elem() < 0.5, "{a:?} != {b:?}");
    }
    #[test]
    fn deep_book_zoom_keeps_page_point_and_round_trips() {
        let sizes: Vec<_> = (0..1000)
            .map(|p| {
                if p % 2 == 0 {
                    vec2(600.0, 800.0)
                } else {
                    vec2(750.0, 1000.0)
                }
            })
            .collect();
        let offset = vec2(370.0, 290.0);
        let original = Layout::new(&sizes, 1.25, 1100.0);
        let page = 800;
        let point = vec2(240.0, 375.0);
        let scroll = original.pages[page].min.to_vec2() + point * 1.25 - offset;
        let anchor = original.anchor(scroll, offset).unwrap();
        for zoom in [0.4, 1.0, 2.0, 5.0, 8.0] {
            let new = Layout::new(&sizes, zoom, 1100.0);
            let new_scroll = new.anchored_scroll(anchor);
            close(
                new.pages[page].min.to_vec2() + point * zoom - new_scroll,
                offset,
            );
            let back = original.anchored_scroll(new.anchor(new_scroll, offset).unwrap());
            close(scroll, back);
        }
    }
    #[test]
    fn fixed_gaps_centering_and_boundaries() {
        let sizes = [vec2(300.0, 500.0), vec2(600.0, 800.0)];
        let old = Layout::new(&sizes, 1.0, 1000.0);
        let new = Layout::new(&sizes, 3.0, 1000.0);
        let offset = vec2(500.0, 120.0);
        let scroll = vec2(0.0, old.pages[1].min.y - 10.0 - offset.y);
        let anchor = old.anchor(scroll, offset).unwrap();
        let scrolled = new.anchored_scroll(anchor);
        assert!((new.pages[1].min.y - scrolled.y - offset.y - 10.0).abs() < 0.001);
        close(
            new.pages[1].center().to_vec2() - scrolled,
            vec2(offset.x, new.pages[1].center().y - scrolled.y),
        );
        assert!(Layout::new(&[], 1.0, 1000.0)
            .anchor(Vec2::ZERO, offset)
            .is_none());
        let start = old.anchor(Vec2::ZERO, Vec2::ZERO).unwrap();
        assert!(new.anchored_scroll(start).min_elem() >= 0.0);
    }
}
