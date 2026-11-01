// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

use pdfoxium::thread::{CancelGroup, Priority, RenderPool, Source};
use pdfoxium::view::{TileGrid, TileGridOptions};
use pdfoxium::{Config, DeviceRect, Document, RenderConfig, Rotation};
use std::collections::HashSet;
use std::sync::Arc;

#[test]
fn planned_pool_regions_reassemble_visible_tiles() {
    let bytes: Arc<[u8]> = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/render_corpus/pdfs/rectangles_multi_pages.pdf"),
    )
    .unwrap()
    .into();
    let doc = Document::from_bytes(bytes.clone()).unwrap();
    let page = doc.page(0).unwrap();
    let pool = RenderPool::new(Source::Bytes(bytes), Config::default(), 2).unwrap();
    for rotation in [Rotation::None, Rotation::Cw90] {
        let base = RenderConfig::fit(192, 192).rotation(rotation);
        let whole = page.render(&base).unwrap();
        let grid = TileGrid::new(
            whole.width(),
            whole.height(),
            TileGridOptions {
                tile_size: 31,
                whole_page_max_pixels: 0,
            },
        )
        .unwrap();
        let mut cached = HashSet::new();
        // First display, diagonal scroll, then the complete page. Reuse tiles
        // only within this rotation/configuration namespace.
        for viewport in [
            DeviceRect::new(0, 0, 60, 60),
            DeviceRect::new(31, 31, 90, 90),
            DeviceRect::new(0, 0, whole.width(), whole.height()),
        ] {
            let plan = grid.plan(viewport, |id| cached.contains(&id));
            let group = CancelGroup::new();
            let pending: Vec<_> = plan
                .requests()
                .iter()
                .map(|request| {
                    (
                        request,
                        pool.render_in_group(0, request.config(&base), Priority::Visible, &group),
                    )
                })
                .collect();
            for (request, ticket) in pending {
                let result = ticket.wait().unwrap();
                assert_eq!(result.source.generation, pool.generation());
                let region = result.bitmap;
                assert_eq!(
                    (region.width(), region.height()),
                    (request.rect().width, request.rect().height)
                );
                for (id, source) in request.tiles() {
                    assert!(cached.insert(id));
                    let target = grid.tile_rect(id).unwrap();
                    for y in 0..source.height as usize {
                        let from =
                            (source.y as usize + y) * region.stride() + source.x as usize * 4;
                        let to = (target.y as usize + y) * whole.stride() + target.x as usize * 4;
                        for (a, b) in region.pixels()[from..from + source.width as usize * 4]
                            .iter()
                            .zip(&whole.pixels()[to..to + source.width as usize * 4])
                        {
                            assert!(a.abs_diff(*b) <= 2, "region AA differs by more than 2/255");
                        }
                    }
                }
            }
            assert!(grid.plan(viewport, |id| cached.contains(&id)).is_empty());
        }
        assert_eq!(cached.len(), (grid.columns() * grid.rows()) as usize);
    }
}
