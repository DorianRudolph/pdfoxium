// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Annotation flattening and field-tree pruning from `fpdfsdk/fpdf_flatten.cpp`.
//! Dictionaries/streams retain source identity; the document is borrowed only
//! while accessing its object table or page list, never while parsing content.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use crate::fpdfapi::page::cpdf_page::CpdfPage;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_indirect_object_holder::HolderRef;
use crate::fpdfapi::parser::cpdf_object::{
    clone_non_cyclic, get_direct_object, make_reference, ObjectRef,
};
use crate::fpdfapi::parser::cpdf_scalar_object::{CpdfScalarObject, ObjectType};
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fpdfapi::parser::cpdf_stream_acc::CpdfStreamAcc;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix};
use crate::fxcrt::stream_write_utils::write_matrix;

pub const FLATTEN_FAIL: i32 = 0;
pub const FLATTEN_SUCCESS: i32 = 1;
pub const FLATTEN_NOTHINGTODO: i32 = 2;
pub const FLAT_NORMALDISPLAY: i32 = 0;
pub const FLAT_PRINT: i32 = 1;
const MAX_RECURSION: usize = 32;

fn dict<R>(object: &ObjectRef, f: impl FnOnce(&CpdfDictionary) -> R) -> Option<R> {
    let borrowed = object.borrow();
    Some(f(borrowed.as_any().downcast_ref::<CpdfDictionary>()?))
}
fn dict_mut<R>(object: &ObjectRef, f: impl FnOnce(&mut CpdfDictionary) -> R) -> Option<R> {
    let mut borrowed = object.borrow_mut();
    Some(f(borrowed.as_any_mut().downcast_mut::<CpdfDictionary>()?))
}
fn entry(object: &ObjectRef, name: &[u8]) -> Option<ObjectRef> {
    let object = dict(object, |d| d.get_object_for(name))??;
    get_direct_object(&object)
}
fn typed_entry(object: &ObjectRef, name: &[u8], kind: ObjectType) -> Option<ObjectRef> {
    let object = entry(object, name)?;
    let matches = object.borrow().object_type() == kind;
    matches.then_some(object)
}
fn array_len(array: &ObjectRef) -> usize {
    array
        .borrow()
        .as_any()
        .downcast_ref::<CpdfArray>()
        .unwrap()
        .size()
}
fn array_direct(array: &ObjectRef, index: usize) -> Option<ObjectRef> {
    let object = array
        .borrow()
        .as_any()
        .downcast_ref::<CpdfArray>()?
        .get_object_at(index)?;
    get_direct_object(&object)
}
fn array_dict(array: &ObjectRef, index: usize) -> Option<ObjectRef> {
    let object = array_direct(array, index)?;
    let matches = object.borrow().object_type() == ObjectType::Dictionary;
    matches.then_some(object)
}
fn stream_dict(stream: &ObjectRef) -> ObjectRef {
    stream
        .borrow()
        .as_any()
        .downcast_ref::<CpdfStream>()
        .unwrap()
        .get_dict()
}
fn string_for(object: &ObjectRef, name: &[u8]) -> Vec<u8> {
    entry(object, name).map_or_else(Vec::new, |o| o.borrow().get_string())
}
fn rect_for(object: &ObjectRef, name: &[u8]) -> CfxFloatRect {
    dict(object, |d| d.get_rect_for(name)).unwrap_or_default()
}
fn get_or_create_dict(object: &ObjectRef, name: &[u8]) -> ObjectRef {
    if let Some(dictionary) = typed_entry(object, name, ObjectType::Dictionary) {
        return dictionary;
    }
    dict_mut(object, |d| {
        d.set_new_for(name, CpdfDictionary::with_pool(d.byte_string_pool()))
    })
    .unwrap()
}
fn set_reference(object: &ObjectRef, name: &[u8], target: &ObjectRef, holder: &HolderRef) {
    let reference = make_reference(target, holder);
    dict_mut(object, |d| d.set_for(name, Some(reference)));
}

