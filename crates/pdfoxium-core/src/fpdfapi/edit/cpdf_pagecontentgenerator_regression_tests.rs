// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Observable generation, CTM, resource-sharing and restoration contracts.

use super::*;
use crate::fpdfapi::page::cpdf_contentmarks::CpdfContentMarks;
use crate::fpdfapi::page::cpdf_image::CpdfImage;
use crate::fpdfapi::page::cpdf_imageobject::CpdfImageObject;
use crate::fpdfapi::page::cpdf_shadingobject::CpdfShadingObject;
use crate::fpdfapi::page::cpdf_shadingpattern::CpdfShadingPattern;
use crate::fpdfapi::parser::cpdf_object::retain_object;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fpdfapi::parser::cpdf_stream_acc::CpdfStreamAcc;

fn document_and_page() -> (CpdfDocument, CpdfPage) {
    let mut document = CpdfDocument::new();
    document.create_new_doc();
    document.create_new_page(0).unwrap();
    let page = CpdfPage::from_document(&mut document, 0).unwrap();
    (document, page)
}

fn rectangle(stream: i32) -> CpdfPathObject {
    let mut path = CpdfPathObject::new(stream);
    path.mutable_path().append_rect(10.0, 5.0, 13.0, 30.0);
    path.set_filltype(FillType::Winding);
    path.base_mut().set_dirty(true);
    path
}

fn generate(document: &mut CpdfDocument, page: &mut CpdfPage) {
    CpdfPageContentGenerator::new(page.mutable_holder(), Some(document)).generate_content();
}

fn contents(page: &CpdfPage) -> Option<ObjectRef> {
    dict(page.holder().dict(), |d| d.get_stream_for(b"Contents"))
}

fn decoded(stream: &ObjectRef) -> Vec<u8> {
    let borrowed = stream.borrow();
    let stream = borrowed.as_any().downcast_ref::<CpdfStream>().unwrap();
    let mut access = CpdfStreamAcc::new(stream);
    access.load_all_data_filtered();
    access.get_span().to_vec()
}

#[test]
fn text_clip_layers_become_outlines_without_new_text_or_dangling_fonts() {
    let (mut document, mut page) = document_and_page();
    let font = CpdfFont::get_stock_font(&document, b"Helvetica").unwrap();
    let make_text = |bytes: &[u8], matrix| {
        let mut text = Box::new(CpdfTextObject::new(0));
        text.base_mut().set_default_states();
        text.base_mut().set_resource_name(b"ClipOnly");
        text.base_mut()
            .mutable_text_state()
            .set_font(Some(font.clone()), font.font_dict());
        text.base_mut().mutable_text_state().set_font_size(12.0);
        text.set_text(bytes);
        text.set_text_matrix(matrix);
        text
    };
    let mut path = rectangle(CpdfPageObject::NO_CONTENT_STREAM);
    path.base_mut().set_default_states();
    // Two glyph runs share one ET layer (union), then another ET layer
    // intersects them. No painted text object remains in the page holder.
    path.base_mut().mutable_clip_path().append_texts(&mut vec![
        make_text(b"A", CfxMatrix::new(1.0, 0.0, 0.0, 1.0, 20.0, 0.0)),
        make_text(b"B", CfxMatrix::default()),
    ]);
    path.base_mut()
        .mutable_clip_path()
        .append_texts(&mut vec![make_text(
            b"C",
            CfxMatrix::new(1.0, 0.0, 0.0, 1.0, 4.0, 0.0),
        )]);
    let resources = retain_object(CpdfDictionary::new());
    let fonts = retain_object(CpdfDictionary::new());
    dict_mut(&fonts, |d| {
        d.set_for(b"ClipOnly", font.font_dict());
    });
    dict_mut(&resources, |d| {
        d.set_for(b"Font", Some(Rc::clone(&fonts)));
    });
    dict_mut(page.holder().dict(), |d| {
        d.set_for(b"Resources", Some(Rc::clone(&resources)));
    });
    page.mutable_holder().set_resources(Some(resources));
    page.mutable_holder().append_page_object(Box::new(path));
    generate(&mut document, &mut page);
    let generated = String::from_utf8(decoded(&contents(&page).unwrap())).unwrap();
    assert_eq!(generated.matches(" W n ").count(), 2);
    assert!(!generated.contains("BT"));
    assert!(dict(&fonts, |d| d.get_object_for(b"ClipOnly")).is_none());
    let mut reloaded = CpdfPage::from_document(&mut document, 0).unwrap();
    reloaded.parse_path_content();
    assert_eq!(reloaded.holder().page_object_count(), 1);
    let path = reloaded.holder().page_object_by_index(0).unwrap();
    assert_eq!(path.object_type(), PageObjectType::Path);
    assert_eq!(path.base().clip_path().path_count(), 2);
    assert_eq!(path.base().clip_path().text_count(), 0);
    let union = path.base().clip_path().path(0).bounding_box();
    let intersection = path.base().clip_path().path(1).bounding_box();
    assert!(union.left < 2.0 && union.right > 20.0);
    assert!(intersection.left > 3.0 && intersection.right < 20.0);
    assert_eq!(
        crate::fpdftext::cpdf_textpage::CpdfTextPage::new(&reloaded, false).count_chars(),
        0
    );
}

