// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Page boxes, content transforms and clipping from `fpdfsdk/fpdf_transformpage.cpp`.

use crate::fpdfapi::page::cpdf_clippath::CpdfClipPath;
use crate::fpdfapi::page::cpdf_page::CpdfPage;
use crate::fpdfapi::page::cpdf_pageobject::{PageObject, PageObjectType};
use crate::fpdfapi::page::cpdf_path::CpdfPath;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_object::{
    get_dict_object, get_direct_object, retain_object, ObjectRef,
};
use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix};
use crate::fxcrt::stream_write_utils::{write_matrix, write_point, write_rect};
use crate::fxge::cfx_fillrenderoptions::FillType;
use crate::fxge::cfx_path::PointType;

pub fn set_bounding_box(page: &mut CpdfPage, key: &[u8], rect: CfxFloatRect) {
    page.dict()
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .expect("page dictionary")
        .set_rect_for(key, rect);
    page.update_dimensions();
}

pub fn bounding_box(page: &CpdfPage, key: &[u8]) -> Option<[f32; 4]> {
    let array = direct_entry(&page.dict(), key)?;
    let borrowed = array.borrow();
    let array = borrowed.as_any().downcast_ref::<CpdfArray>()?;
    Some(std::array::from_fn(|index| array.get_float_at(index)))
}

fn direct_entry(dict: &ObjectRef, key: &[u8]) -> Option<ObjectRef> {
    // Snapshot the entry, then release the dictionary before lazy parsing can
    // call back into document state or visit that same dictionary.
    let entry = dict
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()?
        .get_object_for(key)?;
    get_direct_object(&entry)
}

fn supported_contents(content: &ObjectRef) -> bool {
    let content = content.borrow();
    content.object_type() == ObjectType::Array
        || (content.object_type() == ObjectType::Stream && !content.is_inline())
}

fn insert_content_streams(
    document: &CpdfDocument,
    page: &CpdfPage,
    contents: &ObjectRef,
    prefix: &[u8],
    suffix: Option<&[u8]>,
) {
    let holder = document.holder();
    let start = holder.borrow_mut().new_indirect_stream_from_bytes(prefix);
    let start_number = start.borrow().obj_num();
    let end_number = suffix.map(|bytes| {
        holder
            .borrow_mut()
            .new_indirect_stream_from_bytes(bytes)
            .borrow()
            .obj_num()
    });
    if contents.borrow().object_type() == ObjectType::Array {
        let mut borrowed = contents.borrow_mut();
        let array = borrowed.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
        array.insert_new_at(0, CpdfReference::new(&holder, start_number));
        if let Some(number) = end_number {
            array.append_new(CpdfReference::new(&holder, number));
        }
    } else {
        let original_number = contents.borrow().obj_num();
        let mut array = CpdfArray::with_pool(holder.borrow().byte_string_pool());
        array.append_new(CpdfReference::new(&holder, start_number));
        array.append_new(CpdfReference::new(&holder, original_number));
        if let Some(number) = end_number {
            array.append_new(CpdfReference::new(&holder, number));
        }
        let array = retain_object(array);
        let number = holder.borrow_mut().add_indirect_object(array);
        page.dict()
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"Contents", CpdfReference::new(&holder, number));
    }
}

pub fn transform_with_clip(
    document: &CpdfDocument,
    page: &CpdfPage,
    matrix: Option<CfxMatrix>,
    clip: Option<CfxFloatRect>,
) -> bool {
    if matrix.is_none() && clip.is_none() {
        return false;
    }
    let Some(contents) = direct_entry(&page.dict(), b"Contents") else {
        return false;
    };
    if !supported_contents(&contents) {
        return false;
    }
    let mut prefix = String::from("q ");
    if let Some(mut clip) = clip {
        clip.normalize();
        write_rect(&mut prefix, clip).unwrap();
        prefix.push_str(" re W* n ");
    }
    if let Some(matrix) = matrix {
        write_matrix(&mut prefix, matrix).unwrap();
        prefix.push_str(" cm ");
    }
    insert_content_streams(document, page, &contents, prefix.as_bytes(), Some(b" Q"));

    // Pattern matrices must follow the same transformation as page contents.
    let Some(matrix) = matrix else {
        return true;
    };
    let Some(resources) =
        direct_entry(&page.dict(), b"Resources").and_then(|o| get_dict_object(&o))
    else {
        return true;
    };
    let Some(patterns) = direct_entry(&resources, b"Pattern").and_then(|o| get_dict_object(&o))
    else {
        return true;
    };
    let keys = patterns
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .unwrap()
        .get_keys();
    for key in keys {
        let Some(dict) = direct_entry(&patterns, &key).and_then(|o| get_dict_object(&o)) else {
            continue;
        };
        let mut borrowed = dict.borrow_mut();
        let dict = borrowed
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap();
        let mut pattern_matrix = dict.get_matrix_for(b"Matrix");
        pattern_matrix.concat(matrix);
        dict.set_matrix_for(b"Matrix", pattern_matrix);
    }
    true
}

