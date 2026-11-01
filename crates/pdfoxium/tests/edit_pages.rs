// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

use pdfoxium::{Document, Error, FlattenUsage, PageBox, Rect, RenderConfig, Rotation, Size};

fn source() -> Document {
    Document::from_bytes(
        include_bytes!("../../../tests/render_corpus/pdfs/rectangles_multi_pages.pdf").as_slice(),
    )
    .unwrap()
}

#[test]
fn sequence_edits_preserve_live_ids_and_saved_pixels() {
    let doc = source();
    let pages: Vec<_> = doc.pages().unwrap().map(Result::unwrap).collect();
    let config = RenderConfig::fit(80, 80);
    let pixels: Vec<_> = pages
        .iter()
        .map(|page| page.render(&config).unwrap().pixels().to_vec())
        .collect();
    let mut iteration = doc.pages().unwrap();
    let blank = doc
        .insert_blank_page(
            0,
            Size {
                width: 20.0,
                height: 30.0,
            },
        )
        .unwrap();
    assert_eq!(pages[0].index(), 1);
    assert!(matches!(iteration.next(), Some(Err(Error::Stale))));
    doc.move_pages(&[pages[2].id(), pages[0].id()], 1).unwrap();
    assert_eq!(doc.page(0).unwrap().id(), blank.id());
    assert_eq!(doc.page(1).unwrap().id(), pages[2].id());
    assert_eq!(doc.page(2).unwrap().id(), pages[0].id());
    assert_eq!(doc.page(3).unwrap().id(), pages[1].id());
    doc.delete_page(blank.id()).unwrap();
    assert!(matches!(blank.width(), Err(Error::Stale)));
    assert!(matches!(doc.page_by_id(blank.id()), Err(Error::Stale)));
    for (index, original) in [2, 0, 1].into_iter().enumerate() {
        assert_eq!(pages[original].index(), index);
        pages[original].unload().unwrap();
        assert_eq!(
            pages[original].render(&config).unwrap().pixels(),
            pixels[original]
        );
    }
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    let reopened = Document::from_bytes(bytes).unwrap();
    for (index, original) in [2, 0, 1].into_iter().enumerate() {
        assert_eq!(
            reopened
                .page(index)
                .unwrap()
                .render(&config)
                .unwrap()
                .pixels(),
            pixels[original]
        );
    }
}

#[test]
fn blank_boxes_rotation_and_unload_survive_save() {
    let doc = Document::create().unwrap();
    let page = doc
        .insert_blank_page(
            0,
            Size {
                width: 80.0,
                height: 120.0,
            },
        )
        .unwrap();
    let config = RenderConfig::scale(1.0);
    let blank = page.render(&config).unwrap();
    assert_eq!((blank.width(), blank.height()), (80, 120));
    assert!(blank.pixels().iter().all(|value| *value == 255));
    page.set_box(
        PageBox::Crop,
        Rect {
            left: 10.0,
            bottom: 20.0,
            right: 60.0,
            top: 100.0,
        },
    )
    .unwrap();
    page.set_rotation(Rotation::Cw90).unwrap();
    assert_eq!(
        (page.width().unwrap(), page.height().unwrap()),
        (80.0, 50.0)
    );
    for kind in [PageBox::Bleed, PageBox::Trim, PageBox::Art] {
        page.set_box(
            kind,
            Rect {
                left: 12.0,
                bottom: 22.0,
                right: 58.0,
                top: 98.0,
            },
        )
        .unwrap();
    }
    page.unload().unwrap();
    assert_eq!(page.layout().unwrap().rotation, Rotation::Cw90);
    let expected = page.render(&config).unwrap();
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    let reopened = Document::from_bytes(bytes).unwrap();
    let actual = reopened.page(0).unwrap().render(&config).unwrap();
    assert_eq!((actual.width(), actual.height()), (80, 50));
    assert_eq!(actual.pixels(), expected.pixels());
    doc.delete_page(page.id()).unwrap();
    assert_eq!(doc.page_count().unwrap(), 0);
    assert_eq!(
        doc.insert_blank_page(
            0,
            Size {
                width: 10.0,
                height: 10.0
            }
        )
        .unwrap()
        .index(),
        0
    );
}

