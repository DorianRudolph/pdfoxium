// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use pdfoxium::{DeviceRect, Document, Error, Point, RenderConfig, Rotation};
#[test]
fn render_matrix_matches_region_translation_and_page_lease_rules() {
    let bytes = std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/render_corpus/pdfs/rectangles_multi_pages.pdf"),
    )
    .unwrap();
    let doc = Document::from_bytes(bytes).unwrap();
    let page = doc.page(0).unwrap();
    for rotation in [
        Rotation::None,
        Rotation::Cw90,
        Rotation::Cw180,
        Rotation::Cw270,
    ] {
        let base = RenderConfig::fit(200, 200).rotation(rotation);
        let whole = page.render_matrix(&base).unwrap();
        let region = page
            .render_matrix(&base.clone().device_clip(DeviceRect::new(13, 17, 40, 50)))
            .unwrap();
        for point in [Point { x: 0.0, y: 0.0 }, Point { x: 37.5, y: 65.0 }] {
            let a = whole.transform(point);
            let b = region.transform(point);
            assert!((a.x - b.x - 13.0).abs() < 0.001);
            assert!((a.y - b.y - 17.0).abs() < 0.001);
        }
        let job = page.render_job(&base).unwrap();
        assert!(matches!(page.render_matrix(&base), Err(Error::Busy)));
        drop(job);
    }
    assert!(page.render_matrix(&RenderConfig::scale(f32::NAN)).is_err());
    assert!(page
        .render_matrix(&RenderConfig::default().device_clip(DeviceRect::new(0, 0, 0, 1)))
        .is_err());
}
