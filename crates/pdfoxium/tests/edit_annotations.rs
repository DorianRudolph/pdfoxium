// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use pdfoxium::{AnnotationSpec, Color, Document, Error, Point, Rect, RenderConfig};
#[path = "support/forms.rs"]
#[allow(dead_code)]
mod form_fixture;

fn document() -> Document {
    document_with_form("")
}
fn document_with_form(form: &str) -> Document {
    let objects = [
        format!("<< /Type /Catalog /Pages 2 0 R {form} >>"),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << >> >>".into(),
    ];
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = vec![];
    for (i, o) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend(format!("{} 0 obj\n{o}\nendobj\n", i + 1).bytes());
    }
    let xref = bytes.len();
    bytes.extend(b"xref\n0 4\n0000000000 65535 f \n");
    for offset in offsets {
        bytes.extend(format!("{offset:010} 00000 n \n").bytes());
    }
    bytes.extend(format!("trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").bytes());
    Document::from_bytes(bytes).unwrap()
}
fn rect(x: f32, y: f32) -> Rect {
    Rect {
        left: x,
        bottom: y,
        right: x + 40.0,
        top: y + 20.0,
    }
}

#[test]
fn highlighting_regenerates_appearance_and_survives_unload_save() {
    let doc = document();
    let page = doc.page(0).unwrap();
    let config = RenderConfig::default();
    let blank = page.render(&config).unwrap().into_pixels();
    let id = page
        .add_annotation(AnnotationSpec::highlight(vec![
            rect(10.0, 10.0),
            rect(10.0, 40.0),
        ]))
        .unwrap();
    let yellow = page.render(&config).unwrap().into_pixels();
    assert_ne!(yellow, blank);
    // BGRA: inside the first rectangle must be solid yellow.
    let at = (180 * 200 + 20) * 4;
    assert_eq!(&yellow[at..at + 4], &[0, 255, 255, 255]);
    page.set_annotation_color(id, Color::rgb(255, 0, 0))
        .unwrap();
    let red = page.render(&config).unwrap().into_pixels();
    assert_eq!(&red[at..at + 4], &[0, 0, 255, 255]);
    page.set_annotation_rects(id, &[rect(100.0, 100.0)])
        .unwrap();
    let moved = page.render(&config).unwrap().into_pixels();
    assert_eq!(&moved[at..at + 4], &[255; 4]);
    assert_ne!(moved, blank);
    page.unload().unwrap();
    assert_eq!(page.annotation_ids().unwrap(), vec![id]);
    page.set_annotation_contents(id, "selected 😀").unwrap();
    let mut saved = vec![];
    doc.save_to(&mut saved).unwrap();
    let reopened = Document::from_bytes(saved).unwrap();
    assert_eq!(
        reopened.page(0).unwrap().render(&config).unwrap().pixels(),
        moved
    );
    assert_eq!(
        reopened.page(0).unwrap().annotations().unwrap()[0]
            .contents
            .as_deref(),
        Some("selected 😀")
    );
    page.remove_annotation(id).unwrap();
    assert!(matches!(
        page.set_annotation_color(id, Color::BLACK),
        Err(Error::Stale)
    ));
    assert_eq!(page.render(&config).unwrap().pixels(), blank);
    let next = page
        .add_annotation(AnnotationSpec::highlight(vec![rect(10.0, 10.0)]))
        .unwrap();
    assert_ne!(id, next);
    let last = page
        .add_annotation(AnnotationSpec::square(rect(70.0, 10.0)))
        .unwrap();
    page.remove_annotation(next).unwrap();
    assert_eq!(page.annotation_ids().unwrap(), vec![last]);
    page.set_annotation_contents(last, "identity survives index change")
        .unwrap();
    assert_eq!(
        page.annotations().unwrap()[0].contents.as_deref(),
        Some("identity survives index change")
    );
}