#[test]
fn regenerated_shading_retains_its_resource_across_activation_changes() {
    let (mut document, mut page) = document_and_page();
    let shading_resource = document.holder().borrow_mut().new_indirect_dictionary();
    dict_mut(&shading_resource, |d| {
        d.set_new_for(b"ShadingType", CpdfScalarObject::number_from_signed(2));
        d.set_new_name_for(b"ColorSpace", b"DeviceRGB");
    });
    let resource_number = shading_resource.borrow().obj_num();
    let pattern =
        Rc::new(CpdfShadingPattern::new(shading_resource, true, CfxMatrix::default()).unwrap());
    let mut shading = CpdfShadingObject::new(
        CpdfPageObject::NO_CONTENT_STREAM,
        pattern,
        CfxMatrix::new(0.5, 0.0, 0.0, 0.75, 12.0, 16.0),
    );
    shading.base_mut().set_default_states();
    shading.base_mut().set_dirty(true);
    page.mutable_holder().append_page_object(Box::new(shading));
    for active in [true, false, true] {
        page.mutable_holder()
            .page_object_by_index_mut(0)
            .unwrap()
            .base_mut()
            .set_is_active(active);
        generate(&mut document, &mut page);
        if !active {
            assert!(contents(&page).is_none());
            continue;
        }
        let text = decoded(&contents(&page).unwrap());
        assert!(text.windows(4).any(|v| v == b" sh "));
        let matrix = b".5 0 0 .75 12 16 cm";
        assert!(text.windows(matrix.len()).any(|v| v == matrix));
        let name = page
            .holder()
            .page_object_by_index(0)
            .unwrap()
            .base()
            .resource_name();
        let resources = page.holder().resources().unwrap();
        let shadings = dict(resources, |d| d.get_dict_for(b"Shading")).unwrap();
        let retained = dict(&shadings, |d| d.get_direct_object_for(name)).unwrap();
        assert_eq!(retained.borrow().obj_num(), resource_number);
    }
}

#[test]
fn native_rectangle_activation_removal_and_reactivation_snapshot() {
    // Exact decoded /Contents verified with bundled native Chromium 8100:
    // NewDocument; Page_New(0,100,100); CreateNewRect(10,5,3,25);
    // SetFillColor(255,0,0,255); SetDrawMode(2,false); InsertObject;
    // GenerateContent; SetIsActive(false), GenerateContent; reactivate, generate.
    let (mut document, mut page) = document_and_page();
    let mut path = rectangle(CpdfPageObject::NO_CONTENT_STREAM);
    path.base_mut().set_default_states();
    path.base_mut().mutable_color_state().set_fill_color(
        crate::fpdfapi::page::cpdf_colorspace::get_stock_cs_for_name(b"DeviceRGB"),
        vec![1.0, 0.0, 0.0],
    );
    page.mutable_holder().append_page_object(Box::new(path));
    let expected =
        b"q\n0 0 0 RG 0 0 0 rg 1 w 0 J 0 j\n/FXE1 gs q 1 0 0 rg 0 0 0 RG 10 5 3 25 re f Q\nQ\n";
    generate(&mut document, &mut page);
    let original_stream = contents(&page).unwrap();
    assert_eq!(decoded(&original_stream), expected);
    assert!(!page
        .holder()
        .page_object_by_index(0)
        .unwrap()
        .base()
        .is_dirty());
    assert_eq!(
        page.holder()
            .page_object_by_index(0)
            .unwrap()
            .base()
            .content_stream(),
        0
    );

    // A clean regeneration preserves the existing content object itself.
    generate(&mut document, &mut page);
    assert!(Rc::ptr_eq(&original_stream, &contents(&page).unwrap()));
    page.mutable_holder()
        .page_object_by_index_mut(0)
        .unwrap()
        .base_mut()
        .set_is_active(false);
    generate(&mut document, &mut page);
    assert!(contents(&page).is_none());
    // The orphaned indirect stream is not the page's current /Contents.
    assert_eq!(decoded(&original_stream), expected);
    page.mutable_holder()
        .page_object_by_index_mut(0)
        .unwrap()
        .base_mut()
        .set_is_active(true);
    generate(&mut document, &mut page);
    let restored = contents(&page).unwrap();
    assert!(!Rc::ptr_eq(&original_stream, &restored));
    assert_eq!(decoded(&restored), expected);
}

