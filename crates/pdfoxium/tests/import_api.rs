// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

use pdfoxium::{Document, Error, RenderConfig};

fn source() -> Document {
    Document::from_bytes(
        include_bytes!("../../../tests/render_corpus/pdfs/rectangles_multi_pages.pdf").as_slice(),
    )
    .unwrap()
}

#[test]
fn import_preserves_order_duplicates_existing_identity_and_saved_pixels() {
    let source = source();
    let output = Document::create().unwrap();
    assert_eq!(output.version().unwrap().major, 1);
    assert_eq!(output.version().unwrap().minor, 7);
    assert_eq!(output.page_count().unwrap(), 0);
    output.import_pages(&source, &[2, 0], 0).unwrap();
    let retained = output.page(1).unwrap();
    let alias = retained.clone();
    let id = retained.id();
    let config = RenderConfig::fit(96, 96);
    let original_pixels = retained.render(&config).unwrap();
    let mut old_iteration = output.pages().unwrap();
    assert_eq!(old_iteration.len(), 2);
    output.import_pages(&source, &[1, 1], 1).unwrap();
    assert_eq!(retained.index(), 3);
    assert_eq!(alias.index(), 3);
    assert_eq!(output.page_by_id(id).unwrap().index(), 3);
    assert_eq!(output.page(3).unwrap().id(), id);
    assert_ne!(output.page(1).unwrap().id(), id);
    assert_eq!(old_iteration.len(), 1);
    assert!(matches!(old_iteration.next(), Some(Err(Error::Stale))));
    assert!(old_iteration.next().is_none());
    retained.unload().unwrap();
    assert_eq!(
        alias.render(&config).unwrap().pixels(),
        original_pixels.pixels()
    );
    let mut saved = Vec::new();
    output.save_to(&mut saved).unwrap();
    let reopened = Document::from_bytes(saved).unwrap();
    assert_eq!(reopened.page_count().unwrap(), 4);
    for (index, source_index) in [2, 1, 1, 0].into_iter().enumerate() {
        assert_eq!(
            reopened
                .page(index)
                .unwrap()
                .render(&config)
                .unwrap()
                .pixels(),
            source
                .page(source_index)
                .unwrap()
                .render(&config)
                .unwrap()
                .pixels()
        );
    }
    // Neither imported data nor existing public page handles retain the source.
    drop(source);
    assert_eq!(
        retained.render(&config).unwrap().pixels(),
        original_pixels.pixels()
    );
}

#[test]
fn invalid_imports_and_active_jobs_are_rejected_without_mutation() {
    let source = source();
    let output = Document::create().unwrap();
    output.import_pages(&source, &[0], 0).unwrap();
    let revision = output.revision();
    assert!(matches!(
        output.import_pages(&source, &[999], 0),
        Err(Error::PageOutOfRange { .. })
    ));
    assert!(matches!(
        output.import_pages(&source, &[0], 2),
        Err(Error::InvalidArgument(_))
    ));
    assert!(matches!(
        output.import_pages(&output, &[0], 0),
        Err(Error::InvalidArgument(_))
    ));
    output.import_pages(&source, &[], 0).unwrap();
    assert_eq!(output.revision(), revision);
    let page = output.page(0).unwrap();
    let job = page.render_job(&RenderConfig::fit(16, 16)).unwrap();
    assert!(matches!(
        output.import_pages(&source, &[1], 0),
        Err(Error::Busy)
    ));
    drop(job);
    let source_job = source
        .page(0)
        .unwrap()
        .render_job(&RenderConfig::fit(16, 16))
        .unwrap();
    assert!(matches!(
        output.import_pages(&source, &[1], 0),
        Err(Error::Busy)
    ));
    drop(source_job);
    assert_eq!(output.page_count().unwrap(), 1);
    assert_eq!(output.revision(), revision);
    output.import_pages(&source, &[1], 0).unwrap();
    assert_eq!(page.index(), 1);
}

#[test]
fn partial_native_import_failure_keeps_handles_and_iterators_consistent() {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R null] /Count 2 >>",
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 20 30] >>",
    ];
    let mut bytes = String::from("%PDF-1.7\n");
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.push_str(&format!("{} 0 obj\n{object}\nendobj\n", index + 1));
    }
    let xref = bytes.len();
    bytes.push_str("xref\n0 4\n0000000000 65535 f \n");
    for offset in offsets {
        bytes.push_str(&format!("{offset:010} 00000 n \n"));
    }
    bytes.push_str(&format!(
        "trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"
    ));
    let malformed = Document::from_bytes(bytes.into_bytes()).unwrap();
    assert_eq!(malformed.page_count().unwrap(), 2);
    let output = source();
    let retained = output.page(0).unwrap();
    let id = retained.id();
    let config = RenderConfig::fit(60, 60);
    let pixels = retained.render(&config).unwrap();
    let before = output.page_count().unwrap();
    let mut pages = output.pages().unwrap();
    assert!(matches!(
        output.import_pages(&malformed, &[0, 1], 0),
        Err(Error::Import)
    ));
    // Native creates the failing destination page before looking up its source.
    assert_eq!(output.page_count().unwrap(), before + 2);
    assert_eq!(retained.index(), 2);
    assert_eq!(output.page_index(id).unwrap(), 2);
    retained.unload().unwrap();
    assert_eq!(
        output
            .page_by_id(id)
            .unwrap()
            .render(&config)
            .unwrap()
            .pixels(),
        pixels.pixels()
    );
    assert!(matches!(pages.next(), Some(Err(Error::Stale))));
}

#[test]
fn new_document_version_default_does_not_rewrite_loaded_zero_version() {
    let mut bytes = include_bytes!("../../../tests/render_corpus/pdfs/hello_world.pdf").to_vec();
    bytes[..8].copy_from_slice(b"%PDF-0.0");
    let document = Document::from_bytes(bytes).unwrap();
    assert_eq!(document.version().unwrap().major, 0);
    assert_eq!(document.version().unwrap().minor, 0);
    assert_eq!(Document::create().unwrap().version().unwrap().minor, 7);
}
