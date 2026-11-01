// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use pdfoxium::{
    thread::{Priority, RenderPool, Source},
    Config, Document, RenderConfig,
};
use std::sync::Arc;
fn bytes() -> Arc<[u8]> {
    Arc::from(&include_bytes!("../../pdfoxium-viewer/assets/demos/layers-demo.pdf")[..])
}
fn pixels(doc: &Document, page: usize, config: &RenderConfig) -> Vec<u8> {
    doc.page(page)
        .unwrap()
        .render(config)
        .unwrap()
        .into_pixels()
}
#[test]
fn default_view_snapshots_save_reopen_and_document_identity() {
    let source = bytes();
    let doc = Document::from_bytes(source.clone()).unwrap();
    let other = Document::from_bytes(bytes()).unwrap();
    let layers = doc.optional_content().unwrap();
    let groups = layers.groups().unwrap();
    assert_eq!(groups.len(), 4);
    assert!(groups[3].locked);
    let config = RenderConfig::fit(300, 320);
    let original: Vec<_> = (0..2).map(|p| pixels(&doc, p, &config)).collect();
    let snapshot = layers.snapshot().unwrap();
    let revision = doc.revision();
    assert!(layers.set_visible(groups[0].id, false).unwrap());
    assert!(layers.set_visible(groups[3].id, false).is_err());
    for (p, before) in original.iter().enumerate() {
        assert_ne!(pixels(&doc, p, &config), *before);
        assert_eq!(
            pixels(&doc, p, &config.clone().optional_content(snapshot.clone())),
            *before
        );
    }
    assert!(!doc.is_dirty());
    assert_eq!(doc.revision(), revision);
    assert!(other
        .page(0)
        .unwrap()
        .render(&config.clone().optional_content(snapshot.clone()))
        .is_err());
    assert!(other
        .optional_content()
        .unwrap()
        .set_visible(groups[0].id, false)
        .is_err());
    let worker = Document::from_bytes(source).unwrap();
    assert_eq!(
        pixels(&worker, 0, &config.clone().optional_content(snapshot)),
        original[0]
    );
    let mut saved = vec![];
    doc.save_to(&mut saved).unwrap();
    let reopened = Document::from_bytes(saved).unwrap();
    assert_eq!(pixels(&reopened, 0, &config), original[0]);
    layers.reset().unwrap();
    assert_eq!(pixels(&doc, 0, &config), original[0]);
    let mut independent = layers.state().unwrap();
    independent.set_visible(groups[1].id, false).unwrap();
    assert_ne!(
        pixels(
            &doc,
            0,
            &config.clone().optional_content(independent.snapshot())
        ),
        original[0]
    );
    assert_eq!(pixels(&doc, 0, &config), original[0]);
    independent
        .apply_configuration(independent.configurations()[1].id)
        .unwrap();
    let states: Vec<_> = independent.groups().iter().map(|g| g.visible).collect();
    assert_eq!(states, vec![false, true, false, true]);
}
#[test]
fn worker_pool_uses_immutable_per_request_visibility_without_reopening() {
    let source = bytes();
    let doc = Document::from_bytes(source.clone()).unwrap();
    let config = RenderConfig::fit(200, 220);
    let pool = RenderPool::new(Source::Bytes(source), Config::default(), 2).unwrap();
    let original = pixels(&doc, 0, &config);
    let old = pool.render(0, config.clone(), Priority::Visible);
    let mut state = pool.optional_content_state();
    state.set_visible(state.groups()[0].id, false).unwrap();
    pool.set_optional_content(state.snapshot()).unwrap();
    assert_eq!(old.wait().unwrap().bitmap.pixels(), original);
    let hidden = pool
        .render(0, config.clone(), Priority::Visible)
        .wait()
        .unwrap()
        .bitmap;
    assert_ne!(hidden.pixels(), original);
    doc.set_optional_content_state(&state).unwrap();
    assert_eq!(hidden.pixels(), pixels(&doc, 0, &config));
    state.reset();
    pool.set_optional_content(state.snapshot()).unwrap();
    assert_eq!(
        pool.render(0, config, Priority::Visible)
            .wait()
            .unwrap()
            .bitmap
            .pixels(),
        original
    );
    assert_eq!(pool.worker_count(), 2);
}

