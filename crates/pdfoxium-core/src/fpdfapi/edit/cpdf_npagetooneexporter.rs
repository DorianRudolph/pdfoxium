// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2024 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Direct port of `core/fpdfapi/edit/cpdf_npagetooneexporter.{h,cpp}`.
use super::cpdf_pageorganizer::{dict, dict_mut, CpdfPageOrganizer};
use crate::fpdfapi::page::cpdf_page::CpdfPage;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_object::{get_direct_object, retain_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
use crate::fpdfapi::parser::cpdf_scalar_object::{CpdfScalarObject, ObjectType};
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fpdfapi::parser::cpdf_stream_acc::CpdfRetainedStreamAcc;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF, CfxSizeF};
use crate::fxcrt::stream_write_utils::write_matrix;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::{Rc, Weak};

pub struct XObjectContext {
    // Native UnownedPtr: the context never keeps the destination alive.
    pub dest_doc: Weak<RefCell<CpdfDocument>>,
    pub xobject: ObjectRef,
}
#[derive(Clone, Copy, Default, Debug)]
pub struct NupPageSettings {
    pub sub_page_start_point: CfxPointF,
    pub scale: f32,
}
struct NupState {
    pages_on_x_axis: usize,
    pages_on_y_axis: usize,
    pages_per_sheet: usize,
    sub_page_size: CfxSizeF,
    sub_page_index: usize,
}
impl NupState {
    fn new(size: CfxSizeF, x: usize, y: usize) -> Self {
        debug_assert!(x > 0 && y > 0 && size.width > 0. && size.height > 0.);
        Self {
            pages_on_x_axis: x,
            pages_on_y_axis: y,
            pages_per_sheet: x * y,
            sub_page_size: CfxSizeF::new(size.width / x as f32, size.height / y as f32),
            sub_page_index: 0,
        }
    }
    fn convert_page_order(&self) -> (usize, usize) {
        (
            self.sub_page_index % self.pages_on_x_axis,
            self.pages_on_y_axis - self.sub_page_index / self.pages_on_x_axis - 1,
        )
    }
    fn calculate_page_edit(&self, x: usize, y: usize, size: CfxSizeF) -> NupPageSettings {
        let mut settings = NupPageSettings {
            sub_page_start_point: CfxPointF::new(
                x as f32 * self.sub_page_size.width,
                y as f32 * self.sub_page_size.height,
            ),
            scale: 0.,
        };
        let x_scale = self.sub_page_size.width / size.width;
        let y_scale = self.sub_page_size.height / size.height;
        // std::min preserves its first argument on unordered/equal comparisons.
        settings.scale = if y_scale < x_scale { y_scale } else { x_scale };
        let width = size.width * settings.scale;
        let height = size.height * settings.scale;
        if x_scale > y_scale {
            settings.sub_page_start_point.x += (self.sub_page_size.width - width) / 2.;
        } else {
            settings.sub_page_start_point.y += (self.sub_page_size.height - height) / 2.;
        }
        settings
    }
    fn calculate_new_page_position(&mut self, size: CfxSizeF) -> NupPageSettings {
        if self.sub_page_index >= self.pages_per_sheet {
            self.sub_page_index = 0;
        }
        let (x, y) = self.convert_page_order();
        self.sub_page_index += 1;
        self.calculate_page_edit(x, y, size)
    }
}
fn generate_sub_page_content_stream(name: &[u8], settings: NupPageSettings) -> Vec<u8> {
    let mut matrix = CfxMatrix::default();
    matrix.scale(settings.scale, settings.scale);
    matrix.translate(
        settings.sub_page_start_point.x,
        settings.sub_page_start_point.y,
    );
    let mut content = String::from("q\n");
    write_matrix(&mut content, matrix).unwrap();
    content.push_str(" cm\n/");
    let mut content = content.into_bytes();
    content.extend_from_slice(name);
    content.extend_from_slice(b" Do Q\n");
    content
}

