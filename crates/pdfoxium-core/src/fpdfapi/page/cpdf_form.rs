// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Form resource selection, content parsing, and bounds from
//! `core/fpdfapi/page/cpdf_form.{h,cpp}`.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::{Rc, Weak};

use super::cpdf_allstates::CpdfAllStates;
use super::cpdf_contentparser::CpdfContentParser;
use super::cpdf_docpagedata::CpdfDocPageData;
use super::cpdf_imageobject::CpdfImageObject;
use super::cpdf_pageobjectholder::CpdfPageObjectHolder;
use super::cpdf_path::CpdfPath;
use super::cpdf_streamcontentparser::CpdfStreamContentParser;
use crate::fpdfapi::font::cpdf_type3char::{BitmapRef, CpdfType3Char, Type3Form};
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix};
use crate::fxge::cfx_fillrenderoptions::FillType;
use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
use crate::fxge::dib::fx_dib::BlendMode;

pub struct CpdfForm {
    holder: CpdfPageObjectHolder,
    stream: ObjectRef,
    private_edit_stream: bool,
}

/// Safe counterpart of the CPDF_Form/CPDF_Font::FormIface upcast. The weak
/// document cache reference avoids a font -> form -> page-data ownership cycle.
pub(crate) struct CpdfType3Form {
    form: CpdfForm,
    page_data: Weak<CpdfDocPageData>,
}

impl CpdfType3Form {
    pub(crate) fn new(form: CpdfForm, page_data: Weak<CpdfDocPageData>) -> Self {
        Self { form, page_data }
    }
    pub(crate) fn form(&self) -> &CpdfForm {
        &self.form
    }
}

impl Type3Form for CpdfType3Form {
    fn parse_content_for_type3_char(&mut self, glyph: &mut CpdfType3Char) {
        if let Some(page_data) = self.page_data.upgrade() {
            self.form.parse_content_for_type3_char(glyph, page_data);
        }
    }
    fn has_page_objects(&self) -> bool {
        self.form.has_page_objects()
    }
    fn calc_bounding_box(&self) -> CfxFloatRect {
        self.form.calc_bounding_box()
    }
    fn get_bitmap_and_matrix_from_sole_image_of_form(&self) -> Option<(BitmapRef, CfxMatrix)> {
        let (bitmap, matrix) = self.form.get_bitmap_and_matrix_from_sole_image_of_form()?;
        Some((Rc::new(RefCell::new(bitmap)), matrix))
    }
}

