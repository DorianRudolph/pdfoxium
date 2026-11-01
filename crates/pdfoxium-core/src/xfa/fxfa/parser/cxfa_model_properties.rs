// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Native property algorithms from `cxfa_occur`, `cxfa_bind`, `cxfa_ui`,
//! `cxfa_choicelist`, `cxfa_value`, `cxfa_script`, `cxfa_event`, and the
//! UI/value model portion of `cxfa_node`. Constructor metadata lives in
//! node_schema; CJX bindings, widget creation/cache and layout remain separate.
use super::cxfa_node::{CxfaNode, XFA_PROPERTY_ONE_OF};
use crate::xfa::fxfa::fxfa_basic::{XfaAttribute, XfaAttributeValue, XfaElement};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CxfaScriptType {
    Formcalc,
    Javascript,
    Unknown,
}
impl CxfaNode {
    pub fn occur_get_max(&self) -> i32 {
        self.try_integer(XfaAttribute::Max, true)
            .unwrap_or_else(|| self.occur_get_min())
    }
    pub fn occur_get_min(&self) -> i32 {
        self.try_integer(XfaAttribute::Min, true)
            .filter(|&value| value >= 0)
            .unwrap_or(1)
    }
    pub fn occur_get_occur_info(&self) -> (i32, i32, i32) {
        let min = self.occur_get_min();
        let max = self.occur_get_max();
        let initial = self
            .try_integer(XfaAttribute::Initial, false)
            .filter(|&value| value >= min)
            .unwrap_or(min);
        (min, max, initial)
    }
    pub fn occur_set_max(&self, max: i32) {
        let max = if max != -1 && max < 1 { 1 } else { max };
        self.set_integer(XfaAttribute::Max, max, false);
        let min = self.occur_get_min();
        if max != -1 && max < min {
            self.set_integer(XfaAttribute::Min, max, false);
        }
    }
    pub fn occur_set_min(&self, min: i32) {
        let min = if min < 0 { 1 } else { min };
        self.set_integer(XfaAttribute::Min, min, false);
        let max = self.occur_get_max();
        if max > 0 && max < min {
            self.set_integer(XfaAttribute::Max, min, false);
        }
    }
    pub fn bind_get_picture(&self) -> Vec<u32> {
        self.get_child(0, XfaElement::Picture, false)
            .map_or_else(Vec::new, |picture| picture.get_content())
    }
    pub fn bind_get_match(&self) -> XfaAttributeValue {
        self.get_enum(XfaAttribute::Match)
    }
    pub fn bind_get_ref(&self) -> Vec<u32> {
        self.get_cdata(XfaAttribute::Ref)
    }
    pub fn ui_is_a_one_of_child(&self, child: &CxfaNode) -> bool {
        self.has_property_flag(child.get_element_type(), XFA_PROPERTY_ONE_OF)
    }
    pub fn choice_list_get_value_node_type(&self) -> XfaElement {
        if self.get_enum(XfaAttribute::Open) == XfaAttributeValue::MultiSelect {
            XfaElement::ExData
        } else {
            XfaElement::Text
        }
    }
    /// Virtual value-type dispatch from CXFA_Node and the four UI subclasses.
    pub fn get_value_node_type(&self) -> XfaElement {
        match self.get_element_type() {
            XfaElement::ChoiceList => self.choice_list_get_value_node_type(),
            XfaElement::DateTimeEdit => XfaElement::DateTime,
            XfaElement::ImageEdit => XfaElement::Image,
            XfaElement::NumericEdit => XfaElement::Float,
            _ => XfaElement::Text,
        }
    }
    pub fn value_get_child_value_class_id(&self) -> XfaElement {
        self.first_child()
            .map_or(XfaElement::Unknown, |node| node.get_element_type())
    }
    pub fn value_get_child_value_content(&self) -> Vec<u32> {
        self.first_child()
            .and_then(|node| node.try_content())
            .unwrap_or_default()
    }
    fn value_first_if_type(&self, element: XfaElement) -> Option<CxfaNode> {
        self.first_child()
            .filter(|child| child.get_element_type() == element)
    }
    pub fn value_get_arc_if_exists(&self) -> Option<CxfaNode> {
        self.value_first_if_type(XfaElement::Arc)
    }
    pub fn value_get_line_if_exists(&self) -> Option<CxfaNode> {
        self.value_first_if_type(XfaElement::Line)
    }
    pub fn value_get_rectangle_if_exists(&self) -> Option<CxfaNode> {
        self.value_first_if_type(XfaElement::Rectangle)
    }
    pub fn value_get_text_if_exists(&self) -> Option<CxfaNode> {
        self.value_first_if_type(XfaElement::Text)
    }
    pub fn value_get_ex_data_if_exists(&self) -> Option<CxfaNode> {
        self.value_first_if_type(XfaElement::ExData)
    }
    pub fn value_get_image_if_exists(&self) -> Option<CxfaNode> {
        self.value_first_if_type(XfaElement::Image)
    }
    pub fn script_get_content_type(&self) -> CxfaScriptType {
        let Some(content_type) = self.try_cdata(XfaAttribute::ContentType, false) else {
            return CxfaScriptType::Formcalc;
        };
        if content_type
            .iter()
            .copied()
            .eq(b"application/x-formcalc".iter().copied().map(u32::from))
        {
            CxfaScriptType::Formcalc
        } else if content_type
            .iter()
            .copied()
            .eq(b"application/x-javascript".iter().copied().map(u32::from))
        {
            CxfaScriptType::Javascript
        } else {
            CxfaScriptType::Unknown
        }
    }
    pub fn script_get_run_at(&self) -> XfaAttributeValue {
        self.get_enum(XfaAttribute::RunAt)
    }
    pub fn script_get_expression(&self) -> Vec<u32> {
        self.get_content()
    }
    pub fn event_get_activity(&self) -> XfaAttributeValue {
        self.get_enum(XfaAttribute::Activity)
    }
    pub fn event_get_event_type(&self) -> XfaElement {
        let mut child = self.first_child();
        while let Some(node) = child {
            if node.get_element_type() != XfaElement::Extras {
                return node.get_element_type();
            }
            child = node.next_sibling();
        }
        XfaElement::Unknown
    }
    pub fn event_get_script_if_exists(&self) -> Option<CxfaNode> {
        self.get_child(0, XfaElement::Script, false)
    }
    pub fn event_get_ref(&self) -> Vec<u32> {
        self.get_cdata(XfaAttribute::Ref)
    }
    /// XFA_FieldIsMultiListBox: inspect existing first children without creating
    /// default UI/value nodes. The source utility differs from the node getter.
    pub fn is_field_multi_list_box(&self) -> bool {
        self.get_child(0, XfaElement::Ui, false)
            .and_then(|ui| ui.first_child())
            .is_some_and(|child| {
                child.get_element_type() == XfaElement::ChoiceList
                    && child.get_enum(XfaAttribute::Open) == XfaAttributeValue::MultiSelect
            })
    }
    pub fn is_choice_list_commit_on_select(&self) -> bool {
        self.get_ui_child_node()
            .is_none_or(|child| child.get_enum(XfaAttribute::CommitOn) == XfaAttributeValue::Select)
    }
    pub fn is_choice_list_allow_text_entry(&self) -> bool {
        self.get_ui_child_node()
            .is_some_and(|child| child.get_boolean(XfaAttribute::TextEntry))
    }
    pub fn is_list_box(&self) -> bool {
        self.get_ui_child_node().is_some_and(|child| {
            matches!(
                child.get_enum(XfaAttribute::Open),
                XfaAttributeValue::Always | XfaAttributeValue::MultiSelect
            )
        })
    }
}