#[test]
fn common_appearances_are_visible_and_save_reopen() {
    let doc = document();
    let page = doc.page(0).unwrap();
    let config = RenderConfig::default();
    let mut previous = page.render(&config).unwrap().into_pixels();
    for spec in [
        AnnotationSpec::square(rect(10.0, 10.0)).fill(Color::rgb(0, 255, 0)),
        AnnotationSpec::circle(rect(70.0, 10.0)).color(Color::rgb(0, 0, 255)),
        AnnotationSpec::ink(
            vec![vec![Point { x: 10.0, y: 60.0 }, Point { x: 50.0, y: 80.0 }]],
            3.0,
        ),
        AnnotationSpec::free_text(rect(10.0, 100.0), "Hello", 12.0),
        AnnotationSpec::text_note(rect(100.0, 100.0), "Note"),
        AnnotationSpec::underline(vec![rect(10.0, 140.0)]),
        AnnotationSpec::strikeout(vec![rect(70.0, 140.0)]),
        AnnotationSpec::squiggly(vec![rect(130.0, 140.0)]),
    ] {
        page.add_annotation(spec).unwrap();
        let current = page.render(&config).unwrap().into_pixels();
        assert_ne!(current, previous);
        previous = current;
    }
    let mut saved = vec![];
    doc.save_to(&mut saved).unwrap();
    if let Some(path) = std::env::var_os("PDFOXIUM_EDIT_ARTIFACT_DIR") {
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(std::path::Path::new(&path).join("annotations.pdf"), &saved).unwrap();
        // Isolate annotation geometry from platform font rasterization in the
        // optional native-oracle fixture.
        let nontext = Document::from_bytes(saved.clone()).unwrap();
        let nontext_page = nontext.page(0).unwrap();
        nontext_page
            .remove_annotation(nontext_page.annotation_ids().unwrap()[3])
            .unwrap();
        nontext_page
            .add_annotation(AnnotationSpec::highlight(vec![rect(140.0, 10.0)]))
            .unwrap();
        nontext
            .save_to(
                &mut std::fs::File::create(
                    std::path::Path::new(&path).join("annotations-nontext.pdf"),
                )
                .unwrap(),
            )
            .unwrap();
        page.render(&config)
            .unwrap()
            .write_png(std::path::Path::new(&path).join("annotations.png"))
            .unwrap();
    }
    let reopened = Document::from_bytes(saved).unwrap();
    assert_eq!(
        reopened.page(0).unwrap().render(&config).unwrap().pixels(),
        previous
    );
}

#[test]
fn ids_validation_busy_and_ink_regeneration() {
    let doc = document();
    let page = doc.page(0).unwrap();
    let bad = Rect {
        left: f32::NAN,
        ..rect(0.0, 0.0)
    };
    assert!(matches!(
        page.add_annotation(AnnotationSpec::highlight(vec![bad])),
        Err(Error::InvalidArgument(_))
    ));
    assert!(page.annotation_ids().unwrap().is_empty());
    let strokes = vec![vec![Point { x: 10.0, y: 10.0 }, Point { x: 30.0, y: 40.0 }]];
    let id = page
        .add_annotation(AnnotationSpec::ink(strokes.clone(), 2.0))
        .unwrap();
    let rect = page.annotations().unwrap()[0].rect;
    page.set_annotation_color(id, Color::rgb(255, 0, 0))
        .unwrap();
    page.set_annotation_color(id, Color::BLACK).unwrap();
    assert_eq!(page.annotations().unwrap()[0].rect, rect);
    assert!(matches!(
        document().page(0).unwrap().remove_annotation(id),
        Err(Error::Stale)
    ));
    let job = page.render_job(&RenderConfig::default()).unwrap();
    assert!(matches!(page.remove_annotation(id), Err(Error::Busy)));
    drop(job);
    page.set_annotation_ink(id, &strokes, 5.0).unwrap();
    assert!(page.annotations().unwrap()[0].rect.width() > rect.width());
}