#[test]
fn explicit_default_edit_and_authored_group_membership_survive_save() {
    use pdfoxium::{Color, PathSpec, Rect, Size};
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
    let object = page
        .add_path(&PathSpec {
            fill: Some(Color::rgb(255, 0, 0)),
            ..PathSpec::rect(Rect {
                left: 10.,
                bottom: 10.,
                right: 90.,
                top: 90.,
            })
        })
        .unwrap();
    let layers = doc.optional_content().unwrap();
    let id = layers.create_group("Röda 道路").unwrap();
    assert_eq!(layers.groups().unwrap()[0].name, "Röda 道路");
    page.set_object_layer(object, Some(id)).unwrap();
    let config = RenderConfig::fit(100, 100);
    let shown = pixels(&doc, 0, &config);
    layers.set_visible(id, false).unwrap();
    let hidden = pixels(&doc, 0, &config);
    assert_ne!(shown, hidden);
    assert!(hidden.chunks_exact(4).all(|p| p == [255; 4]));
    layers.save_default(&layers.state().unwrap()).unwrap();
    assert!(doc.is_dirty());
    let mut saved = vec![];
    doc.save_to(&mut saved).unwrap();
    let reopened = Document::from_bytes(saved).unwrap();
    assert_eq!(pixels(&reopened, 0, &config), hidden);
    let layers = reopened.optional_content().unwrap();
    let id = layers.groups().unwrap()[0].id;
    layers.set_visible(id, true).unwrap();
    assert_eq!(pixels(&reopened, 0, &config), shown);
    let page = reopened.page(0).unwrap();
    let object = page.objects().unwrap()[0].id;
    page.set_object_layer(object, None).unwrap();
    layers.set_visible(id, false).unwrap();
    assert_eq!(pixels(&reopened, 0, &config), shown);
}

