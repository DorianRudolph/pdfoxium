// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use pdfoxium::*;
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
fn rectangle() -> PathSpec {
    PathSpec {
        fill: Some(Color::rgb(255, 0, 0)),
        ..PathSpec::rect(Rect {
            left: 10.,
            bottom: 10.,
            right: 50.,
            top: 50.,
        })
    }
}
fn pixels(p: &Page) -> Vec<u8> {
    p.render(&RenderConfig::fit(100, 100))
        .unwrap()
        .into_pixels()
}
#[test]
fn recover_replace_extend_path_and_graphics_state() {
    let d = Document::create().unwrap();
    let p = page(&d);
    let id = p.add_path(&rectangle()).unwrap();
    let path = p.path(id).unwrap();
    assert_eq!(path.commands.len(), 5);
    assert_eq!(path.segments.len(), 4);
    assert!(path.segments[3].close);
    p.set_path_commands(
        id,
        &[
            PathCommand::MoveTo(Point { x: 10., y: 10. }),
            PathCommand::CurveTo(
                Point { x: 20., y: 80. },
                Point { x: 70., y: 80. },
                Point { x: 80., y: 10. },
            ),
            PathCommand::Close,
        ],
    )
    .unwrap();
    assert_eq!(
        p.path(id).unwrap().segments[1].kind,
        PathSegmentKind::Bezier
    );
    p.set_path_draw_mode(id, None, true).unwrap();
    p.set_object_stroke_color(id, Color::rgb(0, 0, 255))
        .unwrap();
    p.set_object_stroke_width(id, 3.).unwrap();
    p.set_object_line_cap(id, LineCap::Round).unwrap();
    p.set_object_line_join(id, LineJoin::Bevel).unwrap();
    p.set_object_dash(id, &[4., 2.], 1.).unwrap();
    let style = p.object_style(id).unwrap();
    assert_eq!(style.dash_array, vec![4., 2.]);
    assert_eq!(style.line_cap, LineCap::Round);
    assert_eq!(style.line_join, LineJoin::Bevel);
    assert!(p.set_object_dash(id, &[0.], 0.).is_err());
    assert!(p.object_pattern(id, false).unwrap().is_none());
    let before = pixels(&p);
    p.set_object_active(id, false).unwrap();
    assert_ne!(pixels(&p), before);
    p.set_object_active(id, true).unwrap();
    assert_eq!(pixels(&p), before);
    let mut b = vec![];
    d.save_to(&mut b).unwrap();
    assert_eq!(
        pixels(&Document::from_bytes(b.clone()).unwrap().page(0).unwrap()),
        before
    );
    if let Some(dir) = std::env::var_os("PDFOXIUM_EDIT_ARTIFACT_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(std::path::Path::new(&dir).join("path-details.pdf"), b).unwrap();
    }
}
#[test]
fn nested_paths_are_editable_and_removed_ids_stay_stale() {
    let source = Document::create().unwrap();
    let sp = page(&source);
    sp.add_path(&rectangle()).unwrap();
    let doc = Document::create().unwrap();
    let p = page(&doc);
    let x = doc.import_page_xobject(&source, 0).unwrap();
    let form = p.insert_object(x.object().unwrap()).unwrap();
    let child = p.form_objects(form).unwrap()[0].id;
    assert!(p.path(child).is_ok());
    p.set_object_fill_color(child, Color::rgb(0, 255, 0))
        .unwrap();
    p.transform_object(child, Matrix::translate(20., 0.))
        .unwrap();
    let expected = pixels(&p);
    let mut bytes = vec![];
    doc.save_to(&mut bytes).unwrap();
    assert_eq!(
        pixels(&Document::from_bytes(bytes).unwrap().page(0).unwrap()),
        expected
    );
    let detached = p.take_object(child).unwrap();
    assert!(matches!(p.object(child), Err(Error::Stale)));
    let fresh = p.insert_object(detached).unwrap();
    assert!(p.path(fresh).is_ok());
    assert!(p.form_objects(form).unwrap().is_empty());
    p.remove_object(form).unwrap();
    assert!(matches!(p.object(form), Err(Error::Stale)));
}
#[test]
fn marks_parameters_and_owned_transfer() {
    let d = Document::create().unwrap();
    let p = page(&d);
    let id = p.add_path(&rectangle()).unwrap();
    let mark = p.add_object_mark(id, b"Span").unwrap();
    p.set_object_mark_parameter(id, &mark, b"MCID", MarkValue::Integer(7))
        .unwrap();
    p.set_object_mark_parameter(id, &mark, b"Label", MarkValue::String(b"hello".to_vec()))
        .unwrap();
    p.set_object_mark_parameter(id, &mark, b"Blob", MarkValue::Blob(vec![0, 255]))
        .unwrap();
    assert_eq!(p.object_marked_content_id(id).unwrap(), 7);
    assert_eq!(mark.name(), b"Span");
    assert_eq!(mark.parameters().unwrap().len(), 3);
    assert!(p.remove_object_mark_parameter(id, &mark, b"Blob").unwrap());
    let mut saved = vec![];
    d.save_to(&mut saved).unwrap();
    let reopened = Document::from_bytes(saved).unwrap();
    let rp = reopened.page(0).unwrap();
    assert_eq!(
        rp.object_marks(rp.objects().unwrap()[0].id).unwrap()[0]
            .parameters()
            .unwrap()
            .len(),
        2
    );
    p.remove_object_mark(id, &mark).unwrap();
    assert!(matches!(
        p.set_object_mark_parameter(id, &mark, b"X", MarkValue::Integer(1)),
        Err(Error::Stale)
    ));
    let mut object = p.take_object(id).unwrap();
    object.transform(Matrix::translate(30., 0.)).unwrap();
    let q = page(&d);
    let new = q.insert_object(object).unwrap();
    assert_eq!(q.object(new).unwrap().bounds.left, 40.);
}
#[test]
fn editing_one_form_instance_preserves_other_instance() {
    let source = Document::create().unwrap();
    let sp = page(&source);
    sp.add_path(&rectangle()).unwrap();
    let doc = Document::create().unwrap();
    let p = page(&doc);
    let q = page(&doc);
    let x = doc.import_page_xobject(&source, 0).unwrap();
    let a = p.insert_object(x.object().unwrap()).unwrap();
    q.insert_object(x.object().unwrap()).unwrap();
    let child = p.form_objects(a).unwrap()[0].id;
    p.set_object_fill_color(child, Color::rgb(0, 255, 0))
        .unwrap();
    let before = [pixels(&p), pixels(&q)];
    let mut bytes = vec![];
    doc.save_to(&mut bytes).unwrap();
    let reopened = Document::from_bytes(bytes).unwrap();
    for i in 0..2 {
        assert!(
            pixels(&reopened.page(i).unwrap()) == before[i],
            "page {i} changed after reopen"
        );
    }
}