pub fn transform_clip_path(object: &mut dyn PageObject, matrix: CfxMatrix) {
    // Shading clips have already been transformed as part of the shading.
    if object.object_type() != PageObjectType::Shading {
        object.base_mut().transform_clip_path(matrix);
    }
}

pub fn create_clip_path(rect: CfxFloatRect) -> CpdfClipPath {
    let mut path = CpdfPath::new();
    path.append_rect(rect.left, rect.bottom, rect.right, rect.top);
    let mut clip = CpdfClipPath::new();
    clip.append_path(path, FillType::EvenOdd);
    clip
}

fn output_path(output: &mut String, path: &CpdfPath) {
    let Some(path) = path.get_object() else {
        return;
    };
    let points = path.points();
    if let Some(corner) = path.rect_opposite_corner() {
        let first = points[0].point;
        write_rect(
            output,
            CfxFloatRect::new(first.x, first.y, corner.x, corner.y),
        )
        .unwrap();
        output.push_str(" re\n");
        return;
    }
    let mut index = 0;
    while let Some(point) = points.get(index) {
        if point.point_type == PointType::Bezier {
            // Malformed partial cubic runs cannot supply a PDF c operator.
            // Avoid the native out-of-bounds access while retaining preceding
            // complete commands.
            let Some(controls) = points.get(index..index.saturating_add(3)) else {
                break;
            };
            if controls.iter().any(|p| p.point_type != PointType::Bezier) {
                index += 1;
                continue;
            }
            write_point(output, controls[0].point).unwrap();
            output.push(' ');
            write_point(output, controls[1].point).unwrap();
            output.push(' ');
            write_point(output, controls[2].point).unwrap();
            output.push_str(" c");
            if controls[2].close_figure {
                output.push_str(" h");
            }
            output.push('\n');
            index += 3;
            continue;
        }
        write_point(output, point.point).unwrap();
        if point.point_type == PointType::Move {
            output.push_str(" m\n");
        } else {
            output.push_str(" l");
            if point.close_figure {
                output.push_str(" h");
            }
            output.push('\n');
        }
        index += 1;
    }
}

