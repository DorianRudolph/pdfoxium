// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! `core/fpdfapi/parser/cpdf_cross_ref_table.{h,cpp}`.

use std::collections::BTreeMap;

use super::cpdf_dictionary::CpdfDictionary;
use super::cpdf_object::ObjectRef;
use crate::fxcrt::fx_types::FxFileSize;

pub const MAX_OBJECT_NUMBER: u32 = 24 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ObjectType {
    #[default]
    Free,
    Normal,
    Compressed,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ObjectInfo {
    pub object_type: ObjectType,
    pub is_object_stream_flag: bool,
    pub gen_num: u16,
    pub pos: FxFileSize,
    pub archive_obj_num: u32,
    pub archive_obj_index: u32,
}

#[derive(Default)]
pub struct CpdfCrossRefTable {
    trailer: Option<ObjectRef>,
    trailer_object_number: u32,
    objects_info: BTreeMap<u32, ObjectInfo>,
}

impl CpdfCrossRefTable {
    pub fn new(trailer: Option<ObjectRef>, trailer_object_number: u32) -> Self {
        Self {
            trailer,
            trailer_object_number,
            objects_info: BTreeMap::new(),
        }
    }

    pub fn merge_up(current: Option<Self>, top: Option<Self>) -> Option<Self> {
        match (current, top) {
            (Some(mut current), Some(top)) => {
                current.update(top);
                Some(current)
            }
            (current, top) => current.or(top),
        }
    }

    pub fn add_compressed(
        &mut self,
        obj_num: u32,
        archive_obj_num: u32,
        archive_obj_index: u32,
    ) -> bool {
        if obj_num > MAX_OBJECT_NUMBER || archive_obj_num > MAX_OBJECT_NUMBER {
            return false;
        }
        let info = self.objects_info.entry(obj_num).or_default();
        if info.gen_num > 0 || info.is_object_stream_flag {
            return true;
        }
        info.object_type = ObjectType::Compressed;
        info.archive_obj_num = archive_obj_num;
        info.archive_obj_index = archive_obj_index;
        info.pos = 0;
        info.gen_num = 0;
        self.objects_info
            .entry(archive_obj_num)
            .or_default()
            .is_object_stream_flag = true;
        true
    }

    pub fn add_normal(
        &mut self,
        obj_num: u32,
        gen_num: u16,
        is_object_stream: bool,
        pos: FxFileSize,
    ) -> bool {
        if obj_num > MAX_OBJECT_NUMBER {
            return false;
        }
        let info = self.objects_info.entry(obj_num).or_default();
        if info.gen_num > gen_num {
            return true;
        }
        info.object_type = ObjectType::Normal;
        info.is_object_stream_flag |= is_object_stream;
        info.gen_num = gen_num;
        info.pos = pos;
        info.archive_obj_num = 0;
        info.archive_obj_index = 0;
        true
    }

    pub fn set_free(&mut self, obj_num: u32, gen_num: u16) -> bool {
        if obj_num > MAX_OBJECT_NUMBER {
            return false;
        }
        let info = self.objects_info.entry(obj_num).or_default();
        info.object_type = ObjectType::Free;
        info.gen_num = gen_num;
        info.pos = 0;
        info.archive_obj_num = 0;
        info.archive_obj_index = 0;
        true
    }

    pub fn set_trailer(&mut self, trailer: Option<ObjectRef>, trailer_object_number: u32) {
        self.trailer = trailer;
        self.trailer_object_number = trailer_object_number;
    }

    pub fn trailer(&self) -> Option<ObjectRef> {
        self.trailer.clone()
    }
    pub fn trailer_object_number(&self) -> u32 {
        self.trailer_object_number
    }
    pub fn get_object_info(&self, obj_num: u32) -> Option<&ObjectInfo> {
        self.objects_info.get(&obj_num)
    }
    pub fn objects_info(&self) -> &BTreeMap<u32, ObjectInfo> {
        &self.objects_info
    }

    pub fn update(&mut self, mut new_cross_ref: Self) {
        self.update_info(std::mem::take(&mut new_cross_ref.objects_info));
        self.update_trailer(new_cross_ref.trailer);
    }

    pub fn set_object_map_size(&mut self, size: u32) {
        self.objects_info.retain(|obj_num, _| *obj_num < size);
        if size > 0 {
            self.objects_info.entry(size - 1).or_default();
        }
    }

    fn update_info(&mut self, mut new_objects_info: BTreeMap<u32, ObjectInfo>) {
        if new_objects_info.is_empty() {
            return;
        }
        for (&obj_num, old_info) in &self.objects_info {
            if let Some(new_info) = new_objects_info.get_mut(&obj_num) {
                if new_info.object_type == ObjectType::Normal
                    && old_info.object_type == ObjectType::Normal
                    && old_info.is_object_stream_flag
                {
                    new_info.is_object_stream_flag = true;
                }
            } else {
                new_objects_info.insert(obj_num, *old_info);
            }
        }
        self.objects_info = new_objects_info;
    }

    fn update_trailer(&mut self, new_trailer: Option<ObjectRef>) {
        let Some(new_trailer) = new_trailer else {
            return;
        };
        let Some(old_trailer) = &self.trailer else {
            self.trailer = Some(new_trailer);
            return;
        };
        let mut old = old_trailer.borrow_mut();
        let old = old
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .expect("cross-reference trailer must be a dictionary");
        let mut new = new_trailer.borrow_mut();
        let new = new
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .expect("cross-reference trailer must be a dictionary");
        new.set_for(b"XRefStm", old.remove_for(b"XRefStm"));
        new.set_for(b"Prev", old.remove_for(b"Prev"));
        for key in new.get_keys() {
            old.set_for(&key, new.remove_for(&key));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CpdfCrossRefTable, ObjectType, MAX_OBJECT_NUMBER};
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

    #[test]
    fn compressed_entry_respects_generation_and_marks_archive() {
        let mut table = CpdfCrossRefTable::default();
        assert!(table.add_normal(1, 2, false, 42));
        assert!(table.add_compressed(1, 4, 3));
        assert_eq!(table.get_object_info(1).unwrap().pos, 42);
        assert!(table.add_compressed(2, 4, 3));
        assert_eq!(
            table.get_object_info(2).unwrap().object_type,
            ObjectType::Compressed
        );
        assert!(table.get_object_info(4).unwrap().is_object_stream_flag);
        assert!(table.add_normal(4, 0, false, 70));
        assert!(table.get_object_info(4).unwrap().is_object_stream_flag);
        assert!(!table.add_normal(MAX_OBJECT_NUMBER + 1, 0, false, 0));
    }

    #[test]
    fn newer_table_overrides_entries_and_merges_trailer() {
        let mut old_dict = CpdfDictionary::new();
        old_dict.set_new_for(b"Root", CpdfScalarObject::number_from_signed(1));
        old_dict.set_new_for(b"Prev", CpdfScalarObject::number_from_signed(10));
        let old = retain_object(old_dict);
        let mut current = CpdfCrossRefTable::new(Some(old.clone()), 7);
        current.add_normal(3, 0, true, 30);
        let mut next_dict = CpdfDictionary::new();
        next_dict.set_new_for(b"Size", CpdfScalarObject::number_from_signed(8));
        let next = retain_object(next_dict);
        let mut top = CpdfCrossRefTable::new(Some(next), 9);
        top.add_normal(3, 0, false, 33);
        current.update(top);
        assert_eq!(current.get_object_info(3).unwrap().pos, 33);
        assert!(current.get_object_info(3).unwrap().is_object_stream_flag);
        assert_eq!(current.trailer_object_number(), 7);
        let borrowed = old.borrow();
        let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
        assert_eq!(dict.get_integer_for(b"Root", 0), 1);
        assert_eq!(dict.get_integer_for(b"Size", 0), 8);
        assert_eq!(dict.get_integer_for(b"Prev", 0), 10);
    }
}
