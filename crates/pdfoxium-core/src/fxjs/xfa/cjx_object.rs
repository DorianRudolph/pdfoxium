// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Executable generic attributes and typed default/raw values from CJX_Object,
//! Field, Draw, Boolean, ExclGroup, Occur, Script and Subform. Specialized
//! appearance, validation, event and FF callbacks remain explicitly unported.
use super::cfxjse_engine::CfxJseEngine;
use crate::{
    fxcrt::{
        fx_string::{string_to_int, utf16_encode},
        fx_system::wtoi,
    },
    fxjs::{cfxjs_engine::CfxJsEngine, cjs_event_context::wide_from_utf16},
    xfa::{
        fgas::crt::cfgas_decimal::CfgasDecimal,
        fxfa::{
            fxfa_basic::{XfaAttribute, XfaElement, XfaPacketType},
            parser::{
                cxfa_node::{CxfaNode, XfaFfWidgetType},
                xfa_basic_data::ScriptAttributeDescriptor,
            },
        },
    },
};
use boa_engine::{Context, JsNativeError, JsResult, JsString, JsValue};
fn string(value: &[u32]) -> JsValue {
    JsString::from(utf16_encode(value).as_slice()).into()
}
fn unsupported(callback: &str) -> boa_engine::JsError {
    JsNativeError::error()
        .with_message(format!("XFA script callback is not ported: {callback}"))
        .into()
}
fn readonly() -> boa_engine::JsError {
    JsNativeError::typ()
        .with_message("Invalid property set operation")
        .into()
}
fn boxed(
    runtime: &CfxJseEngine,
    node: Option<CxfaNode>,
    context: &mut Context,
) -> JsResult<JsValue> {
    node.map_or_else(
        || Ok(JsValue::null()),
        |node| {
            runtime
                .get_or_create_binding(&node, context)
                .map(Into::into)
        },
    )
}
fn default_callback(name: &str) -> bool {
    matches!(
        name,
        "CJX_Object::ScriptSomDefaultValue"
            | "CJX_Field::defaultValue"
            | "CJX_Field::rawValue"
            | "CJX_Draw::defaultValue"
            | "CJX_Draw::rawValue"
            | "CJX_Boolean::defaultValue"
            | "CJX_Boolean::value"
            | "CJX_TextNode::defaultValue"
            | "CJX_TextNode::value"
            | "CJX_ExclGroup::defaultValue"
            | "CJX_ExclGroup::rawValue"
    )
}
pub(crate) fn read_attribute(
    runtime: &CfxJseEngine,
    node: &CxfaNode,
    descriptor: ScriptAttributeDescriptor,
    context: &mut Context,
) -> JsResult<JsValue> {
    let name = descriptor.callback_name;
    if super::cjx_model_noops::is_noop_property(name) {
        return Ok(JsValue::undefined());
    }
    if name == "CJX_Form::checksumS" {
        return Ok(super::cjx_form::read_checksum(node));
    }
    if default_callback(name) {
        return read_default(runtime, node, context);
    }
    if super::cjx_presentation::handles(name) {
        return super::cjx_presentation::read_property(node, name, context);
    }
    if name.starts_with("CJX_Field::") {
        return super::cjx_field::read_property(runtime, node, name, context);
    }
    if name.starts_with("CJX_Subform::") {
        return super::cjx_subform::read_property(runtime, node, name, context);
    }
    if name.starts_with("CJX_ExclGroup::") {
        return super::cjx_exclgroup::read_property(runtime, node, name, context);
    }
    if name.starts_with("CJX_InstanceManager::") {
        return super::cjx_instancemanager::read_attribute(node, name);
    }
    match name {
        "CJX_Model::context" | "CJX_Model::aliasNode" | "CJX_Object::ScriptSubmitFormatMode" => {
            Ok(JsValue::undefined())
        }
        "CJX_Packet::content" => Ok(super::cjx_packet::read_content(node)),
        "CJX_Object::ScriptSomInstanceIndex" => Ok(node.get_instance_index().into()),
        "CJX_Object::ScriptAttributeString" => {
            Ok(string(&node.get_attribute_by_enum(descriptor.attribute)))
        }
        "CJX_Object::ScriptAttributeBool" => {
            Ok(JsString::from(if node.get_boolean(descriptor.attribute) {
                "1"
            } else {
                "0"
            })
            .into())
        }
        "CJX_Object::ScriptAttributeInteger" => Ok(node.get_integer(descriptor.attribute).into()),
        "CJX_Object::className" => Ok(JsString::from(node.get_class_name()).into()),
        "CJX_Object::ScriptSomDefaultValue_Read" => {
            let value = CfxJsEngine::with_host_callback(context, || node.get_content());
            runtime.record_access(node);
            Ok(if value.is_empty() {
                JsValue::null()
            } else {
                string(&value)
            })
        }
        "CJX_Object::ScriptSomDataNode" => boxed(runtime, node.get_bind_data(), context),
        "CJX_Subform::locale" => Ok(string(&node.get_locale_name().unwrap_or_default())),
        "CJX_Subform::instanceManager" => {
            boxed(runtime, node.get_instance_mgr_of_subform(), context)
        }
        "CJX_Occur::min" => Ok(node.occur_get_min().into()),
        "CJX_Occur::max" => Ok(node.occur_get_max().into()),
        "CJX_Script::stateless" => Ok(JsString::from("0").into()),
        "CJX_Xfa::thisValue" => runtime.current_this_object().map_or_else(
            || Ok(JsValue::null()),
            |object| {
                runtime
                    .get_or_create_object_binding(&object, context)
                    .map(Into::into)
            },
        ),
        _ if name.starts_with("CJX_Tree::") || name.starts_with("CJX_Node::") => {
            // JSE_PROP checks CJX declaring-class DynamicTypeIs. Primitive
            // CJX_Object nodes do not inherit Tree/Node accessors even though
            // XML metadata can discover those names. Reentering their absent
            // accessor through the binding would recurse without a base case.
            if !super::cjx_bindings::is_node_class(node.get_element_type()) {
                return Ok(JsValue::undefined());
            }
            let property = name.rsplit("::").next().expect("callback name");
            runtime
                .get_or_create_binding(node, context)?
                .get(JsString::from(property), context)
        }
        _ => Err(unsupported(name)),
    }
}
fn read_default(
    runtime: &CfxJseEngine,
    node: &CxfaNode,
    context: &mut Context,
) -> JsResult<JsValue> {
    let element = node.get_element_type();
    if runtime.widget_script_host().is_some()
        && matches!(element, XfaElement::Field | XfaElement::ExclGroup)
        && !node.is_widget_ready()
    {
        return Ok(JsValue::undefined());
    }
    let content = CfxJsEngine::with_host_callback(context, || node.get_content());
    runtime.record_access(node);
    if element == XfaElement::Boolean {
        return Ok(content == [49]).map(Into::into);
    }
    if content.is_empty() {
        let keep_empty = matches!(element, XfaElement::Text | XfaElement::SubmitUrl)
            || (element == XfaElement::ExclGroup
                && runtime.document().get_cur_version_mode() < 300);
        return Ok(if keep_empty {
            string(&content)
        } else {
            JsValue::null()
        });
    }
    let value_node = if element == XfaElement::Field {
        node.get_form_value_if_exists()
            .and_then(|value| value.first_child())
    } else {
        Some(node.clone())
    };
    match value_node.as_ref().map(CxfaNode::get_element_type) {
        Some(XfaElement::Integer) => Ok(wtoi(&content).into()),
        Some(XfaElement::Boolean) => Ok((wtoi(&content) != 0).into()),
        Some(XfaElement::Float | XfaElement::Decimal) => {
            let unlimited = element == XfaElement::Field
                && value_node.as_ref().is_some_and(|value| {
                    value.get_element_type() == XfaElement::Decimal
                        && value.get_integer(XfaAttribute::FracDigits) == -1
                })
                && CfxJsEngine::with_host_callback(context, || node.get_ui_child_node())
                    .is_some_and(|ui| ui.get_element_type() == XfaElement::NumericEdit);
            if unlimited {
                Ok(string(&content))
            } else {
                Ok(f64::from(CfgasDecimal::from_wide_string(&content).to_float()).into())
            }
        }
        _ => Ok(string(&content)),
    }
}
pub(crate) fn write_attribute(
    runtime: &CfxJseEngine,
    node: &CxfaNode,
    descriptor: ScriptAttributeDescriptor,
    value: &JsValue,
    context: &mut Context,
) -> JsResult<()> {
    let name = descriptor.callback_name;
    if super::cjx_model_noops::is_noop_property(name) {
        return Ok(());
    }
    if name == "CJX_Form::checksumS" {
        return super::cjx_form::write_checksum(node, value, context);
    }
    if default_callback(name) {
        return write_default(runtime, node, value, context);
    }
    if super::cjx_presentation::handles(name) {
        return super::cjx_presentation::write_property(runtime, node, name, value, context);
    }
    if name.starts_with("CJX_Field::") {
        return super::cjx_field::write_property(runtime, node, name, value, context);
    }
    if name.starts_with("CJX_Subform::") {
        return super::cjx_subform::write_property(runtime, node, name, value, context);
    }
    if name.starts_with("CJX_ExclGroup::") {
        return super::cjx_exclgroup::write_property(runtime, node, name, value, context);
    }
    if name.starts_with("CJX_InstanceManager::") {
        return super::cjx_instancemanager::write_attribute(runtime, node, name, value, context);
    }
    match name {
        "CJX_Model::context" | "CJX_Model::aliasNode" | "CJX_Object::ScriptSubmitFormatMode" => {
            Ok(())
        }
        "CJX_Packet::content" => super::cjx_packet::write_content(node, value, context),
        "CJX_Object::ScriptSomInstanceIndex" => {
            let to = value.to_i32(context)?;
            let from = node.get_instance_index();
            let mut previous = node.prev_sibling();
            while let Some(manager) = previous {
                if manager.get_element_type() == XfaElement::InstanceManager {
                    super::cjx_instancemanager::move_instance(
                        runtime, &manager, to, from, context,
                    )?;
                    return super::cjx_instancemanager::notify_index_changes(
                        runtime, &manager, to, from, context,
                    );
                }
                previous = manager.prev_sibling();
            }
            Ok(())
        }
        "CJX_Object::ScriptAttributeString" => {
            let text = wide_from_utf16(&value.to_string(context)?.to_vec());
            CfxJsEngine::with_host_callback(context, || {
                node.set_attribute_by_enum(descriptor.attribute, &text, true);
            });
            // Desc.use has the additional source clone/replace behavior.
            if descriptor.attribute == XfaAttribute::Use
                && node.get_element_type() == XfaElement::Desc
            {
                apply_desc_use(runtime, node, &text, context)?;
            }
            Ok(())
        }
        "CJX_Object::ScriptAttributeBool" => {
            let value = value.to_boolean();
            CfxJsEngine::with_host_callback(context, || {
                node.set_boolean(descriptor.attribute, value, true)
            });
            Ok(())
        }
        "CJX_Object::ScriptAttributeInteger" => {
            let value = value.to_i32(context)?;
            CfxJsEngine::with_host_callback(context, || {
                node.set_integer(descriptor.attribute, value, true)
            });
            Ok(())
        }
        "CJX_Subform::locale" => {
            let text = wide_from_utf16(&value.to_string(context)?.to_vec());
            CfxJsEngine::with_host_callback(context, || {
                node.set_cdata_and_notify(XfaAttribute::Locale, &text, true)
            });
            Ok(())
        }
        "CJX_Occur::min" | "CJX_Occur::max" => {
            let number = value.to_i32(context)?;
            CfxJsEngine::with_host_callback(context, || {
                if name == "CJX_Occur::min" {
                    node.occur_set_min(number)
                } else {
                    node.occur_set_max(number)
                }
            });
            Ok(())
        }
        "CJX_Object::className"
        | "CJX_Object::ScriptSomDefaultValue_Read"
        | "CJX_Object::ScriptSomDataNode"
        | "CJX_Subform::instanceManager"
        | "CJX_Script::stateless" => Err(readonly()),
        // The native root this-value setter intentionally ignores assignments.
        "CJX_Xfa::thisValue" => Ok(()),
        _ if name.starts_with("CJX_Tree::") || name.starts_with("CJX_Node::") => {
            if !super::cjx_bindings::is_node_class(node.get_element_type()) {
                return Ok(());
            }
            let property = name.rsplit("::").next().expect("callback name");
            runtime.get_or_create_binding(node, context)?.set(
                JsString::from(property),
                value.clone(),
                true,
                context,
            )?;
            Ok(())
        }
        _ => Err(unsupported(name)),
    }
}
fn write_default(
    runtime: &CfxJseEngine,
    node: &CxfaNode,
    value: &JsValue,
    context: &mut Context,
) -> JsResult<()> {
    let element = node.get_element_type();
    // Actual FF callbacks gate before conversion; standalone model bindings
    // retain their store-based value API without inventing widget readiness.
    if runtime.widget_script_host().is_some()
        && matches!(element, XfaElement::Field | XfaElement::ExclGroup)
        && !node.is_widget_ready()
    {
        return Ok(());
    }
    if element == XfaElement::Field {
        node.set_pre_null(node.is_null());
        node.set_is_null(value.is_null());
    }
    if element == XfaElement::Draw {
        if !value.is_string() {
            return Ok(());
        }
        let widget = CfxJsEngine::with_host_callback(context, || node.get_ff_widget_type());
        if widget != XfaFfWidgetType::Text {
            return Ok(());
        }
    }
    let mut text = if value.is_null_or_undefined() {
        Vec::new()
    } else {
        wide_from_utf16(&value.to_string(context)?.to_vec())
    };
    if element == XfaElement::Boolean {
        let number = string_to_int(&crate::fxcrt::fx_string::utf8_encode(&text));
        text = vec![if number == 0 { 48 } else { 49 }];
    }
    let success = CfxJsEngine::with_host_callback(context, || {
        if element == XfaElement::ExclGroup {
            return node.set_selected_member_by_value(&text, true, true);
        }
        if element == XfaElement::Field
            && node
                .get_ui_child_node()
                .is_some_and(|ui| ui.get_element_type() == XfaElement::NumericEdit)
        {
            text = node.numeric_limit(&text);
        }
        if node.get_packet_type() == XfaPacketType::Datasets {
            // Dataset default-value writes select a formatting container from
            // their bound forms. GetContainerNode itself accepts only Form
            // nodes, so SyncValue alone cannot perform this source branch.
            let container = node.get_bind_items_copy().into_iter().find_map(|form| {
                if form.has_removed_children() {
                    return None;
                }
                let container = form.get_container_node()?;
                (!container
                    .get_picture_content(
                        crate::xfa::fxfa::parser::cxfa_localevalue::XfaValuePicture::DataBind,
                    )
                    .is_empty())
                .then_some(container)
            });
            let formatted = container.map(|container| container.get_format_data_value(&text));
            return node.set_content_with_script(
                &text,
                formatted.as_deref().unwrap_or(&text),
                true,
                true,
                true,
            );
        }
        node.sync_value_with_script(&text, true, true)
    });
    if !success {
        return Err(JsNativeError::error()
            .with_message("XFA value mutation did not complete")
            .into());
    }
    Ok(())
}
fn apply_desc_use(
    runtime: &CfxJseEngine,
    node: &CxfaNode,
    value: &[u32],
    context: &mut Context,
) -> JsResult<()> {
    use super::{cfxjse_engine::resolve_error, cfxjse_resolveprocessor::XfaResolveFlags};
    use crate::xfa::fxfa::fxfa_basic::XfaHashCode;
    let root = runtime
        .document()
        .get_xfa_node(XfaHashCode::Template as u32)
        .and_then(|root| root.get_first_child_by_class(XfaElement::Subform))
        .and_then(|subform| subform.get_first_child_by_class(XfaElement::Proto));
    let Some(root) = root else {
        return Ok(());
    };
    let prototype = if let Some(id) = value.strip_prefix(&[35]) {
        runtime.document().get_node_by_id(Some(&root), id)
    } else if !value.is_empty() {
        CfxJsEngine::with_host_callback(context, || {
            runtime.resolve_objects(
                Some(&root),
                value,
                XfaResolveFlags::CHILDREN
                    | XfaResolveFlags::ATTRIBUTES
                    | XfaResolveFlags::PROPERTIES
                    | XfaResolveFlags::PARENT
                    | XfaResolveFlags::SIBLINGS,
            )
        })
        .map_err(resolve_error)?
        .and_then(|r| r.objects.into_iter().next())
    } else {
        None
    };
    let Some(prototype) = prototype.filter(|p| p.get_packet_type() == XfaPacketType::Template)
    else {
        return Ok(());
    };
    let success = CfxJsEngine::with_host_callback(context, || {
        while let Some(child) = node.first_child() {
            if !node.remove_child_and_notify(&child, true) {
                return false;
            }
        }
        let Some(form) = prototype.clone_template_to_form(true) else {
            return false;
        };
        while let Some(child) = form.first_child() {
            if !form.remove_child_and_notify(&child, true)
                || !node.insert_child_and_notify(-1, &child)
            {
                return false;
            }
        }
        true
    });
    if !success {
        return Err(JsNativeError::error()
            .with_message("Cannot replace description prototype")
            .into());
    }
    Ok(())
}
