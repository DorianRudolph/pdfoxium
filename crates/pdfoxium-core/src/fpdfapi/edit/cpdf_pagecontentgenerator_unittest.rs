// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Complete cases from `core/fpdfapi/edit/cpdf_pagecontentgenerator_unittest.cpp`.
//! Font/document setup precedes the generator's exclusive scoped borrow; the
//! native constructor does not realize resources until processing begins.
#![allow(non_snake_case)]

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use super::CpdfPageContentGenerator;
use crate::fpdfapi::font::cpdf_font::CpdfFont;
use crate::fpdfapi::page::cpdf_colorspace::get_stock_cs_for_name;
use crate::fpdfapi::page::cpdf_form::CpdfForm;
use crate::fpdfapi::page::cpdf_page::CpdfPage;
use crate::fpdfapi::page::cpdf_pagemodule::{destroy_page_module, initialize_page_module};
use crate::fpdfapi::page::cpdf_pageobject::{CpdfPageObject, PageObject};
use crate::fpdfapi::page::cpdf_pageobjectholder::ParseState;
use crate::fpdfapi::page::cpdf_path::CpdfPath;
use crate::fpdfapi::page::cpdf_pathobject::CpdfPathObject;
use crate::fpdfapi::page::cpdf_textobject::CpdfTextObject;
use crate::fpdfapi::page::cpdf_textstate::TextRenderingMode;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_object::{retain_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fxcrt::fx_coordinates::{CfxMatrix, CfxPointF};
use crate::fxge::cfx_fillrenderoptions::FillType;
use crate::fxge::cfx_gemodule::with_ge_module;
use crate::fxge::cfx_path::PointType;

struct TestWithPageModule;
impl TestWithPageModule {
    fn set_up() -> Self {
        initialize_page_module();
        Self
    }
}
impl Drop for TestWithPageModule {
    fn drop(&mut self) {
        destroy_page_module();
    }
}

fn dummy_page() -> CpdfPage {
    CpdfPage::new(retain_object(CpdfDictionary::new())).unwrap()
}

fn new_document_and_page() -> (CpdfDocument, CpdfPage) {
    let mut doc = CpdfDocument::new();
    doc.create_new_doc();
    let _page_dict = doc.create_new_page(0).unwrap();
    let page = CpdfPage::from_document(&mut doc, 0).unwrap();
    (doc, page)
}

fn with_dict<R>(object: &ObjectRef, f: impl FnOnce(&CpdfDictionary) -> R) -> R {
    let borrowed = object.borrow();
    f(borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap())
}

fn with_dict_mut<R>(object: &ObjectRef, f: impl FnOnce(&mut CpdfDictionary) -> R) -> R {
    let mut borrowed = object.borrow_mut();
    f(borrowed
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap())
}

fn test_get_resource(
    generator: &CpdfPageContentGenerator<'_, '_>,
    kind: &[u8],
    name: &[u8],
) -> ObjectRef {
    let resources = generator.holder().resources().unwrap();
    let category = with_dict(resources, |dict| dict.get_dict_for(kind)).unwrap();
    with_dict(&category, |dict| dict.get_dict_for(name)).unwrap()
}

fn first(value: &[u8], count: usize) -> &[u8] {
    &value[..count.min(value.len())]
}

fn last(value: &[u8], count: usize) -> &[u8] {
    &value[value.len().saturating_sub(count)..]
}

#[test]
fn ProcessRect() {
    let _fixture = TestWithPageModule::set_up();
    let mut path_obj = CpdfPathObject::default();
    path_obj.set_stroke(true);
    path_obj.set_filltype(FillType::EvenOdd);
    path_obj.mutable_path().append_rect(10.0, 5.0, 13.0, 30.0);

    let mut page = dummy_page();
    let mut generator = CpdfPageContentGenerator::new(page.mutable_holder(), None);
    let mut buf = Vec::new();
    generator.process_path(&mut buf, &mut path_obj);
    assert_eq!(buf, b"q 10 5 3 25 re B* Q\n");

    path_obj = CpdfPathObject::default();
    path_obj
        .mutable_path()
        .append_point(CfxPointF::new(0.0, 0.0), PointType::Move);
    path_obj
        .mutable_path()
        .append_point(CfxPointF::new(5.2, 0.0), PointType::Line);
    path_obj
        .mutable_path()
        .append_point(CfxPointF::new(5.2, 3.78), PointType::Line);
    path_obj
        .mutable_path()
        .append_point_and_close(CfxPointF::new(0.0, 3.78), PointType::Line);
    buf.clear();
    generator.process_path(&mut buf, &mut path_obj);
    assert_eq!(buf, b"q 0 0 5.2 3.78 re n Q\n");

    // A rectangle with a duplicate second point: third is not opposite corner.
    path_obj = CpdfPathObject::default();
    path_obj
        .mutable_path()
        .append_point(CfxPointF::new(0.0, 0.0), PointType::Move);
    path_obj
        .mutable_path()
        .append_point(CfxPointF::new(0.0, 0.0), PointType::Line);
    path_obj
        .mutable_path()
        .append_point(CfxPointF::new(2.0, 0.0), PointType::Line);
    path_obj
        .mutable_path()
        .append_point(CfxPointF::new(2.0, 1.0), PointType::Line);
    path_obj
        .mutable_path()
        .append_point(CfxPointF::new(0.0, 1.0), PointType::Line);
    path_obj
        .mutable_path()
        .append_point(CfxPointF::new(0.0, 0.0), PointType::Line);
    buf.clear();
    generator.process_path(&mut buf, &mut path_obj);
    assert_eq!(buf, b"q 0 0 2 1 re n Q\n");
}

#[test]
fn Bug937() {
    let _fixture = TestWithPageModule::set_up();
    let rgb = vec![0.000000000000000000001_f32, 0.7, 0.35];
    let color_space = get_stock_cs_for_name(b"DeviceRGB").unwrap();
    {
        let mut path_obj = CpdfPathObject::default();
        path_obj.set_filltype(FillType::Winding);
        // ProcessPath's rectangle-operator branch, with extreme coordinates.
        path_obj.mutable_path().append_rect(
            0.000000000000000000001,
            0.000000000000000000001,
            100.0,
            100.0,
        );
        path_obj
            .base_mut()
            .mutable_color_state()
            .set_fill_color(Some(color_space.clone()), rgb.clone());
        path_obj
            .base_mut()
            .mutable_color_state()
            .set_stroke_color(Some(color_space.clone()), rgb.clone());
        path_obj
            .base_mut()
            .mutable_graph_state()
            .set_line_width(200000000000000000001.0);
        path_obj.transform(CfxMatrix::new(
            1.0,
            0.0,
            0.0,
            1.0,
            0.000000000000000000001,
            200000000000000.000002,
        ));
        let mut page = dummy_page();
        let mut generator = CpdfPageContentGenerator::new(page.mutable_holder(), None);
        let mut buf = Vec::new();
        generator.process_path(&mut buf, &mut path_obj);
        assert_eq!(
            buf,
            concat!(
                "q .000000000000000000001 .7 .35 rg ",
                ".000000000000000000001 .7 .35 RG ",
                "200000000000000000000 w 1 0 0 1 .000000000000000000001 ",
                "200000000000000 cm .000000000000000000001 ",
                ".000000000000000000001 100 100 re f Q\n",
            )
            .as_bytes()
        );
    }
    {
        // ProcessPath's Bezier-operator branch.
        let mut path_obj = CpdfPathObject::default();
        path_obj
            .base_mut()
            .mutable_color_state()
            .set_fill_color(Some(color_space.clone()), rgb.clone());
        path_obj
            .base_mut()
            .mutable_color_state()
            .set_stroke_color(Some(color_space), rgb);
        path_obj
            .base_mut()
            .mutable_graph_state()
            .set_line_width(2.000000000000000000001);
        path_obj.transform(CfxMatrix::new(
            1.0,
            0.0,
            0.0,
            1.0,
            432.0,
            500000000000000.000002,
        ));
        path_obj.set_filltype(FillType::Winding);
        path_obj.mutable_path().append_point(
            CfxPointF::new(0.000000000000000000001, 4.67),
            PointType::Move,
        );
        path_obj.mutable_path().append_point(
            CfxPointF::new(0.000000000000000000001, 100000000000000.000002),
            PointType::Line,
        );
        path_obj
            .mutable_path()
            .append_point(CfxPointF::new(0.0000000000001, 3.15), PointType::Bezier);
        path_obj
            .mutable_path()
            .append_point(CfxPointF::new(3.57, 2.98), PointType::Bezier);
        path_obj.mutable_path().append_point_and_close(
            CfxPointF::new(53.4, 5000000000000000000.00000000000000004),
            PointType::Bezier,
        );
        let mut page = dummy_page();
        let mut generator = CpdfPageContentGenerator::new(page.mutable_holder(), None);
        let mut buf = Vec::new();
        generator.process_path(&mut buf, &mut path_obj);
        assert_eq!(
            buf,
            concat!(
                "q .000000000000000000001 .7 .35 rg ",
                ".000000000000000000001 .7 .35 RG 2 w 1 0 0 1 432 ",
                "500000000000000 cm .000000000000000000001 4.67 m ",
                ".000000000000000000001 100000000000000 l ",
                ".0000000000001 3.15 3.57 2.98 53.4 ",
                "5000000000000000000 c h f Q\n",
            )
            .as_bytes()
        );
    }
}

#[test]
fn ProcessPath() {
    let _fixture = TestWithPageModule::set_up();
    let mut path_obj = CpdfPathObject::default();
    path_obj.set_filltype(FillType::Winding);
    let path = path_obj.mutable_path();
    path.append_point(CfxPointF::new(3.102, 4.67), PointType::Move);
    path.append_point(CfxPointF::new(5.45, 0.29), PointType::Line);
    path.append_point(CfxPointF::new(4.24, 3.15), PointType::Bezier);
    path.append_point(CfxPointF::new(4.65, 2.98), PointType::Bezier);
    path.append_point(CfxPointF::new(3.456, 0.24), PointType::Bezier);
    path.append_point(CfxPointF::new(10.6, 11.15), PointType::Line);
    path.append_point(CfxPointF::new(11.0, 12.5), PointType::Line);
    path.append_point(CfxPointF::new(11.46, 12.67), PointType::Bezier);
    path.append_point(CfxPointF::new(11.84, 12.96), PointType::Bezier);
    path.append_point_and_close(CfxPointF::new(12.0, 13.64), PointType::Bezier);
    let mut page = dummy_page();
    let mut generator = CpdfPageContentGenerator::new(page.mutable_holder(), None);
    let mut buf = Vec::new();
    generator.process_path(&mut buf, &mut path_obj);
    assert_eq!(
        buf,
        concat!(
            "q 3.102 4.67 m 5.45 .29 l 4.24 ",
            "3.15 4.65 2.98 3.456 .24 c 10.6 ",
            "11.15 l 11 12.5 l 11.46 12.67 11.84 12.96 12 13.64 c h f",
            " Q\n",
        )
        .as_bytes()
    );
}

#[test]
fn ProcessGraphics() {
    let _fixture = TestWithPageModule::set_up();
    let mut path_obj = CpdfPathObject::default();
    path_obj.set_stroke(true);
    path_obj.set_filltype(FillType::Winding);
    path_obj
        .mutable_path()
        .append_point(CfxPointF::new(1.0, 2.0), PointType::Move);
    path_obj
        .mutable_path()
        .append_point(CfxPointF::new(3.0, 4.0), PointType::Line);
    path_obj
        .mutable_path()
        .append_point_and_close(CfxPointF::new(5.0, 6.0), PointType::Line);
    let color_space = get_stock_cs_for_name(b"DeviceRGB").unwrap();
    path_obj
        .base_mut()
        .mutable_color_state()
        .set_fill_color(Some(color_space.clone()), vec![0.5, 0.7, 0.35]);
    path_obj
        .base_mut()
        .mutable_color_state()
        .set_stroke_color(Some(color_space), vec![1.0, 0.9, 0.0]);
    path_obj
        .base_mut()
        .mutable_general_state()
        .set_fill_alpha(0.5);
    path_obj
        .base_mut()
        .mutable_general_state()
        .set_stroke_alpha(0.8);
    let (mut doc, mut page) = new_document_and_page();
    let mut generator = CpdfPageContentGenerator::new(page.mutable_holder(), Some(&mut doc));
    let mut buf = Vec::new();
    generator.process_path(&mut buf, &mut path_obj);
    let path_string = buf.clone();
    let expected_start = b"q .5 .7 .35 rg 1 .9 0 RG /";
    let expected_end = b" gs 1 2 m 3 4 l 5 6 l h B Q\n";
    let min_length = expected_start.len() + expected_end.len();
    assert_eq!(expected_start, first(&path_string, expected_start.len()));
    assert_eq!(expected_end, last(&path_string, expected_end.len()));
    assert!(path_string.len() > min_length);
    let external_gs = test_get_resource(
        &generator,
        b"ExtGState",
        &path_string[expected_start.len()..path_string.len() - expected_end.len()],
    );
    with_dict(&external_gs, |dict| {
        assert_eq!(dict.get_float_for(b"ca"), 0.5);
        assert_eq!(dict.get_float_for(b"CA"), 0.8);
    });
    path_obj
        .base_mut()
        .mutable_graph_state()
        .set_line_width(10.5);
    buf.clear();
    generator.process_path(&mut buf, &mut path_obj);
    let expected_start2 = b"q .5 .7 .35 rg 1 .9 0 RG 10.5 w /";
    let path_string2 = buf;
    assert_eq!(expected_start2, first(&path_string2, expected_start2.len()));
    assert_eq!(expected_end, last(&path_string2, expected_end.len()));
    let difference = expected_start2.len() - expected_start.len();
    assert_eq!(path_string.len() + difference, path_string2.len());
    assert_eq!(
        &path_string[expected_start.len()..path_string.len() - expected_end.len()],
        &path_string2[expected_start2.len()..path_string2.len() - expected_end.len()],
    );
}

#[test]
fn ProcessStandardText() {
    let _fixture = TestWithPageModule::set_up();
    let (mut doc, mut page) = new_document_and_page();
    let font = CpdfFont::get_stock_font(&doc, b"Times-Roman").unwrap();
    let font_dict = font.font_dict().unwrap();
    let mut text_obj = CpdfTextObject::new(CpdfPageObject::NO_CONTENT_STREAM);
    text_obj
        .base_mut()
        .mutable_text_state()
        .set_font(Some(font), Some(font_dict));
    text_obj.base_mut().mutable_text_state().set_font_size(10.0);
    let color_space = get_stock_cs_for_name(b"DeviceRGB").unwrap();
    text_obj
        .base_mut()
        .mutable_color_state()
        .set_fill_color(Some(color_space.clone()), vec![0.5, 0.7, 0.35]);
    text_obj
        .base_mut()
        .mutable_color_state()
        .set_stroke_color(Some(color_space), vec![1.0, 0.9, 0.0]);
    text_obj
        .base_mut()
        .mutable_general_state()
        .set_fill_alpha(0.5);
    text_obj
        .base_mut()
        .mutable_general_state()
        .set_stroke_alpha(0.8);
    text_obj.transform(CfxMatrix::new(1.0, 0.0, 0.0, 1.0, 100.0, 100.0));
    text_obj.set_text(b"Hello World");
    let mut generator = CpdfPageContentGenerator::new(page.mutable_holder(), Some(&mut doc));
    let mut buf = Vec::new();
    generator.process_text(&mut buf, &mut text_obj);
    let text_string = buf;
    let first_resource_at = text_string.iter().position(|&b| b == b'/').unwrap() + 1;
    let second_resource_at = text_string.iter().rposition(|&b| b == b'/').unwrap() + 1;
    let first_string = &text_string[..first_resource_at];
    let mid_string = &text_string[first_resource_at..second_resource_at];
    let last_string = &text_string[second_resource_at..];
    // q and Q must be outside the BT .. ET operations.
    let compare1 = b"q .5 .7 .35 rg 1 .9 0 RG /";
    let compare2 = b" gs BT 1 0 0 1 100 100 Tm /";
    let compare3 = b" 10 Tf 0 Tr [<48656C6C6F20576F726C64>] TJ ET Q\n";
    assert!(compare1.len() + compare2.len() + compare3.len() < text_string.len());
    assert_eq!(compare1, first(first_string, compare1.len()));
    assert_eq!(compare2, last(mid_string, compare2.len()));
    assert_eq!(compare3, last(last_string, compare3.len()));
    let gs = test_get_resource(
        &generator,
        b"ExtGState",
        first(mid_string, mid_string.len() - compare2.len()),
    );
    with_dict(&gs, |dict| {
        assert_eq!(dict.get_float_for(b"ca"), 0.5);
        assert_eq!(dict.get_float_for(b"CA"), 0.8);
    });
    let font = test_get_resource(
        &generator,
        b"Font",
        first(last_string, last_string.len() - compare3.len()),
    );
    with_dict(&font, |dict| {
        assert_eq!(dict.get_name_for(b"Type"), b"Font");
        assert_eq!(dict.get_name_for(b"Subtype"), b"Type1");
        assert_eq!(dict.get_name_for(b"BaseFont"), b"Times-Roman");
    });
}

#[test]
fn ProcessText() {
    let _fixture = TestWithPageModule::set_up();
    let (mut doc, mut page) = new_document_and_page();
    let mut buf = Vec::new();
    let mut text_obj = CpdfTextObject::new(CpdfPageObject::NO_CONTENT_STREAM);
    let objects = doc.holder();
    let font_dict = objects.borrow_mut().new_indirect_dictionary();
    with_dict_mut(&font_dict, |dict| {
        dict.set_new_name_for(b"Type", b"Font");
        dict.set_new_name_for(b"Subtype", b"TrueType");
    });
    let font = CpdfFont::get_stock_font(&doc, b"Arial").unwrap();
    with_dict_mut(&font_dict, |dict| {
        dict.set_new_name_for(b"BaseFont", font.base_font_name())
    });
    let descriptor = objects.borrow_mut().new_indirect_dictionary();
    with_dict_mut(&descriptor, |dict| {
        dict.set_new_name_for(b"Type", b"FontDescriptor");
        dict.set_new_name_for(b"FontName", font.base_font_name());
    });
    with_dict_mut(&font_dict, |dict| {
        dict.set_new_for(
            b"FontDescriptor",
            CpdfReference::new(&objects, descriptor.borrow().obj_num()),
        )
    });
    let loaded = with_ge_module(|module| {
        doc.page_data_ref()
            .get_font(&font_dict, &mut module.font_mgr().builtin_mapper_mut())
    })
    .unwrap();
    text_obj
        .base_mut()
        .mutable_text_state()
        .set_font(Some(loaded), Some(font_dict));
    text_obj.base_mut().mutable_text_state().set_font_size(15.5);
    text_obj.set_text(b"I am indirect");
    text_obj.set_text_render_mode(TextRenderingMode::FillClip);
    let mut path = CpdfPath::new();
    path.append_point(CfxPointF::new(0.0, 0.0), PointType::Move);
    path.append_point(CfxPointF::new(5.0, 0.0), PointType::Line);
    path.append_point(CfxPointF::new(5.0, 4.0), PointType::Line);
    path.append_point_and_close(CfxPointF::new(0.0, 4.0), PointType::Line);
    let clip = text_obj.base_mut().mutable_clip_path();
    clip.emplace();
    clip.append_path(path, FillType::EvenOdd);
    let mut generator = CpdfPageContentGenerator::new(page.mutable_holder(), Some(&mut doc));
    generator.process_text(&mut buf, &mut text_obj);
    drop(text_obj);
    let text_string = buf;
    let first_resource_at = text_string.iter().position(|&b| b == b'/').unwrap() + 1;
    let _first_string = first(&text_string, first_resource_at);
    let _last_string = &text_string[first_resource_at..];
    let compare1 = b"q 0 0 5 4 re W* n BT /";
    let compare2 = b" 15.5 Tf 4 Tr [<4920616D20696E646972656374>] TJ ET Q\n";
    assert!(compare1.len() + compare2.len() < text_string.len());
    assert_eq!(compare1, first(&text_string, compare1.len()));
    assert_eq!(compare2, last(&text_string, compare2.len()));
    let font = test_get_resource(
        &generator,
        b"Font",
        &text_string[compare1.len()..text_string.len() - compare2.len()],
    );
    assert_ne!(font.borrow().obj_num(), 0);
    with_dict(&font, |dict| {
        assert_eq!(dict.get_name_for(b"Type"), b"Font");
        assert_eq!(dict.get_name_for(b"Subtype"), b"TrueType");
        assert_eq!(dict.get_name_for(b"BaseFont"), b"Helvetica");
    });
    let desc = with_dict(&font, |dict| dict.get_dict_for(b"FontDescriptor")).unwrap();
    assert_ne!(desc.borrow().obj_num(), 0);
    with_dict(&desc, |dict| {
        assert_eq!(dict.get_name_for(b"Type"), b"FontDescriptor");
        assert_eq!(dict.get_name_for(b"FontName"), b"Helvetica");
    });
}

#[test]
fn ProcessEmptyForm() {
    let _fixture = TestWithPageModule::set_up();
    let mut doc = CpdfDocument::new();
    doc.create_new_doc();
    let stream = retain_object(CpdfStream::from_dict(retain_object(CpdfDictionary::new())));
    let mut form = CpdfForm::new(stream, None, None).unwrap();
    form.parse_content(
        None,
        None,
        doc.page_data_ref(),
        Rc::new(RefCell::new(HashSet::new())),
        0,
    );
    assert_eq!(form.holder().parse_state(), ParseState::Parsed);
    let mut generator = CpdfPageContentGenerator::new(form.holder_mut(), Some(&mut doc));
    let mut buf = Vec::new();
    assert!(!generator.process_page_objects(&mut buf));
    assert_eq!(buf, b"");
}

#[test]
fn ProcessFormWithPath() {
    let _fixture = TestWithPageModule::set_up();
    let mut doc = CpdfDocument::new();
    doc.create_new_doc();
    // Native DataVector(begin(kContents), end(kContents)) retains the NUL.
    let contents = concat!(
        "q 3.102 4.6700001 m 5.4500012 .28999999 ",
        "l 4.2399998 3.1499999 4.65 2.98 3.456 0.24 c 3.102 4.6700001 l h f Q\n",
        "\0",
    );
    let stream = retain_object(CpdfStream::from_data_and_dict(
        contents.as_bytes().to_vec(),
        retain_object(CpdfDictionary::new()),
    ));
    let mut form = CpdfForm::new(stream, None, None).unwrap();
    form.parse_content(
        None,
        None,
        doc.page_data_ref(),
        Rc::new(RefCell::new(HashSet::new())),
        0,
    );
    assert_eq!(form.holder().parse_state(), ParseState::Parsed);
    let mut generator = CpdfPageContentGenerator::new(form.holder_mut(), Some(&mut doc));
    let mut buf = Vec::new();
    assert!(generator.process_page_objects(&mut buf));
    assert_eq!(
        buf,
        concat!(
            "q 3.102 4.67 m 5.4500012 .29 l 4.24 3.1499999 ",
            "4.65 2.98 3.456 .24 c 3.102 4.67 l h f Q\n",
        )
        .as_bytes()
    );
}

#[test]
fn ProcessContentMarksWithProperties() {
    let _fixture = TestWithPageModule::set_up();
    let (mut doc, mut page) = new_document_and_page();
    let mut path = CpdfPathObject::default();
    path.set_filltype(FillType::Winding);
    path.mutable_path().append_rect(0.0, 0.0, 10.0, 10.0);
    path.base_mut().set_dirty(true);
    let marks = retain_object(CpdfDictionary::new());
    path.base_mut()
        .mutable_content_marks()
        .add_mark_with_properties_holder(b"M1", marks, b"Property Name With Space");
    page.mutable_holder().append_page_object(Box::new(path));
    let mut generator = CpdfPageContentGenerator::new(page.mutable_holder(), Some(&mut doc));
    let mut buf = Vec::new();
    assert!(generator.process_page_objects(&mut buf));
    let expected = b"/M1 /Property#20Name#20With#20Space BDC";
    assert!(buf.windows(expected.len()).any(|part| part == expected));
}