#[test]
fn text_style_shape_geometry_and_visibility_updates() {
    let doc = document();
    let page = doc.page(0).unwrap();
    let config = RenderConfig::default();
    let id = page
        .add_annotation(AnnotationSpec::free_text(rect(10.0, 100.0), "Hello", 8.0))
        .unwrap();
    let original = page.render(&config).unwrap().into_pixels();
    page.set_annotation_contents(id, "World!").unwrap();
    let text = page.render(&config).unwrap().into_pixels();
    assert!(text != original, "contents must change rendered glyphs");
    page.set_annotation_text_style(id, 12.0, Color::rgb(255, 0, 0))
        .unwrap();
    let styled = page.render(&config).unwrap().into_pixels();
    assert!(styled != text, "text style must visibly change");
    page.set_annotation_rect(id, rect(100.0, 100.0)).unwrap();
    assert_ne!(page.render(&config).unwrap().pixels(), styled);
    page.set_annotation_flags(id, 2).unwrap();
    let hidden = page.render(&config).unwrap().into_pixels();
    page.remove_annotation(id).unwrap();
    assert_eq!(page.render(&config).unwrap().pixels(), hidden);
    let square = page
        .add_annotation(AnnotationSpec::square(rect(10.0, 10.0)))
        .unwrap();
    let thin = page.render(&config).unwrap().into_pixels();
    page.set_annotation_border_width(square, 4.0).unwrap();
    assert_ne!(page.render(&config).unwrap().pixels(), thin);
    assert!(matches!(
        page.set_annotation_text_style(square, 12.0, Color::BLACK),
        Err(Error::Unsupported(_))
    ));
    assert!(matches!(
        page.set_annotation_rects(square, &[rect(10.0, 10.0)]),
        Err(Error::Unsupported(_))
    ));
    assert!(matches!(
        page.set_annotation_border_width(square, f32::INFINITY),
        Err(Error::InvalidArgument(_))
    ));
}

#[test]
fn free_text_creation_and_editing_preserves_form_fields_on_another_page() {
    // Like the original real-book regression, retain live fields on another
    // page while FreeText supplies the missing document-level font resources.
    let doc = Document::from_bytes(form_fixture::without_default_resources()).unwrap();
    doc.insert_blank_page(
        0,
        pdfoxium::Size {
            width: 200.,
            height: 200.,
        },
    )
    .unwrap();
    let form = doc.forms(pdfoxium::FormOptions::default()).unwrap();
    let fields: Vec<_> = form
        .fields(1)
        .unwrap()
        .into_iter()
        .map(|f| (f.name, f.value))
        .collect();
    assert!(!fields.is_empty());
    let id = form
        .add_annotation(0, AnnotationSpec::free_text(rect(40., 40.), "Text", 12.))
        .unwrap();
    form.set_annotation_contents(0, id, "Hello editor").unwrap();
    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    let reopened = Document::from_bytes(saved).unwrap();
    let reopened = reopened.forms(pdfoxium::FormOptions::default()).unwrap();
    let after: Vec<_> = reopened
        .fields(1)
        .unwrap()
        .into_iter()
        .map(|f| (f.name, f.value))
        .collect();
    assert_eq!(after, fields);
}

#[test]
fn free_text_supplies_missing_form_resources_without_replacing_existing_form() {
    for resources in ["", "/DR << >>", "/DR << /Font << >> >>"] {
        for live in [false, true] {
            let doc = document_with_form(&format!(
                "/AcroForm << /Fields [] /NeedAppearances true {resources} >>"
            ));
            let page = doc.page(0).unwrap();
            let blank = page.render(&RenderConfig::default()).unwrap().into_pixels();
            let form = live.then(|| doc.forms(pdfoxium::FormOptions::default()).unwrap());
            let spec = AnnotationSpec::free_text(rect(40., 40.), "Visible", 12.);
            let id = if let Some(form) = &form {
                form.add_annotation(0, spec)
            } else {
                page.add_annotation(spec)
            }
            .unwrap();
            let image = page.render(&RenderConfig::default()).unwrap().into_pixels();
            assert_ne!(blank, image);
            if let Some(form) = &form {
                form.set_annotation_contents(0, id, "Edited").unwrap();
            } else {
                page.set_annotation_contents(id, "Edited").unwrap();
            }
            let mut saved = Vec::new();
            doc.save_to(&mut saved).unwrap();
            let reopened = Document::from_bytes(saved).unwrap();
            let page = reopened.page(0).unwrap();
            assert_eq!(
                page.annotation_string(page.annotation_ids().unwrap()[0], b"Contents")
                    .unwrap(),
                "Edited"
            );
        }
    }
    for invalid in ["/DR /Bad", "/DR << /Font /Bad >>"] {
        let doc = document_with_form(&format!("/AcroForm << /Fields [] {invalid} >>"));
        let page = doc.page(0).unwrap();
        assert!(matches!(
            page.add_annotation(AnnotationSpec::free_text(rect(40., 40.), "No", 12.)),
            Err(Error::InvalidArgument(_))
        ));
        assert!(page.annotation_ids().unwrap().is_empty());
    }
}