fn fixture(dictionary: &str, content: &str, membership: &str) -> Vec<u8> {
    fixture_with_page(dictionary, content, membership, "")
}
fn fixture_with_page(
    dictionary: &str,
    content: &str,
    membership: &str,
    page_extra: &str,
) -> Vec<u8> {
    let form_content = "0 0 1 rg 20 20 60 60 re f";
    let objects = [
        format!("<< /Type /Catalog /Pages 2 0 R /OCProperties << /OCGs [5 0 R 6 0 R] /D << /BaseState /ON /OFF [6 0 R] /Order [5 0 R 6 0 R] {dictionary} >> >> >>"),
        "<< /Type /Pages /Count 1 /Kids [3 0 R] >>".into(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources << /Properties << /A 5 0 R /B 6 0 R /M 7 0 R >> /XObject << /F 8 0 R >> >> /Contents 4 0 R {page_extra} >>"),
        format!("<< /Length {} >>\nstream\n{content}\nendstream",content.len()),
        "<< /Type /OCG /Name (A) >>".into(),
        "<< /Type /OCG /Name (B) >>".into(),
        format!("<< /Type /OCMD {membership} >>"),
        format!("<< /Type /XObject /Subtype /Form /BBox [0 0 100 100] /OC 6 0 R /Length {} >>\nstream\n{form_content}\nendstream",form_content.len()),
    ];
    let mut pdf = b"%PDF-1.7\n".to_vec();
    let mut offsets = vec![0];
    for (i, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", i + 1, object).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for offset in offsets.iter().skip(1) {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer << /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len()
        )
        .as_bytes(),
    );
    pdf
}
#[test]
fn membership_policies_expressions_nested_marks_and_xobject_oc() {
    let config = RenderConfig::fit(100, 100);
    for (membership, visible) in [
        ("/OCGs [5 0 R 6 0 R] /P /AnyOn", true),
        ("/OCGs [5 0 R 6 0 R] /P /AllOn", false),
        ("/OCGs [5 0 R 6 0 R] /P /AnyOff", true),
        ("/OCGs [5 0 R 6 0 R] /P /AllOff", false),
        ("/OCGs 6 0 R /P /AllOff", true),
        ("/OCGs 5 0 R /P /AnyOff", false),
        ("/VE [/And 5 0 R [/Not 6 0 R]] /P /AllOff", true),
        ("/VE [/Or [/Not 5 0 R] 6 0 R]", false),
    ] {
        let doc = Document::from_bytes(fixture(
            "",
            "/OC /M BDC 1 0 0 rg 10 10 80 80 re f EMC",
            membership,
        ))
        .unwrap();
        let rendered = pixels(&doc, 0, &config);
        assert_eq!(
            rendered.chunks_exact(4).any(|p| p != [255; 4]),
            visible,
            "{membership}"
        );
    }
    let doc = Document::from_bytes(fixture(
        "",
        "/OC /A BDC /OC /B BDC 1 0 0 rg 10 10 80 80 re f EMC EMC /F Do",
        "/OCGs 5 0 R",
    ))
    .unwrap();
    assert!(pixels(&doc, 0, &config)
        .chunks_exact(4)
        .all(|p| p == [255; 4]));
    let layers = doc.optional_content().unwrap();
    layers
        .set_visible(layers.groups().unwrap()[1].id, true)
        .unwrap();
    assert!(pixels(&doc, 0, &config)
        .chunks_exact(4)
        .any(|p| p != [255; 4]));
    assert_eq!(
        &pixels(&doc, 0, &config)[4 * (50 * 100 + 50)..][..4],
        &[255, 0, 0, 255]
    );
}
#[test]
fn hidden_content_keeps_clipping_and_text_graphics_state() {
    let doc = Document::from_bytes(fixture(
        "",
        "/OC /B BDC 0 0 50 100 re W n 1 0 0 rg EMC 0 0 100 100 re f",
        "",
    ))
    .unwrap();
    let bitmap = doc
        .page(0)
        .unwrap()
        .render(&RenderConfig::fit(100, 100))
        .unwrap();
    assert_eq!(
        &bitmap.pixels()[4 * (50 * 100 + 25)..][..4],
        &[0, 0, 255, 255]
    );
    assert_eq!(&bitmap.pixels()[4 * (50 * 100 + 75)..][..4], &[255; 4]);
}

