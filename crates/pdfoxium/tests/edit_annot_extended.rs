// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use pdfoxium::{
    AnnotationKind, AnnotationValueType, AppearanceMode, Color, Document, Error, Matrix, PathSpec,
    Point, Rect, RenderConfig, Size,
};
fn page() -> (Document, pdfoxium::Page) {
    let d = Document::create().unwrap();
    let p = d
        .insert_blank_page(
            0,
            Size {
                width: 100.,
                height: 100.,
            },
        )
        .unwrap();
    (d, p)
}
fn rect() -> Rect {
    Rect {
        left: 0.,
        bottom: 0.,
        right: 100.,
        top: 100.,
    }
}
fn render(p: &pdfoxium::Page) -> Vec<u8> {
    p.render(&RenderConfig::fit(100, 100))
        .unwrap()
        .into_pixels()
}
#[test]
fn custom_appearance_geometry_and_object_edits_survive_save() {
    let (doc, page) = page();
    assert!(AnnotationKind::Stamp.supports_creation());
    assert!(!AnnotationKind::Line.supports_creation());
    assert!(AnnotationKind::Stamp.supports_objects());
    let stamp = page
        .create_annotation(AnnotationKind::Stamp, rect())
        .unwrap();
    let blank = render(&page);
    page.set_annotation_appearance(
        stamp,
        AppearanceMode::Normal,
        Some("1 0 0 rg 10 10 30 30 re f"),
    )
    .unwrap();
    let red = render(&page);
    assert_ne!(red, blank);
    assert_eq!(
        &red[(70 * 100 + 20) * 4..(70 * 100 + 20) * 4 + 4],
        &[0, 0, 255, 255]
    );
    assert_eq!(
        page.annotation_appearance(stamp, AppearanceMode::Normal)
            .unwrap()
            .as_deref(),
        Some("1 0 0 rg 10 10 30 30 re f")
    );
    let objects = page.annotation_objects(stamp).unwrap();
    assert_eq!(objects.len(), 1);
    assert!(!objects[0].path.as_ref().unwrap().commands.is_empty());
    let handle = page.annotation_object(objects[0].id).unwrap();
    handle.transform(Matrix::translate(30., 0.)).unwrap();
    assert!(matches!(
        page.remove_annotation_object(objects[0].id),
        Err(Error::Stale)
    ));
    let moved = render(&page);
    assert_ne!(red, moved);
    assert_eq!(
        &moved[(70 * 100 + 20) * 4..(70 * 100 + 20) * 4 + 4],
        &[255; 4]
    );
    let id = page.annotation_objects(stamp).unwrap()[0].id;
    let object = page.take_annotation_object(id).unwrap();
    assert_eq!(render(&page), blank);
    let id = page.insert_annotation_object(stamp, object).unwrap();
    page.annotation_object(id)
        .unwrap()
        .set_fill_color(Color::rgb(0, 0, 255))
        .unwrap();
    let blue = render(&page);
    assert_ne!(blue, moved);
    page.unload().unwrap();
    assert_eq!(render(&page), blue);
    let mut saved = vec![];
    doc.save_to(&mut saved).unwrap();
    if let Some(path) = std::env::var_os("PDFOXIUM_EDIT_ARTIFACT_DIR") {
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            std::path::Path::new(&path).join("annotation-objects.pdf"),
            &saved,
        )
        .unwrap();
    }
    assert_eq!(
        render(&Document::from_bytes(saved).unwrap().page(0).unwrap()),
        blue
    );
    let object = page.annotation_objects(stamp).unwrap()[0].id;
    page.remove_annotation_object(object).unwrap();
    assert_eq!(render(&page), blank);
}
#[test]
fn dictionary_link_quads_ink_and_attachments() {
    let (doc, page) = page();
    let link = page
        .create_annotation(AnnotationKind::Link, rect())
        .unwrap();
    page.set_annotation_uri(link, b"https://example.com/")
        .unwrap();
    assert_eq!(page.links().unwrap().len(), 1);
    page.set_annotation_string(link, b"Contents", "Unicode 🦀")
        .unwrap();
    assert_eq!(
        page.annotation_string(link, b"Contents").unwrap(),
        "Unicode 🦀"
    );
    assert_eq!(
        page.annotation_value_type(link, b"Contents").unwrap(),
        Some(AnnotationValueType::String)
    );
    assert!(page.annotation_has_key(link, b"A").unwrap());
    page.set_annotation_flags(link, 4).unwrap();
    assert_eq!(page.annotation_number(link, b"F").unwrap(), Some(4.));
    let quad = [
        Point { x: 1., y: 9. },
        Point { x: 8., y: 9. },
        Point { x: 1., y: 2. },
        Point { x: 8., y: 2. },
    ];
    page.set_annotation_quad(link, None, quad).unwrap();
    assert_eq!(page.annotation(link).unwrap().quads, vec![quad]);
    page.set_annotation_border(link, [1., 2., 3.]).unwrap();
    assert_eq!(page.annotation(link).unwrap().border, Some([1., 2., 3.]));
    page.set_annotation_dictionary_color(link, false, Color::rgb(2, 4, 6))
        .unwrap();
    assert_eq!(
        page.annotation_color(link, false).unwrap(),
        Some(Color::rgb(2, 4, 6))
    );
    let ink = page.create_annotation(AnnotationKind::Ink, rect()).unwrap();
    assert_eq!(page.add_annotation_ink_stroke(ink, &quad[..2]).unwrap(), 0);
    assert_eq!(page.annotation(ink).unwrap().ink[0], quad[..2]);
    page.clear_annotation_ink(ink).unwrap();
    assert!(page.annotation(ink).unwrap().ink.is_empty());
    let file = page
        .create_annotation(AnnotationKind::FileAttachment, rect())
        .unwrap();
    let attachment = page.add_annotation_file_attachment(file, "a.bin").unwrap();
    attachment.set_data(b"payload", None).unwrap();
    assert_eq!(
        page.annotation_file_attachment(file)
            .unwrap()
            .unwrap()
            .read()
            .unwrap(),
        b"payload"
    );
    let mut bytes = vec![];
    doc.save_to(&mut bytes).unwrap();
    let reopened = Document::from_bytes(bytes).unwrap();
    let p = reopened.page(0).unwrap();
    let ids = p.annotation_ids().unwrap();
    assert_eq!(
        p.annotation_file_attachment(ids[2])
            .unwrap()
            .unwrap()
            .read()
            .unwrap(),
        b"payload"
    );
}
#[test]
fn page_object_can_move_into_stamp_and_back() {
    let (_, page) = page();
    let stamp = page
        .create_annotation(AnnotationKind::Stamp, rect())
        .unwrap();
    let id = page
        .add_path(&PathSpec::rect(Rect {
            left: 10.,
            bottom: 10.,
            right: 20.,
            top: 20.,
        }))
        .unwrap();
    let before = render(&page);
    let detached = page.take_object(id).unwrap();
    let id = page.insert_annotation_object(stamp, detached).unwrap();
    assert_eq!(render(&page), before);
    let detached = page.take_annotation_object(id).unwrap();
    page.insert_object(detached).unwrap();
    assert_eq!(render(&page), before);
}
#[test]
fn static_widget_fields_and_flags_without_form_environment() {
    let doc = Document::from_bytes(
        include_bytes!("../../../upstream/pdfium/testing/resources/combobox_form.pdf").as_slice(),
    )
    .unwrap();
    let page = doc.page(0).unwrap();
    let ids = page.annotation_ids().unwrap();
    let field = page.annotation_field(ids[1]).unwrap().unwrap();
    assert_eq!(field.options[0].label, "Apple");
    assert_eq!(field.options[25].label, "Zucchini");
    assert_eq!(field.control_count, 1);
    page.set_annotation_field_flags(ids[1], field.flags | 1)
        .unwrap();
    assert_eq!(
        page.annotation_field(ids[1]).unwrap().unwrap().flags,
        field.flags | 1
    );
    let bounds = page.annotation(ids[1]).unwrap().rect;
    assert_eq!(
        page.annotation_field_at(Point {
            x: (bounds.left + bounds.right) / 2.,
            y: (bounds.bottom + bounds.top) / 2.
        })
        .unwrap(),
        Some(ids[1])
    );
    let mut bytes = vec![];
    doc.save_to(&mut bytes).unwrap();
    let reopened = Document::from_bytes(bytes).unwrap();
    let p = reopened.page(0).unwrap();
    assert_eq!(
        p.annotation_field(p.annotation_ids().unwrap()[1])
            .unwrap()
            .unwrap()
            .flags,
        field.flags | 1
    );
}
#[test]
fn appearance_handles_preserve_modes_and_paint_order() {
    let (_, page) = page();
    let stamp = page
        .create_annotation(AnnotationKind::Stamp, rect())
        .unwrap();
    page.set_annotation_appearance(
        stamp,
        AppearanceMode::Normal,
        Some("1 0 0 rg 10 10 20 20 re f 0 0 1 rg 20 20 20 20 re f"),
    )
    .unwrap();
    page.set_annotation_appearance(stamp, AppearanceMode::Rollover, Some("0 g"))
        .unwrap();
    page.set_annotation_appearance(stamp, AppearanceMode::Down, Some("1 g"))
        .unwrap();
    page.set_annotation_appearance(stamp, AppearanceMode::Rollover, None)
        .unwrap();
    assert!(page
        .annotation_appearance(stamp, AppearanceMode::Rollover)
        .unwrap()
        .is_none());
    assert!(page
        .annotation_appearance(stamp, AppearanceMode::Down)
        .unwrap()
        .is_some());
    let before = render(&page);
    let id = page.annotation_objects(stamp).unwrap()[0].id;
    let object = page.annotation_object(id).unwrap();
    assert!(matches!(
        object.set_stroke_width(f32::NAN),
        Err(Error::InvalidArgument(_))
    ));
    assert_eq!(render(&page), before);
    // Reacquire after a failed edit, which may invalidate the parsed context.
    let id = page.annotation_objects(stamp).unwrap()[0].id;
    let object = page.annotation_object(id).unwrap();
    object.set_fill_color(Color::rgb(0, 255, 0)).unwrap();
    let changed = render(&page);
    assert_ne!(changed, before);
    assert_eq!(
        &changed[(75 * 100 + 25) * 4..(75 * 100 + 25) * 4 + 4],
        &[255, 0, 0, 255]
    );
    page.set_annotation_appearance(stamp, AppearanceMode::Normal, None)
        .unwrap();
    assert!(page
        .annotation_appearance(stamp, AppearanceMode::Down)
        .unwrap()
        .is_none());
}
#[test]
fn annotation_handles_support_text_and_revalidate_revision() {
    let (doc, page) = page();
    let stamp = page
        .create_annotation(AnnotationKind::Stamp, rect())
        .unwrap();
    let font = doc
        .standard_font(pdfoxium::StandardFont::Helvetica)
        .unwrap();
    let text = page
        .add_text(&pdfoxium::TextSpec {
            text: "Before",
            font: &font,
            size: 12.,
            matrix: Matrix::translate(10., 50.),
            color: Color::BLACK,
        })
        .unwrap();
    let detached = page.take_object(text).unwrap();
    let id = page.insert_annotation_object(stamp, detached).unwrap();
    let before = render(&page);
    let object = page.annotation_object(id).unwrap();
    object.set_text("After").unwrap();
    object.transform(Matrix::translate(5., 0.)).unwrap();
    assert_eq!(object.text().unwrap(), "After");
    let after = render(&page);
    assert_ne!(before, after);
    page.set_annotation_string(stamp, b"Contents", "External edit")
        .unwrap();
    assert!(matches!(object.set_text("Discard"), Err(Error::Stale)));
    assert!(matches!(object.text(), Err(Error::Stale)));
    assert_eq!(render(&page), after);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    let reopened = Document::from_bytes(bytes).unwrap();
    assert_eq!(render(&reopened.page(0).unwrap()), after);
}