pub fn insert_clip_path(document: &CpdfDocument, page: &CpdfPage, clip: &CpdfClipPath) -> bool {
    let Some(contents) = direct_entry(&page.dict(), b"Contents") else {
        return false;
    };
    if !supported_contents(&contents) {
        return false;
    }
    let mut prefix = String::new();
    if clip.has_ref() {
        for index in 0..clip.path_count() {
            let path = clip.path(index);
            if path.get_object().is_none_or(|p| p.points().is_empty()) {
                prefix.push_str("0 0 m W n ");
            } else {
                output_path(&mut prefix, &path);
                prefix.push_str(if clip.clip_type(index) == FillType::Winding {
                    "W n\n"
                } else {
                    "W* n\n"
                });
            }
        }
    }
    insert_content_streams(document, page, &contents, prefix.as_bytes(), None);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
    use crate::fpdfsdk::fpdf_editpage::{create_new_document, new_page};
    use crate::fxcrt::fx_coordinates::CfxPointF;

    fn content_streams(page: &CpdfPage) -> Vec<Vec<u8>> {
        let contents = direct_entry(&page.dict(), b"Contents").unwrap();
        let borrowed = contents.borrow();
        let array = borrowed.as_any().downcast_ref::<CpdfArray>().unwrap();
        (0..array.size())
            .map(|index| {
                let stream = array.get_direct_object_at(index).unwrap();
                let borrowed = stream.borrow();
                borrowed
                    .as_any()
                    .downcast_ref::<CpdfStream>()
                    .unwrap()
                    .get_in_memory_raw_data()
                    .to_vec()
            })
            .collect()
    }

    fn page_with_content(document: &std::rc::Rc<std::cell::RefCell<CpdfDocument>>) -> CpdfPage {
        let page = new_page(document, 0, 100.0, 100.0).unwrap();
        let holder = document.borrow().holder();
        let stream = holder
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"10 10 20 20 re f");
        let number = stream.borrow().obj_num();
        page.dict()
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"Contents", CpdfReference::new(&holder, number));
        page
    }

    #[test]
    fn transform_wraps_content_then_prepend_clipping_to_array() {
        let document = create_new_document(false, || None);
        let page = page_with_content(&document);
        assert!(!transform_with_clip(&document.borrow(), &page, None, None));
        assert!(transform_with_clip(
            &document.borrow(),
            &page,
            Some(CfxMatrix::new(2.0, 0.0, 0.0, 3.0, 4.0, 5.0)),
            Some(CfxFloatRect::new(40.0, 50.0, 10.0, 20.0)),
        ));
        let streams = content_streams(&page);
        assert_eq!(streams[0], b"q 10 20 30 30 re W* n 2 0 0 3 4 5 cm ");
        assert_eq!(streams[1], b"10 10 20 20 re f");
        assert_eq!(streams[2], b" Q");
        let clip = create_clip_path(CfxFloatRect::new(1.0, 2.0, 8.0, 9.0));
        assert!(insert_clip_path(&document.borrow(), &page, &clip));
        let streams = content_streams(&page);
        assert_eq!(streams[0], b"1 2 7 7 re\nW* n\n");
        assert_eq!(streams.len(), 4);
    }

    #[test]
    fn page_transform_updates_dictionary_and_stream_pattern_matrices() {
        let document = create_new_document(false, || None);
        let page = page_with_content(&document);
        let patterns = retain_object(CpdfDictionary::new());
        let mut dictionary_pattern = CpdfDictionary::new();
        dictionary_pattern.set_matrix_for(b"Matrix", CfxMatrix::new(1.0, 0.0, 0.0, 1.0, 3.0, 5.0));
        let dictionary_pattern = retain_object(dictionary_pattern);
        let stream_dictionary = retain_object(CpdfDictionary::new());
        let stream_pattern = retain_object(CpdfStream::from_data_and_dict(
            Vec::new(),
            stream_dictionary.clone(),
        ));
        let holder = document.borrow().holder();
        let number = holder.borrow_mut().add_indirect_object(stream_pattern);
        {
            let mut borrowed = patterns.borrow_mut();
            let dict = borrowed
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap();
            dict.set_for(b"DictionaryPattern", Some(dictionary_pattern.clone()));
            dict.set_new_for(b"StreamPattern", CpdfReference::new(&holder, number));
            dict.set_new_for(
                b"NonPattern",
                crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject::number_from_signed(0),
            );
        }
        let resources = direct_entry(&page.dict(), b"Resources").unwrap();
        resources
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_for(b"Pattern", Some(patterns));
        let matrix = CfxMatrix::new(2.0, 0.0, 0.0, 4.0, 7.0, 9.0);
        assert!(transform_with_clip(
            &document.borrow(),
            &page,
            Some(matrix),
            None
        ));
        assert_eq!(
            dictionary_pattern
                .borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .unwrap()
                .get_matrix_for(b"Matrix"),
            CfxMatrix::new(2.0, 0.0, 0.0, 4.0, 13.0, 29.0)
        );
        assert_eq!(
            stream_dictionary
                .borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .unwrap()
                .get_matrix_for(b"Matrix"),
            matrix
        );
    }

    #[test]
    fn boxes_update_geometry_and_clip_serialization_handles_empty_and_cubic_paths() {
        let document = create_new_document(false, || None);
        let mut page = page_with_content(&document);
        assert_eq!(bounding_box(&page, b"CropBox"), None);
        set_bounding_box(
            &mut page,
            b"MediaBox",
            CfxFloatRect::new(0.0, 0.0, 70.0, 80.0),
        );
        assert_eq!((page.width(), page.height()), (70.0, 80.0));
        assert_eq!(
            bounding_box(&page, b"MediaBox"),
            Some([0.0, 0.0, 70.0, 80.0])
        );
        let mut clip = CpdfClipPath::new();
        clip.append_path(CpdfPath::new(), FillType::Winding);
        let mut path = CpdfPath::new();
        path.append_point(CfxPointF::new(1.0, 2.0), PointType::Move);
        path.append_point(CfxPointF::new(3.0, 4.0), PointType::Bezier);
        path.append_point(CfxPointF::new(5.0, 6.0), PointType::Bezier);
        path.append_point_and_close(CfxPointF::new(7.0, 8.0), PointType::Bezier);
        clip.append_path(path, FillType::Winding);
        assert!(insert_clip_path(&document.borrow(), &page, &clip));
        assert_eq!(
            content_streams(&page)[0],
            b"0 0 m W n 1 2 m\n3 4 5 6 7 8 c h\nW n\n"
        );
    }
}