#[test]
fn importing_pages_and_xobjects_registers_layers_and_preserves_saved_visibility() {
    use pdfoxium::Size;
    let source = Document::from_bytes(bytes()).unwrap();
    let mut saved = source.optional_content_state().unwrap();
    saved.set_visible(saved.groups()[0].id, false).unwrap();
    source
        .optional_content()
        .unwrap()
        .save_default(&saved)
        .unwrap();
    let config = RenderConfig::fit(300, 320);
    let expected = pixels(&source, 0, &config);
    let target = Document::create().unwrap();
    target.import_pages(&source, &[0], 0).unwrap();
    assert_eq!(
        target.optional_content().unwrap().groups().unwrap().len(),
        4
    );
    assert_eq!(pixels(&target, 0, &config), expected);
    // Import into an already layered destination and keep the old groups.
    target.import_pages(&source, &[1], 1).unwrap();
    assert_eq!(
        target.optional_content().unwrap().groups().unwrap().len(),
        8
    );
    assert_eq!(pixels(&target, 0, &config), expected);
    assert_eq!(pixels(&target, 1, &config), pixels(&source, 1, &config));
    let mut imported = target.optional_content_state().unwrap();
    assert_eq!(imported.configurations().len(), 3);
    imported
        .apply_configuration(imported.configurations()[2].id)
        .unwrap();
    assert_eq!(
        imported
            .groups()
            .iter()
            .skip(4)
            .map(|g| g.visible)
            .collect::<Vec<_>>(),
        vec![false, true, false, true]
    );
    let output = Document::create().unwrap();
    let page = output
        .insert_blank_page(
            0,
            Size {
                width: 600.,
                height: 640.,
            },
        )
        .unwrap();
    let xobject = output.import_page_xobject(&source, 0).unwrap();
    page.insert_object(xobject.object().unwrap()).unwrap();
    assert_eq!(
        output.optional_content().unwrap().groups().unwrap().len(),
        4
    );
    assert_eq!(pixels(&output, 0, &config), expected);
    let mut bytes = vec![];
    output.save_to(&mut bytes).unwrap();
    assert_eq!(
        pixels(&Document::from_bytes(bytes).unwrap(), 0, &config),
        expected
    );
}
#[test]
fn sample_set_ocg_state_action_changes_visibility_without_a_pdf_edit() {
    let doc = Document::from_bytes(bytes()).unwrap();
    let links = doc.page(0).unwrap().links().unwrap();
    let Some(pdfoxium::LinkTarget::OptionalContent(action)) = &links[0].target else {
        panic!("missing layer action")
    };
    let mut state = doc.optional_content_state().unwrap();
    assert!(state.apply_action(action).unwrap());
    assert!(!state.groups()[0].visible);
    doc.set_optional_content_state(&state).unwrap();
    assert!(!doc.is_dirty());
    assert!(doc
        .optional_content()
        .unwrap()
        .groups()
        .unwrap()
        .iter()
        .skip(1)
        .all(|g| g.visible));
}

#[test]
fn catalog_edits_reject_stale_snapshots_and_page_usage_only_reports_painted_resources() {
    let doc = Document::from_bytes(fixture(
        "/ListMode /VisiblePages",
        "/OC /M BDC 1 0 0 rg 0 0 100 100 re f EMC",
        "/VE [/And 5 0 R [/Not 6 0 R]]",
    ))
    .unwrap();
    let state = doc.optional_content_state().unwrap();
    assert!(!state.lists_all_pages());
    assert_eq!(
        doc.page(0).unwrap().optional_content_groups().unwrap(),
        state.groups().iter().map(|g| g.id).collect::<Vec<_>>()
    );
    let unused = Document::from_bytes(fixture(
        "/ListMode /VisiblePages",
        "/OC /A BDC 1 0 0 rg 0 0 100 100 re f EMC",
        "",
    ))
    .unwrap();
    assert_eq!(
        unused
            .page(0)
            .unwrap()
            .optional_content_groups()
            .unwrap()
            .len(),
        1
    );
    let snapshot = state.snapshot();
    doc.optional_content().unwrap().create_group("New").unwrap();
    assert!(doc
        .page(0)
        .unwrap()
        .render(&RenderConfig::fit(100, 100).optional_content(snapshot))
        .is_err());
    assert!(doc.set_optional_content_state(&state).is_err());
}
#[test]
fn form_owner_dispatches_pdf_layer_action_and_publishes_transient_change() {
    use pdfoxium::{
        thread::{DocEvent, DocOptions, DocumentThread},
        FormOptions,
    };
    let source = fixture_with_page(
        "",
        "/OC /A BDC 1 0 0 rg 0 0 100 100 re f EMC",
        "",
        "/AA << /O << /S /SetOCGState /State [/Toggle 5 0 R] >> >>",
    );
    let worker = DocumentThread::spawn(
        Source::Bytes(source.into()),
        Config::default(),
        DocOptions {
            forms: FormOptions::default(),
            ..Default::default()
        },
    )
    .unwrap();
    worker
        .call(|_, form| form.page_action(0, true))
        .wait()
        .unwrap();
    let state = worker
        .call(|doc, _| doc.optional_content_state())
        .wait()
        .unwrap();
    assert!(!state.groups()[0].visible);
    assert!(worker
        .take_events()
        .iter()
        .any(|e| matches!(e, DocEvent::OptionalContent(_))));
    assert!(!worker.call(|doc, _| Ok(doc.is_dirty())).wait().unwrap());
}

