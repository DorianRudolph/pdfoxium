// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

use super::*;
use std::collections::HashSet;

fn grid(width: u32, height: u32, tile_size: u32) -> TileGrid {
    TileGrid::new(
        width,
        height,
        TileGridOptions {
            tile_size,
            whole_page_max_pixels: 0,
        },
    )
    .unwrap()
}
fn id(column: u32, row: u32) -> TileId {
    TileId { column, row }
}

#[test]
fn viewport_boundaries_edges_and_overflow() {
    let grid = grid(25, 19, 10);
    assert_eq!((grid.columns(), grid.rows()), (3, 2));
    assert_eq!(
        grid.tile_rect(id(2, 1)),
        Some(DeviceRect::new(20, 10, 5, 9))
    );
    assert_eq!(grid.tile_rect(id(3, 0)), None);
    assert_eq!(
        grid.visible_tiles(DeviceRect::new(10, 0, 10, 10))
            .collect::<Vec<_>>(),
        vec![id(1, 0)]
    );
    assert_eq!(
        grid.visible_tiles(DeviceRect::new(9, 9, 2, 2))
            .collect::<Vec<_>>(),
        vec![id(0, 0), id(1, 0), id(0, 1), id(1, 1)]
    );
    assert_eq!(
        grid.visible_tiles(DeviceRect::new(20, 10, u32::MAX, u32::MAX))
            .collect::<Vec<_>>(),
        vec![id(2, 1)]
    );
    for viewport in [
        DeviceRect::new(0, 0, 0, 10),
        DeviceRect::new(25, 0, 10, 10),
        DeviceRect::new(u32::MAX, u32::MAX, 2, 2),
    ] {
        assert!(grid.plan(viewport, |_| panic!("empty viewport")).is_empty());
    }
    let large = super::TileGrid::new(
        u32::MAX,
        u32::MAX,
        TileGridOptions {
            tile_size: u32::MAX - 1,
            whole_page_max_pixels: 0,
        },
    )
    .unwrap();
    assert_eq!(
        large.tile_rect(id(1, 1)),
        Some(DeviceRect::new(u32::MAX - 1, u32::MAX - 1, 1, 1))
    );
    assert_eq!(
        large
            .plan(DeviceRect::new(0, 0, u32::MAX, u32::MAX), |_| false)
            .requests()[0]
            .rect(),
        DeviceRect::new(0, 0, u32::MAX, u32::MAX)
    );
}

#[test]
fn explicit_whole_page_threshold_and_zoom() {
    let options = TileGridOptions {
        tile_size: 10,
        whole_page_max_pixels: 600,
    };
    let small = TileGrid::new(20, 30, options).unwrap();
    assert_eq!((small.columns(), small.rows()), (1, 1));
    let plan = small.plan(DeviceRect::new(5, 5, 1, 1), |_| false);
    assert_eq!(plan.requests()[0].rect(), DeviceRect::new(0, 0, 20, 30));
    let zoomed = TileGrid::new(40, 60, options).unwrap();
    assert_eq!((zoomed.columns(), zoomed.rows()), (4, 6));
    // A new zoom uses its own cache namespace, even for identical tile IDs.
    assert_eq!(
        zoomed.tile_rect(id(0, 0)),
        Some(DeviceRect::new(0, 0, 10, 10))
    );
    for (width, height, tile_size) in [(0, 1, 1), (1, 0, 1), (1, 1, 0)] {
        assert!(TileGrid::new(
            width,
            height,
            TileGridOptions {
                tile_size,
                whole_page_max_pixels: 0
            }
        )
        .is_err());
    }
}

#[test]
fn bands_l_shapes_and_cached_gaps() {
    let grid = grid(40, 40, 10);
    let viewport = DeviceRect::new(0, 0, 40, 40);
    assert_eq!(grid.plan(viewport, |_| false).requests().len(), 1);
    assert!(grid.plan(viewport, |_| true).is_empty());
    for vertical in [false, true] {
        let band = grid.plan(
            viewport,
            |t| if vertical { t.column < 3 } else { t.row < 3 },
        );
        assert_eq!(band.requests().len(), 1);
    }
    let l = grid.plan(viewport, |t| t.column < 3 && t.row < 3);
    assert_eq!(
        l.requests()
            .iter()
            .map(RenderRequest::rect)
            .collect::<Vec<_>>(),
        vec![
            DeviceRect::new(30, 0, 10, 30),
            DeviceRect::new(0, 30, 40, 10)
        ]
    );
    let gap = grid.plan(viewport, |t| t.row == 1);
    assert_eq!(gap.requests().len(), 2);
    assert_eq!(gap.requests()[1].rect(), DeviceRect::new(0, 20, 40, 20));
}

#[test]
fn every_cache_mask_covers_missing_tiles_once_with_correct_source_offsets() {
    let grid = grid(37, 28, 10);
    for mask in 0..(1u32 << 12) {
        let mut queried = HashSet::new();
        let plan = grid.plan(DeviceRect::new(0, 0, 37, 28), |tile| {
            assert!(queried.insert(tile));
            mask & (1 << (tile.row * 4 + tile.column)) != 0
        });
        assert_eq!(queried.len(), 12);
        let mut covered = HashSet::new();
        for request in plan.requests() {
            let rect = request.rect();
            let mut area = 0;
            for (tile, source) in request.tiles() {
                assert!(covered.insert(tile));
                assert_eq!(mask & (1 << (tile.row * 4 + tile.column)), 0);
                assert_eq!(
                    grid.tile_rect(tile).unwrap(),
                    DeviceRect::new(
                        rect.x + source.x,
                        rect.y + source.y,
                        source.width,
                        source.height
                    )
                );
                assert!(source.x + source.width <= rect.width);
                assert!(source.y + source.height <= rect.height);
                area += source.width * source.height;
            }
            assert_eq!(area, rect.width * rect.height);
        }
        assert_eq!(covered.len(), 12 - mask.count_ones() as usize);
    }
}
