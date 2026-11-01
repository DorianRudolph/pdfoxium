// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Choice items, selections and picture values from `cxfa_node.cpp`.
use super::{
    cxfa_localevalue::{CxfaLocaleValue, ValueType, XfaValuePicture},
    cxfa_node::CxfaNode,
    xfa_utils::get_locale_value,
};
use crate::xfa::{
    fgas::crt::{locale_iface::LocaleIface, locale_mgr_iface::LocaleMgrIface},
    fxfa::fxfa_basic::{XfaAttribute, XfaAttributeValue, XfaElement},
};
use std::rc::Rc;

fn children(parent: &CxfaNode) -> Vec<CxfaNode> {
    let mut result = Vec::new();
    let mut child = parent.first_child();
    while let Some(node) = child {
        child = node.next_sibling();
        result.push(node);
    }
    result
}
fn item_groups(node: &CxfaNode) -> Vec<CxfaNode> {
    children(node)
        .into_iter()
        .filter(|n| n.get_element_type() == XfaElement::Items)
        .collect()
}
fn choice_group(node: &CxfaNode, save: bool) -> Option<CxfaNode> {
    let groups = item_groups(node);
    let first = groups.first()?;
    if let Some(second) = groups.get(1) {
        if first.get_boolean(XfaAttribute::Save) != second.get_boolean(XfaAttribute::Save)
            && save == second.get_boolean(XfaAttribute::Save)
        {
            return Some(second.clone());
        }
    }
    Some(first.clone())
}
fn insert_text(items: &CxfaNode, text: &[u32], index: i32) -> bool {
    let Some(document) = items.document() else {
        return false;
    };
    let Some(child) = document.create_node(items.get_packet_type(), XfaElement::Text) else {
        return false;
    };
    items.insert_child_and_notify(index, &child) && child.set_content(text, text, false)
}
fn split_date_time(value: &[u32]) -> Option<(&[u32], &[u32])> {
    let index = value
        .iter()
        .position(|&c| c == 84)
        .or_else(|| value.iter().position(|&c| c == 32))?;
    let (date, rest) = value.split_at(index);
    let time = &rest[1..];
    let valid = |part: &[u32]| part.is_empty() || part.iter().any(|ch| (48..=57).contains(ch));
    (valid(date) && valid(time)).then_some((date, time))
}
fn format_num_str(value: &[u32], locale: &dyn LocaleIface) -> Vec<u32> {
    if value.is_empty() {
        return Vec::new();
    }
    let (negative, source) = value
        .strip_prefix(&[45])
        .map_or((false, value), |rest| (true, rest));
    let dot = source.iter().position(|&c| c == 46).unwrap_or(source.len());
    if dot == 0 {
        return Vec::new();
    }
    let mut result = if negative {
        locale.get_minus_symbol()
    } else {
        Vec::new()
    };
    let grouping = locale.get_grouping_symbol();
    for (index, &unit) in source[..dot].iter().enumerate() {
        if index != 0 && index % 3 == dot % 3 {
            result.extend_from_slice(&grouping);
        }
        result.push(unit);
    }
    if dot < source.len() {
        result.extend(locale.get_decimal_symbol());
        result.extend_from_slice(&source[dot + 1..]);
    }
    result
}