impl CpdfForm {
    pub fn choose_resources_dict(
        resources: Option<ObjectRef>,
        parent_resources: Option<ObjectRef>,
        page_resources: Option<ObjectRef>,
    ) -> Option<ObjectRef> {
        resources.or(parent_resources).or(page_resources)
    }
    pub fn new(
        stream: ObjectRef,
        page_resources: Option<ObjectRef>,
        parent_resources: Option<ObjectRef>,
    ) -> Option<Self> {
        let dict = {
            let borrowed = stream.borrow();
            borrowed.as_any().downcast_ref::<CpdfStream>()?.get_dict()
        };
        let resources = {
            let borrowed = dict.borrow();
            borrowed
                .as_any()
                .downcast_ref::<CpdfDictionary>()?
                .get_dict_for(b"Resources")
        };
        let resources =
            Self::choose_resources_dict(resources, parent_resources, page_resources.clone());
        let mut holder = CpdfPageObjectHolder::new(dict, page_resources, resources);
        holder.load_transparency_info();
        Some(Self {
            holder,
            stream,
            private_edit_stream: false,
        })
    }
    /// Give this parsed form instance private serialized storage before editing
    /// descendants. The safe API uses instance-local edits; the C ABI retains
    /// its existing shared-stream behavior. Parsed child boxes are not cloned.
    /// The next content generation writes every child into the new stream.
    pub fn detach_stream_for_edit(
        &mut self,
        objects: &crate::fpdfapi::parser::cpdf_indirect_object_holder::HolderRef,
    ) {
        if self.private_edit_stream {
            return;
        }
        let dictionary = self.holder.dict().borrow().clone_object();
        let resources = self.holder.resources().map(|r| r.borrow().clone_object());
        if let Some(resources) = &resources {
            // Resource categories can themselves be indirect shared dictionaries.
            let keys = resources.borrow().as_dictionary().unwrap().get_keys();
            for key in keys {
                let entry = resources
                    .borrow()
                    .as_dictionary()
                    .unwrap()
                    .get_object_for(&key);
                let category = entry
                    .and_then(|entry| crate::fpdfapi::parser::cpdf_object::get_dict_object(&entry));
                if let Some(category) = category {
                    let copy = category.borrow().clone_object();
                    resources
                        .borrow_mut()
                        .as_any_mut()
                        .downcast_mut::<CpdfDictionary>()
                        .unwrap()
                        .set_for(&key, Some(copy));
                }
            }
            dictionary
                .borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap()
                .set_for(b"Resources", Some(resources.clone()));
        }
        self.stream = crate::fpdfapi::parser::cpdf_object::retain_object(
            CpdfStream::from_data_and_dict(Vec::new(), dictionary.clone()),
        );
        objects
            .borrow_mut()
            .add_indirect_object(self.stream.clone());
        let (data, _) = self.holder.split_for_generation();
        data.dict = dictionary;
        data.resources = resources;
        self.holder.mark_stream_dirty(0);
        self.private_edit_stream = true;
    }
    pub fn holder(&self) -> &CpdfPageObjectHolder {
        &self.holder
    }
    pub fn holder_mut(&mut self) -> &mut CpdfPageObjectHolder {
        &mut self.holder
    }
    pub fn stream(&self) -> &ObjectRef {
        &self.stream
    }
    pub fn has_page_objects(&self) -> bool {
        self.holder.active_page_object_count() != 0
    }
    pub fn get_bitmap_and_matrix_from_sole_image_of_form(
        &self,
    ) -> Option<(CfxDibitmap, CfxMatrix)> {
        if self.holder.active_page_object_count() != 1 {
            return None;
        }
        let image = self
            .holder
            .iter()
            .find(|object| object.base().is_active())?
            .as_any()
            .downcast_ref::<CpdfImageObject>()?;
        Some((image.get_independent_bitmap()?, image.matrix()))
    }
    pub fn calc_bounding_box(&self) -> CfxFloatRect {
        if !self.has_page_objects() {
            return CfxFloatRect::default();
        }
        let mut rect = CfxFloatRect::new(1_000_000.0, 1_000_000.0, -1_000_000.0, -1_000_000.0);
        for object in self.holder.iter() {
            if !object.base().is_active() {
                continue;
            }
            let item = object.base().rect();
            rect.left = rect.left.min(item.left);
            rect.right = rect.right.max(item.right);
            rect.bottom = rect.bottom.min(item.bottom);
            rect.top = rect.top.max(item.top);
        }
        rect
    }
    pub fn parse_content(
        &mut self,
        states: Option<CpdfAllStates>,
        parent_matrix: Option<CfxMatrix>,
        page_data: Rc<CpdfDocPageData>,
        visited: Rc<RefCell<HashSet<usize>>>,
        depth: usize,
    ) {
        self.parse_content_internal(states, parent_matrix, page_data, visited, depth, None);
    }

    pub fn parse_content_for_type3_char(
        &mut self,
        glyph: &mut CpdfType3Char,
        page_data: Rc<CpdfDocPageData>,
    ) {
        self.parse_content_internal(
            None,
            None,
            page_data,
            Rc::new(RefCell::new(HashSet::new())),
            0,
            Some(glyph),
        );
    }