#[test]
fn appearance_inspection_does_not_edit_the_document() {
    let doc = pdfoxium::Document::create().unwrap();
    let page = doc
        .insert_blank_page(
            0,
            pdfoxium::Size {
                width: 100.,
                height: 100.,
            },
        )
        .unwrap();
    let id = page
        .create_annotation(
            pdfoxium::AnnotationKind::Stamp,
            pdfoxium::Rect {
                left: 0.,
                bottom: 0.,
                right: 100.,
                top: 100.,
            },
        )
        .unwrap();
    page.set_annotation_appearance(
        id,
        pdfoxium::AppearanceMode::Normal,
        Some("1 0 0 rg 10 10 20 20 re f"),
    )
    .unwrap();
    let object = page.annotation_objects(id).unwrap()[0].id;
    let revision = doc.revision();
    let handle = page.annotation_object(object).unwrap();
    let path = handle.path().unwrap();
    assert!(!path.segments.is_empty());
    assert_eq!(doc.revision(), revision);
    assert_eq!(page.annotation_objects(id).unwrap()[0].id, object);
}

#[test]
fn sibling_and_cloned_handles_share_updates_and_keep_owners_alive() {
    let (doc, page) = page();
    let stamp = page
        .create_annotation(AnnotationKind::Stamp, rect())
        .unwrap();
    page.set_annotation_appearance(
        stamp,
        AppearanceMode::Normal,
        Some("1 0 0 rg 10 10 20 20 re f 0 0 1 rg 50 10 20 20 re f"),
    )
    .unwrap();
    let ids = page.annotation_objects(stamp).unwrap();
    let first = page.annotation_object(ids[0].id).unwrap();
    let duplicate = page.annotation_object(ids[0].id).unwrap();
    let alias = first.clone();
    let second = page.annotation_object(ids[1].id).unwrap();
    first.set_fill_color(Color::rgb(0, 255, 0)).unwrap();
    second.set_fill_color(Color::BLACK).unwrap();
    alias.transform(Matrix::translate(0., 20.)).unwrap();
    assert_eq!(first.bounds().unwrap(), duplicate.bounds().unwrap());
    assert_eq!(first.bounds().unwrap(), alias.bounds().unwrap());
    assert_eq!(second.bounds().unwrap().bottom, 10.);
    let pixels = render(&page);
    assert_eq!(
        &pixels[(60 * 100 + 20) * 4..(60 * 100 + 20) * 4 + 4],
        &[0, 255, 0, 255]
    );
    assert_eq!(
        &pixels[(80 * 100 + 60) * 4..(80 * 100 + 60) * 4 + 4],
        &[0, 0, 0, 255]
    );
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    assert_eq!(
        render(&Document::from_bytes(bytes).unwrap().page(0).unwrap()),
        pixels
    );
    drop(page);
    drop(doc);
    duplicate.transform(Matrix::translate(5., 0.)).unwrap();
    assert_eq!(first.bounds().unwrap().left, 15.);
    assert_eq!(second.bounds().unwrap().left, 50.);
}