fn is_valid_rect(rect: CfxFloatRect, page: CfxFloatRect) -> bool {
    if rect.is_empty() || rect.width() < 0.000001 || rect.height() < 0.000001 {
        return false;
    }
    page.is_empty()
        || (rect.left - page.left >= -10.000001
            && rect.right - page.right <= 10.000001
            && rect.top - page.top <= 10.000001
            && rect.bottom - page.bottom >= -10.000001)
}

fn get_contents_rect(page: &CpdfPage, rects: &mut Vec<CfxFloatRect>) {
    let dictionary = page.dict();
    // Source constructs a fresh page so its bounds reflect serialized content,
    // even when the caller's loaded object list has already been edited.
    let Some(mut parsed) = CpdfPage::new_with_page_data(dictionary.clone(), page.page_data_ref())
    else {
        return;
    };
    parsed.parse_path_content();
    let media_box = rect_for(&dictionary, b"MediaBox");
    for object in parsed.holder().iter() {
        let rect = object.base().rect();
        if object.base().is_active() && is_valid_rect(rect, media_box) {
            rects.push(rect);
        }
    }
}

fn parser_annots(page: &CpdfPage, usage: i32) -> Option<(Vec<CfxFloatRect>, Vec<ObjectRef>)> {
    let mut rects = Vec::new();
    get_contents_rect(page, &mut rects);
    let dictionary = page.dict();
    let annots = typed_entry(&dictionary, b"Annots", ObjectType::Array)?;
    let media_box = rect_for(&dictionary, b"MediaBox");
    let mut objects = Vec::new();
    for index in 0..array_len(&annots) {
        let Some(annot) = array_dict(&annots, index) else {
            continue;
        };
        if string_for(&annot, b"Subtype") == b"Popup" {
            continue;
        }
        let flags = dict(&annot, |d| d.get_integer_for(b"F", 0)).unwrap();
        if flags & 2 != 0 {
            continue;
        } // Hidden
        let selected = if usage == FLAT_NORMALDISPLAY {
            // Normal display must preserve NoView as well as Invisible. The
            // pinned native flatten wrapper omits NoView, making a print-only
            // appearance visible after flattening for screen display.
            flags & (1 | 32) == 0
        } else {
            flags & 4 != 0
        };
        if !selected {
            continue;
        }
        let name = if dict(&annot, |d| d.key_exist(b"Rect")).unwrap() {
            &b"Rect"[..]
        } else {
            &b"BBox"[..]
        };
        let rect = rect_for(&annot, name);
        if is_valid_rect(rect, media_box) {
            rects.push(rect);
        }
        objects.push(annot);
    }
    Some((rects, objects))
}

fn calculate_rect(rects: &[CfxFloatRect]) -> CfxFloatRect {
    let Some((&first, remaining)) = rects.split_first() else {
        return CfxFloatRect::default();
    };
    let mut result = first;
    for rect in remaining {
        // Preserve the source's ordered std::min/max comparisons, including NaN.
        if rect.left < result.left {
            result.left = rect.left;
        }
        if rect.bottom < result.bottom {
            result.bottom = rect.bottom;
        }
        if result.right < rect.right {
            result.right = rect.right;
        }
        if result.top < rect.top {
            result.top = rect.top;
        }
    }
    result
}

