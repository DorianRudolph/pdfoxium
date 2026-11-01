// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use pdfoxium::{
    Bitmap, Color, Document, Error, JpegStorage, Matrix, PixelFormat, RenderConfig, Size,
    StandardFont, TextRenderMode, TextSpec,
};
fn setup() -> (Document, pdfoxium::Page) {
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
    (doc, page)
}
fn pixels(color: [u8; 4]) -> Bitmap {
    let mut b = Bitmap::new(2, 2, PixelFormat::Rgba8).unwrap();
    for pixel in b.pixels_mut().chunks_exact_mut(4) {
        pixel.copy_from_slice(&color);
    }
    b
}
#[test]
fn image_inspection_replacement_and_jpeg_roundtrip() {
    let (doc, page) = setup();
    let id = page
        .add_image(
            pixels([255, 0, 0, 128]),
            Matrix::scale(40., 40.).then(Matrix::translate(10., 10.)),
        )
        .unwrap();
    let info = page.image_info(id).unwrap();
    assert_eq!((info.width, info.height), (2, 2));
    assert_eq!(info.dpi, Some((2.0 / 40.0 * 72.0, 2.0 / 40.0 * 72.0)));
    assert_eq!(page.image_data(id, false).unwrap(), [255, 0, 0].repeat(4));
    assert_eq!(page.image_data(id, true).unwrap(), [255, 0, 0].repeat(4));
    assert!(page.image_icc_profile(id).unwrap().is_none());
    let decoded = page.image_bitmap(id).unwrap();
    assert_eq!(&decoded.pixels()[..4], &[0, 0, 255, 255]); // Raw bitmap omits SMask.
    let rendered = page.render_image_object(id).unwrap();
    assert_eq!((rendered.width(), rendered.height()), (40, 40));
    assert_eq!(rendered.pixels()[3], 128);
    let before = page.render(&RenderConfig::default()).unwrap();
    page.set_image_bitmap(id, pixels([0, 255, 0, 255])).unwrap();
    let after = page.render(&RenderConfig::default()).unwrap();
    assert_ne!(before.pixels(), after.pixels());
    assert_eq!(
        &page.image_bitmap(id).unwrap().pixels()[..4],
        &[0, 255, 0, 255]
    );
    let jpg = include_bytes!("../../pdfoxium-core/tests/fixtures/gray.jpg");
    for storage in [JpegStorage::Retained, JpegStorage::Inline] {
        page.set_image_jpeg(id, jpg.as_slice(), storage).unwrap();
        assert_eq!(page.image_data(id, false).unwrap(), jpg);
        assert_eq!(
            page.image_info(id).unwrap().filters,
            vec![b"DCTDecode".to_vec()]
        );
        let data = page.image_data(id, false).unwrap();
        assert!(page.set_image_jpeg(id, vec![1, 2, 3], storage).is_err());
        assert_eq!(page.image_data(id, false).unwrap(), data);
        page.add_jpeg(
            jpg.as_slice(),
            Matrix::scale(20., 20.).then(Matrix::translate(60., 60.)),
            storage,
        )
        .unwrap();
    }
    let expected = page.render(&RenderConfig::default()).unwrap();
    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    if std::env::var_os("PDFOXIUM_EDIT_FIXTURES").is_some() {
        std::fs::create_dir_all("../../target/edit-api").unwrap();
        std::fs::write("../../target/edit-api/media-images.pdf", &saved).unwrap();
    }
    assert_eq!(
        Document::from_bytes(saved)
            .unwrap()
            .page(0)
            .unwrap()
            .render(&RenderConfig::default())
            .unwrap()
            .pixels(),
        expected.pixels()
    );
}
#[test]
fn font_inspection_and_text_editing() {
    let (doc, page) = setup();
    let font = doc.standard_font(StandardFont::Helvetica).unwrap();
    let id = page
        .add_text(&TextSpec {
            text: "ABC",
            font: &font,
            size: 12.,
            matrix: Matrix::translate(10., 50.),
            color: Color::BLACK,
        })
        .unwrap();
    assert_eq!(page.extracted_object_text(id).unwrap(), "ABC");
    let font = page.object_font(id).unwrap();
    assert_eq!(font.info().unwrap().base_name, b"Helvetica");
    assert!(!font.data().unwrap().is_empty());
    assert!(font.ascent(12.).unwrap() > 0.);
    assert!(font.descent(12.).unwrap() <= 0.);
    assert!(font.glyph_width('A', 12.).unwrap() > 0.);
    assert!(!font.glyph_path('A', 12.).unwrap().unwrap().is_empty());
    assert!(font.glyph_width('A', f32::NAN).is_err());
    page.set_text_charcodes(id, &[65, 66, 67]).unwrap();
    page.set_text_positions(id, &[20., 40.]).unwrap();
    assert_eq!(page.text_object_data(id).unwrap().positions, vec![20., 40.]);
    assert!(page.set_text_positions(id, &[20.]).is_err());
    assert!(page.set_text_charcodes(id, &[]).is_err());
    let before = page.render(&RenderConfig::default()).unwrap();
    page.set_text_font_size(id, 18.).unwrap();
    assert_eq!(page.text_object_data(id).unwrap().font_size, 18.);
    assert_ne!(
        before.pixels(),
        page.render(&RenderConfig::default()).unwrap().pixels()
    );
    let text_bitmap = page.render_text_object(id, 2.).unwrap();
    assert!(text_bitmap.pixels().chunks_exact(4).any(|p| p[3] != 0));
    page.set_text_render_mode(id, TextRenderMode::Invisible)
        .unwrap();
    assert!(page
        .render(&RenderConfig::default())
        .unwrap()
        .pixels()
        .iter()
        .all(|b| *b == 255));
    page.set_text_render_mode(id, TextRenderMode::Fill).unwrap();
    let expected = page.render(&RenderConfig::default()).unwrap();
    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    assert_eq!(
        Document::from_bytes(saved)
            .unwrap()
            .page(0)
            .unwrap()
            .render(&RenderConfig::default())
            .unwrap()
            .pixels(),
        expected.pixels()
    );
    let job = page.render_job(&RenderConfig::default()).unwrap();
    assert!(matches!(page.set_text_font_size(id, 12.), Err(Error::Busy)));
    drop(job);
    page.unload().unwrap();
    assert!(matches!(page.object_font(id), Err(Error::Stale)));
    // Retaining a Font does not retain a parsed page; it remains useful after unload.
    assert!(font.glyph_width('A', 12.).unwrap() > 0.);
}
#[test]
fn custom_cid_font_mapping() {
    let (doc, page) = setup();
    let ttf = include_bytes!("../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
    let cmap=b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def /CMapName /Test def /CMapType 2 def 1 begincodespacerange <0000> <FFFF> endcodespacerange 1 beginbfchar <0001> <0041> endbfchar endcmap CMapName currentdict /CMap defineresource pop end end";
    let font = doc.load_cid_type2_font(ttf, cmap, &[0, 0, 0, 1]).unwrap();
    assert!(font.info().unwrap().embedded);
    let id = page
        .add_text(&TextSpec {
            text: "A",
            font: &font,
            size: 12.,
            matrix: Matrix::translate(10., 50.),
            color: Color::BLACK,
        })
        .unwrap();
    assert_eq!(page.text_object_data(id).unwrap().charcodes, vec![1]);
    assert!(doc.load_cid_type2_font(ttf, cmap, &[0]).is_err());
}

#[test]
fn detached_media_inspection_mutation_and_annotation_handle() {
    let (doc, page) = setup();
    let font = doc.standard_font(StandardFont::Helvetica).unwrap();
    let id = page
        .add_text(&TextSpec {
            text: "AB",
            font: &font,
            size: 12.,
            matrix: Matrix::translate(10., 50.),
            color: Color::BLACK,
        })
        .unwrap();
    let mut object = page.take_object(id).unwrap();
    assert_eq!(object.text().unwrap(), "AB");
    assert_eq!(
        object.font().unwrap().info().unwrap().base_name,
        b"Helvetica"
    );
    object.set_text_charcodes(&[65, 66, 67]).unwrap();
    object.set_text_font_size(18.).unwrap();
    object.set_text_positions(&[20., 40.]).unwrap();
    object
        .set_text_render_mode(TextRenderMode::Invisible)
        .unwrap();
    assert_eq!(
        object.text_object_data().unwrap().render_mode,
        TextRenderMode::Invisible
    );
    object.set_text_render_mode(TextRenderMode::Fill).unwrap();
    assert_eq!(object.text().unwrap(), "ABC");
    assert_eq!(object.text_object_data().unwrap().positions, vec![20., 40.]);
    assert!(object
        .render_text_object(1.)
        .unwrap()
        .pixels()
        .chunks_exact(4)
        .any(|p| p[3] != 0));
    let id = page.insert_object(object).unwrap();
    assert_eq!(page.text_object_data(id).unwrap().font_size, 18.);
    let image_id = page
        .add_image(pixels([255, 0, 0, 255]), Matrix::scale(20., 20.))
        .unwrap();
    let mut image = page.take_object(image_id).unwrap();
    assert_eq!(image.image_info().unwrap().width, 2);
    assert_eq!(image.image_info().unwrap().bits_per_pixel, 0);
    assert_eq!(image.image_info_in(&page).unwrap().bits_per_pixel, 24);
    assert!(image.image_icc_profile(&page).unwrap().is_none());
    assert_eq!(image.image_data(false).unwrap(), [255, 0, 0].repeat(4));
    image.set_image_bitmap(pixels([0, 0, 255, 128])).unwrap();
    assert_eq!(
        &image.image_bitmap().unwrap().pixels()[..4],
        &[255, 0, 0, 255]
    );
    assert_eq!(image.render_image_object().unwrap().pixels()[3], 128);
    assert_eq!(
        image.render_image_object_in(&page).unwrap().pixels()[3],
        128
    );
    let jpg = include_bytes!("../../pdfoxium-core/tests/fixtures/gray.jpg");
    image
        .set_image_jpeg(jpg.as_slice(), JpegStorage::Retained)
        .unwrap();
    assert_eq!(image.image_data(false).unwrap(), jpg);
    let (foreign, foreign_page) = setup();
    assert!(image.image_info_in(&foreign_page).is_err());
    drop(foreign);
    let stamp = page
        .create_annotation(
            pdfoxium::AnnotationKind::Stamp,
            pdfoxium::Rect {
                left: 0.,
                bottom: 0.,
                right: 40.,
                top: 40.,
            },
        )
        .unwrap();
    let object_id = page.insert_annotation_object(stamp, image).unwrap();
    let before = page.render(&RenderConfig::default()).unwrap();
    let image = page.annotation_object(object_id).unwrap();
    assert_eq!(image.image_info().unwrap().width, 16);
    image.set_image_bitmap(pixels([0, 255, 0, 255])).unwrap();
    assert_eq!(image.image_info().unwrap().width, 2);
    let detached_text = page.take_object(id).unwrap();
    let text_annotation_id = page.insert_annotation_object(stamp, detached_text).unwrap();
    let text = page.annotation_object(text_annotation_id).unwrap();
    text.set_text_charcodes(&[65, 66]).unwrap();
    text.set_text_font_size(10.).unwrap();
    text.set_text_positions(&[15.]).unwrap();
    assert_eq!(text.font().unwrap().info().unwrap().base_name, b"Helvetica");
    assert_eq!(text.text().unwrap(), "AB");
    let expected = page.render(&RenderConfig::default()).unwrap();
    assert_ne!(before.pixels(), expected.pixels());
    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    assert_eq!(
        Document::from_bytes(saved)
            .unwrap()
            .page(0)
            .unwrap()
            .render(&RenderConfig::default())
            .unwrap()
            .pixels(),
        expected.pixels()
    );
}