#[test]
fn shared_mark_parameters_survive_save_on_every_owner() {
    let d = Document::create().unwrap();
    let p = page(&d);
    let q = page(&d);
    let a = p.add_path(&rectangle()).unwrap();
    let b = q.add_path(&rectangle()).unwrap();
    let mark = p.add_object_mark(a, b"Span").unwrap();
    p.set_object_mark_parameter(a, &mark, b"MCID", MarkValue::Integer(1))
        .unwrap();
    q.add_existing_object_mark(b, &mark).unwrap();
    d.save_to(&mut Vec::new()).unwrap();
    p.set_object_mark_parameter(a, &mark, b"MCID", MarkValue::Integer(2))
        .unwrap();
    assert_eq!(q.object_marked_content_id(b).unwrap(), 2);
    let mut bytes = vec![];
    d.save_to(&mut bytes).unwrap();
    let reopened = Document::from_bytes(bytes).unwrap();
    for i in 0..2 {
        let page = reopened.page(i).unwrap();
        assert_eq!(
            page.object_marked_content_id(page.objects().unwrap()[0].id)
                .unwrap(),
            2
        );
    }
}

#[test]
fn nested_nested_form_edits_isolate_each_level_and_busy_shared_marks() {
    let source = Document::create().unwrap();
    let sp = page(&source);
    sp.add_path(&rectangle()).unwrap();
    let middle = Document::create().unwrap();
    let mp = page(&middle);
    mp.insert_object(
        middle
            .import_page_xobject(&source, 0)
            .unwrap()
            .object()
            .unwrap(),
    )
    .unwrap();
    let doc = Document::create().unwrap();
    let p = page(&doc);
    let q = page(&doc);
    let x = doc.import_page_xobject(&middle, 0).unwrap();
    let a = p.insert_object(x.object().unwrap()).unwrap();
    let b = q.insert_object(x.object().unwrap()).unwrap();
    let inner = p.form_objects(a).unwrap()[0].id;
    let path = p.form_objects(inner).unwrap()[0].id;
    p.set_object_fill_color(path, Color::rgb(0, 0, 255))
        .unwrap();
    let before = [pixels(&p), pixels(&q)];
    let mut saved = vec![];
    doc.save_to(&mut saved).unwrap();
    let reopened = Document::from_bytes(saved.clone()).unwrap();
    for i in 0..2 {
        assert!(pixels(&reopened.page(i).unwrap()) == before[i]);
    }
    let mark = p.add_object_mark(path, b"Span").unwrap();
    q.add_existing_object_mark(b, &mark).unwrap();
    let job = q.render_job(&RenderConfig::default()).unwrap();
    assert!(matches!(
        p.set_object_mark_parameter(path, &mark, b"MCID", MarkValue::Integer(3)),
        Err(Error::Busy)
    ));
    drop(job);
    assert!(mark.parameters().unwrap().is_empty());
    assert!(matches!(
        p.move_object_to(a, &q),
        Err(Error::Unsupported(_))
    ));
    assert!(p.object(a).is_ok());
    if let Some(dir) = std::env::var_os("PDFOXIUM_EDIT_ARTIFACT_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(std::path::Path::new(&dir).join("nested-forms.pdf"), saved).unwrap();
    }
}