impl CxfaNode {
    pub fn get_selected_items_value(&self) -> Vec<Vec<u32>> {
        let value = self.get_raw_value();
        if self.is_choice_list_multi_select() {
            // An empty raw value represents no selection. Native Split would
            // manufacture one empty item, which breaks a data-group roundtrip.
            if value.is_empty() {
                Vec::new()
            } else {
                value
                    .split(|&ch| ch == u32::from(b'\n'))
                    .map(<[u32]>::to_vec)
                    .collect()
            }
        } else {
            vec![value]
        }
    }
    pub fn set_selected_member_by_value(
        &self,
        value: &[u32],
        notify: bool,
        sync_data: bool,
    ) -> bool {
        let mut group_value = Vec::new();
        let mut child = self.first_child();
        while let Some(member) = child {
            if member.get_element_type() == XfaElement::Field {
                if let Some(on) = member
                    .get_child(0, XfaElement::Items, false)
                    .and_then(|items| items.first_child())
                {
                    let on_value = on.get_content();
                    let content = if on_value == value {
                        group_value = value.to_vec();
                        on_value
                    } else {
                        on.next_sibling()
                            .map_or_else(Vec::new, |off| off.get_content())
                    };
                    if !member.set_content_with_sync(&content, &content, notify, false) {
                        return false;
                    }
                }
            }
            child = member.next_sibling();
        }
        self.set_content_with_sync(&group_value, &group_value, notify, sync_data)
    }
}