#[test]
fn dirty_streams_preserve_ctm_effects_and_skip_unchanged_streams() {
    let (mut document, mut page) = document_and_page();
    let first = CfxMatrix::new(2.0, 0.0, 0.0, 2.0, 6.0, 8.0);
    let second = CfxMatrix::new(2.0, 0.0, 0.0, 2.0, 10.0, 12.0);
    let holder = page.mutable_holder();
    holder.start_parse();
    holder.finish_parse(BTreeMap::from([(0, first), (1, second), (2, second)]));
    for index in 0..3 {
        holder.mark_stream_dirty(index);
    }
    let mut unchanged = rectangle(3);
    unchanged.base_mut().set_dirty(false);
    holder.append_page_object(Box::new(unchanged));
    let mut generator = CpdfPageContentGenerator::new(holder, Some(&mut document));
    let streams = generator.generate_modified_streams();
    assert_eq!(streams.len(), 3);
    assert_eq!(
        streams[&0],
        b"q\n0 0 0 RG 0 0 0 rg 1 w 0 J 0 j\n/FXE1 gs Q\n2 0 0 2 6 8 cm\n"
    );
    assert_eq!(
        streams[&1],
        b"q\n.5 0 0 .5 -3 -4 cm\n0 0 0 RG 0 0 0 rg 1 w 0 J 0 j\n/FXE1 gs Q\n1 0 0 1 4 4 cm\n"
    );
    assert!(streams[&2].is_empty());
    assert!(!generator.holder().has_dirty_streams());
    assert!(generator.generate_modified_streams().is_empty());
}

#[test]
fn deactivating_and_reactivating_text_restores_the_same_font_resource() {
    let (mut document, mut page) = document_and_page();
    let font = CpdfFont::get_stock_font(&document, b"Helvetica").unwrap();
    let font_dict = font.font_dict().unwrap();
    let mut text = CpdfTextObject::new(CpdfPageObject::NO_CONTENT_STREAM);
    text.base_mut()
        .mutable_text_state()
        .set_font(Some(font), Some(font_dict));
    text.base_mut().mutable_text_state().set_font_size(12.0);
    text.set_text(b"resource identity");
    page.mutable_holder().append_page_object(Box::new(text));
    generate(&mut document, &mut page);
    let resource_name = page
        .holder()
        .page_object_by_index(0)
        .unwrap()
        .base()
        .resource_name()
        .to_vec();
    let fonts = dict(page.holder().resources().unwrap(), |d| {
        d.get_dict_for(b"Font")
    })
    .unwrap();
    let original = dict(&fonts, |d| d.get_dict_for(&resource_name)).unwrap();
    page.mutable_holder()
        .page_object_by_index_mut(0)
        .unwrap()
        .base_mut()
        .set_is_active(false);
    generate(&mut document, &mut page);
    assert!(!dict(&fonts, |d| d.key_exist(&resource_name)));
    assert!(
        page.mutable_holder().all_removed_resources_map()[b"Font".as_slice()]
            .contains_key(&resource_name)
    );
    page.mutable_holder()
        .page_object_by_index_mut(0)
        .unwrap()
        .base_mut()
        .set_is_active(true);
    generate(&mut document, &mut page);
    let restored = dict(&fonts, |d| d.get_dict_for(&resource_name)).unwrap();
    assert!(Rc::ptr_eq(&original, &restored));
    assert!(
        !page.mutable_holder().all_removed_resources_map()[b"Font".as_slice()]
            .contains_key(&resource_name)
    );
}