#[test]
fn named_property_marks_rebind_and_integer_precision_is_preserved() {
    let contents = b"/Span /P BDC 10 10 30 30 re f EMC";
    let objects=vec![b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources << /Properties << /P << /MCID 16777217 /Label (hi) >> >> >> /Contents 4 0 R >>".to_vec(),[format!("<< /Length {} >>\nstream\n",contents.len()).as_bytes(),contents,b"\nendstream"].concat()];
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = vec![];
    for (i, o) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend(format!("{} 0 obj\n", i + 1).bytes());
        bytes.extend(o);
        bytes.extend(b"\nendobj\n");
    }
    let xref = bytes.len();
    bytes.extend(b"xref\n0 5\n0000000000 65535 f \n");
    for offset in offsets {
        bytes.extend(format!("{offset:010} 00000 n \n").bytes());
    }
    bytes.extend(format!("trailer\n<< /Size 5 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").bytes());
    let doc = Document::from_bytes(bytes).unwrap();
    let p = doc.page(0).unwrap();
    let a = p.objects().unwrap()[0].id;
    let mark = p.object_marks(a).unwrap().remove(0);
    assert!(mark
        .parameters()
        .unwrap()
        .iter()
        .any(|p| p.key == b"MCID" && p.value == MarkValue::Integer(16777217)));
    let q = page(&doc);
    let b = q.add_path(&rectangle()).unwrap();
    q.add_existing_object_mark(b, &mark).unwrap();
    let mut saved = vec![];
    doc.save_to(&mut saved).unwrap();
    let reopened = Document::from_bytes(saved).unwrap();
    for index in 0..2 {
        let page = reopened.page(index).unwrap();
        let object = page.objects().unwrap()[0].id;
        assert_eq!(page.object_marked_content_id(object).unwrap(), 16777217);
    }
    let detached = p.take_object(a).unwrap();
    assert!(matches!(
        q.insert_object(detached),
        Err(Error::Unsupported(_))
    ));
}

