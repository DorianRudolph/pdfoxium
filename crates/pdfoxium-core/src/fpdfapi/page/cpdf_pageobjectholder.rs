// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Page object storage and resource state from
//! `core/fpdfapi/page/cpdf_pageobjectholder.{h,cpp}`. The content parser is
//! separately borrowed while it mutates this holder.

use super::cpdf_docpagedata::CpdfDocPageData;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::rc::{Rc, Weak};

use super::cpdf_pageobject::{CpdfPageObject, PageObject};
use super::cpdf_transparency::CpdfTransparency;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix};
use crate::fxge::dib::fx_dib::BlendMode;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ParseState {
    #[default]
    NotParsed,
    Parsing,
    Parsed,
}

#[derive(Clone, Copy, Debug)]
pub struct GraphicsData {
    pub fill_alpha: f32,
    pub stroke_alpha: f32,
    pub blend_type: BlendMode,
}

fn safe_float_eq(a: f32, b: f32) -> bool {
    (a.is_nan() && b.is_nan()) || (!a.is_nan() && !b.is_nan() && a == b)
}

// FXSYS_SafeLT: all NaNs compare equal and sort after infinity. Signed zeros
// compare equal, so f32::total_cmp() would not match this native ordering.
fn safe_float_cmp(a: f32, b: f32) -> std::cmp::Ordering {
    if safe_float_eq(a, b) {
        std::cmp::Ordering::Equal
    } else if a.is_nan() || b.is_nan() {
        a.is_nan().cmp(&b.is_nan())
    } else if a < b {
        std::cmp::Ordering::Less
    } else {
        std::cmp::Ordering::Greater
    }
}

impl GraphicsData {
    fn matches(self, other: Self) -> bool {
        safe_float_eq(self.fill_alpha, other.fill_alpha)
            && safe_float_eq(self.stroke_alpha, other.stroke_alpha)
            && self.blend_type == other.blend_type
    }
}

impl PartialEq for GraphicsData {
    fn eq(&self, other: &Self) -> bool {
        self.matches(*other)
    }
}
impl Eq for GraphicsData {}
impl PartialOrd for GraphicsData {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for GraphicsData {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        safe_float_cmp(self.fill_alpha, other.fill_alpha)
            .then_with(|| safe_float_cmp(self.stroke_alpha, other.stroke_alpha))
            .then_with(|| (self.blend_type as i32).cmp(&(other.blend_type as i32)))
    }
}

/// Opaque identity of the native page-object allocation. This is compared only
/// against live Box allocations; it never owns or dereferences a pointer.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct PageObjectId(usize);

fn page_object_id(object: &dyn PageObject) -> PageObjectId {
    PageObjectId(std::ptr::from_ref(object).cast::<()>() as usize)
}

pub type RemovedResourceMap = BTreeMap<Vec<u8>, Option<ObjectRef>>;
pub type AllRemovedResourcesMap = BTreeMap<Vec<u8>, RemovedResourceMap>;

pub(crate) struct CpdfPageObjectHolderData {
    pub(crate) document_data: Weak<CpdfDocPageData>,
    pub(crate) dict: ObjectRef,
    pub(crate) page_resources: Option<ObjectRef>,
    pub(crate) resources: Option<ObjectRef>,
    pub(crate) graphics_map: BTreeMap<GraphicsData, Vec<u8>>,
    pub(crate) fonts_map: BTreeMap<(Vec<u8>, Vec<u8>), Vec<u8>>,
    pub(crate) bbox: CfxFloatRect,
    pub(crate) transparency: CpdfTransparency,
    pub(crate) background_alpha_needed: bool,
    pub(crate) parse_state: ParseState,
    pub(crate) mask_bounding_boxes: Vec<CfxFloatRect>,
    pub(crate) all_ctms: BTreeMap<i32, CfxMatrix>,
    pub(crate) dirty_streams: BTreeSet<i32>,
    pub(crate) is_page: bool,
    pub(crate) all_removed_resources_map: AllRemovedResourcesMap,
}

pub struct CpdfPageObjectHolder {
    data: CpdfPageObjectHolderData,
    page_objects: VecDeque<Box<dyn PageObject>>,
}