#[test]
fn removing_unused_resources_copies_shared_top_and_nested_dictionaries() {
    for (shared_top, inherited) in [(true, false), (false, false), (true, true)] {
        let (mut document, _) = document_and_page();
        document.create_new_page(1).unwrap();
        let objects = document.holder();
        let fonts = objects.borrow_mut().new_indirect_dictionary();
        dict_mut(&fonts, |d| {
            d.set_new_for(b"Unused", CpdfDictionary::new());
        });
        let resources = objects.borrow_mut().new_indirect_dictionary();
        dict_mut(&resources, |d| {
            d.set_new_for(
                b"Font",
                CpdfReference::new(&objects, fonts.borrow().obj_num()),
            );
        });
        let other_resources = if shared_top {
            Rc::clone(&resources)
        } else {
            let other = objects.borrow_mut().new_indirect_dictionary();
            dict_mut(&other, |d| {
                d.set_new_for(
                    b"Font",
                    CpdfReference::new(&objects, fonts.borrow().obj_num()),
                );
            });
            other
        };
        let page0 = document.page_dictionary(0).unwrap();
        let page1 = document.page_dictionary(1).unwrap();
        if inherited {
            let parent = dict(&page0, |d| d.get_dict_for(b"Parent")).unwrap();
            dict_mut(&parent, |d| {
                d.set_new_for(
                    b"Resources",
                    CpdfReference::new(&objects, resources.borrow().obj_num()),
                );
            });
        } else {
            for (page, resources) in [(&page0, &resources), (&page1, &other_resources)] {
                dict_mut(page, |d| {
                    d.set_new_for(
                        b"Resources",
                        CpdfReference::new(&objects, resources.borrow().obj_num()),
                    );
                });
            }
        }
        let mut page = CpdfPage::from_document(&mut document, 0).unwrap();
        page.mutable_holder()
            .append_page_object(Box::new(rectangle(CpdfPageObject::NO_CONTENT_STREAM)));
        generate(&mut document, &mut page);
        let edited_resources = page.holder().resources().unwrap();
        let edited_fonts = dict(edited_resources, |d| d.get_dict_for(b"Font")).unwrap();
        assert!(!Rc::ptr_eq(&edited_fonts, &fonts));
        assert!(!dict(&edited_fonts, |d| d.key_exist(b"Unused")));
        assert!(dict(&fonts, |d| d.key_exist(b"Unused")));
        let other_page = CpdfPage::new(Rc::clone(&page1)).unwrap();
        assert!(Rc::ptr_eq(
            &other_page.resources().unwrap(),
            &other_resources
        ));
        if shared_top {
            assert!(!Rc::ptr_eq(edited_resources, &resources));
        }
    }
}

#[test]
fn nested_marks_close_at_the_first_difference_and_balance_at_stream_end() {
    let (mut document, mut page) = document_and_page();
    let mut outer = CpdfContentMarks::default();
    outer.add_mark(b"Outer");
    let mut inner = outer.clone_marks();
    inner.add_mark(b"Inner");
    for marks in [inner, outer] {
        let mut path = rectangle(CpdfPageObject::NO_CONTENT_STREAM);
        path.base_mut().set_content_marks(marks);
        page.mutable_holder().append_page_object(Box::new(path));
    }
    let mut generator = CpdfPageContentGenerator::new(page.mutable_holder(), Some(&mut document));
    let mut buffer = Vec::new();
    assert!(generator.process_page_objects(&mut buffer));
    assert_eq!(
        buffer,
        b"/Outer BMC\n/Inner BMC\nq 10 5 3 25 re f Q\nEMC\nq 10 5 3 25 re f Q\nEMC\n"
    );
}

