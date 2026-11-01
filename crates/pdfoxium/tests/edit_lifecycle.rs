// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use pdfoxium::{Color, Document, Error, PathSpec, Rect, RenderConfig, Size};

fn edited() -> (Document, pdfoxium::Page) {
    let doc = Document::create().unwrap();
    let page = doc
        .insert_blank_page(
            0,
            Size {
                width: 100.,
                height: 100.,
            },
        )
        .unwrap();
    page.add_path(&PathSpec {
        fill: Some(Color::rgb(255, 0, 0)),
        ..PathSpec::rect(Rect {
            left: 10.,
            bottom: 10.,
            right: 60.,
            top: 60.,
        })
    })
    .unwrap();
    (doc, page)
}

#[test]
fn unload_import_and_failed_save_preserve_unsaved_edits() {
    let (doc, page) = edited();
    let expected = page.render(&RenderConfig::fit(100, 100)).unwrap();
    assert!(doc.is_dirty());
    page.unload().unwrap();
    assert!(doc.is_dirty());
    assert_eq!(
        page.render(&RenderConfig::fit(100, 100)).unwrap().pixels(),
        expected.pixels()
    );
    // Import must also generate content for a page that has not been unloaded.
    let id = page.objects().unwrap()[0].id;
    page.transform_object(id, pdfoxium::Matrix::translate(10., 0.))
        .unwrap();
    let expected = page.render(&RenderConfig::fit(100, 100)).unwrap();
    let copy = Document::create().unwrap();
    copy.import_pages(&doc, &[0], 0).unwrap();
    assert_eq!(
        copy.page(0)
            .unwrap()
            .render(&RenderConfig::fit(100, 100))
            .unwrap()
            .pixels(),
        expected.pixels()
    );
    struct Fail;
    impl std::io::Write for Fail {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("failure"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    assert!(matches!(doc.save_to(&mut Fail), Err(Error::Io(_))));
    assert!(doc.is_dirty());
    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    assert!(!doc.is_dirty());
    assert_eq!(
        Document::from_bytes(saved)
            .unwrap()
            .page(0)
            .unwrap()
            .render(&RenderConfig::fit(100, 100))
            .unwrap()
            .pixels(),
        expected.pixels()
    );
}

#[test]
fn edits_invalidate_iterators_but_keep_text_snapshots_and_deleted_handles_safe() {
    let (doc, page) = edited();
    let snapshot = page.text().unwrap();
    let mut pages = doc.pages().unwrap();
    page.remove_object(page.objects().unwrap()[0].id).unwrap();
    assert!(matches!(pages.next(), Some(Err(Error::Stale))));
    assert_eq!(snapshot.as_str(), "");
    let id = page.id();
    doc.delete_page(id).unwrap();
    assert!(matches!(page.layout(), Err(Error::Stale)));
    assert!(matches!(page.objects(), Err(Error::Stale)));
    assert!(matches!(page.text(), Err(Error::Stale)));
    assert!(matches!(page.unload(), Err(Error::Stale)));
    assert!(matches!(doc.page_index(id), Err(Error::Stale)));
}
