// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use pdfoxium::{Document, Error, ObjectKind, PixelFormat};

#[test]
fn colored_and_uncolored_fill_and_stroke_patterns_return_owned_cells() {
    // Same pinned fixtures and exact color assertions as
    // CoveragePinnedEditImage.OriginalColoredAndUncoloredPatternObjectsExtractTheirActualCells.
    for stroke in [false, true] {
        for uncolored in [false, true] {
            let name = format!(
                "pattern_{}{}",
                if stroke { "stroke" } else { "fill" },
                if uncolored { "_uncolored" } else { "" }
            );
            let doc = Document::open(format!("../../tests/render_corpus/pdfs/{name}.pdf")).unwrap();
            let page = doc.page(0).unwrap();
            let object = page.objects().unwrap()[0].clone();
            assert_eq!(object.kind, ObjectKind::Path);
            let cell = page.object_pattern(object.id, stroke).unwrap().unwrap();
            let size = if stroke { 72 } else { 36 };
            assert_eq!((cell.width(), cell.height()), (size, size));
            assert_eq!(cell.format(), PixelFormat::Bgra8);
            assert_eq!(cell.pixels().len(), size as usize * size as usize * 4);
            let color = if uncolored {
                [255, 0, 0, 255]
            } else {
                [44, 27, 238, 255]
            };
            assert_eq!(&cell.pixels()[..4], &color);
            assert!(page.object_pattern(object.id, !stroke).unwrap().is_none());
            // Returned cells own their pixels and survive unloading the source.
            page.unload().unwrap();
            assert!(matches!(
                page.object_pattern(object.id, stroke),
                Err(Error::Stale)
            ));
            let reloaded = page.objects().unwrap()[0].id;
            assert_eq!(
                cell.pixels(),
                page.object_pattern(reloaded, stroke)
                    .unwrap()
                    .unwrap()
                    .pixels()
            );
            if let Some(dir) = std::env::var_os("PDFOXIUM_EDIT_ARTIFACT_DIR") {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(
                    std::path::Path::new(&dir).join(format!("{name}-cell.bgra")),
                    cell.pixels(),
                )
                .unwrap();
            }
            drop(page);
            drop(doc);
            assert_eq!(&cell.pixels()[..4], &color);
        }
    }
}
