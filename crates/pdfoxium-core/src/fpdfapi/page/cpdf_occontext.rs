// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fpdfapi/page/cpdf_occontext.{h,cpp}`.

use crate::fxcrt::bytestring::eq_cstr;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use super::cpdf_contentmarkitem::ParamType;
use super::cpdf_pageobject::PageObject;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UsageType {
    View,
    Design,
    Print,
    Export,
}

impl UsageType {
    fn name(self) -> &'static [u8] {
        match self {
            Self::View => b"View",
            Self::Design => b"Design",
            Self::Print => b"Print",
            Self::Export => b"Export",
        }
    }
}

pub struct CpdfOcContext {
    root: Option<ObjectRef>,
    usage_type: UsageType,
    snapshot: Option<super::optional_content::OcSnapshot>,
    // Like the pinned pointer-keyed std::map: repeated layer checks avoid
    // hashing, with the same entries and context lifetime as before.
    ocg_state_cache: RefCell<BTreeMap<usize, (ObjectRef, bool)>>,
}

fn with_dict<R>(object: &ObjectRef, f: impl FnOnce(&CpdfDictionary) -> R) -> Option<R> {
    let borrowed = object.borrow();
    Some(f(borrowed.as_dictionary()?))
}
fn with_array<R>(object: &ObjectRef, f: impl FnOnce(&CpdfArray) -> R) -> Option<R> {
    let borrowed = object.borrow();
    Some(f(borrowed.as_any().downcast_ref::<CpdfArray>()?))
}
fn has_intent(dict: &CpdfDictionary, element: &[u8], default: &[u8]) -> bool {
    let Some(intent) = dict.get_direct_object_for(b"Intent") else {
        return element == default;
    };
    if let Some(found) = with_array(&intent, |array| {
        (0..array.size()).any(|i| {
            let value = array.get_byte_string_at(i);
            eq_cstr(&value, b"All") || value == element
        })
    }) {
        return found;
    }
    let value = intent.borrow().get_string();
    eq_cstr(&value, b"All") || value == element
}

