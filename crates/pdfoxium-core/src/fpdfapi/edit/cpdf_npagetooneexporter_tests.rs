// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::*;
use crate::fpdfsdk::{fpdf_editpage, fpdf_ppo};
fn new_doc() -> Rc<RefCell<CpdfDocument>> {
    fpdf_editpage::create_new_document(false, || panic!("clock disabled"))
}
fn raw(stream: ObjectRef) -> Vec<u8> {
    CpdfRetainedStreamAcc::load_raw(stream)
        .unwrap()
        .get_span()
        .to_vec()
}
#[test]
fn nup_slots_run_left_to_right_top_to_bottom_and_center_with_uniform_scale() {
    let mut state = NupState::new(CfxSizeF::new(600., 600.), 2, 2);
    for (x, y) in [(0., 375.), (300., 375.), (0., 75.), (300., 75.), (0., 375.)] {
        let s = state.calculate_new_page_position(CfxSizeF::new(200., 100.));
        assert_eq!(s.scale, 1.5);
        assert_eq!(s.sub_page_start_point, CfxPointF::new(x, y));
    }
    let s = state.calculate_new_page_position(CfxSizeF::new(100., 200.));
    assert_eq!(s.scale, 1.5);
    assert_eq!(s.sub_page_start_point, CfxPointF::new(375., 300.));
}
#[test]
fn xobject_joins_filtered_array_streams_keeps_page_geometry_and_uses_weak_destination() {
    let source = new_doc();
    let page = fpdf_editpage::new_page(&source, 0, 200., 300.).unwrap();
    let holder = source.borrow().holder();
    let a = holder
        .borrow_mut()
        .new_indirect_stream_from_bytes(b"1 0 0 rg");
    let b = holder
        .borrow_mut()
        .new_indirect_stream_from_bytes(b"10 20 30 40 re f");
    let mut array = CpdfArray::new();
    for object in [&a, &b] {
        array.append_new(CpdfReference::new(&holder, object.borrow().obj_num()));
    }
    array.append_new(CpdfScalarObject::null());
    dict_mut(&page.dict(), |d| {
        d.set_new_for(b"Contents", array);
        d.set_new_for(b"Rotate", CpdfScalarObject::number_from_signed(90));
    });
    let dest = new_doc();
    let context = CpdfNPageToOneExporter::new(Rc::clone(&dest), Rc::clone(&source))
        .create_xobject_context_from_page(0)
        .unwrap();
    drop(page);
    drop(holder);
    drop(source);
    assert_eq!(
        raw(Rc::clone(&context.xobject)),
        b"1 0 0 rg\n10 20 30 40 re f\n\n"
    );
    let dictionary = context
        .xobject
        .borrow()
        .as_any()
        .downcast_ref::<CpdfStream>()
        .unwrap()
        .get_dict();
    assert_eq!(
        dict(&dictionary, |d| d.get_rect_for(b"BBox")),
        CfxFloatRect::new(0., 0., 200., 300.)
    );
    assert_eq!(
        dict(&dictionary, |d| d.get_matrix_for(b"Matrix")),
        CfxMatrix::new(0., -1., 1., 0., 0., 200.)
    );
    let mut form = fpdf_ppo::new_form_object_from_xobject(&context);
    form.calc_bounding_box();
    assert_eq!(form.form().holder().page_object_count(), 1);
    drop(dest);
    assert!(context.dest_doc.upgrade().is_none());
}
#[test]
fn nup_overflow_and_bad_source_keep_partial_mutations_and_rebind_cached_forms_per_sheet() {
    let source = new_doc();
    fpdf_editpage::new_page(&source, 0, 100., 100.).unwrap();
    let dest = new_doc();
    let mut exporter = CpdfNPageToOneExporter::new(Rc::clone(&dest), Rc::clone(&source));
    assert!(!exporter.export_n_pages_to_one(&[0], CfxSizeF::new(200., 100.), usize::MAX, 2));
    assert_eq!(dest.borrow().page_count(), 0);
    assert!(!exporter.export_n_pages_to_one(&[0, 99], CfxSizeF::new(200., 100.), 2, 1));
    assert_eq!(dest.borrow().page_count(), 1);
    let failed = dest.borrow_mut().page_dictionary(0).unwrap();
    assert!(!dict(&failed, |d| d.key_exist(b"Contents")));
    let dest = new_doc();
    assert!(
        CpdfNPageToOneExporter::new(Rc::clone(&dest), Rc::clone(&source)).export_n_pages_to_one(
            &[0, 0, 0],
            CfxSizeF::new(200., 100.),
            2,
            1
        )
    );
    let first = dest.borrow_mut().page_dictionary(0).unwrap();
    let second = dest.borrow_mut().page_dictionary(1).unwrap();
    let xobjects = |p: &ObjectRef| {
        let r = dict(p, |d| d.get_dict_for(b"Resources")).unwrap();
        dict(&r, |d| d.get_dict_for(b"XObject")).unwrap()
    };
    assert_eq!(dict(&xobjects(&first), |d| d.size()), 1);
    // Reusing a form on a later sheet must retain its resource binding.
    assert_eq!(dict(&xobjects(&second), |d| d.size()), 1);
    let first_form = dict(&xobjects(&first), |d| d.get_stream_for(b"X1")).unwrap();
    let second_form = dict(&xobjects(&second), |d| d.get_stream_for(b"X1")).unwrap();
    assert!(Rc::ptr_eq(&first_form, &second_form));
    let page = source.borrow_mut().page_dictionary(0).unwrap();
    dict_mut(&page, |d| d.set_new_name_for(b"Type", b"NotAPage"));
    let mut exporter = CpdfNPageToOneExporter::new(Rc::clone(&dest), source);
    assert!(exporter.create_xobject_context_from_page(0).is_none());
    assert!(!exporter.export_n_pages_to_one(&[0], CfxSizeF::new(200., 100.), 2, 1));
    for (size, x, y) in [
        (CfxSizeF::new(f32::NAN, 100.), 2, 1),
        (CfxSizeF::new(200., 100.), 0, 1),
    ] {
        assert!(!exporter.export_n_pages_to_one(&[0], size, x, y));
    }
}
#[test]
#[allow(non_snake_case)]
fn ImportNPages_and_Bug925981() {
    fn open(name: &str) -> Rc<RefCell<CpdfDocument>> {
        use crate::fpdfapi::parser::cpdf_parser::ParserError;
        use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;
        let data = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../upstream/pdfium/testing/resources")
                .join(name),
        )
        .unwrap();
        let mut d = CpdfDocument::new();
        assert_eq!(
            d.load_doc(Rc::new(CfxReadOnlyContainerStream::new(data)), b""),
            ParserError::Success
        );
        Rc::new(RefCell::new(d))
    }
    let source = open("rectangles_multi_pages.pdf");
    for (w, h, x, y, count) in [
        (612., 792., 2, 1, 3),
        (612., 792., 5, 1, 1),
        (792., 612., 8, 1, 1),
        (792., 612., 128, 1, 1),
    ] {
        let dest = new_doc();
        assert!(fpdf_ppo::import_n_pages_to_one(
            Rc::clone(&dest),
            Rc::clone(&source),
            w,
            h,
            x,
            y
        ));
        assert_eq!(dest.borrow().page_count(), count);
    }
    let source = open("bug_925981.pdf");
    let dest = new_doc();
    assert!(fpdf_ppo::import_n_pages_to_one(
        Rc::clone(&dest),
        source,
        612.,
        792.,
        2,
        1
    ));
    assert_eq!(dest.borrow().page_count(), 1);
}