#[test]
fn annotation_handles_reject_removed_replaced_or_unloaded_content() {
    for mutation in 0..6 {
        let (doc, page) = page();
        let stamp = page
            .create_annotation(AnnotationKind::Stamp, rect())
            .unwrap();
        page.set_annotation_appearance(
            stamp,
            AppearanceMode::Normal,
            Some("1 0 0 rg 10 10 20 20 re f 0 0 1 rg 50 10 20 20 re f"),
        )
        .unwrap();
        let ids = page.annotation_objects(stamp).unwrap();
        let object = page.annotation_object(ids[0].id).unwrap();
        match mutation {
            0 => page
                .set_annotation_appearance(
                    stamp,
                    AppearanceMode::Normal,
                    Some("0 g 10 10 20 20 re f"),
                )
                .unwrap(),
            1 => page.remove_annotation(stamp).unwrap(),
            2 => page.remove_annotation_object(ids[0].id).unwrap(),
            3 => page.unload().unwrap(),
            4 => doc.delete_page(page.id()).unwrap(),
            5 => {
                page.add_path(&PathSpec::rect(rect())).unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            matches!(object.path(), Err(Error::Stale)),
            "mutation {mutation}"
        );
        assert!(
            matches!(object.set_fill_color(Color::BLACK), Err(Error::Stale)),
            "mutation {mutation}"
        );
        if mutation == 2 {
            // Removing index zero must not retarget the old handle to the next path.
            let id = page.annotation_objects(stamp).unwrap()[0].id;
            assert_eq!(
                page.annotation_object(id).unwrap().bounds().unwrap().left,
                50.
            );
        }
        if mutation == 3 {
            let id = page.annotation_objects(stamp).unwrap()[0].id;
            assert_eq!(
                page.annotation_object(id).unwrap().bounds().unwrap().left,
                10.
            );
        }
    }
}

#[test]
fn annotation_handle_respects_render_lease_and_failed_edits_do_not_leak() {
    let (doc, page) = page();
    let stamp = page
        .create_annotation(AnnotationKind::Stamp, rect())
        .unwrap();
    page.set_annotation_appearance(
        stamp,
        AppearanceMode::Normal,
        Some("1 0 0 rg 10 10 20 20 re f"),
    )
    .unwrap();
    let id = page.annotation_objects(stamp).unwrap()[0].id;
    let object = page.annotation_object(id).unwrap();
    let before = render(&page);
    let revision = doc.revision();
    let job = page.render_job(&RenderConfig::fit(100, 100)).unwrap();
    assert!(matches!(object.path(), Err(Error::Busy)));
    assert!(matches!(
        object.set_fill_color(Color::BLACK),
        Err(Error::Busy)
    ));
    drop(job);
    assert_eq!(doc.revision(), revision);
    assert_eq!(render(&page), before);
    assert!(matches!(
        object.set_text("not text"),
        Err(Error::Unsupported(_))
    ));
    assert_eq!(render(&page), before);
    let id = page.annotation_objects(stamp).unwrap()[0].id;
    let object = page.annotation_object(id).unwrap();
    object.set_fill_color(Color::rgb(0, 255, 0)).unwrap();
    let expected = render(&page);
    assert_ne!(expected, before);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    assert_eq!(
        render(&Document::from_bytes(bytes).unwrap().page(0).unwrap()),
        expected
    );
}

#[test]
fn failed_marked_object_edit_discards_mutated_private_state() {
    let (doc, page) = page();
    let stamp = page
        .create_annotation(AnnotationKind::Stamp, rect())
        .unwrap();
    let appearance = "/Tag BMC /Other BMC 1 0 0 rg 10 10 20 20 re f EMC EMC";
    page.set_annotation_appearance(stamp, AppearanceMode::Normal, Some(appearance))
        .unwrap();
    let id = page.annotation_objects(stamp).unwrap()[0].id;
    let object = page.annotation_object(id).unwrap();
    let alias = object.clone();
    assert_eq!(object.marks().unwrap().len(), 2);
    let geometry = object.path_segments().unwrap();
    let bounds = object.bounds().unwrap();
    let before = render(&page);
    let revision = doc.revision();
    // The detached transform succeeds before commit rejects the retained mark.
    // Its translated geometry must never enter AP or a later successful edit.
    assert!(matches!(
        object.transform(Matrix::translate(40., 0.)),
        Err(Error::Unsupported(_))
    ));
    assert_eq!(doc.revision(), revision);
    assert!(matches!(object.bounds(), Err(Error::Stale)));
    assert!(matches!(alias.path(), Err(Error::Stale)));
    assert_eq!(render(&page), before);
    assert_eq!(
        page.annotation_appearance(stamp, AppearanceMode::Normal)
            .unwrap()
            .as_deref(),
        Some(appearance)
    );
    let id = page.annotation_objects(stamp).unwrap()[0].id;
    let replacement = page.annotation_object(id).unwrap();
    assert_eq!(replacement.path_segments().unwrap(), geometry);
    assert_eq!(replacement.bounds().unwrap(), bounds);
    replacement.clear_marks().unwrap();
    assert!(replacement.marks().unwrap().is_empty());
    replacement.set_fill_color(Color::rgb(0, 255, 0)).unwrap();
    assert_eq!(replacement.bounds().unwrap(), bounds);
    let expected = render(&page);
    assert_eq!(
        &expected[(80 * 100 + 20) * 4..(80 * 100 + 20) * 4 + 4],
        &[0, 255, 0, 255]
    );
    assert_eq!(
        &expected[(80 * 100 + 60) * 4..(80 * 100 + 60) * 4 + 4],
        &[255; 4]
    );
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    assert_eq!(
        render(&Document::from_bytes(bytes).unwrap().page(0).unwrap()),
        expected
    );
}
