// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Ordinary annotation editing from `fpdfsdk/fpdf_annot.cpp`.
//! SDK form-environment/widget callbacks remain a separate connected port.
use crate::fpdfapi::edit::cpdf_pagecontentgenerator::CpdfPageContentGenerator;
use crate::fpdfapi::page::cpdf_annotcontext::CpdfAnnotContext;
use crate::fpdfapi::page::cpdf_page::CpdfPage;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_indirect_object_holder::HolderRef;
use crate::fpdfapi::parser::cpdf_object::{get_direct_object, retain_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
use crate::fpdfapi::parser::cpdf_scalar_object::{CpdfScalarObject, ObjectType};
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fpdfapi::parser::cpdf_string::CpdfString;
use crate::fpdfapi::parser::fpdf_parser_decode::pdf_encode_text;
use crate::fpdfdoc::cpdf_annot::{
    bounding_rect_from_quad_points, get_annot_ap, AppearanceMode, Subtype,
};
use crate::fpdfdoc::cpdf_color_utils::cfx_color_from_array;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxPointF};
use crate::fxge::cfx_color::ColorType;
use std::rc::Rc;

fn dict<R>(object: &ObjectRef, f: impl FnOnce(&CpdfDictionary) -> R) -> R {
    let borrowed = object.borrow();
    f(borrowed
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .expect("annotation dictionary"))
}
fn dict_mut<R>(object: &ObjectRef, f: impl FnOnce(&mut CpdfDictionary) -> R) -> R {
    let mut borrowed = object.borrow_mut();
    f(borrowed
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .expect("annotation dictionary"))
}
fn entry(object: &ObjectRef, key: &[u8]) -> Option<ObjectRef> {
    let object = dict(object, |d| d.get_object_for(key))?;
    get_direct_object(&object)
}
fn array(object: &ObjectRef, key: &[u8]) -> Option<ObjectRef> {
    let object = entry(object, key)?;
    let valid = object.borrow().as_any().is::<CpdfArray>();
    valid.then_some(object)
}
fn object_at(array: &ObjectRef, index: usize) -> Option<ObjectRef> {
    let object = array
        .borrow()
        .as_any()
        .downcast_ref::<CpdfArray>()?
        .get_object_at(index)?;
    get_direct_object(&object)
}
fn get_or_create_array(object: &ObjectRef, key: &[u8]) -> ObjectRef {
    array(object, key).unwrap_or_else(|| dict_mut(object, |d| d.set_new_for(key, CpdfArray::new())))
}

