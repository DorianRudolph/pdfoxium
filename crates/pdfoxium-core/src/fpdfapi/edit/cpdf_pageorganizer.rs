// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2024 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/edit/cpdf_pageorganizer.{h,cpp}`. Cross-document clones are
//! required by the source; references retain weak holders after rebinding.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
use crate::fpdfapi::parser::cpdf_object::{get_direct_object, retain_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
use crate::fpdfapi::parser::cpdf_scalar_object::{CpdfScalarObject, ObjectType};
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fxcrt::bytestring::eq_cstr;

pub(super) fn dict<R>(object: &ObjectRef, f: impl FnOnce(&CpdfDictionary) -> R) -> R {
    let borrowed = object.borrow();
    f(borrowed.as_any().downcast_ref().expect("dictionary"))
}
pub(super) fn dict_mut<R>(object: &ObjectRef, f: impl FnOnce(&mut CpdfDictionary) -> R) -> R {
    let mut borrowed = object.borrow_mut();
    f(borrowed.as_any_mut().downcast_mut().expect("dictionary"))
}

// Native ToDictionary is stricter than GetDictFor: a stream's dictionary does
// not make the stream itself a dictionary (notably for malformed page parents).
fn to_dictionary(object: &ObjectRef) -> Option<ObjectRef> {
    let direct = get_direct_object(object)?;
    let is_dict = direct.borrow().object_type() == ObjectType::Dictionary;
    is_dict.then_some(direct)
}

pub(crate) struct CpdfPageOrganizer {
    pub(super) dest: Rc<RefCell<CpdfDocument>>,
    pub(super) src: Rc<RefCell<CpdfDocument>>,
    object_number_map: BTreeMap<u32, u32>,
    preserve_all_links: bool,
}

impl CpdfPageOrganizer {
    pub(crate) fn new(dest: Rc<RefCell<CpdfDocument>>, src: Rc<RefCell<CpdfDocument>>) -> Self {
        Self {
            dest,
            src,
            object_number_map: BTreeMap::new(),
            preserve_all_links: false,
        }
    }
    /// Isolated object graphs have no page-tree links to replace or exclude.
    pub(crate) fn for_object_graph(
        dest: Rc<RefCell<CpdfDocument>>,
        src: Rc<RefCell<CpdfDocument>>,
    ) -> Self {
        Self {
            preserve_all_links: true,
            ..Self::new(dest, src)
        }
    }
    pub(crate) fn init(&self) -> bool {
        self.init_dest_doc()
    }
    pub(crate) fn add_object_mapping(&mut self, old: u32, new: u32) {
        self.object_number_map.insert(old, new);
    }

    pub(crate) fn clear_object_number_map(&mut self) {
        self.object_number_map.clear();
    }

    fn init_dest_doc(&self) -> bool {
        let Some(root) = self.dest.borrow().root() else {
            return false;
        };
        if let Some(info) = self.dest.borrow_mut().info() {
            dict_mut(&info, |d| d.set_new_string_for(b"Producer", b"PDFium"));
        }
        if dict(&root, |d| d.get_byte_string_for(b"Type", b"").is_empty()) {
            dict_mut(&root, |d| d.set_new_name_for(b"Type", b"Catalog"));
        }
        let holder = self.dest.borrow().holder();
        let pages =
            dict(&root, |d| d.get_object_for(b"Pages")).and_then(|object| to_dictionary(&object));
        let pages = pages.unwrap_or_else(|| {
            let pages = holder.borrow_mut().new_indirect_dictionary();
            dict_mut(&root, |d| {
                d.set_new_for(
                    b"Pages",
                    CpdfReference::new(&holder, pages.borrow().obj_num()),
                )
            });
            pages
        });
        if dict(&pages, |d| d.get_byte_string_for(b"Type", b"").is_empty()) {
            dict_mut(&pages, |d| d.set_new_name_for(b"Type", b"Pages"));
        }
        if dict(&pages, |d| d.get_array_for(b"Kids")).is_none() {
            let kids = holder.borrow().new_array();
            let number = holder.borrow_mut().add_indirect_object(kids);
            dict_mut(&pages, |d| {
                d.set_new_for(b"Count", CpdfScalarObject::number_from_signed(0));
                d.set_new_for(b"Kids", CpdfReference::new(&holder, number));
            });
        }
        true
    }

    pub(crate) fn update_reference(&mut self, object: &ObjectRef) -> bool {
        let kind = object.borrow().object_type();
        match kind {
            ObjectType::Reference => {
                let new = self.get_new_obj_id(object);
                if new == 0 {
                    return false;
                }
                let holder = self.dest.borrow().holder();
                object
                    .borrow_mut()
                    .as_any_mut()
                    .downcast_mut::<CpdfReference>()
                    .unwrap()
                    .set_ref(&holder, new);
                true
            }
            ObjectType::Dictionary => {
                let bad_keys = dict(object, |d| {
                    d.with_locked_entries(|entries| {
                        let mut bad = Vec::new();
                        for (key, next) in entries {
                            // Page exporters already replaced the page's
                            // Parent with a destination-tree reference. Other
                            // Parent entries (e.g. Popup annotations) still
                            // belong to the source and must be remapped.
                            if !self.preserve_all_links
                                && ((eq_cstr(key, b"Parent")
                                    && d.get_byte_string_for(b"Type", b"") == b"Page")
                                    || [b"Prev".as_slice(), b"First"]
                                        .iter()
                                        .any(|skip| eq_cstr(key, skip)))
                            {
                                continue;
                            }
                            if !self.update_reference(next) {
                                bad.push(Rc::clone(key));
                            }
                        }
                        bad
                    })
                });
                dict_mut(object, |d| {
                    for key in bad_keys {
                        d.remove_for(&key);
                    }
                });
                true
            }
            ObjectType::Array => {
                let mut index = 0;
                loop {
                    let next = {
                        let borrowed = object.borrow();
                        let array = borrowed.as_any().downcast_ref::<CpdfArray>().unwrap();
                        if index >= array.size() {
                            break;
                        }
                        array.get_object_at(index).expect("array object")
                    };
                    if !self.update_reference(&next) {
                        return false;
                    }
                    index += 1;
                }
                true
            }
            ObjectType::Stream => {
                let dictionary = object
                    .borrow()
                    .as_any()
                    .downcast_ref::<CpdfStream>()
                    .unwrap()
                    .get_dict();
                self.update_reference(&dictionary)
            }
            _ => true,
        }
    }

    fn get_new_obj_id(&mut self, object: &ObjectRef) -> u32 {
        // Evaluate reference state and release its borrow before lazy input IO.
        let (number, holder) = {
            let borrowed = object.borrow();
            let reference = borrowed.as_any().downcast_ref::<CpdfReference>().unwrap();
            (reference.ref_obj_num(), reference.indirect_object_holder())
        };
        if let Some(&new) = self.object_number_map.get(&number).filter(|&&new| new != 0) {
            return new;
        }
        let Some(direct) = holder.and_then(|holder| {
            CpdfIndirectObjectHolder::get_or_parse_indirect_object(&holder, number)
        }) else {
            return 0;
        };
        let cloned = direct.borrow().clone_object();
        if !self.preserve_all_links
            && cloned.borrow().object_type() == ObjectType::Dictionary
            && dict(&cloned, |d| d.key_exist(b"Type"))
        {
            let kind = dict(&cloned, |d| d.get_byte_string_for(b"Type", b""));
            // Resolve the real destination page tree instead of the source's
            // hardcoded object number 4.
            if kind.eq_ignore_ascii_case(b"Pages") {
                let Some(root) = self.dest.borrow().root() else {
                    return 0;
                };
                let pages =
                    dict(&root, |d| d.get_object_for(b"Pages")).and_then(|o| to_dictionary(&o));
                return pages.map(|p| p.borrow().obj_num()).unwrap_or(0);
            }
            if kind.eq_ignore_ascii_case(b"Page") {
                return 0;
            }
        }
        let holder = self.dest.borrow().holder();
        let new = holder.borrow_mut().add_indirect_object(Rc::clone(&cloned));
        self.add_object_mapping(number, new);
        if !self.update_reference(&cloned) {
            return 0;
        }
        new
    }

    /// Copy the catalog side of optional content with the same object map as
    /// page resources. Otherwise copied marks point to unregistered groups and
    /// lose both their saved visibility and interactive identity.
    pub(crate) fn copy_optional_content(&mut self) -> bool {
        use crate::fpdfapi::page::optional_content::{OcModel, OcState};
        fn array_values(o: &ObjectRef, key: &[u8]) -> Vec<ObjectRef> {
            let Some(array) = dict(o, |d| d.get_array_for(key)) else {
                return Vec::new();
            };
            let a = array.borrow();
            let a = a.as_any().downcast_ref::<CpdfArray>().unwrap();
            (0..a.size()).filter_map(|i| a.get_object_at(i)).collect()
        }
        fn append(o: &ObjectRef, key: &[u8], values: Vec<ObjectRef>) {
            let array = dict(o, |d| d.get_array_for(key))
                .unwrap_or_else(|| dict_mut(o, |d| d.set_new_array_for(key)));
            let mut a = array.borrow_mut();
            let a = a.as_any_mut().downcast_mut::<CpdfArray>().unwrap();
            for value in values {
                a.append(value);
            }
        }
        let source = self
            .src
            .borrow()
            .root()
            .and_then(|r| dict(&r, |d| d.get_dict_for(b"OCProperties")));
        let Some(source) = source else { return true };
        let incoming = source.borrow().clone_object();
        if !self.update_reference(&incoming) {
            return false;
        }
        let Some(root) = self.dest.borrow().root() else {
            return false;
        };
        let existing = dict(&root, |d| d.get_dict_for(b"OCProperties"));
        if let Some(existing) = existing {
            // Keep the destination's current configuration and add the source's
            // registered groups, saved states and presentation metadata.
            let temporary = retain_object(CpdfDictionary::new());
            dict_mut(&temporary, |d| {
                d.set_for(b"OCProperties", Some(incoming.clone()))
            });
            let model = OcModel::load(Some(temporary));
            let current_numbers: BTreeSet<_> = array_values(&existing, b"OCGs")
                .iter()
                .filter_map(get_direct_object)
                .map(|o| o.borrow().obj_num())
                .collect();
            let new_groups: Vec<_> = array_values(&incoming, b"OCGs")
                .into_iter()
                .filter(|o| {
                    get_direct_object(o)
                        .is_some_and(|o| !current_numbers.contains(&o.borrow().obj_num()))
                })
                .collect();
            if new_groups.is_empty() {
                return true;
            }
            append(&existing, b"OCGs", new_groups);
            let default = dict_mut(&existing, |d| d.get_or_create_dict_for(b"D"));
            let destination_order = array_values(&default, b"Order");
            let incoming_default = dict(&incoming, |d| d.get_dict_for(b"D"));
            if let Some(from) = &incoming_default {
                for key in [b"Order".as_slice(), b"RBGroups", b"Locked", b"AS"] {
                    append(&default, key, array_values(&from, key));
                }
            }
            if let Some(model) = model {
                let mut state = OcState::new(model.clone());
                state.environment.automatic = false;
                state.reset();
                let mut configurations = vec![default.clone()];
                configurations.extend(
                    array_values(&existing, b"Configs")
                        .iter()
                        .filter_map(get_direct_object)
                        .filter(|o| o.borrow().as_dictionary().is_some()),
                );
                for config in configurations {
                    for (i, g) in model
                        .groups
                        .iter()
                        .enumerate()
                        .filter(|(_, g)| !current_numbers.contains(&g.object))
                    {
                        let mut refs = CpdfArray::new();
                        refs.append_new(CpdfReference::new(&self.dest.borrow().holder(), g.object));
                        append(
                            &config,
                            if state.visible(i) == Some(false) {
                                b"OFF"
                            } else {
                                b"ON"
                            },
                            (0..refs.size())
                                .filter_map(|i| refs.get_object_at(i))
                                .collect(),
                        );
                    }
                }
                for (index, source_config) in array_values(&incoming, b"Configs")
                    .into_iter()
                    .filter_map(|o| get_direct_object(&o))
                    .filter(|o| o.borrow().as_dictionary().is_some())
                    .enumerate()
                {
                    if state.apply_configuration(index + 1).is_err() {
                        return false;
                    }
                    let config = source_config.borrow().clone_object();
                    let unchanged = dict(&config, |d| d.get_byte_string_for(b"BaseState", b"ON"))
                        == b"Unchanged";
                    let source_order = if dict(&config, |d| d.key_exist(b"Order")) {
                        array_values(&config, b"Order")
                    } else {
                        incoming_default
                            .as_ref()
                            .map_or_else(Vec::new, |d| array_values(d, b"Order"))
                    };
                    dict_mut(&config, |d| {
                        d.set_new_name_for(b"BaseState", b"Unchanged");
                        if !unchanged {
                            d.remove_for(b"ON");
                            d.remove_for(b"OFF");
                        }
                        d.remove_for(b"Order");
                    });
                    append(
                        &config,
                        b"Order",
                        destination_order
                            .iter()
                            .map(|o| o.borrow().clone_object())
                            .chain(source_order)
                            .collect(),
                    );
                    for (i, g) in model
                        .groups
                        .iter()
                        .enumerate()
                        .filter(|(_, g)| !current_numbers.contains(&g.object))
                    {
                        if unchanged {
                            continue;
                        }
                        let reference = retain_object(CpdfReference::new(
                            &self.dest.borrow().holder(),
                            g.object,
                        ));
                        append(
                            &config,
                            if state.visible(i) == Some(false) {
                                b"OFF"
                            } else {
                                b"ON"
                            },
                            vec![reference],
                        );
                    }
                    if !dict(&config, |d| d.key_exist(b"RBGroups")) {
                        if let Some(default) = &incoming_default {
                            append(&config, b"RBGroups", array_values(default, b"RBGroups"));
                        }
                    }
                    append(&existing, b"Configs", vec![config]);
                }
            }
        } else {
            dict_mut(&root, |d| d.set_for(b"OCProperties", Some(incoming)));
        }
        fn version(doc: &CpdfDocument) -> i32 {
            let header = if doc.parser().document_size().is_none() {
                17
            } else {
                doc.parser().file_version()
            };
            let catalog = doc
                .root()
                .map(|r| dict(&r, |d| d.get_byte_string_for(b"Version", b"")))
                .unwrap_or_default();
            if catalog.len() == 3
                && catalog[0].is_ascii_digit()
                && catalog[1] == b'.'
                && catalog[2].is_ascii_digit()
            {
                header.max(i32::from(catalog[0] - b'0') * 10 + i32::from(catalog[2] - b'0'))
            } else {
                header
            }
        }
        let required = version(&self.src.borrow()).max(15);
        if version(&self.dest.borrow()) < required {
            dict_mut(&root, |d| {
                d.set_new_name_for(
                    b"Version",
                    format!("{}.{}", required / 10, required % 10).as_bytes(),
                )
            });
        }
        self.dest.borrow_mut().reset_optional_content();
        true
    }

    pub(crate) fn copy_inheritable(dest: &ObjectRef, src: &ObjectRef, key: &[u8]) -> bool {
        if dict(dest, |d| d.key_exist(key)) {
            return true;
        }
        let Some(inherited) = Self::page_dict_get_inheritable_tag(src, key) else {
            return false;
        };
        let cloned = inherited.borrow().clone_object();
        dict_mut(dest, |d| {
            d.set_for(key, Some(cloned));
        });
        true
    }

    pub(crate) fn page_dict_get_inheritable_tag(page: &ObjectRef, key: &[u8]) -> Option<ObjectRef> {
        if key.is_empty() || !dict(page, |d| d.key_exist(b"Parent") && d.key_exist(b"Type")) {
            return None;
        }
        let kind = dict(page, |d| d.get_object_for(b"Type"))?;
        let kind = get_direct_object(&kind)?;
        if kind.borrow().object_type() != ObjectType::Name
            || !eq_cstr(&kind.borrow().get_string(), b"Page")
        {
            return None;
        }
        let parent = dict(page, |d| d.get_object_for(b"Parent"))?;
        let mut parent = to_dictionary(&parent)?;
        if let Some(value) = dict(page, |d| d.get_object_for(key)) {
            return Some(value);
        }
        let mut visited = BTreeSet::new();
        loop {
            if !visited.insert(Rc::as_ptr(&parent) as *const () as usize) {
                return None;
            }
            if let Some(value) = dict(&parent, |d| d.get_object_for(key)) {
                return Some(value);
            }
            let ancestor = dict(&parent, |d| d.get_object_for(b"Parent"))?;
            parent = to_dictionary(&ancestor)?;
        }
    }
}

#[cfg(test)]
#[path = "cpdf_pageorganizer_tests.rs"]
mod tests;