pub struct CpdfNPageToOneExporter {
    organizer: CpdfPageOrganizer,
    object_number: u32,
    xobject_name_to_number_map: BTreeMap<Vec<u8>, u32>,
    src_page_xobject_map: BTreeMap<u32, (Vec<u8>, u32)>,
}
impl CpdfNPageToOneExporter {
    pub fn new(dest: Rc<RefCell<CpdfDocument>>, src: Rc<RefCell<CpdfDocument>>) -> Self {
        Self {
            organizer: CpdfPageOrganizer::new(dest, src),
            object_number: 0,
            xobject_name_to_number_map: BTreeMap::new(),
            src_page_xobject_map: BTreeMap::new(),
        }
    }
    pub fn generate_sub_page_content_stream_for_testing(
        name: &[u8],
        settings: NupPageSettings,
    ) -> Vec<u8> {
        generate_sub_page_content_stream(name, settings)
    }
    pub fn export_n_pages_to_one(
        &mut self,
        indices: &[u32],
        size: CfxSizeF,
        x: usize,
        y: usize,
    ) -> bool {
        if x == 0
            || y == 0
            || !size.width.is_finite()
            || !size.height.is_finite()
            || size.width <= 0.
            || size.height <= 0.
        {
            return false;
        }
        if !self.organizer.init() {
            return false;
        }
        let Some(per_sheet) = x.checked_mul(y) else {
            return false;
        };
        self.organizer.clear_object_number_map();
        self.src_page_xobject_map.clear();
        let mut state = NupState::new(size, x, y);
        let mut current = 0i32;
        let rect = CfxFloatRect::new(0., 0., size.width, size.height);
        let mut outer = 0usize;
        while outer < indices.len() {
            self.xobject_name_to_number_map.clear();
            let Some(page_dict) = self.organizer.dest.borrow_mut().create_new_page(current) else {
                return false;
            };
            dict_mut(&page_dict, |d| d.set_rect_for(b"MediaBox", rect));
            let mut content = Vec::new();
            let inner_max = outer.wrapping_add(per_sheet).min(indices.len());
            for &index in &indices[outer..inner_max] {
                let Some(page_dict) = self
                    .organizer
                    .src
                    .borrow_mut()
                    .page_dictionary(index as usize)
                else {
                    return false;
                };
                let page_data = self.organizer.src.borrow().page_data_ref();
                let Some(page) = CpdfPage::new_with_page_data(page_dict, page_data) else {
                    return false;
                };
                // CpdfPage constructs its image cache along with the page.
                let settings = state.calculate_new_page_position(page.page_size());
                content.extend(self.add_sub_page(&page, settings));
            }
            self.finish_page(&page_dict, &content);
            current = current
                .checked_add(1)
                .expect("native checked page index overflow");
            outer = outer.wrapping_add(per_sheet);
        }
        self.organizer.copy_optional_content()
    }
    fn add_sub_page(&mut self, page: &CpdfPage, settings: NupPageSettings) -> Vec<u8> {
        let number = page.dict().borrow().obj_num();
        let name = match self.src_page_xobject_map.get(&number) {
            Some((name, number)) => {
                // A cached form still needs a binding on each output sheet.
                self.xobject_name_to_number_map
                    .insert(name.clone(), *number);
                name.clone()
            }
            None => self.make_xobject_from_page(page),
        };
        generate_sub_page_content_stream(&name, settings)
    }
    fn make_xobject_from_page_raw(&mut self, page: &CpdfPage) -> ObjectRef {
        let source = page.dict();
        let contents =
            dict(&source, |d| d.get_object_for(b"Contents")).and_then(|o| get_direct_object(&o));
        let holder = self.organizer.dest.borrow().holder();
        let dictionary = holder.borrow().new_dictionary();
        let xobject = retain_object(CpdfStream::from_dict(Rc::clone(&dictionary)));
        holder.borrow_mut().add_indirect_object(Rc::clone(&xobject));
        if !CpdfPageOrganizer::copy_inheritable(&dictionary, &source, b"Resources") {
            dict_mut(&dictionary, |d| {
                d.set_new_for(
                    b"Resources",
                    CpdfDictionary::with_pool(d.byte_string_pool()),
                )
            });
        }
        // References target the indirect stream, not its inline dictionary.
        self.organizer
            .add_object_mapping(source.borrow().obj_num(), xobject.borrow().obj_num());
        self.organizer.update_reference(&dictionary);
        dict_mut(&dictionary, |d| {
            d.set_new_name_for(b"Type", b"XObject");
            d.set_new_name_for(b"Subtype", b"Form");
            d.set_new_for(b"FormType", CpdfScalarObject::number_from_signed(1));
            d.set_rect_for(b"BBox", page.bbox());
            d.set_matrix_for(b"Matrix", page.page_matrix());
        });
        let Some(contents) = contents else {
            return xobject;
        };
        let is_array = contents.borrow().object_type() == ObjectType::Array;
        if !is_array {
            // A null/non-stream StreamAcc reads an empty span in the source.
            if let Some(acc) = CpdfRetainedStreamAcc::load_filtered(contents) {
                xobject
                    .borrow_mut()
                    .as_any_mut()
                    .downcast_mut::<CpdfStream>()
                    .unwrap()
                    .set_data_and_remove_filter(&acc.get_span());
            } else {
                xobject
                    .borrow_mut()
                    .as_any_mut()
                    .downcast_mut::<CpdfStream>()
                    .unwrap()
                    .set_data_and_remove_filter(b"");
            }
            return xobject;
        }
        let mut content = Vec::new();
        let count = contents
            .borrow()
            .as_any()
            .downcast_ref::<CpdfArray>()
            .unwrap()
            .size();
        for i in 0..count {
            // Release the array borrow before resolving a lazy reference and
            // invoking the embedder's read callback.
            let object = contents
                .borrow()
                .as_any()
                .downcast_ref::<CpdfArray>()
                .unwrap()
                .get_object_at(i);
            let stream = object.and_then(|object| get_direct_object(&object));
            if let Some(acc) = stream.and_then(CpdfRetainedStreamAcc::load_filtered) {
                content.extend_from_slice(&acc.get_span());
            }
            content.push(b'\n');
        }
        xobject
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfStream>()
            .unwrap()
            .set_data_and_remove_filter(&content);
        xobject
    }
    fn make_xobject_from_page(&mut self, page: &CpdfPage) -> Vec<u8> {
        let xobject = self.make_xobject_from_page_raw(page);
        self.object_number = self.object_number.wrapping_add(1);
        // Native %d prints the uint32_t's signed representation.
        let name = format!("X{}", self.object_number as i32).into_bytes();
        self.xobject_name_to_number_map
            .insert(name.clone(), xobject.borrow().obj_num());
        self.src_page_xobject_map.insert(
            page.dict().borrow().obj_num(),
            (name.clone(), xobject.borrow().obj_num()),
        );
        name
    }
    pub fn create_xobject_context_from_page(&mut self, index: i32) -> Option<XObjectContext> {
        let dictionary = self
            .organizer
            .src
            .borrow_mut()
            .page_dictionary(index as usize)?;
        let page_data = self.organizer.src.borrow().page_data_ref();
        let page = CpdfPage::new_with_page_data(dictionary, page_data)?;
        let xobject = self.make_xobject_from_page_raw(&page);
        if !self.organizer.copy_optional_content() {
            return None;
        }
        Some(XObjectContext {
            dest_doc: Rc::downgrade(&self.organizer.dest),
            xobject,
        })
    }
    fn finish_page(&self, page: &ObjectRef, content: &[u8]) {
        let resources = dict_mut(page, |d| d.get_or_create_dict_for(b"Resources"));
        let xobjects = dict_mut(&resources, |d| d.get_or_create_dict_for(b"XObject"));
        let holder = self.organizer.dest.borrow().holder();
        for (name, &number) in &self.xobject_name_to_number_map {
            dict_mut(&xobjects, |d| {
                d.set_new_for(name, CpdfReference::new(&holder, number))
            });
        }
        let dictionary = holder.borrow().new_dictionary();
        let mut stream = CpdfStream::from_dict(dictionary);
        stream.set_data(content);
        let number = holder
            .borrow_mut()
            .add_indirect_object(retain_object(stream));
        dict_mut(page, |d| {
            d.set_new_for(b"Contents", CpdfReference::new(&holder, number))
        });
    }
}
#[cfg(test)]
#[path = "cpdf_npagetooneexporter_tests.rs"]
mod tests;
#[cfg(test)]
#[path = "cpdf_npagetooneexporter_unittest.rs"]
mod upstream_tests;