#[test]
fn annotation_visibility_renders_and_saves_with_indirect_group_membership() {
    use pdfoxium::{AnnotationSpec, Color, Rect, Size};
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
    let layer = doc
        .optional_content()
        .unwrap()
        .create_group("Annotations")
        .unwrap();
    let annot = page
        .add_annotation(
            AnnotationSpec::square(Rect {
                left: 10.,
                bottom: 10.,
                right: 90.,
                top: 90.,
            })
            .fill(Color::rgb(255, 0, 0)),
        )
        .unwrap();
    page.set_annotation_layer(annot, Some(layer)).unwrap();
    let config = RenderConfig::fit(100, 100);
    let shown = pixels(&doc, 0, &config);
    assert!(page.annotation_layer_visible(annot).unwrap());
    doc.optional_content()
        .unwrap()
        .set_visible(layer, false)
        .unwrap();
    assert!(!page.annotation_layer_visible(annot).unwrap());
    let hidden = pixels(&doc, 0, &config);
    assert!(hidden.chunks_exact(4).all(|p| p == [255; 4]));
    assert_ne!(shown, hidden);
    doc.optional_content()
        .unwrap()
        .save_default(&doc.optional_content_state().unwrap())
        .unwrap();
    let mut saved = vec![];
    doc.save_to(&mut saved).unwrap();
    let reopened = Document::from_bytes(saved).unwrap();
    assert_eq!(pixels(&reopened, 0, &config), hidden);
    reopened
        .optional_content()
        .unwrap()
        .set_visible(
            reopened.optional_content().unwrap().groups().unwrap()[0].id,
            true,
        )
        .unwrap();
    assert_eq!(pixels(&reopened, 0, &config), shown);
    page.set_annotation_layer(annot, None).unwrap();
    assert_eq!(pixels(&doc, 0, &config), shown);
}
#[test]
fn progressive_render_retains_its_snapshot_and_blocks_live_mutation() {
    let doc = Document::from_bytes(bytes()).unwrap();
    let page = doc.page(0).unwrap();
    let layers = doc.optional_content().unwrap();
    let mut state = layers.state().unwrap();
    let config = RenderConfig::fit(200, 220).optional_content(state.snapshot());
    let expected = pixels(&doc, 0, &config);
    let mut job = page.render_job(&config).unwrap();
    state.set_visible(state.groups()[0].id, false).unwrap();
    assert!(matches!(
        layers.set_visible(state.groups()[0].id, false),
        Err(pdfoxium::Error::Busy)
    ));
    while job.resume(&mut || false).unwrap() == pdfoxium::Progress::Paused {}
    let pdfoxium::RenderOutput::Owned(bitmap) = job.finish().unwrap() else {
        panic!("owned render")
    };
    assert_eq!(bitmap.pixels(), expected);
}

#[test]
fn creating_layers_declares_the_required_pdf_version() {
    let mut source = fixture("", "", "");
    source[7] = b'4'; // Same-length PDF 1.4 header keeps xref offsets valid.
    let doc = Document::from_bytes(source).unwrap();
    assert_eq!(doc.version().unwrap().minor, 4);
    doc.optional_content()
        .unwrap()
        .create_group("New layer")
        .unwrap();
    assert_eq!(doc.version().unwrap().minor, 5);
    let mut bytes = vec![];
    doc.save_to(&mut bytes).unwrap();
    assert_eq!(
        Document::from_bytes(bytes)
            .unwrap()
            .version()
            .unwrap()
            .minor,
        5
    );
}