impl CpdfOcContext {
    pub fn new(document: &CpdfDocument, usage_type: UsageType) -> Self {
        let mut context = Self::with_root(document.root(), usage_type);
        context.snapshot = document
            .optional_content()
            .map(|state| state.borrow().snapshot(usage_type));
        context
    }
    fn with_root(root: Option<ObjectRef>, usage_type: UsageType) -> Self {
        Self {
            root,
            usage_type,
            snapshot: None,
            ocg_state_cache: RefCell::new(BTreeMap::new()),
        }
    }
    pub fn with_snapshot(
        document: &CpdfDocument,
        usage_type: UsageType,
        snapshot: super::optional_content::OcSnapshot,
    ) -> Self {
        let mut context = Self::with_root(document.root(), usage_type);
        context.snapshot = Some(snapshot);
        context
    }
    fn config(&self, ocg: &ObjectRef) -> Option<ObjectRef> {
        let properties = with_dict(self.root.as_ref()?, |root| {
            root.get_dict_for(b"OCProperties")
        })??;
        let ocgs = with_dict(&properties, |dict| dict.get_array_for(b"OCGs"))??;
        if !with_array(&ocgs, |array| array.contains(ocg))? {
            return None;
        }
        let default = with_dict(&properties, |dict| dict.get_dict_for(b"D"))?;
        let Some(configs) = with_dict(&properties, |dict| dict.get_array_for(b"Configs"))? else {
            return default;
        };
        with_array(&configs, |array| {
            (0..array.size()).find_map(|i| {
                let config = array.get_dict_at(i)?;
                with_dict(&config, |dict| has_intent(dict, b"View", b""))
                    .filter(|&has| has)
                    .map(|_| config)
            })
        })
        .flatten()
        .or(default)
    }
    fn load_ocg_state_from_config(&self, usage: &[u8], ocg: &ObjectRef) -> bool {
        let Some(config) = self.config(ocg) else {
            return true;
        };
        with_dict(&config, |dict| {
            let mut state = !eq_cstr(&dict.get_byte_string_for(b"BaseState", b"ON"), b"OFF");
            if dict
                .get_array_for(b"ON")
                .and_then(|o| with_array(&o, |a| a.contains(ocg)))
                .unwrap_or(false)
            {
                state = true;
            }
            if dict
                .get_array_for(b"OFF")
                .and_then(|o| with_array(&o, |a| a.contains(ocg)))
                .unwrap_or(false)
            {
                state = false;
            }
            let Some(as_array) = dict.get_array_for(b"AS") else {
                return state;
            };
            let key = [usage, b"State"].concat();
            with_array(&as_array, |array| {
                for i in 0..array.size() {
                    let Some(entry) = array.get_dict_at(i) else {
                        continue;
                    };
                    let change = with_dict(&entry, |entry| {
                        if entry.get_byte_string_for(b"Event", b"View") != usage {
                            return None;
                        }
                        let ocgs = entry.get_array_for(b"OCGs")?;
                        if !with_array(&ocgs, |array| array.contains(ocg))? {
                            return None;
                        }
                        let usage_dict = entry.get_dict_for(usage)?;
                        with_dict(&usage_dict, |dict| {
                            !eq_cstr(&dict.get_byte_string_for(&key, b""), b"OFF")
                        })
                    })
                    .flatten();
                    if let Some(value) = change {
                        state = value;
                    }
                }
                state
            })
            .unwrap_or(state)
        })
        .unwrap_or(true)
    }
    fn load_ocg_state(&self, ocg: &ObjectRef) -> bool {
        if !with_dict(ocg, |dict| has_intent(dict, b"View", b"View")).unwrap_or(false) {
            return true;
        }
        let usage = self.usage_type.name();
        let setting = with_dict(ocg, |dict| dict.get_dict_for(b"Usage")).flatten();
        if let Some(setting) = setting {
            let explicit = with_dict(&setting, |dict| dict.get_dict_for(usage)).flatten();
            if let Some(explicit) = explicit {
                let key = [usage, b"State"].concat();
                if let Some(value) = with_dict(&explicit, |dict| {
                    dict.key_exist(&key)
                        .then(|| !eq_cstr(&dict.get_byte_string_for(&key, b""), b"OFF"))
                })
                .flatten()
                {
                    return value;
                }
            }
            if usage != b"View" {
                let view = with_dict(&setting, |dict| dict.get_dict_for(b"View")).flatten();
                if let Some(view) = view {
                    if let Some(value) = with_dict(&view, |dict| {
                        dict.key_exist(b"ViewState")
                            .then(|| !eq_cstr(&dict.get_byte_string_for(b"ViewState", b""), b"OFF"))
                    })
                    .flatten()
                    {
                        return value;
                    }
                }
            }
        }
        self.load_ocg_state_from_config(usage, ocg)
    }
    fn get_ocg_visible(&self, ocg: &ObjectRef) -> bool {
        if let Some(visible) = self
            .snapshot
            .as_ref()
            .and_then(|s| s.visible(ocg.borrow().obj_num()))
        {
            return visible;
        }

        let key = Rc::as_ptr(ocg) as *const () as usize;
        if let Some((_, value)) = self.ocg_state_cache.borrow().get(&key) {
            return *value;
        }
        let value = self.load_ocg_state(ocg);
        self.ocg_state_cache
            .borrow_mut()
            .insert(key, (ocg.clone(), value));
        value
    }
    fn get_ocg_ve(&self, expression: &ObjectRef, level: u32) -> bool {
        if level > 32 {
            return false;
        }
        with_array(expression, |array| {
            let op = array.get_byte_string_at(0);
            if eq_cstr(&op, b"Not") {
                let Some(item) = array.get_direct_object_at(1) else {
                    return false;
                };
                if with_dict(&item, |_| ()).is_some() {
                    return !self.get_ocg_visible(&item);
                }
                if with_array(&item, |_| ()).is_some() {
                    return !self.get_ocg_ve(&item, level + 1);
                }
                return false;
            }
            if !eq_cstr(&op, b"Or") && !eq_cstr(&op, b"And") {
                return false;
            }
            let mut value = false;
            for i in 1..array.size() {
                let Some(item) = array.get_direct_object_at(i) else {
                    continue;
                };
                let item_value = if with_dict(&item, |_| ()).is_some() {
                    self.get_ocg_visible(&item)
                } else if with_array(&item, |_| ()).is_some() {
                    self.get_ocg_ve(&item, level + 1)
                } else {
                    false
                };
                if i == 1 {
                    value = item_value;
                } else if eq_cstr(&op, b"Or") {
                    value |= item_value;
                } else {
                    value &= item_value;
                }
            }
            value
        })
        .unwrap_or(false)
    }
    fn load_ocmd_state(&self, ocmd: &ObjectRef) -> bool {
        let Some((expression, policy, ocgs)) = with_dict(ocmd, |dict| {
            (
                dict.get_array_for(b"VE"),
                dict.get_byte_string_for(b"P", b"AnyOn"),
                dict.get_direct_object_for(b"OCGs"),
            )
        }) else {
            return false;
        };
        if let Some(expression) = expression {
            return self.get_ocg_ve(&expression, 0);
        }
        let Some(ocgs) = ocgs else {
            return true;
        };
        if with_dict(&ocgs, |_| ()).is_some() {
            let visible = self.get_ocg_visible(&ocgs);
            return if policy == b"AllOff" || policy == b"AnyOff" {
                !visible
            } else {
                visible
            };
        }
        let Some(value) = with_array(&ocgs, |array| {
            let mut valid = false;
            for i in 0..array.size() {
                let Some(ocg) = array.get_dict_at(i) else {
                    continue;
                };
                valid = true;
                let on = self.get_ocg_visible(&ocg);
                if (eq_cstr(&policy, b"AnyOn") && on) || (eq_cstr(&policy, b"AnyOff") && !on) {
                    return true;
                }
                if (eq_cstr(&policy, b"AllOn") && !on) || (eq_cstr(&policy, b"AllOff") && on) {
                    return false;
                }
            }
            !valid || eq_cstr(&policy, b"AllOn") || eq_cstr(&policy, b"AllOff")
        }) else {
            return true;
        };
        value
    }
    pub fn check_ocg_dict_visible(&self, ocg: Option<&ObjectRef>) -> bool {
        let Some(ocg) = ocg else {
            return true;
        };
        let is_ocg = with_dict(ocg, |dict| {
            dict.byte_string_eq_cstr_for(b"Type", b"OCG", b"OCG")
        })
        .unwrap_or(false);
        if is_ocg {
            self.get_ocg_visible(ocg)
        } else {
            self.load_ocmd_state(ocg)
        }
    }
    pub fn check_page_object_visible(&self, object: &dyn PageObject) -> bool {
        let marks = object.base().content_marks();
        let Some(items) = marks.items() else {
            return true;
        };
        for mark in items.iter() {
            let mark = mark.borrow();
            if eq_cstr(mark.name(), b"OC")
                && mark.param_type() == ParamType::PropertiesDict
                && !self.check_ocg_dict_visible(mark.param().as_ref())
            {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::page::cpdf_pathobject::CpdfPathObject;
    use crate::fpdfapi::parser::cpdf_object::retain_object;

    fn ocg_with_view_state(state: &[u8]) -> ObjectRef {
        let mut view = CpdfDictionary::new();
        view.set_new_name_for(b"ViewState", state);
        let mut usage = CpdfDictionary::new();
        usage.set_new_for(b"View", view);
        let mut ocg = CpdfDictionary::new();
        ocg.set_new_name_for(b"Type", b"OCG");
        ocg.set_new_for(b"Usage", usage);
        retain_object(ocg)
    }

    #[test]
    fn literal_intents_and_view_intents_keep_their_native_overloads() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Intent", b"All\0tail");
        assert!(has_intent(&dict, b"View", b"View"));
        dict.set_new_name_for(b"Intent", b"View\0tail");
        assert!(!has_intent(&dict, b"View", b"View"));
        let group = ocg_with_view_state(b"OFF\0tail");
        let context = CpdfOcContext::with_root(None, UsageType::View);
        assert!(!context.check_ocg_dict_visible(Some(&group)));
    }

    #[test]
    fn config_and_usage_states_control_group_visibility() {
        let ocg = retain_object(CpdfDictionary::new());
        let mut groups = CpdfArray::new();
        groups.append(ocg.clone());
        let mut default = CpdfDictionary::new();
        default.set_new_name_for(b"BaseState", b"OFF");
        let mut properties = CpdfDictionary::new();
        properties.set_new_for(b"OCGs", groups);
        properties.set_new_for(b"D", default);
        let mut root = CpdfDictionary::new();
        root.set_new_for(b"OCProperties", properties);
        let context = CpdfOcContext::with_root(Some(retain_object(root)), UsageType::View);
        assert!(!context.check_ocg_dict_visible(Some(&ocg)));
        assert!(context.check_ocg_dict_visible(None));
        // A group's explicit usage state takes precedence over the OFF default.
        let on = ocg_with_view_state(b"ON");
        assert!(context.check_ocg_dict_visible(Some(&on)));
    }

    #[test]
    fn membership_policies_and_marks_hide_content() {
        let off = ocg_with_view_state(b"OFF");
        let on = ocg_with_view_state(b"ON");
        let mut groups = CpdfArray::new();
        groups.append(off.clone());
        groups.append(on);
        let mut ocmd = CpdfDictionary::new();
        ocmd.set_new_name_for(b"Type", b"OCMD");
        ocmd.set_new_for(b"OCGs", groups);
        let ocmd = retain_object(ocmd);
        let context = CpdfOcContext::with_root(None, UsageType::View);
        assert!(context.check_ocg_dict_visible(Some(&ocmd)));
        {
            let mut borrowed = ocmd.borrow_mut();
            borrowed
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap()
                .set_new_name_for(b"P", b"AllOn");
        }
        assert!(!context.check_ocg_dict_visible(Some(&ocmd)));
        let mut expression = CpdfArray::new();
        expression.append_new_name(b"Not");
        expression.append(off.clone());
        {
            let mut borrowed = ocmd.borrow_mut();
            borrowed
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap()
                .set_new_for(b"VE", expression);
        }
        assert!(context.check_ocg_dict_visible(Some(&ocmd)));
        let mut properties = CpdfDictionary::new();
        properties.set_for(b"Layer", Some(off));
        let mut path = CpdfPathObject::default();
        path.base_mut()
            .mutable_content_marks()
            .add_mark_with_properties_holder(b"OC", retain_object(properties), b"Layer");
        assert!(!context.check_page_object_visible(&path));
    }

    #[test]
    fn visibility_traversal_observes_property_and_mark_list_changes() {
        let on = ocg_with_view_state(b"ON");
        let off = ocg_with_view_state(b"OFF");
        let mut properties = CpdfDictionary::new();
        properties.set_for(b"Layer", Some(on));
        let properties = retain_object(properties);
        let context = CpdfOcContext::with_root(None, UsageType::View);
        let mut path = CpdfPathObject::default();
        let marks = path.base_mut().mutable_content_marks();
        // These marks must not affect visibility, even with an OFF dictionary.
        marks.add_mark_with_direct_dict(b"OC", &off).unwrap();
        marks.add_mark_with_properties_holder(b"Span", properties.clone(), b"Layer");
        marks.add_mark_with_properties_holder(b"OC", properties.clone(), b"Missing");
        for _ in 0..40 {
            marks.add_mark_with_properties_holder(b"OC", properties.clone(), b"Layer");
        }
        assert!(context.check_page_object_visible(&path));
        properties
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_for(b"Layer", Some(off));
        assert!(!context.check_page_object_visible(&path));

        // A fresh list is read on the next call, without retaining the old
        // list's borrow or caching its visibility.
        path.base_mut().set_content_marks(Default::default());
        assert!(context.check_page_object_visible(&path));
        path.base_mut().mutable_content_marks().add_mark(b"OC");
        assert!(context.check_page_object_visible(&path));
    }
}