#[test]
fn image_generation_promotes_new_and_inline_streams_and_skips_degenerate_images() {
    let (mut document, mut page) = document_and_page();
    let stream = retain_object(CpdfStream::from_data_and_dict(
        vec![255],
        retain_object(CpdfDictionary::new()),
    ));
    let image = Rc::new(CpdfImage::from_stream(Rc::clone(&stream), false).unwrap());
    let mut image_object = CpdfImageObject::default();
    image_object.set_image(Some(image));
    image_object.set_image_matrix(CfxMatrix::new(2.0, 0.0, 0.0, 3.0, 4.0, 5.0));
    image_object.base_mut().set_dirty(true);
    page.mutable_holder()
        .append_page_object(Box::new(image_object));
    for (inline, matrix) in [
        (true, CfxMatrix::default()),
        (false, CfxMatrix::new(0.0, 0.0, 0.0, 1.0, 0.0, 0.0)),
    ] {
        let stream = retain_object(CpdfStream::from_dict(retain_object(CpdfDictionary::new())));
        let mut object = CpdfImageObject::default();
        object.set_image(Some(Rc::new(
            CpdfImage::from_stream(stream, inline).unwrap(),
        )));
        object.set_image_matrix(matrix);
        object.base_mut().set_dirty(true);
        page.mutable_holder().append_page_object(Box::new(object));
    }
    let mut generator = CpdfPageContentGenerator::new(page.mutable_holder(), Some(&mut document));
    let mut buffer = Vec::new();
    assert!(generator.process_page_objects(&mut buffer));
    // The pinned source drops the second (BI) image. Both images must survive
    // regeneration; the existing promotion path retains the encoded stream.
    assert_eq!(buffer, b"q 2 0 0 3 4 5 cm /FXX1 Do Q\nq /FXX2 Do Q\n");
    assert_ne!(stream.borrow().obj_num(), 0);
    let xobjects = dict(generator.holder().resources().unwrap(), |d| {
        d.get_dict_for(b"XObject")
    })
    .unwrap();
    assert!(Rc::ptr_eq(
        &dict(&xobjects, |d| d.get_stream_for(b"FXX1")).unwrap(),
        &stream
    ));
    let inline = dict(&xobjects, |d| d.get_stream_for(b"FXX2")).unwrap();
    assert_ne!(inline.borrow().obj_num(), 0);
    let promoted = generator
        .holder()
        .page_object_by_index(1)
        .unwrap()
        .as_any()
        .downcast_ref::<CpdfImageObject>()
        .unwrap()
        .image()
        .unwrap();
    assert!(!promoted.is_inline());
    assert!(Rc::ptr_eq(&inline, &promoted.stream()));
    for object in generator.holder().iter() {
        assert!(!object.base().is_dirty());
    }
}

#[test]
fn form_regeneration_keeps_unchanged_children_before_appended_children_and_can_empty_the_stream() {
    let (mut document, mut page) = document_and_page();
    let stream = document
        .holder()
        .borrow_mut()
        .new_indirect_stream_from_bytes(b"10 5 3 25 re f");
    let mut form =
        crate::fpdfapi::page::cpdf_form::CpdfForm::new(Rc::clone(&stream), None, None).unwrap();
    let mut original = rectangle(0);
    original.base_mut().set_default_states();
    original.base_mut().set_dirty(false);
    form.holder_mut().append_page_object(Box::new(original));
    let mut appended = rectangle(CpdfPageObject::NO_CONTENT_STREAM);
    appended.base_mut().set_default_states();
    appended.transform(CfxMatrix::new(1.0, 0.0, 0.0, 1.0, 20.0, 0.0));
    form.holder_mut().append_page_object(Box::new(appended));
    let mut object = CpdfFormObject::new(
        CpdfPageObject::NO_CONTENT_STREAM,
        form,
        CfxMatrix::default(),
    );
    object.base_mut().set_dirty(true);
    page.mutable_holder().append_page_object(Box::new(object));
    generate(&mut document, &mut page);
    let bytes = decoded(&stream);
    let text = String::from_utf8(bytes).unwrap();
    assert_eq!(text.matches("10 5 3 25 re").count(), 2);
    assert!(text.find("10 5 3 25 re").unwrap() < text.find("1 0 0 1 20 0 cm").unwrap());
    {
        let object = page.mutable_holder().page_object_by_index_mut(0).unwrap();
        let object = object
            .as_any_mut()
            .downcast_mut::<CpdfFormObject>()
            .unwrap();
        let holder = object.form_mut().holder_mut();
        let ids: Vec<_> = (0..holder.page_object_count())
            .map(|index| holder.page_object_id_by_index(index).unwrap())
            .collect();
        for id in ids {
            assert!(holder.remove_page_object(id).is_some());
        }
        object.base_mut().set_dirty(true);
    }
    generate(&mut document, &mut page);
    assert!(decoded(&stream).is_empty());
    let dictionary = stream
        .borrow()
        .as_any()
        .downcast_ref::<CpdfStream>()
        .unwrap()
        .get_dict();
    assert!(!dict(&dictionary, |d| d.key_exist(b"Contents")));
}