#[test]
fn free_text_supports_newlines_wrapping_and_reflow_after_resize() {
    let doc = document();
    let page = doc.page(0).unwrap();
    let rect = Rect {
        left: 20.,
        bottom: 20.,
        right: 160.,
        top: 160.,
    };
    let id = page
        .add_annotation(AnnotationSpec::free_text(rect, "First\nSecond", 12.))
        .unwrap();
    let text_rows = |page: &pdfoxium::Page, id| {
        page.annotation_objects(id)
            .unwrap()
            .into_iter()
            .filter(|o| o.kind == pdfoxium::ObjectKind::Text)
            .map(|o| o.bounds)
            .collect::<Vec<_>>()
    };
    let rows = text_rows(&page, id);
    assert_eq!(rows.len(), 2);
    assert!(rows[0].top > 140. && rows[0].bottom > rows[1].top);
    let text = "alpha beta gamma delta epsilon";
    page.set_annotation_contents(id, text).unwrap();
    let wide = text_rows(&page, id).len();
    page.set_annotation_rect(id, Rect { right: 75., ..rect })
        .unwrap();
    let narrow = text_rows(&page, id).len();
    assert!(narrow > wide && narrow >= 3);
    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    let reopened = Document::from_bytes(saved).unwrap();
    let page = reopened.page(0).unwrap();
    let id = page.annotation_ids().unwrap()[0];
    assert_eq!(page.annotation_string(id, b"Contents").unwrap(), text);
    assert_eq!(text_rows(&page, id).len(), narrow);
}

#[test]
fn live_annotation_styles_round_trip_and_clear_fill_without_partial_invalid_edits() {
    use pdfoxium::{AnnotationStyle as S, FormOptions};
    let doc = document();
    let page = doc.page(0).unwrap();
    let text = page
        .add_annotation(AnnotationSpec::free_text(
            rect(20., 100.),
            "First line\nSecond line",
            12.,
        ))
        .unwrap();
    let shape = page
        .add_annotation(AnnotationSpec::square(rect(70., 20.)))
        .unwrap();
    let highlight = page
        .add_annotation(AnnotationSpec::highlight(vec![rect(10., 140.)]))
        .unwrap();
    let form = doc.forms(FormOptions::default()).unwrap();
    let styles = [
        (
            text,
            S::FreeText {
                font_size: 18.,
                color: Color::rgb(100, 20, 160),
            },
        ),
        (
            shape,
            S::Shape {
                border_color: Color::rgb(20, 140, 60),
                border_width: 4.,
                fill: Some(Color::rgb(255, 220, 100)),
            },
        ),
        (
            highlight,
            S::Highlight {
                color: Color::rgba(20, 160, 255, 64),
            },
        ),
    ];
    for (id, style) in styles {
        let before = form
            .render(0, &RenderConfig::default())
            .unwrap()
            .into_pixels();
        form.set_annotation_style(0, id, style).unwrap();
        assert_eq!(page.annotation_style(id).unwrap(), Some(style));
        assert_ne!(
            form.render(0, &RenderConfig::default()).unwrap().pixels(),
            before
        );
    }
    let style = S::Shape {
        border_color: Color::rgb(20, 140, 60),
        border_width: 4.,
        fill: None,
    };
    form.set_annotation_style(0, shape, style).unwrap();
    assert_eq!(page.annotation_style(shape).unwrap(), Some(style));
    let revision = doc.revision();
    assert!(matches!(
        form.set_annotation_style(
            0,
            shape,
            S::Shape {
                border_color: Color::BLACK,
                border_width: f32::NAN,
                fill: None
            }
        ),
        Err(Error::InvalidArgument(_))
    ));
    assert!(matches!(
        form.set_annotation_style(
            0,
            text,
            S::Highlight {
                color: Color::BLACK
            }
        ),
        Err(Error::Unsupported(_))
    ));
    assert_eq!(doc.revision(), revision);
    assert_eq!(page.annotation_style(shape).unwrap(), Some(style));
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    let reopened = Document::from_bytes(bytes).unwrap();
    let saved = reopened.page(0).unwrap();
    let ids = saved.annotation_ids().unwrap();
    assert_eq!(saved.annotation_style(ids[0]).unwrap(), Some(styles[0].1));
    assert_eq!(saved.annotation_style(ids[1]).unwrap(), Some(style));
    assert_eq!(saved.annotation_style(ids[2]).unwrap(), Some(styles[2].1));
}
