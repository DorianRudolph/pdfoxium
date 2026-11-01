// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Public CXFA_Node event/script/calculation/validation/reset source extensions
//! used by the real FF document view (`parser/cxfa_node.cpp`).
use super::*;
use crate::{
    fxjs::cjs_event_context::wide_from_utf16,
    xfa::fxfa::parser::{
        cxfa_localevalue::XfaValuePicture, cxfa_model_properties::CxfaScriptType,
        xfa_utils::get_locale_value,
    },
};

struct ExecuteScope<'a> {
    view: &'a CxfaFfDocView,
    node: CxfaNode,
}
impl Drop for ExecuteScope<'_> {
    fn drop(&mut self) {
        let mut depths = self.view.execute_recursions.borrow_mut();
        if let Some(depth) = depths.get_mut(&self.node) {
            *depth = depth.saturating_sub(1);
            if *depth == 0 {
                depths.remove(&self.node);
            }
        }
    }
}
impl CxfaNode {
    pub fn events_by_activity(&self, activity: V, form_ready: bool) -> Vec<Self> {
        let mut result = Vec::new();
        let mut child = self.first_child();
        while let Some(node) = child {
            child = node.next_sibling();
            if node.get_element_type() != E::Event || node.event_get_activity() != activity {
                continue;
            }
            if activity == V::Ready
                && node.event_get_ref() != wide(if form_ready { "$form" } else { "$layout" })
            {
                continue;
            }
            result.push(node);
        }
        result
    }
    pub fn process_event(
        &self,
        view: &CxfaFfDocView,
        activity: V,
        event: &mut CxfaEventParam,
    ) -> EventError {
        if self.get_element_type() == E::Draw {
            return EventError::kNotExist;
        }
        let mut result = EventError::kNotExist;
        let mut first = true;
        for node in self.events_by_activity(activity, event.is_form_ready) {
            let current = match node.event_get_event_type() {
                E::Script if activity != V::DocClose => {
                    self.execute_bool_script(
                        view,
                        node.event_get_script_if_exists().as_ref(),
                        event,
                    )
                    .0
                }
                E::Submit => EventError::kDisabled,
                _ => EventError::kNotExist,
            };
            if first || current == EventError::kSuccess {
                result = current;
            }
            first = false;
        }
        result
    }
    pub fn execute_script(
        &self,
        view: &CxfaFfDocView,
        script: Option<&CxfaNode>,
        event: &mut CxfaEventParam,
    ) -> EventError {
        self.execute_bool_script(view, script, event).0
    }
    pub fn execute_bool_script(
        &self,
        view: &CxfaFfDocView,
        script: Option<&CxfaNode>,
        event: &mut CxfaEventParam,
    ) -> (EventError, bool) {
        let depth = view
            .execute_recursions
            .borrow()
            .get(self)
            .copied()
            .unwrap_or(0);
        if depth > 2 {
            return (EventError::kSuccess, false);
        }
        let Some(script) = script else {
            return (EventError::kNotExist, false);
        };
        if script.script_get_run_at() == V::Server {
            return (EventError::kDisabled, false);
        }
        let expression = script.script_get_expression();
        if expression.is_empty() {
            return (EventError::kNotExist, false);
        }
        let kind = script.script_get_content_type();
        if kind == CxfaScriptType::Unknown {
            return (EventError::kSuccess, false);
        }
        let Some(doc) = view.doc() else {
            return (EventError::kNotExist, false);
        };
        let Some(engine) = doc.try_script_context() else {
            return (EventError::kNotExist, false);
        };
        let calculate = matches!(
            event.event_type,
            XfaEventType::Calculate | XfaEventType::InitCalculate
        );
        let previous_accesses = if calculate {
            engine.take_accessed_nodes()
        } else {
            Vec::new()
        };
        let target = event.targeted.then_some(self);
        let record = Rc::new(RefCell::new(event.clone()));
        engine.set_run_at_type(script.script_get_run_at());
        *view
            .execute_recursions
            .borrow_mut()
            .entry(self.clone())
            .or_default() += 1;
        let _depth = ExecuteScope {
            view,
            node: self.clone(),
        };
        let (value, final_event) = engine.with_event(target, record.clone(), || {
            let result = (|| {
                let value = engine.run_script(kind, &expression, Some(self))?;
                let boolean = value.as_boolean().unwrap_or(false);
                if calculate {
                    if value.is_undefined() {
                        return Ok((EventError::kError, boolean));
                    }
                    if !value.is_null() {
                        let text = engine
                            .javascript()
                            .with_context(|context| value.to_string(context))?;
                        let active = engine.current_event().unwrap_or_else(|| record.clone());
                        active.borrow_mut().result = wide_from_utf16(&text.to_vec());
                    }
                }
                Ok::<_, boa_engine::JsError>((EventError::kSuccess, boolean))
            })();
            // xfa.event.reset replaces the active record. Capture that record
            // before the scope restores its caller's event frame.
            let active = engine.current_event().unwrap_or_else(|| record.clone());
            let event = active.borrow().clone();
            (result, event)
        });
        *event = final_event;
        let result = value.unwrap_or((EventError::kError, false));
        if calculate {
            let references = engine.take_accessed_nodes();
            for node in previous_accesses {
                engine.record_access(&node);
            }
            if result.0 == EventError::kSuccess {
                if event.event_type == XfaEventType::InitCalculate
                    && self.get_raw_value() != event.result
                {
                    self.set_value(XfaValuePicture::Raw, &event.result);
                    view.add_validate_node(self);
                }
                for node in references {
                    if node == *self {
                        continue;
                    }
                    node.add_calculation_dependent(self);
                }
            }
        }
        result
    }
    pub fn process_calculate(&self, view: &CxfaFfDocView) -> EventError {
        if self.get_element_type() == E::Draw {
            return EventError::kNotExist;
        }
        let Some(calc) = self.get_child(0, E::Calculate, false) else {
            return EventError::kNotExist;
        };
        if self.is_user_interactive() {
            return EventError::kDisabled;
        }
        let mut event = CxfaEventParam::new(XfaEventType::Calculate);
        event.targeted = false;
        let result = self.execute_script(
            view,
            calc.get_child(0, E::Script, false).as_ref(),
            &mut event,
        );
        if result == EventError::kSuccess && self.get_raw_value() != event.result {
            self.set_value(XfaValuePicture::Raw, &event.result);
            view.update_ui_display(self, None);
        }
        result
    }
    fn validate_caption_name(&self, old_version: bool) -> Vec<u32> {
        if !old_version {
            if let Some(text) = self
                .get_child(0, E::Caption, false)
                .and_then(|caption| caption.get_child(0, E::Value, false))
                .and_then(|value| value.value_get_text_if_exists())
            {
                let content = text.get_content();
                if !content.is_empty() {
                    return content;
                }
            }
        }
        self.get_cdata(A::Name)
    }
    fn validate_message(&self, error: bool, old_version: bool) -> Vec<u32> {
        let name = self.validate_caption_name(old_version);
        if old_version {
            return [name, wide(" validation failed")].concat();
        }
        let mut message = wide("The value you entered for ");
        message.extend_from_slice(&name);
        if error {
            message.extend(wide(" is invalid."));
        } else {
            message.extend(wide(" is invalid. To ignore validations for "));
            message.extend_from_slice(&name);
            message.extend(wide(", click Ignore."));
        }
        message
    }
    fn validation_message_box(
        &self,
        view: &CxfaFfDocView,
        message: &[u32],
        icon: AlertIcon,
        buttons: AlertButton,
    ) -> Option<AlertReturn> {
        let app = view.doc()?.app();
        app_message(
            &app,
            message,
            &app.provider().get_app_title(),
            icon,
            buttons,
        )
    }
    fn process_script_test_validate(
        &self,
        view: &CxfaFfDocView,
        validate: &Self,
        old_version: bool,
    ) {
        let warning = validate.validate_get_script_test() == V::Warning;
        if warning && self.is_user_interactive() {
            return;
        }
        let mut message = validate.validate_get_message_text(&wide("scriptTest"));
        if message.is_empty() {
            message = self.validate_message(!warning, old_version);
        }
        let (icon, buttons) = if warning {
            (
                AlertIcon::kWarning,
                if old_version {
                    AlertButton::kOK
                } else {
                    AlertButton::kYesNo
                },
            )
        } else {
            (AlertIcon::kError, AlertButton::kOK)
        };
        if self.validation_message_box(view, &message, icon, buttons) == Some(AlertReturn::kYes)
            && warning
            && !old_version
        {
            self.set_flag(XfaNodeFlag::UserInteractive);
        }
    }
    fn process_format_test_validate(
        &self,
        view: &CxfaFfDocView,
        validate: &Self,
        old_version: bool,
    ) -> EventError {
        let picture = validate.validate_get_picture();
        if picture.is_empty() {
            return EventError::kNotExist;
        }
        if self.get_raw_value().is_empty() {
            return EventError::kError;
        }
        let Some(locale) = self.get_locale() else {
            return EventError::kNotExist;
        };
        let value = get_locale_value(self);
        if value.validate_value(value.get_value(), &picture, Some(locale), None) {
            return EventError::kSuccess;
        }
        let mut message = validate.validate_get_message_text(&wide("formatTest"));
        let error = validate.validate_get_format_test() == V::Error;
        if message.is_empty() {
            message = self.validate_message(error, old_version);
        }
        let (icon, buttons) = if error {
            (AlertIcon::kError, AlertButton::kOK)
        } else {
            (
                AlertIcon::kWarning,
                if old_version {
                    AlertButton::kOK
                } else {
                    AlertButton::kYesNo
                },
            )
        };
        let Some(answer) = self.validation_message_box(view, &message, icon, buttons) else {
            return EventError::kNotExist;
        };
        if !error && !old_version && answer == AlertReturn::kYes {
            self.set_flag(XfaNodeFlag::UserInteractive);
        }
        EventError::kError
    }
    fn process_null_test_validate(
        &self,
        view: &CxfaFfDocView,
        validate: &Self,
        flags: i32,
        old_version: bool,
    ) -> EventError {
        if !self.get_value(XfaValuePicture::Raw).is_empty() || (self.is_null() && self.pre_null()) {
            return EventError::kSuccess;
        }
        let null_test = validate.validate_get_null_test();
        let mut message = validate.validate_get_message_text(&wide("nullTest"));
        if flags & 1 != 0 {
            if null_test == V::Disabled {
                return EventError::kSuccess;
            }
            if !message.is_empty() {
                view.add_null_test_msg(&message);
            }
            return EventError::kError;
        }
        if message.is_empty() && old_version && null_test != V::Disabled {
            return EventError::kError;
        }
        match null_test {
            V::Error => {
                if message.is_empty() {
                    message = self.validate_caption_name(old_version);
                    message.extend(wide(" cannot be blank."));
                }
                if self
                    .validation_message_box(view, &message, AlertIcon::kStatus, AlertButton::kOK)
                    .is_none()
                {
                    EventError::kNotExist
                } else {
                    EventError::kError
                }
            }
            V::Warning => {
                if self.is_user_interactive() {
                    return EventError::kSuccess;
                }
                if message.is_empty() {
                    let name = self.validate_caption_name(old_version);
                    message.extend_from_slice(&name);
                    message.extend(wide(" cannot be blank. To ignore validations for "));
                    message.extend(name);
                    message.extend(wide(", click Ignore."));
                }
                let Some(answer) = self.validation_message_box(
                    view,
                    &message,
                    AlertIcon::kWarning,
                    AlertButton::kYesNo,
                ) else {
                    return EventError::kNotExist;
                };
                if answer == AlertReturn::kYes {
                    self.set_flag(XfaNodeFlag::UserInteractive);
                }
                EventError::kError
            }
            _ => EventError::kSuccess,
        }
    }
    pub fn process_validate(&self, view: &CxfaFfDocView, flags: i32) -> EventError {
        if self.get_element_type() == E::Draw {
            return EventError::kNotExist;
        }
        let Some(validate) = self.get_child(0, E::Validate, false) else {
            return EventError::kNotExist;
        };
        let initialize = validate.has_flag(XfaNodeFlag::NeedsInitApp);
        let has_bool = (initialize || view.status.get() != LayoutStatus::End)
            && self.get_raw_value().is_empty();
        let mut event = CxfaEventParam::new(XfaEventType::Validate);
        let (mut result, boolean) = self.execute_bool_script(
            view,
            validate.validate_get_script_if_exists().as_ref(),
            &mut event,
        );
        let mut old_version = view
            .doc()
            .is_some_and(|doc| doc.model_document().get_cur_version_mode() < 208);
        let mut format = EventError::kNotExist;
        if initialize {
            validate.clear_flag(XfaNodeFlag::NeedsInitApp);
        } else {
            format = self.process_format_test_validate(view, &validate, old_version);
            if !old_version {
                old_version = view
                    .doc()
                    .is_some_and(|doc| doc.model_document().is_scripting());
            }
            event_error_accumulate(
                &mut result,
                self.process_null_test_validate(view, &validate, flags, old_version),
            );
        }
        if result == EventError::kSuccess && format != EventError::kSuccess && has_bool && !boolean
        {
            self.process_script_test_validate(view, &validate, old_version);
        }
        event_error_accumulate(&mut result, format);
        result
    }
    pub fn set_image_edit(&self, content_type: &[u32], href: &[u32], data: &[u32]) {
        let image = self
            .get_form_value_if_exists()
            .and_then(|value| value.value_get_image_if_exists());
        if let Some(image) = image.as_ref() {
            image.image_set_content_type(content_type);
            image.image_set_href(href);
        }
        self.set_content_with_sync(data, &self.get_format_data_value(data), true, true);
        let Some(binding) = self.get_bind_data() else {
            if let Some(image) = image {
                image.image_set_transfer_encoding(V::Base64);
            }
            return;
        };
        binding.set_cdata(A::ContentType, content_type);
        if let Some(href_node) = binding.first_child() {
            href_node.set_cdata(A::Value, href);
        } else if let Some(xml) = binding.get_xml_mapping_node() {
            if xml.node_type() == crate::fxcrt::xml::cfx_xmlnode::XmlNodeType::Element {
                xml.set_attribute(&wide("href"), href);
            }
        }
    }
    pub fn reset_data(&self) {
        let mut value = Vec::new();
        match self.get_ff_widget_type() {
            XfaFfWidgetType::ImageEdit => {
                let image = self
                    .get_default_value_if_exists()
                    .and_then(|value| value.value_get_image_if_exists());
                let (content_type, href) = image.as_ref().map_or_else(
                    || (Vec::new(), Vec::new()),
                    |image| {
                        value = image.image_get_content();
                        (image.image_get_content_type(), image.image_get_href())
                    },
                );
                self.set_image_edit(&content_type, &href, &value);
            }
            XfaFfWidgetType::ExclGroup => {
                let mut child = self.get_first_container_child();
                while let Some(node) = child {
                    child = node.get_next_container_sibling();
                    if !node.is_widget_ready() {
                        continue;
                    }
                    let default = if value.is_empty() {
                        node.get_default_value_if_exists()
                    } else {
                        None
                    };
                    if let Some(default) = default {
                        value = default.value_get_child_value_content();
                        self.set_value(XfaValuePicture::Raw, &value);
                        node.set_value(XfaValuePicture::Raw, &value);
                    } else {
                        let off = node
                            .get_child(0, E::Items, false)
                            .and_then(|items| items.get_child(1, E::Unknown, false))
                            .map_or_else(Vec::new, |node| node.get_content());
                        node.set_value(XfaValuePicture::Raw, &off);
                    }
                }
            }
            widget => {
                if widget == XfaFfWidgetType::ChoiceList {
                    self.clear_all_selections();
                }
                if let Some(default) = self.get_default_value_if_exists() {
                    value = default.value_get_child_value_content();
                }
                self.set_value(XfaValuePicture::Raw, &value);
            }
        }
    }
}
