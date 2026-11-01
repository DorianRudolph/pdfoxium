// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use pdfoxium::{
    Bitmap, Color, Document, Error, MarkValue, Matrix, ObjectKind, Page, PathSpec, PixelFormat,
    Rect, RenderConfig, Size, StandardFont, TextSpec,
};

fn page(doc: &Document) -> Page {
    doc.insert_blank_page(
        doc.page_count().unwrap(),
        Size {
            width: 100.,
            height: 100.,
        },
    )
    .unwrap()
}
fn rectangle(left: f32) -> PathSpec {
    PathSpec {
        fill: Some(Color::rgb(255, 0, 0)),
        ..PathSpec::rect(Rect {
            left,
            bottom: 10.,
            right: left + 20.,
            top: 30.,
        })
    }
}
fn render(page: &Page) -> Vec<u8> {
    page.render(&RenderConfig::fit(100, 100))
        .unwrap()
        .into_pixels()
}
fn bitmap(color: [u8; 4]) -> Bitmap {
    let mut pixels = Bitmap::new(2, 2, PixelFormat::Rgba8).unwrap();
    for p in pixels.pixels_mut().chunks_exact_mut(4) {
        p.copy_from_slice(&color);
    }
    pixels
}
fn pixel(pixels: &[u8], x: usize, y: usize) -> &[u8] {
    &pixels[(y * 100 + x) * 4..(y * 100 + x) * 4 + 4]
}

#[test]
fn aliases_keep_identity_across_edits_reorder_and_parent_drop() {
    let doc = Document::create().unwrap();
    let page = page(&doc);
    let id = page.add_path(&rectangle(10.)).unwrap();
    let object = page.object_handle(id).unwrap();
    let alias = object.clone();
    let sibling_id = page.add_path(&rectangle(50.)).unwrap();
    let sibling = page.object_handle(sibling_id).unwrap();
    let duplicate = page.object_handle_at(0).unwrap().unwrap();
    assert!(page.object_handle_at(2).unwrap().is_none());
    alias.set_fill_color(Color::rgb(0, 255, 0)).unwrap();
    assert_eq!(duplicate.style().unwrap().fill, Some(Color::rgb(0, 255, 0)));
    object.reorder(1).unwrap();
    assert_eq!(page.object_handle_at(1).unwrap().unwrap().id().unwrap(), id);
    assert_eq!(sibling.id().unwrap(), sibling_id);
    assert_eq!(object.kind().unwrap(), ObjectKind::Path);
    // Existing ID edits and new handles access the same object, without revision expiry.
    page.transform_object(id, Matrix::translate(5., 0.))
        .unwrap();
    assert_eq!(alias.bounds().unwrap().left, 15.);
    assert_eq!(sibling.bounds().unwrap().left, 50.);
    drop(page);
    drop(doc);
    duplicate.transform(Matrix::translate(5., 0.)).unwrap();
    assert_eq!(object.bounds().unwrap().left, 20.);
    assert_eq!(object.info().unwrap().id, id);
}

#[test]
fn removed_transferred_unloaded_and_deleted_objects_stay_stale() {
    for action in 0..5 {
        let doc = Document::create().unwrap();
        let source = page(&doc);
        let destination = page(&doc);
        let id = source.add_path(&rectangle(10.)).unwrap();
        let object = source.object_handle(id).unwrap();
        let alias = object.clone();
        match action {
            0 => object.remove().unwrap(),
            1 => {
                let detached = object.take().unwrap();
                let replacement = source.insert_object(detached).unwrap();
                assert_ne!(id, replacement);
            }
            2 => source.unload().unwrap(),
            3 => doc.delete_page(source.id()).unwrap(),
            4 => {
                let moved = object.move_to(&destination).unwrap();
                assert_eq!(moved.bounds().unwrap().left, 10.);
                moved.set_fill_color(Color::BLACK).unwrap();
                assert_eq!(source.object_count().unwrap(), 0);
                assert_eq!(destination.object_count().unwrap(), 1);
            }
            _ => unreachable!(),
        }
        assert!(matches!(object.id(), Err(Error::Stale)), "action {action}");
        assert!(matches!(alias.path(), Err(Error::Stale)), "action {action}");
        assert!(matches!(
            object.set_fill_color(Color::BLACK),
            Err(Error::Stale)
        ));
        if action == 2 {
            let fresh = source.object_handle_at(0).unwrap().unwrap();
            assert_ne!(fresh.id().unwrap(), id);
            assert_eq!(fresh.bounds().unwrap().left, 10.);
        }
    }
}

#[test]
fn render_lease_and_foreign_transfer_fail_without_detaching_object() {
    let doc = Document::create().unwrap();
    let page = page(&doc);
    let id = page.add_path(&rectangle(10.)).unwrap();
    let object = page.object_handle(id).unwrap();
    let before = render(&page);
    let job = page.render_job(&RenderConfig::fit(100, 100)).unwrap();
    assert!(matches!(object.path(), Err(Error::Busy)));
    assert!(matches!(
        object.transform(Matrix::translate(10., 0.)),
        Err(Error::Busy)
    ));
    assert!(matches!(object.take(), Err(Error::Busy)));
    drop(job);
    assert_eq!(render(&page), before);
    let other_doc = Document::create().unwrap();
    let other = other_doc
        .insert_blank_page(
            0,
            Size {
                width: 100.,
                height: 100.,
            },
        )
        .unwrap();
    assert!(matches!(other.object_handle(id), Err(Error::Stale)));
    assert!(matches!(
        object.move_to(&other),
        Err(Error::InvalidArgument(_))
    ));
    assert_eq!(object.id().unwrap(), id);
    assert_eq!(other.object_count().unwrap(), 0);
    assert!(matches!(
        object.set_stroke_width(f32::NAN),
        Err(Error::InvalidArgument(_))
    ));
    assert_eq!(render(&page), before);
    object.set_fill_color(Color::rgb(0, 255, 0)).unwrap();
    assert_ne!(render(&page), before);
}