fn new_contents_reference(holder: &HolderRef, data: &[u8]) -> ObjectRef {
    let stream = holder.borrow_mut().new_indirect_stream_from_bytes(data);
    make_reference(&stream, holder)
}
fn generate_flattened_content(key: &[u8]) -> Vec<u8> {
    let mut data = b"q 1 0 0 1 0 0 cm /".to_vec();
    data.extend_from_slice(key);
    data.extend_from_slice(b" Do Q");
    data
}
fn set_page_contents(key: &[u8], page: &ObjectRef, holder: &HolderRef) {
    let array = typed_entry(page, b"Contents", ObjectType::Array);
    let stream = typed_entry(page, b"Contents", ObjectType::Stream);
    if stream.is_none() && array.is_none() {
        if !key.is_empty() {
            dict_mut(page, |d| {
                d.set_for(
                    b"Contents",
                    Some(new_contents_reference(
                        holder,
                        &generate_flattened_content(key),
                    )),
                )
            });
        }
        return;
    }
    dict_mut(page, |d| {
        d.convert_to_indirect_object_for(b"Contents", holder)
    });
    let array = if let Some(array) = array {
        let prefix = new_contents_reference(holder, b"q");
        let suffix = new_contents_reference(holder, b"Q");
        let mut borrowed = array.borrow_mut();
        let a = borrowed.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
        a.insert_at(0, prefix);
        a.append(suffix);
        drop(borrowed);
        array
    } else {
        let stream = stream.unwrap();
        let data = {
            let borrowed = stream.borrow();
            let mut access =
                CpdfStreamAcc::new(borrowed.as_any().downcast_ref::<CpdfStream>().unwrap());
            access.load_all_data_filtered();
            let mut data = Vec::with_capacity(access.get_span().len() + 4);
            data.extend_from_slice(b"q\n");
            data.extend_from_slice(access.get_span());
            data.extend_from_slice(b"\nQ");
            data
        };
        stream
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfStream>()
            .unwrap()
            .set_data_and_remove_filter(&data);
        let array = holder.borrow_mut().new_indirect_array();
        array
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfArray>()
            .unwrap()
            .append(make_reference(&stream, holder));
        set_reference(page, b"Contents", &array, holder);
        array
    };
    if !key.is_empty() {
        let reference = new_contents_reference(holder, &generate_flattened_content(key));
        array
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfArray>()
            .unwrap()
            .append(reference);
    }
}

fn get_matrix(annot: CfxFloatRect, stream: CfxFloatRect, matrix: CfxMatrix) -> Option<CfxMatrix> {
    if stream.is_empty() {
        return Some(CfxMatrix::default());
    }
    let mut transformed = matrix.transform_float_rect(stream);
    transformed.normalize();
    if transformed.is_empty() {
        return None;
    }
    let a = annot.width() / transformed.width();
    let d = annot.height() / transformed.height();
    let matrix = CfxMatrix::new(
        a,
        0.,
        0.,
        d,
        annot.left - transformed.left * a,
        annot.bottom - transformed.bottom * d,
    );
    [matrix.a, matrix.d, matrix.e, matrix.f]
        .iter()
        .all(|n| n.is_finite())
        .then_some(matrix)
}

