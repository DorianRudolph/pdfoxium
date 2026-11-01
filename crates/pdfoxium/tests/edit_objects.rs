// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use pdfoxium::{
    Bitmap, Color, Document, Error, FillRule, Matrix, PathSpec, PixelFormat, Rect, RenderConfig,
    Size, StandardFont, Stroke, TextSpec,
};
fn rect(color: Color) -> PathSpec {
    PathSpec {
        fill: Some(color),
        ..PathSpec::rect(Rect {
            left: 10.,
            bottom: 10.,
            right: 60.,
            top: 60.,
        })
    }
}
fn page(doc: &Document) -> pdfoxium::Page {
    doc.insert_blank_page(
        doc.page_count().unwrap(),
        Size {
            width: 100.,
            height: 100.,
        },
    )
    .unwrap()
}
fn pixel(page: &pdfoxium::Page, x: usize, y: usize) -> [u8; 4] {
    let b = page.render(&RenderConfig::fit(100, 100)).unwrap();
    b.pixels()[y * b.stride() + x * 4..y * b.stride() + x * 4 + 4]
        .try_into()
        .unwrap()
}
#[test]
fn paint_order_transfer_failure_identity_and_save() {
    let doc = Document::create().unwrap();
    let a = page(&doc);
    let b = page(&doc);
    let red = a.add_path(&rect(Color::rgb(255, 0, 0))).unwrap();
    let blue = a.add_path(&rect(Color::rgb(0, 0, 255))).unwrap();
    assert_eq!(pixel(&a, 30, 60), [255, 0, 0, 255]);
    a.reorder_object(red, 1).unwrap();
    assert_eq!(pixel(&a, 30, 60), [0, 0, 255, 255]);
    assert_eq!(a.objects().unwrap()[0].id, blue);
    assert!(a.reorder_object(red, 99).is_err());
    assert_eq!(a.object(red).unwrap().id, red);
    let foreign = Document::create().unwrap();
    let other = page(&foreign);
    assert!(a.move_object_to(red, &other).is_err());
    assert_eq!(a.object(red).unwrap().id, red);
    let moved = a.move_object_to(red, &b).unwrap();
    assert!(matches!(a.object(red), Err(Error::Stale)));
    assert_eq!(pixel(&a, 30, 60), [255, 0, 0, 255]);
    assert_eq!(pixel(&b, 30, 60), [0, 0, 255, 255]);
    b.transform_object(moved, Matrix::translate(20., 0.))
        .unwrap();
    assert_eq!(pixel(&b, 20, 60), [255, 255, 255, 255]);
    assert_eq!(pixel(&b, 40, 60), [0, 0, 255, 255]);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    if std::env::var_os("PDFOXIUM_EDIT_FIXTURES").is_some() {
        std::fs::create_dir_all("../../target/edit-api").unwrap();
        std::fs::write("../../target/edit-api/objects.pdf", &bytes).unwrap();
    }
    let reopened = Document::from_bytes(bytes).unwrap();
    for i in 0..2 {
        assert_eq!(
            doc.page(i)
                .unwrap()
                .render(&RenderConfig::fit(100, 100))
                .unwrap()
                .pixels(),
            reopened
                .page(i)
                .unwrap()
                .render(&RenderConfig::fit(100, 100))
                .unwrap()
                .pixels()
        );
    }
    b.unload().unwrap();
    assert!(matches!(b.object(moved), Err(Error::Stale)));
    let new = b.objects().unwrap()[0].id;
    assert_ne!(new, moved);
    b.remove_object(new).unwrap();
    assert_eq!(pixel(&b, 40, 60), [255, 255, 255, 255]);
}
#[test]
fn text_image_style_and_invalid_inputs() {
    let doc = Document::create().unwrap();
    let p = page(&doc);
    let font = doc.standard_font(StandardFont::Helvetica).unwrap();
    let text = p
        .add_text(&TextSpec {
            text: "Hello",
            font: &font,
            size: 14.,
            matrix: Matrix::translate(10., 75.),
            color: Color::BLACK,
        })
        .unwrap();
    assert!(p.text().unwrap().as_str().contains("Hello"));
    assert_eq!(p.object(text).unwrap().text.as_deref(), Some("Hello"));
    assert_eq!(p.object(text).unwrap().font_size, Some(14.));
    p.set_object_text(text, "World").unwrap();
    assert!(p.text().unwrap().as_str().contains("World"));
    assert!(p.set_object_text(text, "🦀").is_err());
    assert!(p.text().unwrap().as_str().contains("World"));
    let mut bitmap = Bitmap::new(2, 2, PixelFormat::Rgba8).unwrap();
    for px in bitmap.pixels_mut().chunks_exact_mut(4) {
        px.copy_from_slice(&[20, 180, 40, 255]);
    }
    p.add_image(
        bitmap,
        Matrix {
            a: 20.,
            b: 0.,
            c: 0.,
            d: 20.,
            e: 70.,
            f: 10.,
        },
    )
    .unwrap();
    assert_eq!(pixel(&p, 80, 80), [40, 180, 20, 255]);
    let shape = p.add_path(&rect(Color::rgb(255, 0, 0))).unwrap();
    p.set_object_fill(shape, Some(Color::rgb(0, 255, 0)), FillRule::EvenOdd)
        .unwrap();
    p.set_object_stroke(
        shape,
        Some(Stroke {
            color: Color::BLACK,
            width: 2.,
        }),
    )
    .unwrap();
    assert_eq!(pixel(&p, 30, 60), [0, 255, 0, 255]);
    assert_eq!(p.object(shape).unwrap().stroke_width, 2.);
    assert!(p
        .set_object_stroke(
            shape,
            Some(Stroke {
                color: Color::BLACK,
                width: f32::NAN
            })
        )
        .is_err());
    assert!(p
        .set_object_matrix(
            shape,
            Matrix {
                a: f32::NAN,
                ..Matrix::default()
            }
        )
        .is_err());
    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    if std::env::var_os("PDFOXIUM_EDIT_FIXTURES").is_some() {
        std::fs::create_dir_all("../../target/edit-api").unwrap();
        std::fs::write("../../target/edit-api/text-image.pdf", &saved).unwrap();
    }
    let reopened = Document::from_bytes(saved).unwrap();
    assert_eq!(
        p.render(&RenderConfig::fit(100, 100)).unwrap().pixels(),
        reopened
            .page(0)
            .unwrap()
            .render(&RenderConfig::fit(100, 100))
            .unwrap()
            .pixels()
    );
    let active = p.render_job(&RenderConfig::fit(100, 100)).unwrap();
    assert!(matches!(p.remove_object(shape), Err(Error::Busy)));
    drop(active);
}