#[test]
fn path_and_image_handle_edits_render_and_roundtrip() {
    let doc = Document::create().unwrap();
    let page = page(&doc);
    let path = page
        .object_handle(page.add_path(&rectangle(10.)).unwrap())
        .unwrap();
    let image = page
        .object_handle(
            page.add_image(
                bitmap([255, 0, 0, 255]),
                Matrix::scale(20., 20.).then(Matrix::translate(60., 10.)),
            )
            .unwrap(),
        )
        .unwrap();
    let before = render(&page);
    assert_eq!(path.path().unwrap().segments.len(), 4);
    path.set_fill_color(Color::rgb(0, 255, 0)).unwrap();
    path.transform(Matrix::translate(10., 0.)).unwrap();
    image.set_image_bitmap(bitmap([0, 0, 255, 255])).unwrap();
    assert_eq!(
        (
            image.image_info().unwrap().width,
            image.image_info().unwrap().height
        ),
        (2, 2)
    );
    assert_eq!(
        &image.image_bitmap().unwrap().pixels()[..4],
        &[255, 0, 0, 255]
    );
    let expected = render(&page);
    assert_ne!(expected, before);
    assert_eq!(pixel(&expected, 15, 80), &[255; 4]);
    assert_eq!(pixel(&expected, 25, 80), &[0, 255, 0, 255]);
    assert_eq!(pixel(&expected, 65, 80), &[255, 0, 0, 255]);
    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    assert_eq!(
        render(
            &Document::from_bytes(saved.clone())
                .unwrap()
                .page(0)
                .unwrap()
        ),
        expected
    );
    if let Some(dir) = std::env::var_os("PDFOXIUM_EDIT_ARTIFACT_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(std::path::Path::new(&dir).join("page-handles.pdf"), saved).unwrap();
    }
    // Saving preserves live handle identity; unloading deliberately expires it.
    assert_eq!(path.bounds().unwrap().left, 20.);
    page.unload().unwrap();
    assert!(matches!(path.bounds(), Err(Error::Stale)));
    assert_eq!(render(&page), expected);
}

#[test]
fn text_and_mark_edits_share_the_page_serialization_path() {
    let doc = Document::create().unwrap();
    let page = page(&doc);
    let font = doc.standard_font(StandardFont::Helvetica).unwrap();
    let text = page
        .object_handle(
            page.add_text(&TextSpec {
                text: "Before",
                font: &font,
                size: 12.,
                matrix: Matrix::translate(10., 50.),
                color: Color::BLACK,
            })
            .unwrap(),
        )
        .unwrap();
    let before = render(&page);
    text.set_text("After").unwrap();
    text.set_text_font_size(18.).unwrap();
    assert_eq!(text.text().unwrap(), "After");
    assert_eq!(text.font().unwrap().info().unwrap().base_name, b"Helvetica");
    let mark = text.add_mark(b"Span").unwrap();
    text.set_mark_parameter(&mark, b"MCID", MarkValue::Integer(7))
        .unwrap();
    assert_eq!(text.marked_content_id().unwrap(), 7);
    let expected = render(&page);
    assert_ne!(expected, before);
    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    let reopened = Document::from_bytes(saved).unwrap();
    let page = reopened.page(0).unwrap();
    assert_eq!(render(&page), expected);
    let reopened_text = page.object_handle_at(0).unwrap().unwrap();
    assert_eq!(reopened_text.text().unwrap(), "After");
    assert_eq!(reopened_text.marked_content_id().unwrap(), 7);
}

#[test]
fn nested_handles_isolate_form_instances_and_expire_with_ancestor() {
    let source = Document::create().unwrap();
    page(&source).add_path(&rectangle(10.)).unwrap();
    let doc = Document::create().unwrap();
    let first = page(&doc);
    let second = page(&doc);
    let xobject = doc.import_page_xobject(&source, 0).unwrap();
    let form = first
        .object_handle(first.insert_object(xobject.object().unwrap()).unwrap())
        .unwrap();
    second.insert_object(xobject.object().unwrap()).unwrap();
    assert_eq!(form.child_count().unwrap(), 1);
    assert!(form.child_at(1).unwrap().is_none());
    let child = form.child_at(0).unwrap().unwrap();
    let alias = form.children().unwrap().remove(0);
    assert_eq!(child.id().unwrap(), alias.id().unwrap());
    let untouched = render(&second);
    child.set_fill_color(Color::rgb(0, 255, 0)).unwrap();
    child.transform(Matrix::translate(10., 0.)).unwrap();
    assert_eq!(alias.bounds().unwrap().left, 20.);
    assert_eq!(render(&second), untouched);
    let edited = render(&first);
    assert_ne!(edited, untouched);
    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    let reopened = Document::from_bytes(saved).unwrap();
    assert_eq!(render(&reopened.page(0).unwrap()), edited);
    assert_eq!(render(&reopened.page(1).unwrap()), untouched);
    form.remove().unwrap();
    assert!(matches!(child.path(), Err(Error::Stale)));
    assert!(matches!(
        alias.set_fill_color(Color::BLACK),
        Err(Error::Stale)
    ));
}
