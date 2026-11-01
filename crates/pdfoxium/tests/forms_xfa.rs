// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
#![cfg(feature = "xfa")]
use pdfoxium::*;
fn demo() -> Document {
    Document::open(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../crates/pdfoxium-viewer/assets/demos/xfa-demo.pdf"
    ))
    .unwrap()
}
#[test]
fn full_xfa_fields_input_render_and_save_reopen() {
    let doc = demo();
    let form = doc
        .forms(FormOptions {
            javascript: true,
            ..Default::default()
        })
        .unwrap();
    form.initialize().unwrap();
    assert!(form.is_full_xfa());
    let sizes = form.page_sizes().unwrap();
    assert_eq!(sizes.len(), 1);
    assert!(sizes[0].width > 500.);
    assert_eq!(doc.page_count().unwrap(), sizes.len());
    assert_eq!(doc.page_layout(0).unwrap().size, sizes[0]);
    assert!(matches!(doc.page(0), Err(Error::Unsupported(_))));
    form.page_action(0, true).unwrap();
    form.page_action(0, false).unwrap();
    let fields = form.fields(0).unwrap();
    let name = fields.iter().find(|f| f.name == "customer").unwrap();
    let point = Point {
        x: (name.rect.left + name.rect.right) / 2.,
        y: (name.rect.bottom + name.rect.top) / 2.,
    };
    for input in [
        FormInput::MouseMove(point, Default::default()),
        FormInput::MouseDown(point, Default::default()),
        FormInput::MouseUp(point, Default::default()),
        FormInput::SelectAll,
        FormInput::Text("Grace Hopper".into(), Default::default()),
        FormInput::Blur,
    ] {
        form.input(0, input).unwrap();
    }
    assert_eq!(
        form.fields(0)
            .unwrap()
            .into_iter()
            .find(|f| f.name == "customer")
            .unwrap()
            .value,
        "Grace Hopper"
    );
    let bitmap = form.render(0, &RenderConfig::default()).unwrap();
    assert!(bitmap
        .pixels()
        .chunks_exact(4)
        .any(|p| p[0] < 240 && p[1] < 240 && p[2] < 240));
    struct FailedWriter;
    impl std::io::Write for FailedWriter {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("test writer failure"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    assert!(doc.save_to(&mut FailedWriter).is_err());
    assert!(form.state().unwrap().dirty);
    let bytes = doc.snapshot().unwrap();
    assert!(doc.is_dirty());
    let reopened = Document::from_bytes(bytes).unwrap();
    let form = reopened
        .forms(FormOptions {
            javascript: true,
            ..Default::default()
        })
        .unwrap();
    form.initialize().unwrap();
    assert_eq!(
        form.fields(0)
            .unwrap()
            .into_iter()
            .find(|f| f.name == "customer")
            .unwrap()
            .value,
        "Grace Hopper"
    );
}

#[test]
fn xfa_document_worker_renders_real_layout_and_rejects_invalid_pages() {
    use pdfoxium::thread::*;
    let worker = DocumentThread::spawn(
        Source::Bytes(
            std::fs::read(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../crates/pdfoxium-viewer/assets/demos/xfa-demo.pdf"
            ))
            .unwrap()
            .into(),
        ),
        Config {
            // Browser hosts have no font directories. The document embeds
            // Helvetica/Arial; theme startup must resolve it after PDF loading.
            font_paths: Some(Vec::new()),
            ..Config::default()
        },
        DocOptions {
            forms: FormOptions {
                javascript: true,
                ..Default::default()
            },
            ..Default::default()
        },
    )
    .unwrap();
    assert!(worker.is_xfa());
    assert_eq!(worker.page_count(), 1);
    let bitmap = worker
        .render(0, RenderConfig::fit(306, 396), Priority::Visible)
        .wait()
        .unwrap()
        .bitmap;
    assert_eq!((bitmap.width(), bitmap.height()), (306, 396));
    assert!(
        bitmap
            .pixels()
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[0] < 128)
            .count()
            > 100
    );
    assert!(worker
        .render(1, RenderConfig::default(), Priority::Visible)
        .wait()
        .is_err());
    assert_eq!(
        worker
            .call(|_, form| Ok(form.fields(0)?.len()))
            .wait()
            .unwrap(),
        8
    );
}

#[test]
fn failed_xfa_initialization_does_not_become_success_on_retry() {
    let mut bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../crates/pdfoxium-viewer/assets/demos/xfa-demo.pdf"
    ))
    .unwrap();
    let start = bytes.windows(9).position(|w| w == b"<template").unwrap();
    bytes[start..start + 9].copy_from_slice(b"<brokenxx");
    let document = Document::from_bytes(bytes).unwrap();
    let form = document
        .forms(FormOptions {
            javascript: true,
            ..Default::default()
        })
        .unwrap();
    assert!(form.initialize().is_err());
    assert!(form.initialize().is_err());
}

#[test]
fn dynamic_xfa_page_count_and_cached_page_geometry_follow_row_removal() {
    let doc = Document::open(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../crates/pdfoxium-viewer/assets/demos/dynamic-xfa-demo.pdf"
    ))
    .unwrap();
    let form = doc
        .forms(FormOptions {
            javascript: true,
            ..Default::default()
        })
        .unwrap();
    form.initialize().unwrap();
    let click = |x| {
        for input in [
            FormInput::MouseDown(Point { x, y: 125. }, Default::default()),
            FormInput::MouseUp(Point { x, y: 125. }, Default::default()),
        ] {
            form.input(0, input).unwrap();
        }
    };
    assert_eq!(doc.page_count().unwrap(), 1);
    click(111.);
    click(111.);
    assert_eq!(doc.page_count().unwrap(), 2);
    assert_eq!(form.page_sizes().unwrap().len(), 2);
    assert!(!form.fields(1).unwrap().is_empty());
    form.render(1, &RenderConfig::fit(306, 396)).unwrap();
    click(296.);
    click(296.);
    assert_eq!(doc.page_count().unwrap(), 1);
    assert_eq!(form.page_sizes().unwrap().len(), 1);
    assert!(form.fields(1).is_err());
    assert!(form.page_matrix(1, &RenderConfig::default()).is_err());
    assert!(form.render(1, &RenderConfig::default()).is_err());
}