pub fn supported_subtype(subtype: i32) -> bool {
    matches!(
        subtype,
        1 | 2 | 3 | 5 | 6 | 9 | 10 | 11 | 12 | 13 | 15 | 16 | 17
    )
}
pub fn object_supported_subtype(subtype: i32) -> bool {
    matches!(subtype, 13 | 15)
}
pub fn subtype(annotation: &ObjectRef) -> Subtype {
    dict(annotation, |d| {
        Subtype::from_pdf(&d.get_name_for(b"Subtype"))
    })
}
pub fn create_annotation(page: &mut CpdfPage, kind: i32) -> Option<ObjectRef> {
    if !supported_subtype(kind) {
        return None;
    }
    let objects = page.page_data().document_objects()?;
    let annotation = objects.borrow().new_dictionary();
    let name = match kind {
        1 => Subtype::Text,
        2 => Subtype::Link,
        3 => Subtype::FreeText,
        5 => Subtype::Square,
        6 => Subtype::Circle,
        9 => Subtype::Highlight,
        10 => Subtype::Underline,
        11 => Subtype::Squiggly,
        12 => Subtype::StrikeOut,
        13 => Subtype::Stamp,
        15 => Subtype::Ink,
        16 => Subtype::Popup,
        17 => Subtype::FileAttachment,
        _ => return None,
    };
    dict_mut(&annotation, |d| {
        d.set_new_name_for(b"Type", b"Annot");
        d.set_new_name_for(b"Subtype", name.as_pdf());
    });
    page.get_or_create_annots_array()
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfArray>()
        .unwrap()
        .append(annotation.clone());
    Some(annotation)
}
pub fn annotation_count(page: &CpdfPage) -> usize {
    page.annots_array().map_or(0, |a| {
        a.borrow()
            .as_any()
            .downcast_ref::<CpdfArray>()
            .unwrap()
            .size()
    })
}
pub fn annotation_at(page: &CpdfPage, index: usize) -> Option<ObjectRef> {
    let annotation = object_at(&page.annots_array()?, index)?;
    let valid = annotation.borrow().as_any().is::<CpdfDictionary>();
    valid.then_some(annotation)
}
pub fn annotation_index(page: &CpdfPage, annotation: &ObjectRef) -> Option<usize> {
    (0..annotation_count(page))
        .find(|&i| annotation_at(page, i).is_some_and(|a| Rc::ptr_eq(&a, annotation)))
}
pub fn remove_annotation(page: &CpdfPage, index: usize) -> bool {
    let Some(array) = page.annots_array() else {
        return false;
    };
    let mut borrowed = array.borrow_mut();
    let array = borrowed.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
    if index >= array.size() {
        return false;
    }
    array.remove_at(index);
    true
}
pub fn appearance(annotation: &ObjectRef, mode: i32, fallback: bool) -> Option<ObjectRef> {
    let mode = match mode {
        0 => AppearanceMode::Normal,
        1 => AppearanceMode::Rollover,
        2 => AppearanceMode::Down,
        _ => return None,
    };
    dict(annotation, |d| get_annot_ap(d, mode, fallback))
}
pub fn ensure_form(context: &mut CpdfAnnotContext, page: &CpdfPage) -> bool {
    if context.form().is_some() {
        return true;
    }
    let Some(stream) = appearance(context.dict(), 0, true) else {
        return false;
    };
    context.set_form(page, stream)
}
pub fn ensure_append_form(context: &mut CpdfAnnotContext, page: &CpdfPage) -> bool {
    if !object_supported_subtype(subtype(context.dict()) as i32) {
        return false;
    }
    if appearance(context.dict(), 0, true).is_none() {
        let Some(objects) = page.page_data().document_objects() else {
            return false;
        };
        if !dict_mut(context.dict(), |d| {
            crate::fpdfdoc::cpdf_generateap::generate_empty_ap(&objects, d)
        }) {
            return false;
        }
    }
    ensure_form(context, page)
}
pub fn update_content_stream(context: &mut CpdfAnnotContext, document: &mut CpdfDocument) -> bool {
    let Some(stream) = appearance(context.dict(), 0, true) else {
        return false;
    };
    let Some(form) = context.form_mut() else {
        return false;
    };
    let mut bytes = Vec::new();
    CpdfPageContentGenerator::new(form.holder_mut(), Some(document))
        .process_page_objects(&mut bytes);
    stream
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfStream>()
        .unwrap()
        .set_data_and_remove_filter(&bytes);
    true
}
pub fn add_ink_stroke(
    annotation: &ObjectRef,
    points: impl ExactSizeIterator<Item = CfxPointF>,
) -> Option<i32> {
    if subtype(annotation) != Subtype::Ink || points.len() == 0 || points.len() > i32::MAX as usize
    {
        return None;
    }
    let list = get_or_create_array(annotation, b"InkList");
    let mut borrowed = list.borrow_mut();
    let list = borrowed.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
    if list.size() >= i32::MAX as usize {
        return None;
    }
    let mut stroke = CpdfArray::new();
    for point in points {
        stroke.append_new(CpdfScalarObject::number_from_float(point.x));
        stroke.append_new(CpdfScalarObject::number_from_float(point.y));
    }
    let index = list.size() as i32;
    list.append_new(stroke);
    Some(index)
}
pub fn remove_ink_list(annotation: &ObjectRef) -> bool {
    if subtype(annotation) != Subtype::Ink {
        return false;
    }
    dict_mut(annotation, |d| d.remove_for(b"InkList"));
    true
}
pub fn set_color(annotation: &ObjectRef, interior: bool, rgba: [u32; 4]) -> bool {
    if rgba.iter().any(|&v| v > 255) || appearance(annotation, 0, true).is_some() {
        return false;
    }
    let [r, g, b, a] = rgba;
    dict_mut(annotation, |d| {
        d.set_new_for(b"CA", CpdfScalarObject::number_from_float(a as f32 / 255.));
    });
    let color = get_or_create_array(annotation, if interior { b"IC" } else { b"C" });
    let mut borrowed = color.borrow_mut();
    let color = borrowed.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
    color.clear();
    for value in [r, g, b] {
        color.append_new(CpdfScalarObject::number_from_float(value as f32 / 255.));
    }
    true
}
pub fn color(annotation: &ObjectRef, interior: bool) -> Option<[u32; 4]> {
    if appearance(annotation, 0, true).is_some() {
        return None;
    }
    let alpha = dict(annotation, |d| {
        if d.key_exist(b"CA") {
            d.get_float_for(b"CA")
        } else {
            1.
        }
    });
    let color = array(annotation, if interior { b"IC" } else { b"C" });
    let [r, g, b] = if let Some(color) = color {
        let borrowed = color.borrow();
        let color = cfx_color_from_array(borrowed.as_any().downcast_ref::<CpdfArray>().unwrap());
        match color.color_type {
            ColorType::Rgb => [color.color1, color.color2, color.color3],
            ColorType::Gray => [color.color1; 3],
            ColorType::Cmyk => [
                (1. - color.color1) * (1. - color.color4),
                (1. - color.color2) * (1. - color.color4),
                (1. - color.color3) * (1. - color.color4),
            ],
            ColorType::Transparent => [0.; 3],
        }
    } else if subtype(annotation) == Subtype::Highlight {
        [1., 1., 0.]
    } else {
        [0.; 3]
    };
    Some([
        (r * 255.) as u32,
        (g * 255.) as u32,
        (b * 255.) as u32,
        (alpha * 255.) as u32,
    ])
}
pub fn has_attachment_points(annotation: &ObjectRef) -> bool {
    matches!(
        subtype(annotation),
        Subtype::Link
            | Subtype::Highlight
            | Subtype::Underline
            | Subtype::Squiggly
            | Subtype::StrikeOut
    )
}
pub fn attachment_points_count(annotation: &ObjectRef) -> usize {
    if !has_attachment_points(annotation) {
        return 0;
    }
    array(annotation, b"QuadPoints").map_or(0, |a| {
        a.borrow()
            .as_any()
            .downcast_ref::<CpdfArray>()
            .unwrap()
            .size()
            / 8
    })
}
pub fn attachment_points(annotation: &ObjectRef, index: usize) -> Option<[f32; 8]> {
    if !has_attachment_points(annotation) || index >= attachment_points_count(annotation) {
        return None;
    }
    let points = array(annotation, b"QuadPoints")?;
    let borrowed = points.borrow();
    let points = borrowed.as_any().downcast_ref::<CpdfArray>()?;
    Some(std::array::from_fn(|i| points.get_float_at(index * 8 + i)))
}
pub fn set_attachment_points(
    annotation: &ObjectRef,
    index: Option<usize>,
    points: [f32; 8],
) -> bool {
    if !has_attachment_points(annotation) {
        return false;
    }
    if let Some(index) = index {
        if index >= attachment_points_count(annotation) {
            return false;
        }
    }
    let array = get_or_create_array(annotation, b"QuadPoints");
    {
        let mut borrowed = array.borrow_mut();
        let array = borrowed.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
        for (i, value) in points.into_iter().enumerate() {
            let number = CpdfScalarObject::number_from_float(value);
            if let Some(index) = index {
                array.set_new_at(index * 8 + i, number);
            } else {
                array.append_new(number);
            }
        }
    }
    if let Some(stream) = appearance(annotation, 0, true) {
        let bbox = dict(annotation, bounding_rect_from_quad_points);
        let stream_dict = stream
            .borrow()
            .as_any()
            .downcast_ref::<CpdfStream>()
            .unwrap()
            .get_dict();
        if dict(&stream_dict, |d| {
            bbox.contains_rect(d.get_rect_for(b"BBox"))
        }) {
            dict_mut(&stream_dict, |d| d.set_rect_for(b"BBox", bbox));
        }
    }
    true
}
pub fn rect(annotation: &ObjectRef) -> CfxFloatRect {
    dict(annotation, |d| d.get_rect_for(b"Rect"))
}
pub fn set_rect(annotation: &ObjectRef, rect: CfxFloatRect) {
    dict_mut(annotation, |d| d.set_rect_for(b"Rect", rect));
}
pub fn vertices(annotation: &ObjectRef) -> Option<Vec<CfxPointF>> {
    if !matches!(subtype(annotation), Subtype::Polygon | Subtype::PolyLine) {
        return None;
    }
    points_from_array(&array(annotation, b"Vertices")?)
}
fn points_from_array(object: &ObjectRef) -> Option<Vec<CfxPointF>> {
    let borrowed = object.borrow();
    let array = borrowed.as_any().downcast_ref::<CpdfArray>()?;
    Some(
        (0..array.size() / 2)
            .map(|i| CfxPointF::new(array.get_float_at(i * 2), array.get_float_at(i * 2 + 1)))
            .collect(),
    )
}
pub fn ink_list_count(annotation: &ObjectRef) -> usize {
    if subtype(annotation) != Subtype::Ink {
        return 0;
    }
    array(annotation, b"InkList").map_or(0, |a| {
        a.borrow()
            .as_any()
            .downcast_ref::<CpdfArray>()
            .unwrap()
            .size()
    })
}
pub fn ink_list_path(annotation: &ObjectRef, index: usize) -> Option<Vec<CfxPointF>> {
    if subtype(annotation) != Subtype::Ink {
        return None;
    }
    points_from_array(&object_at(&array(annotation, b"InkList")?, index)?)
}
pub fn line(annotation: &ObjectRef) -> Option<[CfxPointF; 2]> {
    if subtype(annotation) != Subtype::Line {
        return None;
    }
    let points = array(annotation, b"L")?;
    let borrowed = points.borrow();
    let a = borrowed.as_any().downcast_ref::<CpdfArray>()?;
    (a.size() >= 4).then(|| {
        [
            CfxPointF::new(a.get_float_at(0), a.get_float_at(1)),
            CfxPointF::new(a.get_float_at(2), a.get_float_at(3)),
        ]
    })
}
pub fn set_border(annotation: &ObjectRef, values: [f32; 3]) {
    dict_mut(annotation, |d| {
        d.remove_for(b"AP");
        let mut border = CpdfArray::new();
        for value in values {
            border.append_new(CpdfScalarObject::number_from_float(value));
        }
        d.set_new_for(b"Border", border);
    });
}
pub fn border(annotation: &ObjectRef) -> Option<[f32; 3]> {
    let a = array(annotation, b"Border")?;
    let borrowed = a.borrow();
    let a = borrowed.as_any().downcast_ref::<CpdfArray>()?;
    (a.size() >= 3).then(|| std::array::from_fn(|i| a.get_float_at(i)))
}
pub fn has_key(annotation: &ObjectRef, key: &[u8]) -> bool {
    dict(annotation, |d| d.key_exist(key))
}
pub fn value_type(annotation: &ObjectRef, key: &[u8]) -> Option<ObjectType> {
    Some(
        dict(annotation, |d| d.get_object_for(key))?
            .borrow()
            .object_type(),
    )
}
pub fn set_string(annotation: &ObjectRef, key: &[u8], value: &[u32]) {
    dict_mut(annotation, |d| {
        d.set_new_for(key, CpdfString::from_unicode(None, value));
    });
}
pub fn string(annotation: &ObjectRef, key: &[u8]) -> Vec<u32> {
    dict(annotation, |d| d.get_unicode_text_for(key))
}
pub fn number(annotation: &ObjectRef, key: &[u8]) -> Option<f32> {
    let object = dict(annotation, |d| d.get_object_for(key))?;
    let b = object.borrow();
    (b.object_type() == ObjectType::Number).then(|| b.get_number())
}
pub fn flags(annotation: &ObjectRef) -> i32 {
    dict(annotation, |d| d.get_integer_for(b"F", 0))
}
pub fn set_flags(annotation: &ObjectRef, flags: i32) {
    dict_mut(annotation, |d| {
        d.set_new_for(b"F", CpdfScalarObject::number_from_signed(flags));
    });
}
pub fn linked_annotation(annotation: &ObjectRef, key: &[u8]) -> Option<ObjectRef> {
    let linked = entry(annotation, key)?;
    let valid = linked
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .is_some_and(|d| d.get_name_for(b"Type") == b"Annot");
    valid.then_some(linked)
}
pub fn set_uri(annotation: &ObjectRef, uri: &[u8]) -> bool {
    if subtype(annotation) != Subtype::Link {
        return false;
    }
    let mut action = CpdfDictionary::new();
    action.set_new_name_for(b"Type", b"Action");
    action.set_new_name_for(b"S", b"URI");
    action.set_new_for(b"URI", CpdfString::from_bytes(None, uri));
    dict_mut(annotation, |d| d.set_new_for(b"A", action));
    true
}
pub fn appearance_text(annotation: &ObjectRef, mode: i32) -> Option<Vec<u32>> {
    if !(0..3).contains(&mode) {
        return None;
    }
    Some(
        appearance(annotation, mode, false)
            .map_or_else(Vec::new, |s| s.borrow().get_unicode_text()),
    )
}
pub fn set_appearance(
    annotation: &ObjectRef,
    objects: &HolderRef,
    mode: i32,
    value: Option<&[u32]>,
) -> bool {
    let key: &[u8] = match mode {
        0 => b"N",
        1 => b"R",
        2 => b"D",
        _ => return false,
    };
    let ap = entry(annotation, b"AP").filter(|o| o.borrow().as_any().is::<CpdfDictionary>());
    let Some(value) = value else {
        if let Some(ap) = ap {
            if mode == 0 {
                dict_mut(annotation, |d| d.remove_for(b"AP"));
            } else {
                dict_mut(&ap, |d| d.remove_for(key));
            }
        }
        return true;
    };
    let rect = rect(annotation);
    if rect.width() < 0.000001 || rect.height() < 0.000001 {
        return false;
    }
    let mut stream_dict = CpdfDictionary::new();
    stream_dict.set_new_name_for(b"Type", b"XObject");
    stream_dict.set_new_name_for(b"Subtype", b"Form");
    stream_dict.set_rect_for(b"BBox", rect);
    if has_key(annotation, b"CA") && number(annotation, b"CA").unwrap_or(0.) < 1. {
        let alpha = number(annotation, b"CA").unwrap_or(0.);
        let mut gs = CpdfDictionary::new();
        gs.set_new_name_for(b"Type", b"ExtGState");
        gs.set_new_for(b"CA", CpdfScalarObject::number_from_float(alpha));
        gs.set_new_for(b"ca", CpdfScalarObject::number_from_float(alpha));
        gs.set_new_for(b"AIS", CpdfScalarObject::boolean(false));
        gs.set_new_name_for(b"BM", b"Normal");
        let mut ext = CpdfDictionary::new();
        ext.set_new_for(b"GS", gs);
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"ExtGState", ext);
        stream_dict.set_new_for(b"Resources", resources);
    }
    let stream = retain_object(CpdfStream::from_data_and_dict(
        pdf_encode_text(value),
        retain_object(stream_dict),
    ));
    let number = objects.borrow_mut().add_indirect_object(stream);
    let ap =
        ap.unwrap_or_else(|| dict_mut(annotation, |d| d.set_new_for(b"AP", CpdfDictionary::new())));
    dict_mut(&ap, |d| {
        d.set_new_for(key, CpdfReference::new(objects, number))
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfsdk::fpdf_editpage::{create_new_document, new_page};
    use crate::fxcrt::fx_coordinates::CfxMatrix;

    #[test]
    fn lifecycle_colors_strings_and_appearance_precedence() {
        let document = create_new_document(false, || None);
        let mut page = new_page(&document, 0, 100., 100.).unwrap();
        for kind in 0..29 {
            assert_eq!(
                create_annotation(&mut page, kind).is_some(),
                supported_subtype(kind)
            );
        }
        assert_eq!(annotation_count(&page), 13);
        let annotation = create_annotation(&mut page, 9).unwrap();
        assert_eq!(color(&annotation, false), Some([255, 255, 0, 255]));
        assert!(!set_color(&annotation, false, [256, 0, 0, 255]));
        assert!(set_color(&annotation, false, [10, 20, 30, 127]));
        assert_eq!(color(&annotation, false), Some([10, 20, 30, 127]));
        let text = [0x53, 0x65, 0x1f642];
        set_string(&annotation, b"Contents", &text);
        assert_eq!(string(&annotation, b"Contents"), text);
        assert_eq!(
            value_type(&annotation, b"Contents"),
            Some(ObjectType::String)
        );
        assert_eq!(number(&annotation, b"Contents"), None);
        set_flags(&annotation, 4 | 32);
        assert_eq!(flags(&annotation), 36);
        set_rect(&annotation, CfxFloatRect::new(0., 0., 20., 30.));
        let objects = document.borrow().holder();
        let appearance_data: Vec<u32> = "1 0 0 rg 0 0 10 10 re f".chars().map(u32::from).collect();
        assert!(!set_appearance(
            &annotation,
            &objects,
            3,
            Some(&appearance_data)
        ));
        assert!(set_appearance(
            &annotation,
            &objects,
            0,
            Some(&appearance_data)
        ));
        assert_eq!(appearance_text(&annotation, 0), Some(appearance_data));
        assert_eq!(appearance_text(&annotation, 1), Some(Vec::new()));
        assert_eq!(color(&annotation, false), None);
        assert!(!set_color(&annotation, false, [1, 2, 3, 4]));
        set_border(&annotation, [1., 2., 3.]);
        assert_eq!(border(&annotation), Some([1., 2., 3.]));
        assert!(appearance(&annotation, 0, true).is_none());
        assert_eq!(annotation_index(&page, &annotation), Some(13));
        assert!(remove_annotation(&page, 13));
        assert_eq!(annotation_index(&page, &annotation), None);
        assert_eq!(subtype(&annotation), Subtype::Highlight);
        assert!(!remove_annotation(&page, 13));
    }

    #[test]
    fn quads_ink_paths_and_supported_subtype_boundaries() {
        let document = create_new_document(false, || None);
        let mut page = new_page(&document, 0, 100., 100.).unwrap();
        let highlight = create_annotation(&mut page, 9).unwrap();
        let quad = [1., 5., 4., 5., 1., 2., 4., 2.];
        assert!(!set_attachment_points(&highlight, Some(0), quad));
        assert!(set_attachment_points(&highlight, None, quad));
        assert_eq!(attachment_points_count(&highlight), 1);
        assert_eq!(attachment_points(&highlight, 0), Some(quad));
        assert_eq!(attachment_points(&highlight, usize::MAX), None);
        let ink = create_annotation(&mut page, 15).unwrap();
        let points = [
            CfxPointF::new(80., 90.),
            CfxPointF::new(81., 91.),
            CfxPointF::new(82., 92.),
        ];
        assert_eq!(add_ink_stroke(&highlight, points.into_iter()), None);
        assert_eq!(add_ink_stroke(&ink, std::iter::empty()), None);
        assert_eq!(add_ink_stroke(&ink, points.into_iter()), Some(0));
        assert_eq!(add_ink_stroke(&ink, points[..2].iter().copied()), Some(1));
        assert_eq!(ink_list_count(&ink), 2);
        assert_eq!(ink_list_path(&ink, 0), Some(points.to_vec()));
        assert_eq!(ink_list_path(&ink, 2), None);
        assert!(!has_attachment_points(&ink));
        assert!(remove_ink_list(&ink));
        assert_eq!(ink_list_count(&ink), 0);
        let link = create_annotation(&mut page, 2).unwrap();
        assert!(set_uri(&link, b"https://example.com"));
        assert!(!set_uri(&ink, b"https://example.com"));
        assert!(object_supported_subtype(13));
        assert!(object_supported_subtype(15));
        assert!(!object_supported_subtype(12));
    }

    #[test]
    fn annotation_form_parsing_cancels_stream_matrix_without_mutating_pdf() {
        let document = create_new_document(false, || None);
        let mut page = new_page(&document, 0, 100., 100.).unwrap();
        let annotation = create_annotation(&mut page, 13).unwrap();
        set_rect(&annotation, CfxFloatRect::new(0., 0., 100., 100.));
        let objects = document.borrow().holder();
        let data: Vec<u32> = "0 0 10 20 re f".chars().map(u32::from).collect();
        assert!(set_appearance(&annotation, &objects, 0, Some(&data)));
        let stream = appearance(&annotation, 0, true).unwrap();
        let stream_dict = stream
            .borrow()
            .as_any()
            .downcast_ref::<CpdfStream>()
            .unwrap()
            .get_dict();
        let matrix = CfxMatrix::new(3., 0., 0., 4., 5., 6.);
        dict_mut(&stream_dict, |d| d.set_matrix_for(b"Matrix", matrix));
        let mut context = CpdfAnnotContext::new(annotation);
        assert!(ensure_form(&mut context, &page));
        let holder = context.form().unwrap().holder();
        assert_eq!(holder.page_object_count(), 1);
        let rect = holder.page_object_by_index(0).unwrap().base().rect();
        // Inverting the 3x scale and composing it with its original matrix
        // incurs ordinary f32 cancellation error at the translated origin.
        for (actual, expected) in [rect.left, rect.bottom, rect.right, rect.top]
            .into_iter()
            .zip([0., 0., 10., 20.])
        {
            assert!((actual - expected).abs() < 0.00001);
        }
        assert_eq!(dict(&stream_dict, |d| d.get_matrix_for(b"Matrix")), matrix);
    }
}