#[test]
fn embedded_unicode_font_and_resource_transfer() {
    let doc = Document::create().unwrap();
    let a = page(&doc);
    let b = page(&doc);
    let font = doc
        .load_font(
            include_bytes!("../../../tests/capi/resources/test_fonts/DejaVuSans.ttf"),
            pdfoxium::FontType::TrueType,
            true,
        )
        .unwrap();
    let id = a
        .add_text(&TextSpec {
            text: "AΩ",
            font: &font,
            size: 18.,
            matrix: Matrix::translate(10., 50.),
            color: Color::BLACK,
        })
        .unwrap();
    let before = a.render(&RenderConfig::fit(100, 100)).unwrap();
    assert!(before.pixels().chunks_exact(4).any(|p| p[0] < 128));
    a.move_object_to(id, &b).unwrap();
    assert_eq!(
        before.pixels(),
        b.render(&RenderConfig::fit(100, 100)).unwrap().pixels()
    );
    let foreign = Document::create().unwrap();
    let p = page(&foreign);
    assert!(p
        .add_text(&TextSpec {
            text: "A",
            font: &font,
            size: 18.,
            matrix: Matrix::default(),
            color: Color::BLACK
        })
        .is_err());
    assert!(p.objects().unwrap().is_empty());
    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    let reopened = Document::from_bytes(saved).unwrap();
    assert!(reopened
        .page(1)
        .unwrap()
        .text()
        .unwrap()
        .as_str()
        .contains("AΩ"));
    assert_eq!(
        before.pixels(),
        reopened
            .page(1)
            .unwrap()
            .render(&RenderConfig::fit(100, 100))
            .unwrap()
            .pixels()
    );
    assert!(doc
        .load_font(b"not a font", pdfoxium::FontType::TrueType, true)
        .is_err());
}

fn marked_document() -> Document {
    // A marked path whose MCID belongs to its original page.
    let stream = b"/Span << /MCID 0 >> BDC 1 0 0 rg 10 10 40 40 re f EMC";
    let pdf=format!("%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Contents 4 0 R >> endobj\n4 0 obj << /Length {} >> stream\n{}\nendstream endobj\ntrailer << /Root 1 0 R >>\n%%EOF",stream.len(),std::str::from_utf8(stream).unwrap());
    Document::from_bytes(pdf.into_bytes()).unwrap()
}
#[test]
fn loaded_objects_reorder_and_marked_transfer_is_preserved_on_failure() {
    let source = Document::create().unwrap();
    let p = page(&source);
    p.add_path(&rect(Color::rgb(255, 0, 0))).unwrap();
    let mut bytes = Vec::new();
    source.save_to(&mut bytes).unwrap();
    let doc = Document::from_bytes(bytes).unwrap();
    let p = doc.page(0).unwrap();
    let red = p.objects().unwrap()[0].id;
    let blue = p.add_path(&rect(Color::rgb(0, 0, 255))).unwrap();
    p.reorder_object(blue, 0).unwrap();
    assert_eq!(pixel(&p, 30, 60), [0, 0, 255, 255]);
    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    assert_eq!(
        pixel(
            &Document::from_bytes(saved).unwrap().page(0).unwrap(),
            30,
            60
        ),
        [0, 0, 255, 255]
    );
    p.reorder_object(red, 0).unwrap();
    p.unload().unwrap();
    assert_eq!(pixel(&p, 30, 60), [255, 0, 0, 255]);
    let tagged = marked_document();
    let from = tagged.page(0).unwrap();
    let to = page(&tagged);
    let id = from.objects().unwrap()[0].id;
    assert!(matches!(
        from.move_object_to(id, &to),
        Err(Error::Unsupported(_))
    ));
    assert_eq!(from.object(id).unwrap().id, id);
    assert!(to.objects().unwrap().is_empty());
    assert_eq!(pixel(&from, 30, 60), [0, 0, 255, 255]);
}