#[test]
fn detached_geometry_style_and_shared_marks_cover_edit_callbacks() {
    let doc = Document::create().unwrap();
    let p = page(&doc);
    let id = p.add_path(&rectangle()).unwrap();
    assert_eq!(p.object_count().unwrap(), 1);
    assert_eq!(p.object_at(0).unwrap().unwrap().id, id);
    assert!(p.object_at(1).unwrap().is_none());
    let mut object = p.take_object(id).unwrap();
    p.unload().unwrap();
    object
        .set_path_commands(&[
            PathCommand::MoveTo(Point { x: 20., y: 20. }),
            PathCommand::LineTo(Point { x: 70., y: 20. }),
            PathCommand::LineTo(Point { x: 45., y: 70. }),
            PathCommand::Close,
        ])
        .unwrap();
    object
        .set_path_draw_mode(Some(FillRule::EvenOdd), true)
        .unwrap();
    object.set_line_cap(LineCap::Round).unwrap();
    object.set_line_join(LineJoin::Bevel).unwrap();
    object.set_dash(&[3., 2.], 1.).unwrap();
    object.set_dash_phase(2.).unwrap();
    object.set_blend_mode("Multiply").unwrap();
    assert_eq!(object.path_segments().unwrap().len(), 3);
    assert_eq!(object.style().dash_phase, 2.);
    let mark = object.add_mark(b"Span").unwrap();
    object
        .set_mark_parameter(&mark, b"MCID", MarkValue::Integer(7))
        .unwrap();
    assert_eq!(object.marked_content_id(), 7);
    let q = page(&doc);
    let other = q.add_path(&rectangle()).unwrap();
    q.add_existing_object_mark(other, &mark).unwrap();
    object
        .set_mark_parameter(&mark, b"MCID", MarkValue::Integer(9))
        .unwrap();
    assert_eq!(q.object_marked_content_id(other).unwrap(), 9);
    object.remove_mark(&mark).unwrap();
    p.insert_object_at(0, object).unwrap();
    let before = pixels(&p);
    let mut saved = vec![];
    doc.save_to(&mut saved).unwrap();
    let reopened = Document::from_bytes(saved).unwrap();
    assert!(pixels(&reopened.page(0).unwrap()) == before);
}

fn multistream_document() -> Document {
    let red = b"1 0 0 rg 10 10 60 60 re f";
    let blue = b"0 0 1 rg 10 10 60 60 re f";
    let objects=vec![b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources << >> /Contents [4 0 R 5 0 R] >>".to_vec(),[format!("<< /Length {} >>\nstream\n",red.len()).as_bytes(),red,b"\nendstream"].concat(),[format!("<< /Length {} >>\nstream\n",blue.len()).as_bytes(),blue,b"\nendstream"].concat()];
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = vec![];
    for (i, o) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend(format!("{} 0 obj\n", i + 1).bytes());
        bytes.extend(o);
        bytes.extend(b"\nendobj\n");
    }
    let xref = bytes.len();
    bytes.extend(b"xref\n0 6\n0000000000 65535 f \n");
    for offset in offsets {
        bytes.extend(format!("{offset:010} 00000 n \n").bytes());
    }
    bytes.extend(format!("trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").bytes());
    Document::from_bytes(bytes).unwrap()
}
#[test]
fn arbitrary_paint_order_across_original_streams_survives_generation() {
    let doc = multistream_document();
    let p = doc.page(0).unwrap();
    let originals = p.objects().unwrap();
    let green = p
        .add_path(&PathSpec {
            fill: Some(Color::rgb(0, 255, 0)),
            ..PathSpec::rect(Rect {
                left: 10.,
                bottom: 10.,
                right: 70.,
                top: 70.,
            })
        })
        .unwrap();
    p.reorder_object(green, 0).unwrap();
    let live = pixels(&p);
    let mut saved = vec![];
    doc.save_to(&mut saved).unwrap();
    assert!(
        pixels(&Document::from_bytes(saved).unwrap().page(0).unwrap()) == live,
        "new object moved behind originals changed order on reopen"
    );
    p.reorder_object(originals[0].id, 1).unwrap();
    let live = pixels(&p);
    let mut saved = vec![];
    doc.save_to(&mut saved).unwrap();
    assert!(
        pixels(&Document::from_bytes(saved).unwrap().page(0).unwrap()) == live,
        "original object moved across streams changed order on reopen"
    );
    let other = page(&doc);
    let temporary = other
        .add_path(&PathSpec {
            fill: Some(Color::rgb(255, 255, 0)),
            ..rectangle()
        })
        .unwrap();
    let detached = other.take_object(temporary).unwrap();
    p.insert_object_at(1, detached).unwrap();
    let live = pixels(&p);
    let mut saved = vec![];
    doc.save_to(&mut saved).unwrap();
    assert!(
        pixels(&Document::from_bytes(saved).unwrap().page(0).unwrap()) == live,
        "indexed insertion changed order on reopen"
    );
}