fn sanitize_resources(resources: &ObjectRef) {
    let Some(fonts) = typed_entry(resources, b"Font", ObjectType::Dictionary) else {
        return;
    };
    let keys = dict(&fonts, CpdfDictionary::get_keys).unwrap();
    for key in keys {
        let Some(font) = typed_entry(&fonts, &key, ObjectType::Dictionary) else {
            continue;
        };
        let Some(encoding) = typed_entry(&font, b"Encoding", ObjectType::Dictionary) else {
            continue;
        };
        let name = dict(&encoding, |d| d.get_name_for(b"BaseEncoding")).unwrap();
        if !matches!(
            name.as_slice(),
            b"" | b"WinAnsiEncoding" | b"MacRomanEncoding" | b"MacExpertEncoding"
        ) {
            dict_mut(&font, |d| d.remove_for(b"Encoding"));
        }
    }
}
fn is_widget(annot: &ObjectRef) -> bool {
    dict(annot, |d| d.get_name_for(b"Subtype")).is_some_and(|name| name == b"Widget")
}
fn identity(object: &ObjectRef) -> usize {
    Rc::as_ptr(object).cast::<()>() as usize
}
fn collect_page_widgets(page: &ObjectRef) -> HashSet<usize> {
    let Some(annots) = typed_entry(page, b"Annots", ObjectType::Array) else {
        return HashSet::new();
    };
    (0..array_len(&annots))
        .filter_map(|i| array_dict(&annots, i))
        .filter(is_widget)
        .map(|o| identity(&o))
        .collect()
}
fn prune_field_array(
    fields: &ObjectRef,
    widgets: &HashSet<usize>,
    visited: &mut HashSet<usize>,
    level: usize,
) -> bool {
    if level > MAX_RECURSION {
        return array_len(fields) == 0;
    }
    for index in (0..array_len(fields)).rev() {
        let Some(field) = array_dict(fields, index) else {
            continue;
        };
        let mut prune = widgets.contains(&identity(&field));
        if !prune {
            if let Some(kids) = typed_entry(&field, b"Kids", ObjectType::Array) {
                if visited.insert(identity(&field)) {
                    prune = prune_field_array(&kids, widgets, visited, level + 1);
                }
            }
        }
        if prune {
            fields
                .borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfArray>()
                .unwrap()
                .remove_at(index);
        }
    }
    array_len(fields) == 0
}
fn remove_flattened_fields(document: &Rc<RefCell<CpdfDocument>>, page: &ObjectRef) {
    let mut widgets = collect_page_widgets(page);
    if widgets.is_empty() {
        return;
    }
    let count = document.borrow().page_count();
    for index in 0..count {
        let Some(other) = document.borrow_mut().page_dictionary(index) else {
            continue;
        };
        if Rc::ptr_eq(page, &other) {
            continue;
        }
        let Some(annots) = typed_entry(&other, b"Annots", ObjectType::Array) else {
            continue;
        };
        for index in 0..array_len(&annots) {
            if let Some(annot) = array_dict(&annots, index) {
                widgets.remove(&identity(&annot));
            }
            if widgets.is_empty() {
                return;
            }
        }
    }
    let Some(root) = document.borrow().root() else {
        return;
    };
    let Some(acroform) = typed_entry(&root, b"AcroForm", ObjectType::Dictionary) else {
        return;
    };
    let Some(fields) = typed_entry(&acroform, b"Fields", ObjectType::Array) else {
        return;
    };
    if prune_field_array(&fields, &widgets, &mut HashSet::new(), 0)
        && !dict(&acroform, |d| d.key_exist(b"XFA")).unwrap()
    {
        dict_mut(&root, |d| d.remove_for(b"AcroForm"));
    }
}
fn appearance_stream(annot: &ObjectRef) -> Option<ObjectRef> {
    let appearances = typed_entry(annot, b"AP", ObjectType::Dictionary)?;
    if let Some(stream) = typed_entry(&appearances, b"N", ObjectType::Stream) {
        return Some(stream);
    }
    let normal = typed_entry(&appearances, b"N", ObjectType::Dictionary)?;
    let state = string_for(annot, b"AS");
    if !state.is_empty() {
        return typed_entry(&normal, &state, ObjectType::Stream);
    }
    let key = dict(&normal, CpdfDictionary::get_keys)?
        .into_iter()
        .next()?;
    typed_entry(&normal, &key, ObjectType::Stream)
}