#[cfg(test)]
mod tests {
    use super::super::cxfa_document::CxfaDocument;
    use super::*;
    use crate::xfa::fxfa::fxfa_basic::XfaPacketType;
    use std::rc::Rc;
    fn wide(value: &str) -> Vec<u32> {
        value.chars().map(u32::from).collect()
    }
    fn node(doc: &Rc<CxfaDocument>, element: XfaElement) -> CxfaNode {
        doc.create_node(XfaPacketType::Form, element).unwrap()
    }
    fn add(parent: &CxfaNode, child: &CxfaNode) {
        assert!(parent.insert_child_and_notify(-1, child));
    }
    #[test]
    fn occur_defaults_clamping_unbounded_max_and_initial_minimum() {
        let doc = CxfaDocument::new();
        let occur = node(&doc, XfaElement::Occur);
        assert_eq!(occur.occur_get_occur_info(), (1, 1, 1));
        occur.occur_set_min(5);
        assert_eq!(occur.occur_get_occur_info(), (5, 5, 5));
        occur.occur_set_max(3);
        assert_eq!(occur.occur_get_occur_info(), (3, 3, 3));
        occur.occur_set_max(-1);
        occur.occur_set_min(0);
        assert_eq!(occur.occur_get_occur_info(), (0, -1, 0));
        occur.occur_set_max(0);
        assert_eq!(occur.occur_get_occur_info(), (0, 1, 0));
        occur.occur_set_min(-9);
        assert_eq!(occur.occur_get_occur_info(), (1, 1, 1));
        occur.set_integer(XfaAttribute::Min, -2, false);
        assert_eq!(occur.occur_get_min(), 1);
        occur.occur_set_min(3);
        occur.set_integer(XfaAttribute::Initial, 2, false);
        assert_eq!(occur.occur_get_occur_info(), (3, 3, 3));
        occur.set_integer(XfaAttribute::Initial, 100, false);
        assert_eq!(occur.occur_get_occur_info(), (3, 3, 100));
        occur.occur_set_max(-7);
        assert_eq!(occur.occur_get_occur_info(), (1, 1, 100));
    }
    #[test]
    fn bind_picture_and_value_first_child_access_preserve_typed_identity() {
        let doc = CxfaDocument::new();
        let bind = node(&doc, XfaElement::Bind);
        assert_eq!(bind.bind_get_match(), XfaAttributeValue::Once);
        assert!(bind.bind_get_picture().is_empty());
        bind.set_enum(XfaAttribute::Match, XfaAttributeValue::DataRef, false);
        bind.set_cdata(XfaAttribute::Ref, &wide("$record.amount"));
        assert_eq!(bind.bind_get_match(), XfaAttributeValue::DataRef);
        assert_eq!(bind.bind_get_ref(), wide("$record.amount"));
        let picture = node(&doc, XfaElement::Picture);
        assert!(picture.set_content(&wide("num{zz9.99}"), &wide("num{zz9.99}"), false));
        add(&bind, &picture);
        assert_eq!(bind.bind_get_picture(), wide("num{zz9.99}"));
        let value = node(&doc, XfaElement::Value);
        assert_eq!(value.value_get_child_value_class_id(), XfaElement::Unknown);
        assert!(value.value_get_child_value_content().is_empty());
        let text = node(&doc, XfaElement::Text);
        assert!(text.set_content(&wide("content"), &wide("content"), false));
        add(&value, &text);
        assert_eq!(value.value_get_child_value_class_id(), XfaElement::Text);
        assert_eq!(value.value_get_child_value_content(), wide("content"));
        assert_eq!(value.value_get_text_if_exists(), Some(text));
        assert!(value.value_get_arc_if_exists().is_none());
        assert!(value.value_get_line_if_exists().is_none());
        assert!(value.value_get_rectangle_if_exists().is_none());
        assert!(value.value_get_ex_data_if_exists().is_none());
        assert!(value.value_get_image_if_exists().is_none());
    }
    #[test]
    fn ui_infers_every_existing_value_type_without_replacing_it() {
        let doc = CxfaDocument::new();
        for (value_type, ui_type) in [
            (XfaElement::Boolean, XfaElement::CheckButton),
            (XfaElement::Integer, XfaElement::NumericEdit),
            (XfaElement::Decimal, XfaElement::NumericEdit),
            (XfaElement::Float, XfaElement::NumericEdit),
            (XfaElement::ExData, XfaElement::TextEdit),
            (XfaElement::Text, XfaElement::TextEdit),
            (XfaElement::Date, XfaElement::DateTimeEdit),
            (XfaElement::Time, XfaElement::DateTimeEdit),
            (XfaElement::DateTime, XfaElement::DateTimeEdit),
            (XfaElement::Image, XfaElement::ImageEdit),
            (XfaElement::Arc, XfaElement::DefaultUi),
            (XfaElement::Line, XfaElement::DefaultUi),
            (XfaElement::Rectangle, XfaElement::DefaultUi),
        ] {
            let field = node(&doc, XfaElement::Field);
            let value = node(&doc, XfaElement::Value);
            let content = node(&doc, value_type);
            add(&field, &value);
            add(&value, &content);
            let ui = field.get_ui_child_node().unwrap();
            assert_eq!(ui.get_element_type(), ui_type, "{value_type:?}");
            assert_eq!(value.first_child(), Some(content));
            assert_eq!(value.count_children(XfaElement::Unknown, false), 1);
            assert_eq!(field.get_ui_child_node(), Some(ui));
        }
    }
    #[test]
    fn selected_ui_creates_value_types_and_checkbox_uses_first_item_type() {
        let doc = CxfaDocument::new();
        for (ui_type, value_type) in [
            (XfaElement::TextEdit, XfaElement::Text),
            (XfaElement::NumericEdit, XfaElement::Float),
            (XfaElement::DateTimeEdit, XfaElement::DateTime),
            (XfaElement::ImageEdit, XfaElement::Image),
            (XfaElement::ChoiceList, XfaElement::Text),
        ] {
            let field = node(&doc, XfaElement::Field);
            let ui = node(&doc, XfaElement::Ui);
            let selected = node(&doc, ui_type);
            add(&field, &ui);
            add(&ui, &selected);
            assert_eq!(field.get_ui_child_node(), Some(selected));
            assert_eq!(
                field
                    .get_child(0, XfaElement::Value, false)
                    .unwrap()
                    .value_get_child_value_class_id(),
                value_type
            );
        }
        let field = node(&doc, XfaElement::Field);
        let ui = node(&doc, XfaElement::Ui);
        let button = node(&doc, XfaElement::CheckButton);
        let items = node(&doc, XfaElement::Items);
        let integer = node(&doc, XfaElement::Integer);
        add(&field, &ui);
        add(&ui, &button);
        add(&field, &items);
        add(&items, &integer);
        field.get_ui_child_node().unwrap();
        assert_eq!(
            field
                .get_child(0, XfaElement::Value, false)
                .unwrap()
                .value_get_child_value_class_id(),
            XfaElement::Integer
        );
        let draw = node(&doc, XfaElement::Draw);
        assert_eq!(
            draw.get_ui_child_node().unwrap().get_element_type(),
            XfaElement::TextEdit
        );
        assert_eq!(
            draw.get_child(0, XfaElement::Value, false)
                .unwrap()
                .value_get_child_value_class_id(),
            XfaElement::Text
        );
    }
    #[test]
    fn multi_select_ui_one_of_scan_and_non_creating_utility_have_distinct_semantics() {
        let doc = CxfaDocument::new();
        let field = node(&doc, XfaElement::Field);
        assert!(!field.is_field_multi_list_box());
        assert!(field.first_child().is_none());
        let ui = node(&doc, XfaElement::Ui);
        let extras = node(&doc, XfaElement::Extras);
        let choice = node(&doc, XfaElement::ChoiceList);
        choice.set_enum(XfaAttribute::Open, XfaAttributeValue::MultiSelect, false);
        choice.set_boolean(XfaAttribute::TextEntry, true, false);
        add(&field, &ui);
        add(&ui, &extras);
        add(&ui, &choice);
        assert!(!ui.ui_is_a_one_of_child(&extras));
        assert!(ui.ui_is_a_one_of_child(&choice));
        assert!(!field.is_field_multi_list_box());
        assert_eq!(field.get_ui_child_node(), Some(choice.clone()));
        assert!(field.is_choice_list_multi_select());
        assert!(field.is_list_box());
        assert!(field.is_choice_list_allow_text_entry());
        assert!(field.is_choice_list_commit_on_select());
        assert_eq!(
            field
                .get_child(0, XfaElement::Value, false)
                .unwrap()
                .value_get_child_value_class_id(),
            XfaElement::ExData
        );
        assert!(ui.remove_child_and_notify(&extras, false));
        assert!(field.is_field_multi_list_box());
        choice.set_enum(XfaAttribute::Open, XfaAttributeValue::Always, false);
        choice.set_enum(XfaAttribute::CommitOn, XfaAttributeValue::Exit, false);
        assert!(!field.is_choice_list_multi_select());
        assert!(field.is_list_box());
        assert!(!field.is_choice_list_commit_on_select());
    }
    #[test]
    fn script_language_defaults_and_event_first_non_extras_type() {
        let doc = CxfaDocument::new();
        let script = node(&doc, XfaElement::Script);
        assert_eq!(script.script_get_content_type(), CxfaScriptType::Formcalc);
        assert_eq!(script.script_get_run_at(), XfaAttributeValue::Client);
        for (value, expected) in [
            ("application/x-formcalc", CxfaScriptType::Formcalc),
            ("application/x-javascript", CxfaScriptType::Javascript),
            ("", CxfaScriptType::Unknown),
            ("application/X-javascript", CxfaScriptType::Unknown),
        ] {
            script.set_cdata(XfaAttribute::ContentType, &wide(value));
            assert_eq!(script.script_get_content_type(), expected);
        }
        assert!(script.set_content(&wide("1 + 2"), &wide("1 + 2"), false));
        assert_eq!(script.script_get_expression(), wide("1 + 2"));
        script.set_enum(XfaAttribute::RunAt, XfaAttributeValue::Server, false);
        assert_eq!(script.script_get_run_at(), XfaAttributeValue::Server);
        let event = node(&doc, XfaElement::Event);
        assert_eq!(event.event_get_activity(), XfaAttributeValue::Click);
        assert_eq!(event.event_get_event_type(), XfaElement::Unknown);
        assert!(event.event_get_script_if_exists().is_none());
        let extras = node(&doc, XfaElement::Extras);
        add(&event, &extras);
        assert_eq!(event.event_get_event_type(), XfaElement::Unknown);
        add(&event, &script);
        assert_eq!(event.event_get_event_type(), XfaElement::Script);
        assert_eq!(event.event_get_script_if_exists(), Some(script));
        event.set_cdata(XfaAttribute::Ref, &wide("$form.item"));
        assert_eq!(event.event_get_ref(), wide("$form.item"));
    }
}
