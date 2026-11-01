// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Direct port of `core/fpdfapi/edit/cpdf_pagecontentgenerator.{h,cpp}`.
//! Exclusive holder access freezes the native pointer snapshot. Splitting its
//! state from its object deque permits resource mutation without moving objects.

use super::cpdf_pagecontentmanager::CpdfPageContentManager;
use super::cpdf_stringarchivestream::CpdfStringArchiveStream;
use crate::fpdfapi::font::cpdf_font::CpdfFont;
use crate::fpdfapi::page::cpdf_color::CpdfColor;
use crate::fpdfapi::page::cpdf_contentmarkitem::ParamType;
use crate::fpdfapi::page::cpdf_contentmarks::CpdfContentMarks;
use crate::fpdfapi::page::cpdf_formobject::CpdfFormObject;
use crate::fpdfapi::page::cpdf_imageobject::CpdfImageObject;
use crate::fpdfapi::page::cpdf_page::CpdfPage;
use crate::fpdfapi::page::cpdf_pageobject::{CpdfPageObject, PageObject, PageObjectType};
use crate::fpdfapi::page::cpdf_pageobjectholder::{
    CpdfPageObjectHolder, CpdfPageObjectHolderData, GraphicsData,
};
use crate::fpdfapi::page::cpdf_path::CpdfPath;
use crate::fpdfapi::page::cpdf_pathobject::CpdfPathObject;
use crate::fpdfapi::page::cpdf_shadingobject::CpdfShadingObject;
use crate::fpdfapi::page::cpdf_textobject::CpdfTextObject;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
use crate::fpdfapi::parser::cpdf_scalar_object::{CpdfScalarObject, ObjectType};
use crate::fpdfapi::parser::fpdf_parser_decode::pdf_hex_encode_string;
use crate::fpdfapi::parser::fpdf_parser_utility::pdf_name_encode;
use crate::fpdfapi::parser::object_tree_traversal_util::get_objects_with_multiple_references;
use crate::fxcrt::fx_coordinates::{CfxMatrix, CfxPointF};
use crate::fxcrt::stream_write_utils::float_to_decimal;
use crate::fxge::cfx_fillrenderoptions::FillType;
use crate::fxge::cfx_graphstatedata::{LineCap, LineJoin};
use crate::fxge::cfx_path::PointType;
use crate::fxge::dib::fx_dib::BlendMode;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