    fn parse_content_internal(
        &mut self,
        states: Option<CpdfAllStates>,
        parent_matrix: Option<CfxMatrix>,
        page_data: Rc<CpdfDocPageData>,
        visited: Rc<RefCell<HashSet<usize>>>,
        depth: usize,
        type3_char: Option<&mut CpdfType3Char>,
    ) {
        if self.holder.parse_state() != super::cpdf_pageobjectholder::ParseState::NotParsed {
            return;
        }
        self.holder.bind_page_data(&page_data);
        let (mut form_matrix, bbox) = {
            let borrowed = self.holder.dict().borrow();
            let dict = borrowed
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .expect("form dictionary");
            (
                dict.get_matrix_for(b"Matrix"),
                dict.get_array_for(b"BBox")
                    .map(|_| dict.get_rect_for(b"BBox")),
            )
        };
        if let Some(states) = &states {
            form_matrix.concat(states.current_transformation_matrix());
        }
        let mut parser = CpdfStreamContentParser::with_page_data_and_forms(
            states,
            parent_matrix.unwrap_or_default(),
            page_data,
            visited,
            depth,
        );
        parser
            .cur_states_mut()
            .set_current_transformation_matrix(form_matrix);
        parser.cur_states_mut().set_parent_matrix(form_matrix);
        if let Some(bbox) = bbox {
            let mut clip = CpdfPath::new();
            clip.append_float_rect(bbox);
            clip.transform(form_matrix);
            if let Some(parent_matrix) = parent_matrix {
                clip.transform(parent_matrix);
            }
            parser
                .cur_states_mut()
                .mutable_clip_path()
                .append_path_with_auto_merge(clip, FillType::Winding);
            let mut bbox = form_matrix.transform_float_rect(bbox);
            if let Some(parent_matrix) = parent_matrix {
                bbox = parent_matrix.transform_float_rect(bbox);
            }
            self.holder.set_bbox(bbox);
        }
        if self.holder.transparency().is_group() {
            let state = parser.cur_states_mut().mutable_general_state();
            state.set_blend_type(BlendMode::Normal);
            state.set_stroke_alpha(1.0);
            state.set_fill_alpha(1.0);
            state.set_soft_mask(None);
        }
        CpdfContentParser::from_stream(&self.stream).parse_all_for_type3(
            &mut self.holder,
            &mut parser,
            type3_char,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

    #[test]
    fn detach_edit_stream_keeps_child_identity_and_detaches_resources_once() {
        use crate::fpdfapi::page::cpdf_pathobject::CpdfPathObject;
        use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
        let document = CpdfDocument::new();
        let mut resource_dict = CpdfDictionary::new();
        resource_dict.set_new_dictionary_for(b"XObject");
        let resources = retain_object(resource_dict);
        let mut dict = CpdfDictionary::new();
        dict.set_for(b"Resources", Some(resources.clone()));
        let stream = retain_object(CpdfStream::from_data_and_dict(
            b"original".to_vec(),
            retain_object(dict),
        ));
        let mut form = CpdfForm::new(stream.clone(), None, None).unwrap();
        form.holder_mut()
            .append_page_object(Box::new(CpdfPathObject::default()));
        let id = form.holder().page_object_id_by_index(0).unwrap();
        form.detach_stream_for_edit(&document.holder());
        assert!(!Rc::ptr_eq(form.stream(), &stream));
        assert_ne!(form.stream().borrow().obj_num(), 0);
        assert!(!Rc::ptr_eq(form.holder().resources().unwrap(), &resources));
        assert_eq!(form.holder().page_object_id_by_index(0), Some(id));
        assert!(form.holder().has_dirty_streams());
        let detached = form.stream().clone();
        form.detach_stream_for_edit(&document.holder());
        assert!(Rc::ptr_eq(form.stream(), &detached));
    }

    #[test]
    fn sole_image_form_returns_independent_bitmap_and_matrix() {
        let stream = retain_object(CpdfStream::from_bytes(b""));
        let mut form = CpdfForm::new(stream, None, None).unwrap();
        assert!(form
            .get_bitmap_and_matrix_from_sole_image_of_form()
            .is_none());
        let mut image_dict = CpdfDictionary::new();
        image_dict.set_new_for(b"Width", CpdfScalarObject::number_from_signed(1));
        image_dict.set_new_for(b"Height", CpdfScalarObject::number_from_signed(1));
        image_dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
        image_dict.set_new_name_for(b"ColorSpace", b"DeviceRGB");
        let image = Rc::new(
            super::super::cpdf_image::CpdfImage::from_stream(
                retain_object(CpdfStream::from_data_and_dict(
                    vec![10, 20, 30],
                    retain_object(image_dict),
                )),
                false,
            )
            .unwrap(),
        );
        let mut object = CpdfImageObject::new(0);
        let matrix = CfxMatrix::new(2.0, 0.0, 0.0, 3.0, 4.0, 5.0);
        object.set_image(Some(image));
        object.set_initial_image_matrix(matrix);
        form.holder_mut().append_page_object(Box::new(object));
        let (mut first, actual_matrix) = form
            .get_bitmap_and_matrix_from_sole_image_of_form()
            .unwrap();
        assert_eq!(actual_matrix, matrix);
        assert_eq!(&first.buffer()[..3], &[30, 20, 10]);
        first.buffer_mut()[0] = 0;
        let (second, _) = form
            .get_bitmap_and_matrix_from_sole_image_of_form()
            .unwrap();
        assert_eq!(&second.buffer()[..3], &[30, 20, 10]);
    }

    #[test]
    fn type3_form_parse_passes_d0_and_d1_metrics_to_glyph() {
        let page_data = Rc::new(CpdfDocPageData::new());
        let stream = retain_object(CpdfStream::from_bytes(b"0.5 0 0 0 1 1 d1"));
        let mut form = CpdfForm::new(stream, None, None).unwrap();
        let mut glyph = CpdfType3Char::new();
        form.parse_content_for_type3_char(&mut glyph, Rc::clone(&page_data));
        assert!(!glyph.colored());
        assert_eq!(glyph.width(), 500);
        assert_eq!(
            glyph.bbox(),
            crate::fxcrt::fx_coordinates::FxRect::new(0, 1000, 1000, 0)
        );

        let stream = retain_object(CpdfStream::from_bytes(b"0.25 0 d0"));
        let mut form = CpdfForm::new(stream, None, None).unwrap();
        let mut glyph = CpdfType3Char::new();
        form.parse_content_for_type3_char(&mut glyph, page_data);
        assert!(glyph.colored());
        assert_eq!(glyph.width(), 250);
    }
}
