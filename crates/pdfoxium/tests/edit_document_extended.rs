// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use pdfoxium::{Color, Document, Matrix, PathSpec, Rect, RenderConfig, SaveOptions, Size, Version};
fn fixture() -> (Document, pdfoxium::Page) {
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
fn pixels(page: &pdfoxium::Page) -> Vec<u8> {
    page.render(&RenderConfig::fit(100, 100))
        .unwrap()
        .into_pixels()
}
#[test]
fn content_transform_clip_xobject_nup_and_save() {
    let (source, page) = fixture();
    let before = pixels(&page);
    page.transform_content(Some(Matrix::translate(20., 0.)), None)
        .unwrap();
    let translated = pixels(&page);
    assert_ne!(before, translated);
    page.clip_content(Rect {
        left: 0.,
        bottom: 0.,
        right: 50.,
        top: 100.,
    })
    .unwrap();
    let clipped = pixels(&page);
    assert_ne!(translated, clipped);
    let output = Document::create().unwrap();
    let target = output
        .insert_blank_page(
            0,
            Size {
                width: 100.,
                height: 100.,
            },
        )
        .unwrap();
    let xobject = output.import_page_xobject(&source, 0).unwrap();
    let id = target.insert_object(xobject.object().unwrap()).unwrap();
    assert_eq!(pixels(&target), clipped);
    assert!(!target.form_objects(id).unwrap().is_empty());
    drop(source);
    drop(page);
    drop(xobject);
    let mut bytes = vec![];
    output.save_to(&mut bytes).unwrap();
    assert_eq!(
        pixels(
            &Document::from_bytes(bytes.clone())
                .unwrap()
                .page(0)
                .unwrap()
        ),
        clipped
    );
    let nup = output
        .n_up(
            Size {
                width: 200.,
                height: 100.,
            },
            2,
            1,
        )
        .unwrap();
    assert_eq!(nup.page_count().unwrap(), 1);
    assert_eq!(nup.page(0).unwrap().width().unwrap(), 200.);
    assert_ne!(pixels(&nup.page(0).unwrap()), vec![255; 40000]);
    assert!(output
        .n_up(
            Size {
                width: 200.,
                height: 100.
            },
            usize::MAX,
            2
        )
        .is_err());
    if let Some(dir) = std::env::var_os("PDFOXIUM_EDIT_ARTIFACT_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            std::path::Path::new(&dir).join("extended-transform.pdf"),
            bytes,
        )
        .unwrap();
        nup.save_to(
            &mut std::fs::File::create(std::path::Path::new(&dir).join("extended-nup.pdf"))
                .unwrap(),
        )
        .unwrap();
    }
}
#[test]
fn save_modes_and_attachment_changes_round_trip() {
    let (doc, page) = fixture();
    let expected = pixels(&page);
    let mut attachment = doc.add_attachment("example.txt", b"before").unwrap();
    attachment.set_data(b"after", Some("D:20261026")).unwrap();
    attachment.set_description("description").unwrap();
    attachment.set_parameter("ModDate", "D:20261027").unwrap();
    assert_eq!(
        attachment.parameter("ModDate").unwrap().as_deref(),
        Some("D:20261027")
    );
    let mut bytes = vec![];
    doc.save_with_options(
        &mut bytes,
        &SaveOptions {
            version: Some(Version { major: 1, minor: 4 }),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(bytes.starts_with(b"%PDF-1.4"));
    let reopened = Document::from_bytes(bytes).unwrap();
    assert_eq!(reopened.attachments().unwrap()[0].read().unwrap(), b"after");
    assert_eq!(
        reopened.attachments().unwrap()[0].description(),
        "description"
    );
    let mut incremental = vec![];
    reopened
        .page(0)
        .unwrap()
        .transform_content(Some(Matrix::translate(5., 0.)), None)
        .unwrap();
    reopened
        .save_with_options(
            &mut incremental,
            &SaveOptions {
                incremental: true,
                ..Default::default()
            },
        )
        .unwrap();
    let again = Document::from_bytes(incremental).unwrap();
    assert_ne!(pixels(&again.page(0).unwrap()), expected);
    reopened.delete_attachment(0).unwrap();
    assert!(reopened.attachments().unwrap().is_empty());
    assert_eq!(attachment.read().unwrap(), b"after");
    assert!(reopened
        .save_with_options(
            &mut vec![],
            &SaveOptions {
                version: Some(Version { major: 2, minor: 0 }),
                ..Default::default()
            }
        )
        .is_err());
}

#[test]
fn security_removal_and_clip_snapshot() {
    let encrypted = Document::from_bytes_with_password(
        include_bytes!("../../../tests/render_corpus/pdfs/encrypted_r6.pdf"),
        "âge",
    )
    .unwrap();
    let mut plain = vec![];
    encrypted
        .save_with_options(
            &mut plain,
            &SaveOptions {
                remove_security: true,
                ..Default::default()
            },
        )
        .unwrap();
    let reopened = Document::from_bytes(plain).unwrap();
    assert_eq!(
        reopened.page_count().unwrap(),
        encrypted.page_count().unwrap()
    );
    let (doc, page) = fixture();
    let clip = pdfoxium::ClipPath::rectangle(Rect {
        left: 0.,
        bottom: 0.,
        right: 40.,
        top: 100.,
    })
    .unwrap();
    assert_eq!(clip.paths().len(), 1);
    let original = pixels(&page);
    page.insert_clip(&clip).unwrap();
    assert_ne!(original, pixels(&page));
    let id = page.objects().unwrap()[0].id;
    let recovered = page.object_clip(id).unwrap().unwrap();
    assert!(!recovered.paths().is_empty());
    page.transform_object_clip(id, Matrix::translate(10., 0.))
        .unwrap();
    page.generate_content().unwrap();
    let expected = pixels(&page);
    let mut saved = vec![];
    doc.save_to(&mut saved).unwrap();
    if let Some(dir) = std::env::var_os("PDFOXIUM_EDIT_ARTIFACT_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            std::path::Path::new(&dir).join("extended-object-clip.pdf"),
            &saved,
        )
        .unwrap();
    }
    assert_eq!(
        pixels(&Document::from_bytes(saved).unwrap().page(0).unwrap()),
        expected
    );
}

#[test]
fn new_objects_do_not_inherit_serialized_clip_prefixes() {
    let (doc, page) = fixture();
    page.clip_content(Rect {
        left: 0.,
        bottom: 0.,
        right: 40.,
        top: 100.,
    })
    .unwrap();
    page.add_path(&PathSpec {
        fill: Some(Color::rgb(0, 0, 255)),
        ..PathSpec::rect(Rect {
            left: 70.,
            bottom: 10.,
            right: 90.,
            top: 30.,
        })
    })
    .unwrap();
    let before = pixels(&page);
    assert_eq!(
        &before[(80 * 100 + 80) * 4..(80 * 100 + 80) * 4 + 4],
        &[255, 0, 0, 255]
    );
    let mut bytes = vec![];
    doc.save_to(&mut bytes).unwrap();
    let after = pixels(&Document::from_bytes(bytes).unwrap().page(0).unwrap());
    assert!(
        before == after,
        "new object clipping must survive save/reopen"
    );
}