impl CxfaNode {
    pub fn count_choice_list_items(&self, save: bool) -> usize {
        choice_group(self, save).map_or(0, |items| items.count_children(XfaElement::Unknown, false))
    }
    pub fn get_choice_list_item(&self, index: i32, save: bool) -> Option<Vec<u32>> {
        let index = usize::try_from(index).ok()?;
        Some(
            choice_group(self, save)?
                .get_child(index, XfaElement::Unknown, false)?
                .get_content(),
        )
    }
    pub fn get_choice_list_items(&self, save: bool) -> Vec<Vec<u32>> {
        choice_group(self, save).map_or_else(Vec::new, |items| {
            children(&items).iter().map(CxfaNode::get_content).collect()
        })
    }
    pub fn count_selected_items(&self) -> usize {
        let values = self.get_selected_items_value();
        if self.is_list_box() || !self.is_choice_list_allow_text_entry() {
            return values.len();
        }
        let saved = self.get_choice_list_items(true);
        values.iter().filter(|value| saved.contains(value)).count()
    }
    pub fn get_selected_item(&self, index: i32) -> i32 {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.get_selected_items_value().get(index).cloned())
            .and_then(|value| {
                self.get_choice_list_items(true)
                    .iter()
                    .position(|saved| *saved == value)
            })
            .and_then(|index| i32::try_from(index).ok())
            .unwrap_or(-1)
    }
    pub fn get_selected_items(&self) -> Vec<i32> {
        let saved = self.get_choice_list_items(true);
        self.get_selected_items_value()
            .iter()
            .filter_map(|value| saved.iter().position(|saved| saved == value))
            .filter_map(|index| i32::try_from(index).ok())
            .collect()
    }
    pub fn get_item_state(&self, index: i32) -> bool {
        self.get_choice_list_item(index, true)
            .is_some_and(|value| self.get_selected_items_value().contains(&value))
    }
    pub fn set_item_state(&self, index: i32, selected: bool, notify: bool) -> bool {
        self.set_item_state_with_script(index, selected, notify, false)
    }
    // CJX_Field passes bScriptModify through SetItemState/SetSelectedItems.
    // FFNotify uses it to refresh the loaded field rather than only its Value
    // child. Ordinary model/interaction callers retain their existing flag.
    pub(crate) fn set_item_state_with_script(
        &self,
        index: i32,
        selected: bool,
        notify: bool,
        script_modify: bool,
    ) -> bool {
        let Some(value) = self.get_choice_list_item(index, true) else {
            return false;
        };
        let current = self.get_selected_items_value();
        let found = current.contains(&value);
        if self.is_choice_list_multi_select() {
            if selected && !found {
                let mut content = self.get_raw_value();
                if !content.is_empty() {
                    content.push(10);
                }
                content.extend(value);
                return self.set_content_with_script(
                    &content,
                    &content,
                    notify,
                    true,
                    script_modify,
                );
            }
            if !selected && found {
                let selected: Vec<i32> = self
                    .get_selected_items()
                    .into_iter()
                    .filter(|&i| i != index)
                    .collect();
                return self.set_selected_items_with_script(&selected, notify, true, script_modify);
            }
        } else if selected && !found {
            let formatted = self.get_format_data_value(&value);
            return self.set_content_with_script(&value, &formatted, notify, true, script_modify);
        } else if !selected && found {
            return self.set_content_with_script(&[], &[], notify, true, script_modify);
        }
        true
    }
    pub fn set_selected_items(&self, indices: &[i32], notify: bool, sync_data: bool) -> bool {
        self.set_selected_items_with_script(indices, notify, sync_data, false)
    }
    fn set_selected_items_with_script(
        &self,
        indices: &[i32],
        notify: bool,
        sync_data: bool,
        script_modify: bool,
    ) -> bool {
        let saved = self.get_choice_list_items(true);
        let mut value = Vec::new();
        for (position, &index) in indices.iter().enumerate() {
            let Some(item) = usize::try_from(index).ok().and_then(|i| saved.get(i)) else {
                return false;
            };
            // Delimit entries, rather than append a final newline that creates
            // an extra empty item in the bound structured data representation.
            if position != 0 {
                value.push(10);
            }
            value.extend_from_slice(item);
        }
        let formatted = if self.is_choice_list_multi_select() {
            value.clone()
        } else {
            self.get_format_data_value(&value)
        };
        self.set_content_with_script(&value, &formatted, notify, sync_data, script_modify)
    }
    pub fn clear_all_selections(&self) -> bool {
        // The existing binding dispatcher clears structured data children and
        // every bound peer together; do not leave the form's raw value stale.
        self.sync_value(&[], false)
    }
    pub fn insert_choice_list_item(&self, label: &[u32], value: &[u32]) -> bool {
        self.insert_choice_list_item_with_notify(label, value, false)
    }
    pub fn insert_choice_list_item_with_notify(
        &self,
        label: &[u32],
        value: &[u32],
        notify: bool,
    ) -> bool {
        let _document = self.document();
        let added = self.insert_choice_list_item_impl(label, value);
        if added && notify {
            if let Some(callback) = self.document().and_then(|document| document.notify()) {
                callback.on_widget_list_item_added(self, label, -1);
            }
        }
        added
    }
    fn insert_choice_list_item_impl(&self, label: &[u32], value: &[u32]) -> bool {
        let Some(document) = self.document() else {
            return false;
        };
        let value = if value.is_empty() { label } else { value };
        let groups = item_groups(self);
        if groups.is_empty() {
            let Some(labels) = document.create_node(self.get_packet_type(), XfaElement::Items)
            else {
                return false;
            };
            if !self.insert_child_and_notify(-1, &labels) || !insert_text(&labels, label, -1) {
                return false;
            }
            let Some(saved) = document.create_node(self.get_packet_type(), XfaElement::Items)
            else {
                return false;
            };
            if !self.insert_child_and_notify(-1, &saved) {
                return false;
            }
            saved.set_boolean(XfaAttribute::Save, true, false);
            return insert_text(&saved, value, -1);
        }
        if groups.len() > 1 {
            return groups[..2].iter().all(|items| {
                insert_text(
                    items,
                    if items.get_boolean(XfaAttribute::Save) {
                        value
                    } else {
                        label
                    },
                    -1,
                )
            });
        }
        let labels = &groups[0];
        labels.set_boolean(XfaAttribute::Save, false, false);
        labels.set_enum(XfaAttribute::Presence, XfaAttributeValue::Visible, false);
        let Some(saved) = document.create_node(self.get_packet_type(), XfaElement::Items) else {
            return false;
        };
        if !self.insert_child_and_notify(-1, &saved) {
            return false;
        }
        saved.set_boolean(XfaAttribute::Save, true, false);
        saved.set_enum(XfaAttribute::Presence, XfaAttributeValue::Hidden, false);
        for child in children(labels) {
            if !insert_text(&saved, &child.get_content(), -1) {
                return false;
            }
        }
        insert_text(labels, label, -1) && insert_text(&saved, value, -1)
    }
    pub fn get_item_label(&self, value: &[u32]) -> Vec<u32> {
        let groups = item_groups(self);
        if groups.len() <= 1 {
            return value.to_vec();
        }
        let (labels, saved) = if groups[0].get_boolean(XfaAttribute::Save) {
            (&groups[1], &groups[0])
        } else {
            (&groups[0], &groups[1])
        };
        children(saved)
            .iter()
            .position(|node| node.get_content() == value)
            .and_then(|i| labels.get_child(i, XfaElement::Unknown, false))
            .map_or_else(Vec::new, |n| n.get_content())
    }
    pub fn get_item_value(&self, label: &[u32]) -> Vec<u32> {
        let groups = item_groups(self);
        if groups.len() <= 1 {
            return label.to_vec();
        }
        let (labels, saved) = if groups[0].get_boolean(XfaAttribute::Save) {
            (&groups[1], &groups[0])
        } else {
            (&groups[0], &groups[1])
        };
        children(labels)
            .iter()
            .position(|node| node.get_content() == label)
            .and_then(|i| saved.get_child(i, XfaElement::Unknown, false))
            .map_or_else(Vec::new, |n| n.get_content())
    }
    pub fn delete_item(&self, index: i32) -> bool {
        self.delete_item_with_notify(index, false)
    }
    pub fn delete_item_with_notify(&self, index: i32, notify: bool) -> bool {
        self.delete_item_with_script(index, notify, false)
    }
    pub(crate) fn delete_item_with_script(
        &self,
        index: i32,
        notify: bool,
        script_modify: bool,
    ) -> bool {
        let _document = self.document();
        // Selection is cleared while the complete saved-value group still exists.
        if index >= 0
            && item_groups(self)
                .iter()
                .any(|items| items.get_boolean(XfaAttribute::Save))
        {
            self.set_item_state_with_script(index, false, true, script_modify);
        }
        for items in item_groups(self) {
            if index < 0 {
                for child in children(&items) {
                    items.remove_child_and_notify(&child, true);
                }
            } else {
                if let Some(child) = items.get_child(index as usize, XfaElement::Unknown, false) {
                    items.remove_child_and_notify(&child, true);
                }
            }
        }
        if notify {
            if let Some(callback) = self.document().and_then(|document| document.notify()) {
                callback.on_widget_list_item_removed(self, index);
            }
        }
        true
    }
    pub fn get_selected_member(&self) -> Option<Self> {
        if self.get_raw_value().is_empty() {
            return None;
        }
        children(self).into_iter().find(|member| {
            member.get_element_type() == XfaElement::Field
                && member
                    .get_child(0, XfaElement::Items, false)
                    .and_then(|items| items.first_child())
                    .is_some_and(|on| member.get_raw_value() == on.get_content())
        })
    }
    pub fn set_selected_member(&self, name: &[u32]) -> Option<Self> {
        let hash = super::cxfa_node::hash_name(name);
        let member = children(self)
            .into_iter()
            .find(|node| node.get_name_hash() == hash)?;
        let value = member
            .get_child(0, XfaElement::Items, false)?
            .first_child()?
            .get_content();
        self.set_selected_member_by_value(&value, true, true)
            .then_some(member)
    }
    pub fn set_value(&self, picture: XfaValuePicture, value: &[u32]) -> bool {
        if value.is_empty() {
            return self.sync_value(value, true);
        }
        self.set_pre_null(self.is_null());
        self.set_is_null(false);
        let mut text = value.to_vec();
        let pattern = self.get_picture_content(picture);
        let Some(ui) = self.get_ui_child_node() else {
            return true;
        };
        let numeric = ui.get_element_type() == XfaElement::NumericEdit;
        let mut valid = true;
        let mut sync_data = false;
        if !pattern.is_empty() {
            let widget = get_locale_value(self);
            let locale = self.get_locale();
            let mut matched = Vec::new();
            valid = widget.validate_value(value, &pattern, locale.clone(), Some(&mut matched));
            if valid {
                let manager = self
                    .document()
                    .and_then(|doc| doc.get_locale_mgr())
                    .map(|m| m as Rc<dyn LocaleMgrIface>);
                text = CxfaLocaleValue::with_pattern(
                    widget.get_type(),
                    &text,
                    &matched,
                    locale,
                    manager,
                )
                .get_value()
                .to_vec();
                if numeric {
                    text = self.numeric_limit(&text);
                }
                sync_data = true;
            }
        } else if numeric {
            if text != [48] {
                text = self.numeric_limit(&text);
            }
            sync_data = true;
        }
        if !numeric || sync_data {
            self.sync_value(&text, true);
        }
        valid
    }
    pub fn get_value(&self, picture: XfaValuePicture) -> Vec<u32> {
        let mut value = self.get_content();
        if picture == XfaValuePicture::Display {
            value = self.get_item_label(&value);
        }
        let mut pattern = self.get_picture_content(picture);
        let Some(ui) = self.get_ui_child_node() else {
            return value;
        };
        if ui.get_element_type() == XfaElement::ChoiceList && picture == XfaValuePicture::Display {
            let selected = self.get_selected_item(0);
            if selected >= 0 {
                value = self
                    .get_choice_list_item(selected, false)
                    .unwrap_or_default();
                pattern.clear();
            }
        } else if ui.get_element_type() == XfaElement::NumericEdit
            && picture == XfaValuePicture::Display
            && pattern.is_empty()
        {
            if let Some(locale) = self.get_locale() {
                value = format_num_str(&self.normalize_num_str(&value), locale.as_ref());
            }
        }
        if pattern.is_empty() {
            return value;
        }
        let Some(locale) = self.get_locale() else {
            return value;
        };
        let widget = get_locale_value(self);
        let manager = self
            .document()
            .and_then(|doc| doc.get_locale_mgr())
            .map(|m| m as Rc<dyn LocaleMgrIface>);
        if matches!(widget.get_type(), ValueType::Date | ValueType::Time) {
            if let Some((date, time)) = split_date_time(&value) {
                let part = if widget.get_type() == ValueType::Date {
                    date
                } else {
                    time
                };
                let dated = CxfaLocaleValue::with_value(widget.get_type(), part, manager);
                let mut result = value.clone();
                if dated.format_patterns(&mut result, &pattern, Some(locale.clone()), picture) {
                    return result;
                }
            }
        }
        widget.format_patterns(&mut value, &pattern, Some(locale), picture);
        value
    }
}