/// FPDFPage_Flatten. Loaded page objects stay valid; the serialized page content
/// is updated, as in PDFium, and callers can reload to parse flattened objects.
pub fn flatten(document: &Rc<RefCell<CpdfDocument>>, page: &CpdfPage, usage: i32) -> i32 {
    let Some((rects, objects)) = parser_annots(page, usage) else {
        return FLATTEN_NOTHINGTODO;
    };
    let page_dict = page.dict();
    let holder = document.borrow().holder();
    let mut merged = calculate_rect(&rects);
    let has_crop = dict(&page_dict, |d| d.key_exist(b"CropBox")).unwrap();
    let mut media = rect_for(&page_dict, if has_crop { b"CropBox" } else { b"MediaBox" });
    media.normalize();
    if media.is_empty() {
        media = CfxFloatRect::new(0., 0., 612., 792.);
    }
    let mut crop = if has_crop {
        rect_for(&page_dict, b"CropBox")
    } else {
        CfxFloatRect::default()
    };
    crop.normalize();
    if crop.is_empty() {
        crop = media;
    }
    merged.left = merged.left.max(media.left);
    merged.right = merged.right.min(media.right);
    merged.bottom = merged.bottom.max(media.bottom);
    merged.top = merged.top.min(media.top);
    // `rcMerger` is calculated but not subsequently consumed by the source.
    let _ = merged;
    dict_mut(&page_dict, |d| {
        d.set_rect_for(b"MediaBox", media);
        d.set_rect_for(b"CropBox", crop);
    });
    let resources = get_or_create_dict(&page_dict, b"Resources");
    let flattened = holder.borrow_mut().new_indirect_stream_from_bytes(&[]);
    let page_xobjects = get_or_create_dict(&resources, b"XObject");
    let mut key = Vec::new();
    if !objects.is_empty() {
        for index in 0..i32::MAX {
            let candidate = format!("FFT{index}").into_bytes();
            if !dict(&page_xobjects, |d| d.key_exist(&candidate)).unwrap() {
                key = candidate;
                break;
            }
        }
        if key.is_empty() {
            return FLATTEN_FAIL;
        }
    }
    set_page_contents(&key, &page_dict, &holder);
    let mut flattened_resources = None;
    if !key.is_empty() {
        set_reference(&page_xobjects, &key, &flattened, &holder);
        let dictionary = stream_dict(&flattened);
        flattened_resources = Some(
            dict_mut(&dictionary, |d| {
                d.set_new_name_for(b"Type", b"XObject");
                d.set_new_name_for(b"Subtype", b"Form");
                d.set_new_for(b"FormType", CpdfScalarObject::number_from_signed(1));
                d.set_rect_for(b"BBox", crop);
                d.set_new_for(b"Resources", CpdfDictionary::new())
            })
            .unwrap(),
        );
    }
    // Append commands once to avoid repeatedly copying the complete accumulated
    // stream for every annotation. Command order and resource lifetimes match.
    let mut commands = String::new();
    for (index, annot) in objects.iter().enumerate() {
        let mut annot_rect = rect_for(annot, b"Rect");
        annot_rect.normalize();
        let Some(original) = appearance_stream(annot) else {
            continue;
        };
        let original_dict = stream_dict(&original);
        let mut stream_rect = rect_for(
            &original_dict,
            if dict(&original_dict, |d| d.key_exist(b"Rect")).unwrap() {
                b"Rect"
            } else {
                b"BBox"
            },
        );
        stream_rect.normalize();
        if stream_rect.is_empty() {
            continue;
        }
        let stream = if original.borrow().is_inline() {
            let Some(stream) = clone_non_cyclic(&original, false, &HashSet::new()) else {
                continue;
            };
            holder.borrow_mut().add_indirect_object(stream.clone());
            stream
        } else {
            original
        };
        let stream_dictionary = stream_dict(&stream);
        dict_mut(&stream_dictionary, |d| {
            d.set_new_name_for(b"Type", b"XObject");
            d.set_new_name_for(b"Subtype", b"Form");
        });
        if let Some(resources) =
            typed_entry(&stream_dictionary, b"Resources", ObjectType::Dictionary)
        {
            sanitize_resources(&resources);
        }
        let matrix = dict(&original_dict, |d| d.get_matrix_for(b"Matrix")).unwrap();
        let Some(matrix) = get_matrix(annot_rect, stream_rect, matrix) else {
            continue;
        };
        let xobjects = get_or_create_dict(flattened_resources.as_ref().unwrap(), b"XObject");
        let name = format!("F{index}").into_bytes();
        set_reference(&xobjects, &name, &stream, &holder);
        commands.push_str("q ");
        write_matrix(&mut commands, matrix).unwrap();
        commands.push_str(&format!(" cm /F{index} Do Q\n"));
    }
    flattened
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfStream>()
        .unwrap()
        .set_data_and_remove_filter(commands.as_bytes());
    remove_flattened_fields(document, &page_dict);
    dict_mut(&page_dict, |d| d.remove_for(b"Annots"));
    FLATTEN_SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::edit::cpdf_creator::CpdfCreator;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_parser::ParserError;
    use crate::fpdfapi::render::cpdf_renderoptions::CpdfRenderOptions;
    use crate::fpdfapi::render::cpdf_renderstatus::render_page_objects_with_clip;
    use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;
    use crate::fxcrt::fx_coordinates::FxRect;
    use crate::fxcrt::fx_stream::WriteStream;
    use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
    use crate::fxge::dib::fx_dib::FxdibFormat;

    fn document(bytes: &[u8]) -> Rc<RefCell<CpdfDocument>> {
        let mut document = CpdfDocument::new();
        assert_eq!(
            document.load_doc(
                Rc::new(CfxReadOnlyContainerStream::new(bytes.to_vec())),
                b""
            ),
            ParserError::Success
        );
        Rc::new(RefCell::new(document))
    }
    fn fixture(name: &str) -> Rc<RefCell<CpdfDocument>> {
        document(
            &std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../upstream/pdfium/testing/resources")
                    .join(name),
            )
            .unwrap(),
        )
    }
    fn page(document: &Rc<RefCell<CpdfDocument>>, index: usize) -> CpdfPage {
        let mut page = CpdfPage::from_document(&mut document.borrow_mut(), index).unwrap();
        page.parse_path_content();
        page
    }
    fn has_acroform(document: &Rc<RefCell<CpdfDocument>>) -> bool {
        typed_entry(
            &document.borrow().root().unwrap(),
            b"AcroForm",
            ObjectType::Dictionary,
        )
        .is_some()
    }
    #[test]
    fn normal_flatten_respects_no_view_for_print_only_annotations() {
        let document = fixture("annotiter.pdf");
        let page = page(&document, 0);
        let annots = entry(&page.dict(), b"Annots").unwrap();
        for index in 0..array_len(&annots) {
            if let Some(annot) = array_dict(&annots, index) {
                dict_mut(&annot, |d| {
                    d.set_new_for(b"F", CpdfScalarObject::number_from_signed(2))
                });
            }
        }
        let annot = array_dict(&annots, 0).unwrap();
        for (flags, normal, print) in [(36, 0, 1), (0, 1, 0), (1, 0, 0), (4, 1, 1), (6, 0, 0)] {
            dict_mut(&annot, |d| {
                d.set_new_for(b"F", CpdfScalarObject::number_from_signed(flags))
            });
            assert_eq!(
                parser_annots(&page, FLAT_NORMALDISPLAY).unwrap().1.len(),
                normal
            );
            assert_eq!(parser_annots(&page, FLAT_PRINT).unwrap().1.len(), print);
        }
    }
    #[test]
    fn ordinary_source_cases_distinguish_absent_annots_from_normal_and_print_flattening() {
        let document = fixture("hello_world.pdf");
        assert_eq!(
            flatten(&document, &page(&document, 0), FLAT_NORMALDISPLAY),
            FLATTEN_NOTHINGTODO
        );
        for usage in [FLAT_NORMALDISPLAY, FLAT_PRINT] {
            let document = fixture("annotiter.pdf");
            let page = page(&document, 0);
            assert_eq!(flatten(&document, &page, usage), FLATTEN_SUCCESS);
            assert!(entry(&page.dict(), b"Annots").is_none());
            assert_eq!(flatten(&document, &page, usage), FLATTEN_NOTHINGTODO);
        }
    }
    #[test]
    fn flattened_field_pruning_preserves_widgets_shared_with_another_page() {
        // Complete structural assertions from the three source AcroForm cleanup
        // embedder cases; serialization/rendering is exercised separately below.
        for name in [
            "text_form.pdf",
            "bug_498010830_shared_annots.pdf",
            "bug_498010830_shared_widget.pdf",
        ] {
            let document = fixture(name);
            assert!(has_acroform(&document));
            let first = page(&document, 0);
            assert_eq!(
                flatten(&document, &first, FLAT_NORMALDISPLAY),
                FLATTEN_SUCCESS
            );
            if name == "text_form.pdf" {
                assert!(!has_acroform(&document));
            } else {
                assert!(has_acroform(&document));
                let second = page(&document, 1);
                assert_eq!(
                    flatten(&document, &second, FLAT_NORMALDISPLAY),
                    FLATTEN_SUCCESS
                );
                assert!(!has_acroform(&document));
            }
        }
    }
    fn render(page: &CpdfPage, annots: bool) -> Vec<u8> {
        let mut bitmap = CfxDibitmap::create(100, 100, FxdibFormat::Bgra, 0).unwrap();
        bitmap.clear(0xffffffff);
        let mut options = CpdfRenderOptions::new();
        options.set_draw_annots(annots);
        assert!(render_page_objects_with_clip(
            page,
            &mut bitmap,
            page.display_matrix_for_rect(FxRect::new(0, 0, 100, 100), 0),
            &options,
            false,
            None
        ));
        bitmap.buffer().to_vec()
    }
    #[test]
    fn stamp_appearance_is_preserved_through_flatten_save_reload_and_rendering() {
        let document = super::super::fpdf_editpage::create_new_document(false, || None);
        let page = super::super::fpdf_editpage::new_page(&document, 0, 100., 100.).unwrap();
        let holder = document.borrow().holder();
        let mut appearance = CpdfDictionary::new();
        appearance.set_rect_for(b"BBox", CfxFloatRect::new(0., 0., 10., 10.));
        appearance.set_matrix_for(b"Matrix", CfxMatrix::default());
        let stream = retain_object(CpdfStream::from_data_and_dict(
            b"1 0 0 rg 0 0 10 10 re f".to_vec(),
            retain_object(appearance),
        ));
        holder.borrow_mut().add_indirect_object(stream.clone());
        let mut ap = CpdfDictionary::new();
        ap.set_for(b"N", Some(make_reference(&stream, &holder)));
        let mut annot = CpdfDictionary::new();
        annot.set_new_name_for(b"Type", b"Annot");
        annot.set_new_name_for(b"Subtype", b"Stamp");
        annot.set_rect_for(b"Rect", CfxFloatRect::new(20., 30., 50., 60.));
        annot.set_new_for(b"F", CpdfScalarObject::number_from_signed(4));
        annot.set_new_for(b"AP", ap);
        let annot = retain_object(annot);
        holder.borrow_mut().add_indirect_object(annot.clone());
        let mut annots = CpdfArray::new();
        annots.append(make_reference(&annot, &holder));
        dict_mut(&page.dict(), |d| d.set_new_for(b"Annots", annots));
        let expected = render(&page, true);
        assert!(expected.chunks_exact(4).any(|p| p == [0, 0, 255, 255]));
        assert_eq!(
            flatten(&document, &page, FLAT_NORMALDISPLAY),
            FLATTEN_SUCCESS
        );
        struct Sink(Vec<u8>);
        impl WriteStream for Sink {
            fn write_block(&mut self, data: &[u8]) -> bool {
                self.0.extend_from_slice(data);
                true
            }
        }
        let mut sink = Sink(Vec::new());
        assert!(CpdfCreator::new(document.clone(), &mut sink).create(0, 0));
        let reloaded = self::document(&sink.0);
        let page = self::page(&reloaded, 0);
        assert!(entry(&page.dict(), b"Annots").is_none());
        assert_eq!(render(&page, false), expected);
    }
    #[test]
    fn resource_sanitization_keeps_optional_base_encoding_and_rejects_degenerate_matrices() {
        let resources = retain_object(CpdfDictionary::new());
        let fonts = get_or_create_dict(&resources, b"Font");
        for (key, encoding) in [
            (&b"Invalid"[..], &b"PDFDocEncoding"[..]),
            (b"Empty", b""),
            (b"Ansi", b"WinAnsiEncoding"),
        ] {
            let font = get_or_create_dict(&fonts, key);
            let encoding_dict = get_or_create_dict(&font, b"Encoding");
            dict_mut(&encoding_dict, |d| {
                d.set_new_name_for(b"BaseEncoding", encoding);
                d.set_new_for(b"Differences", CpdfArray::new());
            });
        }
        sanitize_resources(&resources);
        assert!(entry(
            &typed_entry(&fonts, b"Invalid", ObjectType::Dictionary).unwrap(),
            b"Encoding"
        )
        .is_none());
        for key in [b"Empty".as_slice(), b"Ansi".as_slice()] {
            assert!(entry(
                &typed_entry(&fonts, key, ObjectType::Dictionary).unwrap(),
                b"Encoding"
            )
            .is_some());
        }
        let rect = CfxFloatRect::new(0., 0., 10., 10.);
        assert_eq!(
            get_matrix(rect, rect, CfxMatrix::new(0., 0., 0., 1., 0., 0.)),
            None
        );
        assert_eq!(
            get_matrix(rect, rect, CfxMatrix::default()),
            Some(CfxMatrix::default())
        );
    }
}