impl CpdfPageObjectHolder {
    pub fn new(
        dict: ObjectRef,
        page_resources: Option<ObjectRef>,
        resources: Option<ObjectRef>,
    ) -> Self {
        Self {
            data: CpdfPageObjectHolderData {
                document_data: Weak::new(),
                dict,
                page_resources,
                resources,
                graphics_map: BTreeMap::new(),
                fonts_map: BTreeMap::new(),
                bbox: CfxFloatRect::default(),
                transparency: CpdfTransparency::default(),
                background_alpha_needed: false,
                parse_state: ParseState::NotParsed,
                mask_bounding_boxes: Vec::new(),
                all_ctms: BTreeMap::new(),
                dirty_streams: BTreeSet::new(),
                is_page: false,
                all_removed_resources_map: BTreeMap::new(),
            },
            page_objects: VecDeque::new(),
        }
    }
    pub(crate) fn split_for_generation(
        &mut self,
    ) -> (
        &mut CpdfPageObjectHolderData,
        &mut VecDeque<Box<dyn PageObject>>,
    ) {
        (&mut self.data, &mut self.page_objects)
    }
    pub(crate) fn set_is_page(&mut self, is_page: bool) {
        self.data.is_page = is_page;
    }
    pub fn is_page(&self) -> bool {
        self.data.is_page
    }
    pub fn mark_stream_dirty(&mut self, stream: i32) {
        self.data.dirty_streams.insert(stream);
    }
    /// Prepare a parsed page for complete serialization in current paint order.
    /// The safe API uses this when new objects must not inherit persistent state
    /// from old prefix/suffix streams. This is opt-in: the C ABI's incremental
    /// content-stream editing behavior is unchanged. Parsed object identities,
    /// matrices, clip paths and shared resources remain intact.
    pub fn prepare_full_rewrite(&mut self) {
        self.data
            .dict
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .expect("page dictionary")
            .remove_for(b"Contents");
        self.data.all_ctms.clear();
        self.data.dirty_streams.clear();
        self.data
            .dirty_streams
            .insert(CpdfPageObject::NO_CONTENT_STREAM);
        for object in &mut self.page_objects {
            object
                .base_mut()
                .set_content_stream(CpdfPageObject::NO_CONTENT_STREAM);
            object.base_mut().set_dirty(true);
        }
    }
    pub fn all_removed_resources_map(&mut self) -> &mut AllRemovedResourcesMap {
        &mut self.data.all_removed_resources_map
    }
    pub fn dict(&self) -> &ObjectRef {
        &self.data.dict
    }
    pub fn resources(&self) -> Option<&ObjectRef> {
        self.data.resources.as_ref()
    }
    pub fn set_resources(&mut self, resources: Option<ObjectRef>) {
        self.data.resources = resources;
    }
    pub fn page_resources(&self) -> Option<&ObjectRef> {
        self.data.page_resources.as_ref()
    }
    pub fn set_page_resources(&mut self, resources: Option<ObjectRef>) {
        self.data.page_resources = resources;
    }
    pub fn bbox(&self) -> CfxFloatRect {
        self.data.bbox
    }
    pub fn set_bbox(&mut self, bbox: CfxFloatRect) {
        self.data.bbox = bbox;
    }
    pub fn transparency(&self) -> CpdfTransparency {
        self.data.transparency
    }
    pub(crate) fn mutable_transparency(&mut self) -> &mut CpdfTransparency {
        &mut self.data.transparency
    }
    pub fn background_alpha_needed(&self) -> bool {
        self.data.background_alpha_needed
    }
    pub fn set_background_alpha_needed(&mut self, needed: bool) {
        self.data.background_alpha_needed = needed;
    }
    pub fn parse_state(&self) -> ParseState {
        self.data.parse_state
    }
    pub(crate) fn bind_page_data(&mut self, data: &Rc<CpdfDocPageData>) {
        self.data.document_data = Rc::downgrade(data);
    }
    pub fn start_parse(&mut self) {
        assert_eq!(self.data.parse_state, ParseState::NotParsed);
        self.data.parse_state = ParseState::Parsing;
    }
    pub fn finish_parse(&mut self, all_ctms: BTreeMap<i32, CfxMatrix>) {
        assert_eq!(self.data.parse_state, ParseState::Parsing);
        self.data.parse_state = ParseState::Parsed;
        if let Some(document_data) = self.data.document_data.upgrade() {
            document_data.increment_parsed_page_count();
        }
        self.data.all_ctms = all_ctms;
    }
    pub fn add_image_mask_bounding_box(&mut self, box_rect: CfxFloatRect) {
        self.data.mask_bounding_boxes.push(box_rect);
    }
    pub fn image_mask_bounding_boxes(&self) -> &[CfxFloatRect] {
        &self.data.mask_bounding_boxes
    }
    pub fn has_image_mask(&self) -> bool {
        !self.data.mask_bounding_boxes.is_empty()
    }
    pub fn has_dirty_streams(&self) -> bool {
        !self.data.dirty_streams.is_empty()
    }
    pub fn take_dirty_streams(&mut self) -> BTreeSet<i32> {
        std::mem::take(&mut self.data.dirty_streams)
    }
    pub fn graphics_map_search(&self, data: GraphicsData) -> Option<&[u8]> {
        self.data.graphics_map.get(&data).map(Vec::as_slice)
    }
    pub fn graphics_map_insert(&mut self, data: GraphicsData, name: &[u8]) {
        self.data.graphics_map.insert(data, name.to_vec());
    }
    pub fn fonts_map_search(&self, base_font: &[u8], font_type: &[u8]) -> Option<&[u8]> {
        self.data
            .fonts_map
            .get(&(base_font.to_vec(), font_type.to_vec()))
            .map(Vec::as_slice)
    }
    pub fn fonts_map_insert(&mut self, base_font: &[u8], font_type: &[u8], name: &[u8]) {
        self.data
            .fonts_map
            .insert((base_font.to_vec(), font_type.to_vec()), name.to_vec());
    }
    pub fn ctm_at_beginning_of_stream(&self, stream: i32) -> CfxMatrix {
        assert!(stream >= 0 || stream == CpdfPageObject::NO_CONTENT_STREAM);
        if stream == 0 || self.data.all_ctms.is_empty() {
            return CfxMatrix::default();
        }
        if stream == CpdfPageObject::NO_CONTENT_STREAM {
            return *self.data.all_ctms.last_key_value().unwrap().1;
        }
        self.ctm_at_end_of_stream(stream - 1)
    }
    pub fn ctm_at_end_of_stream(&self, stream: i32) -> CfxMatrix {
        assert!(stream >= 0);
        if let Some((_, matrix)) = self.data.all_ctms.range(stream..).next() {
            return *matrix;
        }
        self.data
            .all_ctms
            .last_key_value()
            .map_or(CfxMatrix::default(), |(_, matrix)| *matrix)
    }
    pub fn load_transparency_info(&mut self) {
        let group = {
            let borrowed = self.data.dict.borrow();
            borrowed
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .and_then(|dict| dict.get_dict_for(b"Group"))
        };
        let Some(group) = group else { return };
        let borrowed = group.borrow();
        let Some(group) = borrowed.as_any().downcast_ref::<CpdfDictionary>() else {
            return;
        };
        if group.get_byte_string_for(b"S", b"") != b"Transparency" {
            return;
        }
        self.data.transparency.set_group();
        if group.get_integer_for(b"I", 0) != 0 {
            self.data.transparency.set_isolated();
        }
    }
    pub fn page_object_count(&self) -> usize {
        self.page_objects.len()
    }
    pub fn active_page_object_count(&self) -> usize {
        self.page_objects
            .iter()
            .filter(|object| object.base().is_active())
            .count()
    }
    pub fn page_object_by_index(&self, index: usize) -> Option<&dyn PageObject> {
        self.page_objects.get(index).map(|object| object.as_ref())
    }
    pub fn page_object_by_index_mut(&mut self, index: usize) -> Option<&mut (dyn PageObject + '_)> {
        if let Some(object) = self.page_objects.get_mut(index) {
            Some(object.as_mut())
        } else {
            None
        }
    }
    pub fn append_page_object(&mut self, object: Box<dyn PageObject>) {
        self.page_objects.push_back(object);
    }
    pub fn page_object_id_by_index(&self, index: usize) -> Option<PageObjectId> {
        self.page_object_by_index(index).map(page_object_id)
    }
    pub fn insert_page_object_at_index(
        &mut self,
        index: usize,
        mut object: Box<dyn PageObject>,
    ) -> bool {
        if index > self.page_objects.len() {
            return false;
        }
        if index < self.page_objects.len()
            && object.base().content_stream() == CpdfPageObject::NO_CONTENT_STREAM
        {
            let stream = self.page_objects[index].base().content_stream();
            if stream != CpdfPageObject::NO_CONTENT_STREAM {
                object.base_mut().set_content_stream(stream);
                self.data.dirty_streams.insert(stream);
            }
        }
        self.page_objects.insert(index, object);
        true
    }
    pub fn remove_page_object(&mut self, id: PageObjectId) -> Option<Box<dyn PageObject>> {
        let index = self
            .page_objects
            .iter()
            .position(|object| page_object_id(object.as_ref()) == id)?;
        let object = self.page_objects.remove(index).unwrap();
        let stream = object.base().content_stream();
        if stream >= 0 {
            self.data.dirty_streams.insert(stream);
        }
        Some(object)
    }
    pub fn erase_page_object_at_index(&mut self, index: usize) -> bool {
        // Unlike RemovePageObject(), the native erase operation does not mark
        // the old stream dirty.
        self.page_objects.remove(index).is_some()
    }
    pub fn iter(&self) -> impl Iterator<Item = &Box<dyn PageObject>> {
        self.page_objects.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::page::cpdf_pathobject::CpdfPathObject;
    use crate::fpdfapi::parser::cpdf_object::retain_object;

    #[test]
    fn full_rewrite_preserves_objects_but_discards_stream_state() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Contents", b"placeholder");
        let mut holder = CpdfPageObjectHolder::new(retain_object(dict), None, None);
        holder.start_parse();
        holder.finish_parse(BTreeMap::from([(
            2,
            CfxMatrix::new(2., 0., 0., 2., 10., 20.),
        )]));
        holder.append_page_object(Box::new(CpdfPathObject::new(2)));
        let id = holder.page_object_id_by_index(0);
        holder.prepare_full_rewrite();
        assert_eq!(holder.page_object_id_by_index(0), id);
        assert_eq!(
            holder
                .page_object_by_index(0)
                .unwrap()
                .base()
                .content_stream(),
            CpdfPageObject::NO_CONTENT_STREAM
        );
        assert!(holder.page_object_by_index(0).unwrap().base().is_dirty());
        assert_eq!(
            holder.ctm_at_beginning_of_stream(CpdfPageObject::NO_CONTENT_STREAM),
            CfxMatrix::default()
        );
        assert_eq!(
            holder.take_dirty_streams(),
            BTreeSet::from([CpdfPageObject::NO_CONTENT_STREAM])
        );
        assert!(!holder
            .dict()
            .borrow()
            .as_dictionary()
            .unwrap()
            .key_exist(b"Contents"));
    }