#[test]
fn validation_and_active_jobs_leave_document_unchanged() {
    let doc = source();
    let page = doc.page(0).unwrap();
    let revision = doc.revision();
    let count = doc.page_count().unwrap();
    assert!(matches!(
        doc.move_pages(&[page.id(), page.id()], 0),
        Err(Error::InvalidArgument(_))
    ));
    assert!(matches!(
        doc.move_pages(&[page.id()], count),
        Err(Error::InvalidArgument(_))
    ));
    assert!(matches!(
        doc.insert_blank_page(
            0,
            Size {
                width: f32::NAN,
                height: 1.0
            }
        ),
        Err(Error::InvalidArgument(_))
    ));
    assert!(matches!(
        page.set_box(
            PageBox::Media,
            Rect {
                left: 5.0,
                right: 0.0,
                bottom: 0.0,
                top: 5.0
            }
        ),
        Err(Error::InvalidArgument(_))
    ));
    let other = source();
    assert!(matches!(
        doc.delete_page(other.page(0).unwrap().id()),
        Err(Error::Stale)
    ));
    let job = page.render_job(&RenderConfig::fit(20, 20)).unwrap();
    assert!(matches!(doc.delete_page(page.id()), Err(Error::Busy)));
    assert!(matches!(doc.move_pages(&[page.id()], 1), Err(Error::Busy)));
    assert!(matches!(
        doc.insert_blank_page(
            0,
            Size {
                width: 20.0,
                height: 20.0
            }
        ),
        Err(Error::Busy)
    ));
    assert!(matches!(
        page.set_rotation(Rotation::Cw90),
        Err(Error::Busy)
    ));
    assert!(matches!(
        page.flatten(FlattenUsage::Display),
        Err(Error::Busy)
    ));
    drop(job);
    assert_eq!(doc.revision(), revision);
    assert_eq!(doc.page_count().unwrap(), count);
}

#[test]
fn flatten_preserves_visible_annotation_and_saved_render() {
    let doc = Document::from_bytes(
        include_bytes!("../../../tests/render_corpus/pdfs/stamp_appearance.pdf").as_slice(),
    )
    .unwrap();
    let page = doc.page(0).unwrap();
    let config = RenderConfig::fit(150, 150).annotations(false);
    let without = page.render(&config).unwrap();
    let with = page.render(&config.clone().annotations(true)).unwrap();
    assert_ne!(without.pixels(), with.pixels());
    assert!(page.flatten(FlattenUsage::Display).unwrap());
    assert_eq!(page.render(&config).unwrap().pixels(), with.pixels());
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    let reopened = Document::from_bytes(bytes).unwrap();
    assert_eq!(
        reopened.page(0).unwrap().render(&config).unwrap().pixels(),
        with.pixels()
    );
    assert!(!page.flatten(FlattenUsage::Print).unwrap());
}

#[test]
fn repeated_dictionary_references_keep_occurrence_identity() {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R 3 0 R 4 0 R] /Count 3 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 20 30] >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 40 50] >>",
    ];
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend_from_slice(b"xref\n0 5\n0000000000 65535 f \n");
    for offset in offsets {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(
        format!("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes(),
    );
    let doc = Document::from_bytes(bytes).unwrap();
    let first = doc.page(0).unwrap();
    let second = doc.page(1).unwrap();
    let third = doc.page(2).unwrap();
    doc.move_pages(&[second.id()], 0).unwrap();
    assert_eq!(doc.page(0).unwrap().id(), second.id());
    assert_eq!(doc.page(1).unwrap().id(), first.id());
    assert_eq!(doc.page(2).unwrap().id(), third.id());
    doc.delete_page(second.id()).unwrap();
    assert!(matches!(second.width(), Err(Error::Stale)));
    assert_eq!(first.width().unwrap(), 20.0);
    assert_eq!(first.index(), 0);
    assert_eq!(third.index(), 1);
}

#[test]
fn snapshots_include_edits_without_marking_them_saved() {
    let doc = source();
    assert!(!doc.is_dirty());
    let clean = doc.snapshot().unwrap();
    assert!(!doc.is_dirty());
    assert_eq!(
        Document::from_bytes(clean).unwrap().page_count().unwrap(),
        doc.page_count().unwrap()
    );
    doc.page(0).unwrap().set_rotation(Rotation::Cw90).unwrap();
    assert!(doc.is_dirty());
    let revision = doc.revision();
    let bytes = doc.snapshot().unwrap();
    assert!(doc.is_dirty());
    assert_eq!(doc.revision(), revision);
    let copy = Document::from_bytes(bytes).unwrap();
    assert_eq!(copy.page_layout(0).unwrap().rotation, Rotation::Cw90);
    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    assert!(!doc.is_dirty());
}