const RESOURCE_KEYS: [&[u8]; 4] = [b"ExtGState", b"Font", b"XObject", b"Shading"];
type ResourcesMap = BTreeMap<Vec<u8>, BTreeSet<Vec<u8>>>;
fn dict<R>(o: &ObjectRef, f: impl FnOnce(&CpdfDictionary) -> R) -> R {
    let b = o.borrow();
    f(b.as_any().downcast_ref().unwrap())
}
fn dict_mut<R>(o: &ObjectRef, f: impl FnOnce(&mut CpdfDictionary) -> R) -> R {
    let mut b = o.borrow_mut();
    f(b.as_any_mut().downcast_mut().unwrap())
}
fn write_float(b: &mut Vec<u8>, v: f32) {
    b.extend_from_slice(float_to_decimal(v).as_bytes());
}
fn write_point(b: &mut Vec<u8>, p: CfxPointF) {
    write_float(b, p.x);
    b.push(b' ');
    write_float(b, p.y);
}
fn write_matrix(b: &mut Vec<u8>, m: CfxMatrix) {
    for (i, v) in [m.a, m.b, m.c, m.d, m.e, m.f].into_iter().enumerate() {
        if i > 0 {
            b.push(b' ');
        }
        write_float(b, v);
    }
}
fn write_name(b: &mut Vec<u8>, n: &[u8]) {
    b.push(b'/');
    b.extend_from_slice(&pdf_name_encode(n));
}
fn write_color(b: &mut Vec<u8>, c: Option<&CpdfColor>) -> bool {
    let Some(c) = c else { return false };
    if !c.is_color_space_rgb() && !c.is_color_space_gray() {
        return false;
    }
    let Some(c) = c.get_rgb() else { return false };
    write_float(b, c.red);
    b.push(b' ');
    write_float(b, c.green);
    b.push(b' ');
    write_float(b, c.blue);
    true
}
fn finish_marks(b: &mut Vec<u8>, m: &CpdfContentMarks) {
    for _ in 0..m.count_items() {
        b.extend_from_slice(b"EMC\n");
    }
}
fn process_content_marks(
    b: &mut Vec<u8>,
    object: &dyn PageObject,
    prev: &CpdfContentMarks,
) -> CpdfContentMarks {
    let next = object.base().content_marks();
    let first = prev.find_first_difference(next);
    for _ in first..prev.count_items() {
        b.extend_from_slice(b"EMC\n");
    }
    for i in first..next.count_items() {
        let item = next.get_item(i);
        let item = item.borrow();
        write_name(b, item.name());
        b.push(b' ');
        match item.param_type() {
            ParamType::None => {
                b.extend_from_slice(b"BMC\n");
                continue;
            }
            ParamType::DirectDict => {
                let param = item.param().unwrap();
                let _ = param
                    .borrow()
                    .write_to(&mut CpdfStringArchiveStream::new(b), None);
                b.push(b' ');
            }
            ParamType::PropertiesDict => {
                write_name(b, item.property_name());
                b.push(b' ');
            }
        }
        b.extend_from_slice(b"BDC\n");
    }
    next.clone()
}
fn record_usage(object: &dyn PageObject, seen: &mut ResourcesMap) {
    let name = object.base().resource_name();
    if !name.is_empty() {
        let key = match object.object_type() {
            PageObjectType::Text => Some(b"Font".as_slice()),
            PageObjectType::Image | PageObjectType::Form => Some(b"XObject".as_slice()),
            PageObjectType::Shading => Some(b"Shading".as_slice()),
            _ => None,
        };
        if let Some(key) = key {
            seen.entry(key.to_vec()).or_default().insert(name.to_vec());
        }
    }
    for name in object.base().graphics_resource_names().iter() {
        assert!(!name.is_empty());
        seen.entry(b"ExtGState".to_vec())
            .or_default()
            .insert(name.clone());
    }
}
fn remove_or_restore_unused_resources(
    resources: &ObjectRef,
    in_use: &ResourcesMap,
    data: &mut CpdfPageObjectHolderData,
) {
    for key in RESOURCE_KEYS {
        let used = in_use.get(key);
        let current = dict(resources, |d| d.get_dict_for(key));
        if current.is_none() && used.is_none() {
            continue;
        }
        // Native dereferences current_resource_dict here, before its later create branch.
        let current = current.expect("in-use resource type must have a dictionary");
        let keys = dict(&current, CpdfDictionary::get_keys);
        let mut removed = BTreeMap::new();
        for name in &keys {
            if used.is_some_and(|s| s.contains(name)) {
                continue;
            }
            removed.insert(name.clone(), dict_mut(&current, |d| d.remove_for(name)));
        }
        if !removed.is_empty() {
            data.all_removed_resources_map
                .entry(key.to_vec())
                .or_default()
                .extend(removed);
        }
        let Some(saved) = data.all_removed_resources_map.get_mut(key) else {
            continue;
        };
        let Some(used) = used else { continue };
        for name in used {
            if !keys.contains(name) && saved.contains_key(name) {
                let value = saved.remove(name).expect("saved resource exists");
                dict_mut(&current, |d| d.set_for(name, value));
            }
        }
    }
}
fn is_page_resource_shared(
    doc: &mut CpdfDocument,
    page: &ObjectRef,
    resources: &ObjectRef,
) -> bool {
    let number = resources.borrow().obj_num();
    if number != 0 && get_objects_with_multiple_references(doc).contains(&number) {
        return true;
    }
    let (mut pages, mut seen) = (0, 0);
    for i in 0..doc.page_count() {
        let Some(current) = doc.page_dictionary(i) else {
            continue;
        };
        if Rc::ptr_eq(&current, page) {
            pages += 1;
            if pages == 2 {
                return true;
            }
        }
        let page = CpdfPage::new(current).expect("page dictionary");
        if page.resources().is_some_and(|r| Rc::ptr_eq(&r, resources)) {
            seen += 1;
            if seen == 2 {
                return true;
            }
        }
    }
    false
}
fn clone_resources_dict_entries(doc: &CpdfDocument, resources: &ObjectRef, cloned_parent: bool) {
    let entries = dict(resources, |d| {
        d.with_locked_entries(|entries| {
            entries
                .iter()
                .filter_map(|(key, value)| {
                    if !Rc::ptr_eq(value, resources)
                        && value.borrow().object_type() == ObjectType::Reference
                        && RESOURCE_KEYS.contains(&key.as_ref())
                    {
                        let direct = value
                            .borrow()
                            .as_any()
                            .downcast_ref::<CpdfReference>()
                            .unwrap()
                            .get_direct();
                        direct.map(|v| (key.to_vec(), v))
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        })
    });
    if entries.is_empty() {
        return;
    }
    let shared = get_objects_with_multiple_references(doc);
    if shared.is_empty() && !cloned_parent {
        return;
    }
    for (key, object) in entries {
        if cloned_parent || shared.contains(&object.borrow().obj_num()) {
            let cloned = object.borrow().clone_object();
            let holder = doc.holder();
            let number = holder.borrow_mut().add_indirect_object(cloned);
            dict_mut(resources, |d| {
                d.set_new_for(&key, CpdfReference::new(&holder, number));
            });
        }
    }
}

pub struct CpdfPageContentGenerator<'holder, 'document> {
    holder: &'holder mut CpdfPageObjectHolder,
    document: Option<&'document mut CpdfDocument>,
    object_count: usize,
    default_graphics_name: Vec<u8>,
}
impl<'holder, 'document> CpdfPageContentGenerator<'holder, 'document> {
    pub fn new(
        holder: &'holder mut CpdfPageObjectHolder,
        document: Option<&'document mut CpdfDocument>,
    ) -> Self {
        let object_count = holder.page_object_count();
        Self {
            holder,
            document,
            object_count,
            default_graphics_name: Vec::new(),
        }
    }
    pub fn holder(&self) -> &CpdfPageObjectHolder {
        self.holder
    }
    pub fn generate_content(&mut self) {
        let streams = self.generate_modified_streams();
        if streams.is_empty() {
            return;
        }
        self.update_content_streams(streams);
        self.update_resources_dict();
    }
    fn generate_modified_streams(&mut self) -> BTreeMap<i32, Vec<u8>> {
        let mut dirty = BTreeSet::new();
        for object in self.holder.iter().take(self.object_count) {
            if object.base().is_dirty() {
                dirty.insert(object.base().content_stream());
            }
        }
        dirty.extend(self.holder.take_dirty_streams());
        let mut streams = BTreeMap::new();
        let mut empty = BTreeSet::new();
        let mut marks = BTreeMap::new();
        for &index in &dirty {
            let mut b = b"q\n".to_vec();
            let ctm = self.holder.ctm_at_beginning_of_stream(index);
            if !ctm.is_identity() {
                write_matrix(&mut b, ctm.get_inverse());
                b.extend_from_slice(b" cm\n");
            }
            self.process_default_graphics(&mut b);
            streams.insert(index, b);
            empty.insert(index);
            marks.insert(index, CpdfContentMarks::new());
        }
        {
            let (data, objects) = self.holder.split_for_generation();
            let mut context = GeneratorContext {
                data,
                document: self.document.as_deref_mut(),
            };
            for object in objects.iter_mut().take(self.object_count) {
                if !object.base().is_active() {
                    continue;
                }
                let index = object.base().content_stream();
                let Some(b) = streams.get_mut(&index) else {
                    continue;
                };
                empty.remove(&index);
                let previous = marks.get(&index).unwrap();
                let next = process_content_marks(b, object.as_ref(), previous);
                marks.insert(index, next);
                context.process_page_object(b, object.as_mut());
            }
        }
        for index in dirty {
            let mut previous = CfxMatrix::default();
            let mut ctm = CfxMatrix::default();
            let affects = if index == 0 {
                ctm = self.holder.ctm_at_end_of_stream(index);
                !ctm.is_identity()
            } else if index > 0 {
                previous = self.holder.ctm_at_end_of_stream(index - 1);
                ctm = self.holder.ctm_at_end_of_stream(index);
                previous != ctm
            } else {
                assert_eq!(index, CpdfPageObject::NO_CONTENT_STREAM);
                false
            };
            let b = streams.get_mut(&index).unwrap();
            let is_empty = empty.contains(&index);
            if is_empty && !affects {
                b.clear();
                continue;
            }
            if !is_empty {
                finish_marks(b, &marks[&index]);
            }
            b.extend_from_slice(b"Q\n");
            if affects {
                let diff = previous.get_inverse() * ctm;
                if !diff.is_identity() {
                    write_matrix(b, diff);
                    b.extend_from_slice(b" cm\n");
                }
            }
        }
        streams
    }
    fn update_content_streams(&mut self, streams: BTreeMap<i32, Vec<u8>>) {
        assert!(!streams.is_empty());
        self.default_graphics_name = self.get_or_create_default_graphics();
        let document = self.document.as_deref().expect("document");
        let mut manager = CpdfPageContentManager::new(self.holder, document);
        for (index, b) in streams {
            let position = i64::try_from(b.len()).unwrap();
            if index == CpdfPageObject::NO_CONTENT_STREAM {
                let new_index =
                    i32::try_from(manager.add_stream(&b, position)).expect("content stream index");
                for i in 0..self.object_count {
                    let object = manager
                        .page_object_holder_mut()
                        .page_object_by_index_mut(i)
                        .unwrap();
                    if object.base().is_active()
                        && object.base().content_stream() == CpdfPageObject::NO_CONTENT_STREAM
                    {
                        object.base_mut().set_content_stream(new_index);
                    }
                }
                continue;
            }
            let index = index as usize;
            if manager.has_stream_at_index(index) {
                manager.update_stream(index, &b, position);
            } else {
                manager.add_stream(&b, position);
            }
        }
    }
    fn update_resources_dict(&mut self) {
        let Some(mut resources) = self.holder.resources().cloned() else {
            return;
        };
        let doc = self.document.as_deref_mut().expect("document");
        let cloned_parent = is_page_resource_shared(doc, self.holder.dict(), &resources);
        if cloned_parent {
            let cloned_resources = resources.borrow().clone_object();
            resources = cloned_resources;
            let objects = doc.holder();
            let number = objects
                .borrow_mut()
                .add_indirect_object(Rc::clone(&resources));
            self.holder.set_resources(Some(Rc::clone(&resources)));
            dict_mut(self.holder.dict(), |d| {
                d.set_new_for(b"Resources", CpdfReference::new(&objects, number));
            });
        }
        // Children of a cloned resource dictionary now have a second owner.
        // The traversal's cycle heuristic can omit inherited-resource edges,
        // so these entries must be detached even if its snapshot says unique.
        clone_resources_dict_entries(doc, &resources, cloned_parent);
        let mut seen = ResourcesMap::new();
        for object in self.holder.iter().take(self.object_count) {
            if object.base().is_active() {
                record_usage(object.as_ref(), &mut seen);
            }
        }
        if !self.default_graphics_name.is_empty() {
            seen.entry(b"ExtGState".to_vec())
                .or_default()
                .insert(self.default_graphics_name.clone());
        }
        let (data, _) = self.holder.split_for_generation();
        remove_or_restore_unused_resources(&resources, &seen, data);
    }
    pub fn process_page_objects(&mut self, b: &mut Vec<u8>) -> bool {
        let mut dirty = false;
        let mut marks = CpdfContentMarks::new();
        let (data, objects) = self.holder.split_for_generation();
        let mut context = GeneratorContext {
            data,
            document: self.document.as_deref_mut(),
        };
        for object in objects.iter_mut().take(self.object_count) {
            if context.data.is_page && (!object.base().is_dirty() || !object.base().is_active()) {
                continue;
            }
            dirty = true;
            marks = process_content_marks(b, object.as_ref(), &marks);
            context.process_page_object(b, object.as_mut());
        }
        finish_marks(b, &marks);
        dirty
    }
    #[cfg(test)]
    pub(crate) fn process_path(&mut self, b: &mut Vec<u8>, o: &mut CpdfPathObject) {
        let (data, _) = self.holder.split_for_generation();
        GeneratorContext {
            data,
            document: self.document.as_deref_mut(),
        }
        .process_path(b, o);
    }
    #[cfg(test)]
    pub(crate) fn process_text(&mut self, b: &mut Vec<u8>, o: &mut CpdfTextObject) {
        let (data, _) = self.holder.split_for_generation();
        GeneratorContext {
            data,
            document: self.document.as_deref_mut(),
        }
        .process_text(b, o);
    }
    fn get_or_create_default_graphics(&mut self) -> Vec<u8> {
        let (data, _) = self.holder.split_for_generation();
        GeneratorContext {
            data,
            document: self.document.as_deref_mut(),
        }
        .get_or_create_default_graphics()
    }
    fn process_default_graphics(&mut self, b: &mut Vec<u8>) {
        b.extend_from_slice(b"0 0 0 RG 0 0 0 rg 1 w 0 J 0 j\n");
        self.default_graphics_name = self.get_or_create_default_graphics();
        write_name(b, &self.default_graphics_name);
        b.extend_from_slice(b" gs ");
    }
}
struct GeneratorContext<'a, 'doc> {
    data: &'a mut CpdfPageObjectHolderData,
    document: Option<&'doc mut CpdfDocument>,
}
impl GeneratorContext<'_, '_> {
    fn realize_resource(&mut self, resource: &ObjectRef, kind: &[u8]) -> Vec<u8> {
        let objects = self.document.as_deref().expect("document").holder();
        if self.data.resources.is_none() {
            let resources = objects.borrow_mut().new_indirect_dictionary();
            let number = resources.borrow().obj_num();
            self.data.resources = Some(resources);
            dict_mut(&self.data.dict, |d| {
                d.set_new_for(b"Resources", CpdfReference::new(&objects, number));
            });
        }
        // A category can refer back to the resources dictionary. Release the
        // lookup borrow before mutating it, as the native getter permits this.
        let resources = dict(self.data.resources.as_ref().unwrap(), |d| {
            d.get_dict_for(kind)
        })
        .unwrap_or_else(|| {
            dict_mut(self.data.resources.as_ref().unwrap(), |d| {
                d.set_new_dictionary_for(kind)
            })
        });
        let removed = self.data.all_removed_resources_map.get(kind);
        let mut id = 1i32;
        loop {
            let name = format!("FX{}{}", kind[0] as char, id).into_bytes();
            if dict(&resources, |d| d.key_exist(&name))
                || removed.is_some_and(|m| m.contains_key(&name))
            {
                id += 1;
                continue;
            }
            let resource_number = resource.borrow().obj_num();
            dict_mut(&resources, |d| {
                d.set_new_for(&name, CpdfReference::new(&objects, resource_number));
            });
            return name;
        }
    }
    fn process_page_object(&mut self, b: &mut Vec<u8>, object: &mut dyn PageObject) {
        match object.object_type() {
            PageObjectType::Image => {
                self.process_image(b, object.as_any_mut().downcast_mut().unwrap())
            }
            PageObjectType::Form => {
                self.process_form(b, object.as_any_mut().downcast_mut().unwrap())
            }
            PageObjectType::Path => {
                self.process_path(b, object.as_any_mut().downcast_mut().unwrap())
            }
            PageObjectType::Text => {
                self.process_text(b, object.as_any_mut().downcast_mut().unwrap())
            }
            PageObjectType::Shading => {
                self.process_shading(b, object.as_any_mut().downcast_mut().unwrap())
            }
        }
        object.base_mut().set_dirty(false);
    }
    fn process_shading(&mut self, b: &mut Vec<u8>, o: &mut CpdfShadingObject) {
        // Native's switch omits this case, which silently erases an existing
        // shading whenever another object dirties its content stream. Preserve
        // the existing shading resource and emit the PDF `sh` painting operator
        // using the same graphics-state and matrix ordering as image objects.
        let Some(mut resource) = o.pattern().shading_object() else {
            return;
        };
        if resource.borrow().is_inline() {
            let cloned = resource.borrow().clone_object();
            self.document
                .as_deref()
                .expect("document")
                .holder()
                .borrow_mut()
                .add_indirect_object(Rc::clone(&cloned));
            resource = cloned;
        }
        self.process_graphics(b, o);
        let matrix = o.matrix();
        if !matrix.is_identity() {
            write_matrix(b, matrix);
            b.extend_from_slice(b" cm ");
        }
        let name = self.realize_resource(&resource, b"Shading");
        o.base_mut().set_resource_name(&name);
        write_name(b, &name);
        b.extend_from_slice(b" sh Q\n");
    }
    fn process_image(&mut self, b: &mut Vec<u8>, o: &mut CpdfImageObject) {
        let m = o.matrix();
        if (m.a == 0.0 && m.b == 0.0) || (m.c == 0.0 && m.d == 0.0) {
            return;
        }
        let image = o.image().expect("image object image");
        // Inline images have no resource name yet, but their encoded stream
        // can use the same promotion path as a newly created external image.
        // Skipping them here, as native does, loses them on regeneration.
        if !image.has_stream() {
            return;
        }
        let stream = image.stream();
        self.process_graphics(b, o);
        if !m.is_identity() {
            write_matrix(b, m);
            b.extend_from_slice(b" cm ");
        }
        let was_inline = stream.borrow().is_inline();
        if was_inline {
            image.convert_stream_to_indirect_object(
                &self.document.as_deref().expect("document").holder(),
            );
        }
        let name = self.realize_resource(&stream, b"XObject");
        o.base_mut().set_resource_name(&name);
        if was_inline {
            o.set_image(
                self.document
                    .as_deref()
                    .unwrap()
                    .page_data()
                    .get_image(&stream),
            );
        }
        write_name(b, &name);
        b.extend_from_slice(b" Do Q\n");
    }
    fn process_form(&mut self, b: &mut Vec<u8>, o: &mut CpdfFormObject) {
        if o.form().holder().has_dirty_streams()
            || o.form()
                .holder()
                .iter()
                .any(|child| child.base().is_dirty())
        {
            // Form XObjects paint their own stream bytes, unlike pages whose
            // /Contents entry points at separate streams. Updating /Contents on
            // a form dictionary leaves the original children visible on reload.
            let stream = Rc::clone(o.form().stream());
            // A form has one stream. Regenerate all children in their actual
            // drawing order, including unchanged original and appended objects.
            // Page-style dirty-stream maps would otherwise omit unchanged
            // children or sort new (-1) objects before original stream 0.
            let holder = o.form_mut().holder_mut();
            holder.take_dirty_streams();
            for index in 0..holder.page_object_count() {
                holder
                    .page_object_by_index_mut(index)
                    .unwrap()
                    .base_mut()
                    .set_content_stream(0);
            }
            holder.mark_stream_dirty(0);
            let mut generator = CpdfPageContentGenerator::new(
                o.form_mut().holder_mut(),
                self.document.as_deref_mut(),
            );
            let streams = generator.generate_modified_streams();
            if !streams.is_empty() {
                let data: Vec<u8> = streams.into_values().flatten().collect();
                stream
                    .borrow_mut()
                    .as_any_mut()
                    .downcast_mut::<crate::fpdfapi::parser::cpdf_stream::CpdfStream>()
                    .expect("form stream")
                    .set_data_and_remove_filter(&data);
                generator.update_resources_dict();
            }
        }
        let m = o.form_matrix();
        if (m.a == 0.0 && m.b == 0.0) || (m.c == 0.0 && m.d == 0.0) {
            return;
        }
        let stream = Rc::clone(o.form().stream());
        let name = self.realize_resource(&stream, b"XObject");
        o.base_mut().set_resource_name(&name);
        self.process_graphics(b, o);
        if !m.is_identity() {
            write_matrix(b, m);
            b.extend_from_slice(b" cm ");
        }
        write_name(b, &name);
        b.extend_from_slice(b" Do Q\n");
    }
    fn process_path(&mut self, b: &mut Vec<u8>, o: &mut CpdfPathObject) {
        self.process_graphics(b, o);
        let m = o.matrix();
        if !m.is_identity() {
            write_matrix(b, m);
            b.extend_from_slice(b" cm ");
        }
        process_path_points(b, o.path());
        b.extend_from_slice(if o.has_no_filltype() {
            if o.stroke() {
                b" S"
            } else {
                b" n"
            }
        } else if o.has_winding_filltype() {
            if o.stroke() {
                b" B"
            } else {
                b" f"
            }
        } else if o.stroke() {
            b" B*"
        } else {
            b" f*"
        });
        b.extend_from_slice(b" Q\n");
    }
    fn process_graphics(&mut self, b: &mut Vec<u8>, o: &mut dyn PageObject) {
        b.extend_from_slice(b"q ");
        if write_color(b, o.base().color_state().get_fill_color()) {
            b.extend_from_slice(b" rg ");
        }
        if write_color(b, o.base().color_state().get_stroke_color()) {
            b.extend_from_slice(b" RG ");
        }
        let graph = o.base().graph_state();
        let width = graph.get_line_width();
        if width != 1.0 {
            write_float(b, width);
            b.extend_from_slice(b" w ");
        }
        let cap = graph.get_line_cap();
        if cap != LineCap::BUTT {
            b.extend_from_slice((cap.0 as i32).to_string().as_bytes());
            b.extend_from_slice(b" J ");
        }
        let join = graph.get_line_join();
        if join != LineJoin::MITER {
            b.extend_from_slice((join.0 as i32).to_string().as_bytes());
            b.extend_from_slice(b" j ");
        }
        let dash = graph.get_line_dash_array();
        if !dash.is_empty() {
            b.push(b'[');
            for (i, v) in dash.into_iter().enumerate() {
                if i > 0 {
                    b.push(b' ');
                }
                write_float(b, v);
            }
            b.extend_from_slice(b"] ");
            write_float(b, graph.get_line_dash_phase());
            b.extend_from_slice(b" d ");
        }
        let clip = o.base().clip_path();
        if clip.has_ref() {
            for i in 0..clip.path_count() {
                process_path_points(b, &clip.path(i));
                b.extend_from_slice(match clip.clip_type(i) {
                    FillType::Winding => b" W ",
                    FillType::EvenOdd => b" W* ",
                    FillType::NoFill => panic!("clip NoFill is NOTREACHED"),
                });
                b.extend_from_slice(b"n ");
            }
            process_text_clip_paths(b, clip);
        }
        let general = o.base().general_state();
        let data = GraphicsData {
            fill_alpha: general.get_fill_alpha(),
            stroke_alpha: general.get_stroke_alpha(),
            blend_type: general.get_blend_type(),
        };
        if data.fill_alpha == 1.0
            && data.stroke_alpha == 1.0
            && data.blend_type == BlendMode::Normal
        {
            return;
        }
        let name = if let Some(name) = self.data.graphics_map.get(&data) {
            name.clone()
        } else {
            let objects = self.document.as_deref().expect("document").holder();
            let gs = objects.borrow().new_dictionary();
            dict_mut(&gs, |d| {
                if data.fill_alpha != 1.0 {
                    d.set_new_for(b"ca", CpdfScalarObject::number_from_float(data.fill_alpha));
                }
                if data.stroke_alpha != 1.0 {
                    d.set_new_for(
                        b"CA",
                        CpdfScalarObject::number_from_float(data.stroke_alpha),
                    );
                }
                if data.blend_type != BlendMode::Normal {
                    d.set_new_name_for(b"BM", &general.get_blend_mode());
                }
            });
            objects.borrow_mut().add_indirect_object(Rc::clone(&gs));
            let name = self.realize_resource(&gs, b"ExtGState");
            o.base_mut()
                .mutable_general_state()
                .set_graphics_resource_names(vec![name.clone()]);
            self.data.graphics_map.insert(data, name.clone());
            name
        };
        write_name(b, &name);
        b.extend_from_slice(b" gs ");
    }
    fn get_or_create_default_graphics(&mut self) -> Vec<u8> {
        let data = GraphicsData {
            fill_alpha: 1.0,
            stroke_alpha: 1.0,
            blend_type: BlendMode::Normal,
        };
        if let Some(name) = self.data.graphics_map.get(&data) {
            return name.clone();
        }
        let objects = self.document.as_deref().expect("document").holder();
        let gs = objects.borrow().new_dictionary();
        dict_mut(&gs, |d| {
            d.set_new_for(b"ca", CpdfScalarObject::number_from_float(1.0));
            d.set_new_for(b"CA", CpdfScalarObject::number_from_float(1.0));
            d.set_new_name_for(b"BM", b"Normal");
        });
        objects.borrow_mut().add_indirect_object(Rc::clone(&gs));
        let name = self.realize_resource(&gs, b"ExtGState");
        self.data.graphics_map.insert(data, name.clone());
        name
    }
    fn process_text(&mut self, b: &mut Vec<u8>, o: &mut CpdfTextObject) {
        self.process_graphics(b, o);
        b.extend_from_slice(b"BT ");
        let matrix = o.text_matrix();
        if !matrix.is_identity() {
            write_matrix(b, matrix);
            b.extend_from_slice(b" Tm ");
        }
        let font = o.font().unwrap_or_else(|| {
            CpdfFont::get_stock_font(self.document.as_deref().expect("document"), b"Helvetica")
                .expect("stock font")
        });
        let kind: &[u8] = if font.is_type1() {
            b"Type1"
        } else if font.is_true_type() {
            b"TrueType"
        } else if font.is_cid_font() {
            b"Type0"
        } else {
            return;
        };
        let encoding = if font.is_type1() || font.is_true_type() {
            font.encoding()
        } else {
            None
        };
        let key = (font.base_font_name().to_vec(), kind.to_vec());
        let name = if let Some(name) = self.data.fonts_map.get(&key) {
            name.clone()
        } else {
            let mut indirect = font.font_dict().expect("font dictionary");
            if indirect.borrow().is_inline() {
                let objects = self.document.as_deref().expect("document").holder();
                let font_dict = objects.borrow().new_dictionary();
                dict_mut(&font_dict, |d| {
                    d.set_new_name_for(b"Type", b"Font");
                    d.set_new_name_for(b"Subtype", kind);
                    d.set_new_name_for(b"BaseFont", font.base_font_name());
                    if let Some(encoding) = encoding.as_ref() {
                        d.set_for(
                            b"Encoding",
                            encoding.realize(objects.borrow().byte_string_pool()),
                        );
                    }
                });
                objects
                    .borrow_mut()
                    .add_indirect_object(Rc::clone(&font_dict));
                indirect = font_dict;
            }
            let name = self.realize_resource(&indirect, b"Font");
            self.data.fonts_map.insert(key, name.clone());
            name
        };
        o.base_mut().set_resource_name(&name);
        write_name(b, &name);
        b.push(b' ');
        write_float(b, o.font_size());
        b.extend_from_slice(b" Tf ");
        b.extend_from_slice((o.text_render_mode() as i32).to_string().as_bytes());
        b.extend_from_slice(b" Tr [");
        let mut text = Vec::new();
        for (&code, &kerning) in o.char_codes().iter().zip(o.char_kernings()) {
            font.append_char(&mut text, code);
            if kerning != 0.0 {
                b.extend_from_slice(&pdf_hex_encode_string(&text));
                b.push(b' ');
                text.clear();
                write_float(b, kerning);
                b.push(b' ');
            }
        }
        if !text.is_empty() {
            b.extend_from_slice(&pdf_hex_encode_string(&text));
        }
        b.extend_from_slice(b"] TJ ET Q\n");
    }
}
// Retained clip text is geometry, not new logical text. Reuse the renderer's
// glyph positioning/outline pipeline and emit one compound winding clip per
// ET-delimited layer. This preserves extraction and works after the original
// painted text/font resource is removed.
fn process_text_clip_paths(
    b: &mut Vec<u8>,
    clip: &crate::fpdfapi::page::cpdf_clippath::CpdfClipPath,
) {
    use crate::fpdfapi::render::cpdf_textrenderer::append_pdf_text_clip_path;
    use crate::fxge::cfx_path::CfxPath;
    let flush = |b: &mut Vec<u8>, outlines: CfxPath| {
        let mut path = CpdfPath::new();
        if outlines.points().is_empty() {
            path.append_rect(0.0, 0.0, 0.0, 0.0);
        } else {
            path.append(&outlines, None);
        }
        process_path_points(b, &path);
        b.extend_from_slice(b" W n ");
    };
    let mut outlines = None;
    for index in 0..clip.text_count() {
        if let Some(text) = clip.text(index) {
            let path = outlines.get_or_insert_with(CfxPath::new);
            if let Some(font) = text.font() {
                append_pdf_text_clip_path(
                    path,
                    text.char_codes(),
                    text.char_positions(),
                    &font,
                    text.font_size(),
                    text.text_matrix(),
                );
            }
        } else if let Some(path) = outlines.take() {
            flush(b, path);
        }
    }
    if let Some(path) = outlines {
        flush(b, path);
    }
}

fn process_path_points(b: &mut Vec<u8>, path: &CpdfPath) {
    let points = path.points();
    if let Some(corner) = path.rect_opposite_corner() {
        let diff = corner - points[0].point;
        write_point(b, points[0].point);
        b.push(b' ');
        write_point(b, diff);
        b.extend_from_slice(b" re");
        return;
    }
    let mut i = 0;
    while i < points.len() {
        if i > 0 {
            b.push(b' ');
        }
        write_point(b, points[i].point);
        match points[i].point_type {
            PointType::Move => b.extend_from_slice(b" m"),
            PointType::Line => b.extend_from_slice(b" l"),
            PointType::Bezier => {
                if i + 2 >= points.len()
                    || points[i].close_figure
                    || points[i + 1].point_type != PointType::Bezier
                    || points[i + 1].close_figure
                    || points[i + 2].point_type != PointType::Bezier
                {
                    b.extend_from_slice(b" h");
                    break;
                }
                b.push(b' ');
                write_point(b, points[i + 1].point);
                b.push(b' ');
                write_point(b, points[i + 2].point);
                b.extend_from_slice(b" c");
                i += 2;
            }
        }
        if points[i].close_figure {
            b.extend_from_slice(b" h");
        }
        i += 1;
    }
}

#[cfg(test)]
#[path = "cpdf_pagecontentgenerator_unittest.rs"]
mod upstream_tests;

#[cfg(test)]
#[path = "cpdf_pagecontentgenerator_regression_tests.rs"]
mod regression_tests;
