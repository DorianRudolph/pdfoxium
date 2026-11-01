// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Check/radio/button model methods from `cxfa_node.cpp`, consumed by the real
//! FF controls. Values use the existing data-binding synchronization path.
use super::cxfa_node::CxfaNode;
use crate::xfa::fxfa::fxfa_basic::{XfaAttribute as A, XfaElement as E, XfaUnit};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum XfaCheckState {
    On = 0,
    Off = 1,
    Neutral = 2,
}
impl CxfaNode {
    fn has_button_item(&self, name: &[u32]) -> bool {
        let Some(items) = self.get_child(0, E::Items, false) else {
            return false;
        };
        let mut item = items.first_child();
        while let Some(current) = item {
            if current.get_cdata(A::Name) == name {
                return !current.get_content().is_empty();
            }
            item = current.next_sibling();
        }
        false
    }
    pub fn has_button_rollover(&self) -> bool {
        self.has_button_item(&[114, 111, 108, 108, 111, 118, 101, 114])
    }
    pub fn has_button_down(&self) -> bool {
        self.has_button_item(&[100, 111, 119, 110])
    }
    pub fn is_radio_button(&self) -> bool {
        self.parent()
            .is_some_and(|p| p.get_element_type() == E::ExclGroup)
    }
    pub fn get_check_button_size(&self) -> f32 {
        self.get_ui_child_node()
            .map_or(10., |ui| ui.get_measure_in_unit(A::Size, XfaUnit::Pt))
    }
    pub fn get_check_state(&self) -> XfaCheckState {
        let value = self.get_raw_value();
        if value.is_empty() {
            return XfaCheckState::Off;
        }
        let Some(items) = self.get_child(0, E::Items, false) else {
            return XfaCheckState::Off;
        };
        let mut item = items.first_child();
        let mut index = 0;
        while let Some(current) = item {
            if current.try_content().as_ref() == Some(&value) {
                return match index {
                    0 => XfaCheckState::On,
                    2 => XfaCheckState::Neutral,
                    _ => XfaCheckState::Off,
                };
            }
            item = current.next_sibling();
            index += 1;
        }
        XfaCheckState::Off
    }
    pub fn set_check_state(&self, state: XfaCheckState) -> bool {
        let _store = self.retain_store();
        let Some(group) = self.get_excl_group_if_exists() else {
            let Some(items) = self.get_child(0, E::Items, false) else {
                return false;
            };
            let content = items
                .get_child(state as usize, E::Unknown, false)
                .map_or_else(Vec::new, |n| n.get_content());
            return self.sync_value(&content, true);
        };
        let value = if state != XfaCheckState::Off {
            self.get_child(0, E::Items, false)
                .and_then(|items| items.first_child())
                .map_or_else(Vec::new, |n| n.get_content())
        } else {
            Vec::new()
        };
        // Snapshot only weak node identities. A SyncValue observer can remove
        // siblings; do not follow a detached sibling chain or hold tree borrows
        // across those callbacks.
        let mut members = Vec::new();
        let mut item = group.first_child();
        while let Some(current) = item {
            item = current.next_sibling();
            if current.get_element_type() == E::Field {
                members.push(current);
            }
        }
        for member in members {
            if member.parent().as_ref() != Some(&group) {
                continue;
            }
            let Some(first) = member
                .get_child(0, E::Items, false)
                .and_then(|items| items.first_child())
            else {
                continue;
            };
            let on = first.get_content();
            let content = if value == on {
                on
            } else {
                first
                    .next_sibling()
                    .map_or_else(Vec::new, |n| n.get_content())
            };
            if !member.sync_value(&content, true) {
                return false;
            }
        }
        group.sync_value(&value, true)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::xfa::fxfa::{fxfa_basic::XfaPacketType, parser::cxfa_document::CxfaDocument};
    use std::rc::Rc;
    fn wide(s: &str) -> Vec<u32> {
        s.chars().map(u32::from).collect()
    }
    fn node(doc: &Rc<CxfaDocument>, element: E) -> CxfaNode {
        doc.create_node(XfaPacketType::Form, element).unwrap()
    }
    fn field(doc: &Rc<CxfaDocument>, name: &str, on: &str) -> CxfaNode {
        let field = node(doc, E::Field);
        field.set_cdata(A::Name, &wide(name));
        let ui = field.get_or_create_property(0, E::Ui).unwrap();
        ui.get_or_create_property(0, E::CheckButton).unwrap();
        let items = node(doc, E::Items);
        field.insert_child_before_and_notify(&items, None);
        for value in [on, "off", "neutral"] {
            let text = node(doc, E::Text);
            items.insert_child_before_and_notify(&text, None);
            text.set_content(&wide(value), &wide(value), false);
        }
        field
    }
    #[test]
    fn checkbox_items_select_on_off_neutral_through_sync_value() {
        let doc = CxfaDocument::new();
        let field = field(&doc, "check", "yes");
        assert_eq!(field.get_check_state(), XfaCheckState::Off);
        assert_eq!(field.get_check_button_size(), 10.);
        for (state, value) in [
            (XfaCheckState::On, "yes"),
            (XfaCheckState::Off, "off"),
            (XfaCheckState::Neutral, "neutral"),
        ] {
            assert!(field.set_check_state(state));
            assert_eq!(field.get_raw_value(), wide(value));
            assert_eq!(field.get_check_state(), state);
        }
    }
    #[test]
    fn radio_group_selection_syncs_all_members_and_empty_off_group() {
        let doc = CxfaDocument::new();
        let group = node(&doc, E::ExclGroup);
        let a = field(&doc, "a", "A");
        let b = field(&doc, "b", "B");
        group.insert_child_before_and_notify(&a, None);
        group.insert_child_before_and_notify(&b, None);
        assert!(a.is_radio_button());
        assert!(a.set_check_state(XfaCheckState::On));
        assert_eq!(group.get_raw_value(), wide("A"));
        assert_eq!(b.get_raw_value(), wide("off"));
        assert_eq!(group.get_selected_member(), Some(a.clone()));
        assert_eq!(group.set_selected_member(&wide("b")), Some(b.clone()));
        assert_eq!(a.get_check_state(), XfaCheckState::Off);
        assert_eq!(b.get_check_state(), XfaCheckState::On);
        assert!(b.set_check_state(XfaCheckState::Off));
        assert!(group.get_raw_value().is_empty());
        assert!(group.get_selected_member().is_none());
    }
    #[test]
    fn highlighted_button_items_require_named_nonempty_content() {
        let doc = CxfaDocument::new();
        let button = node(&doc, E::Field);
        assert!(!button.has_button_rollover());
        let items = node(&doc, E::Items);
        button.insert_child_before_and_notify(&items, None);
        for (name, value) in [("rollover", "hover"), ("down", "")] {
            let item = node(&doc, E::Text);
            item.set_cdata(A::Name, &wide(name));
            items.insert_child_before_and_notify(&item, None);
            item.set_content(&wide(value), &wide(value), false);
        }
        assert!(button.has_button_rollover());
        assert!(!button.has_button_down());
    }
}