#[test]
fn form_generation_preserves_the_stream_and_applies_its_object_matrix() {
    let (mut document, mut page) = document_and_page();
    let stream = document
        .holder()
        .borrow_mut()
        .new_indirect_stream_from_bytes(b"q 0 0 1 1 re f Q\n");
    let form =
        crate::fpdfapi::page::cpdf_form::CpdfForm::new(Rc::clone(&stream), None, None).unwrap();
    let mut object = CpdfFormObject::new(
        CpdfPageObject::NO_CONTENT_STREAM,
        form,
        CfxMatrix::new(2.0, 0.0, 0.0, 3.0, 4.0, 5.0),
    );
    object.base_mut().set_dirty(true);
    page.mutable_holder().append_page_object(Box::new(object));
    let degenerate_stream = document
        .holder()
        .borrow_mut()
        .new_indirect_stream_from_bytes(b"");
    let degenerate_form =
        crate::fpdfapi::page::cpdf_form::CpdfForm::new(degenerate_stream, None, None).unwrap();
    let mut degenerate = CpdfFormObject::new(
        CpdfPageObject::NO_CONTENT_STREAM,
        degenerate_form,
        CfxMatrix::new(1.0, 0.0, 0.0, 0.0, 0.0, 0.0),
    );
    degenerate.base_mut().set_dirty(true);
    page.mutable_holder()
        .append_page_object(Box::new(degenerate));
    let mut generator = CpdfPageContentGenerator::new(page.mutable_holder(), Some(&mut document));
    let mut buffer = Vec::new();
    assert!(generator.process_page_objects(&mut buffer));
    assert_eq!(buffer, b"q 2 0 0 3 4 5 cm /FXX1 Do Q\n");
    assert_eq!(decoded(&stream), b"q 0 0 1 1 re f Q\n");
    let resources = generator.holder().resources().unwrap();
    let xobjects = dict(resources, |d| d.get_dict_for(b"XObject")).unwrap();
    assert!(Rc::ptr_eq(
        &dict(&xobjects, |d| d.get_stream_for(b"FXX1")).unwrap(),
        &stream
    ));
    assert_eq!(
        dict(&xobjects, CpdfDictionary::get_keys),
        vec![b"FXX1".to_vec()]
    );
}

#[test]
fn self_referential_font_category_preserves_resource_identity_and_text_output() {
    // RealizeResource's GetOrCreateDictFor can return the resources dictionary
    // itself. Native references do not prohibit this self-referencing graph.
    let (mut document, mut page) = document_and_page();
    let objects = document.holder();
    let resources = objects.borrow_mut().new_indirect_dictionary();
    let number = resources.borrow().obj_num();
    dict_mut(&resources, |d| {
        d.set_new_for(b"Font", CpdfReference::new(&objects, number));
    });
    dict_mut(page.holder().dict(), |d| {
        d.set_new_for(b"Resources", CpdfReference::new(&objects, number));
    });
    page.mutable_holder()
        .set_resources(Some(Rc::clone(&resources)));
    let font = CpdfFont::get_stock_font(&document, b"Helvetica").unwrap();
    let mut text = CpdfTextObject::new(-1);
    let font_dict = font.font_dict();
    text.base_mut()
        .mutable_text_state()
        .set_font(Some(font), font_dict);
    text.base_mut().mutable_text_state().set_font_size(12.0);
    text.set_text(b"A");
    let mut generator = CpdfPageContentGenerator::new(page.mutable_holder(), Some(&mut document));
    let mut first = Vec::new();
    generator.process_text(&mut first, &mut text);
    assert_eq!(first, b"q BT /FXF1 12 Tf 0 Tr [<41>] TJ ET Q\n");
    assert!(Rc::ptr_eq(
        generator.holder().resources().unwrap(),
        &resources
    ));
    let font_category = dict(&resources, |d| d.get_dict_for(b"Font")).unwrap();
    assert!(Rc::ptr_eq(&font_category, &resources));
    let font_resource = dict(&resources, |d| d.get_dict_for(b"FXF1")).unwrap();
    assert_ne!(font_resource.borrow().obj_num(), number);
    let mut second = Vec::new();
    generator.process_text(&mut second, &mut text);
    assert_eq!(second, first);
    assert_eq!(dict(&resources, CpdfDictionary::size), 2);
    // The object being realized may itself also be the category dictionary.
    let (data, _) = generator.holder.split_for_generation();
    let name = GeneratorContext {
        data,
        document: generator.document.as_deref_mut(),
    }
    .realize_resource(&resources, b"Font");
    assert_eq!(name, b"FXF2");
    let realized = dict(&resources, |d| d.get_dict_for(&name)).unwrap();
    assert!(Rc::ptr_eq(&realized, &resources));
}