    #[test]
    fn stream_ctm_uses_next_known_end_and_last_for_new_objects() {
        let mut holder =
            CpdfPageObjectHolder::new(retain_object(CpdfDictionary::new()), None, None);
        holder.start_parse();
        let matrix1 = CfxMatrix::new(1.0, 0.0, 0.0, 1.0, 4.0, 5.0);
        let matrix3 = CfxMatrix::new(1.0, 0.0, 0.0, 1.0, 8.0, 9.0);
        holder.finish_parse(BTreeMap::from([(1, matrix1), (3, matrix3)]));
        assert_eq!(holder.ctm_at_beginning_of_stream(0), CfxMatrix::default());
        assert_eq!(holder.ctm_at_end_of_stream(0), matrix1);
        assert_eq!(holder.ctm_at_beginning_of_stream(3), matrix3);
        assert_eq!(
            holder.ctm_at_beginning_of_stream(CpdfPageObject::NO_CONTENT_STREAM),
            matrix3
        );
    }

    #[test]
    fn counts_active_objects_and_matches_nan_graphics_keys() {
        let mut holder =
            CpdfPageObjectHolder::new(retain_object(CpdfDictionary::new()), None, None);
        holder.append_page_object(Box::new(CpdfPathObject::new(0)));
        let mut inactive = CpdfPathObject::new(1);
        inactive.base_mut().set_is_active(false);
        holder.append_page_object(Box::new(inactive));
        assert_eq!(holder.page_object_count(), 2);
        assert_eq!(holder.active_page_object_count(), 1);
        let key = GraphicsData {
            fill_alpha: f32::NAN,
            stroke_alpha: 0.5,
            blend_type: BlendMode::Normal,
        };
        holder.graphics_map_insert(key, b"GS1");
        assert_eq!(holder.graphics_map_search(key), Some(b"GS1".as_slice()));
    }
}

#[cfg(test)]
#[path = "cpdf_pageobjectholder_unittest.rs"]
mod upstream_tests;

#[cfg(test)]
#[path = "cpdf_pageobjectholder_edit_tests.rs"]
mod edit_tests;
