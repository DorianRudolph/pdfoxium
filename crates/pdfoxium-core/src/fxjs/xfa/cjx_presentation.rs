// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! CJX_Object presentation, mandatory and validation-message callbacks.
use super::cfxjse_engine::CfxJseEngine;
use crate::{
    fxcrt::fx_string::utf16_encode,
    fxjs::{cfxjs_engine::CfxJsEngine, cjs_event_context::wide_from_utf16},
    xfa::fxfa::{
        fxfa_basic::{XfaAttribute, XfaElement, XfaUnit},
        parser::{
            cxfa_measurement::CxfaMeasurement, cxfa_node::CxfaNode,
            cxfa_presentation::color_to_string, xfa_basic_data::attribute_value_to_name,
        },
    },
};
use boa_engine::{Context, JsNativeError, JsResult, JsString, JsValue};
fn string(value: &[u32]) -> JsValue {
    JsString::from(utf16_encode(value).as_slice()).into()
}
fn color(value: u32) -> JsValue {
    string(&color_to_string(value))
}
fn error() -> boa_engine::JsError {
    JsNativeError::typ()
        .with_message("Invalid XFA presentation property")
        .into()
}
fn is(callback: &str, method: &str) -> bool {
    callback.strip_prefix("CJX_Object::") == Some(method)
}
pub(crate) fn handles(callback: &str) -> bool {
    matches!(
        callback,
        "CJX_Object::ScriptSomFontColor"
            | "CJX_Object::ScriptSomFillColor"
            | "CJX_Object::ScriptSomBorderColor"
            | "CJX_Object::ScriptSomBorderWidth"
            | "CJX_Object::ScriptSomMandatory"
            | "CJX_Object::ScriptSomValidationMessage"
            | "CJX_Object::ScriptSomMandatoryMessage"
    )
}
fn message_kind(callback: &str) -> Option<Vec<u32>> {
    let kind = if is(callback, "ScriptSomValidationMessage") {
        "scriptTest"
    } else if is(callback, "ScriptSomMandatoryMessage") {
        "nullTest"
    } else {
        return None;
    };
    Some(kind.chars().map(u32::from).collect())
}
fn presentation_node(node: &CxfaNode, callback: &str) -> Option<CxfaNode> {
    node.get_or_create_property(
        0,
        if is(callback, "ScriptSomFontColor") {
            XfaElement::Font
        } else {
            XfaElement::Border
        },
    )
}
pub(crate) fn read_property(
    node: &CxfaNode,
    callback: &str,
    context: &mut Context,
) -> JsResult<JsValue> {
    CfxJsEngine::with_host_callback(context, || {
        if let Some(kind) = message_kind(callback) {
            let validate = node
                .get_or_create_property(0, XfaElement::Validate)
                .ok_or_else(error)?;
            return Ok(string(&validate.validate_get_message_text(&kind)));
        }
        if is(callback, "ScriptSomMandatory") {
            return Ok(node
                .get_or_create_property(0, XfaElement::Validate)
                .map_or_else(JsValue::undefined, |validate| {
                    JsString::from(attribute_value_to_name(validate.validate_get_null_test()))
                        .into()
                }));
        }
        let Some(model) = presentation_node(node, callback) else {
            return Ok(JsValue::undefined());
        };
        Ok(if is(callback, "ScriptSomFontColor") {
            color(model.font_get_color())
        } else if is(callback, "ScriptSomFillColor") {
            model
                .get_or_create_property(0, XfaElement::Fill)
                .map_or_else(JsValue::undefined, |fill| color(fill.fill_get_color(false)))
        } else if is(callback, "ScriptSomBorderColor") {
            color(
                model
                    .box_get_edge(0)
                    .map_or(0xff000000, |edge| edge.stroke_get_color()),
            )
        } else if is(callback, "ScriptSomBorderWidth") {
            let width = model
                .box_get_edge(0)
                .map_or(CxfaMeasurement::new(0.5, XfaUnit::Pt), |edge| {
                    edge.get_measure(XfaAttribute::Thickness)
                });
            string(&width.to_string())
        } else {
            return Err(error());
        })
    })
}
/// Native StrToRGB ignores nondigits. Wrapping byte accumulation preserves
/// ordinary RGB inputs while avoiding signed overflow on malformed long text.
fn parse_color(value: &[u32]) -> u32 {
    let mut components = [0u8; 3];
    let mut index = 0;
    for &c in value {
        if c == 44 {
            index += 1;
        }
        if index == 3 {
            break;
        }
        if let Some(digit) = c.checked_sub(48).filter(|d| *d <= 9) {
            components[index] = components[index].wrapping_mul(10).wrapping_add(digit as u8);
        }
    }
    0xff000000
        | (u32::from(components[0]) << 16)
        | (u32::from(components[1]) << 8)
        | u32::from(components[2])
}
pub(crate) fn write_property(
    runtime: &CfxJseEngine,
    node: &CxfaNode,
    callback: &str,
    value: &JsValue,
    context: &mut Context,
) -> JsResult<()> {
    if let Some(kind) = message_kind(callback) {
        let (validate, existed) = CfxJsEngine::with_host_callback(context, || {
            let existing = node.get_property(0, XfaElement::Validate).0;
            let existed = existing.is_some();
            (
                existing.or_else(|| node.get_or_create_property(0, XfaElement::Validate)),
                existed,
            )
        });
        if let Some(validate) = validate {
            let text = wide_from_utf16(&value.to_string(context)?.to_vec());
            if !CfxJsEngine::with_host_callback(context, || {
                validate.validate_set_message_text(&kind, &text)
            }) {
                return Err(error());
            }
        }
        if existed {
            if let Some(host) = runtime.widget_script_host() {
                CfxJsEngine::with_host_callback(context, || host.add_calc_validate(node))?;
            }
        }
        return Ok(());
    }
    let model = CfxJsEngine::with_host_callback(context, || {
        if is(callback, "ScriptSomMandatory") {
            node.get_or_create_property(0, XfaElement::Validate)
        } else {
            presentation_node(node, callback)
        }
    });
    let Some(model) = model else {
        return Ok(());
    };
    let text = wide_from_utf16(&value.to_string(context)?.to_vec());
    CfxJsEngine::with_host_callback(context, || {
        if is(callback, "ScriptSomMandatory") {
            model.validate_set_null_test(&text);
        } else if is(callback, "ScriptSomFontColor") {
            model.font_set_color(parse_color(&text));
        } else if is(callback, "ScriptSomFillColor") {
            if let Some(fill) = model.get_or_create_property(0, XfaElement::Fill) {
                fill.fill_set_color(parse_color(&text));
            }
        } else if is(callback, "ScriptSomBorderColor") || is(callback, "ScriptSomBorderWidth") {
            let count = model.count_children(XfaElement::Edge, false);
            let rgb = parse_color(&text);
            let width = CxfaMeasurement::from_string(&text);
            for index in 0..count {
                if let Some(edge) = model.box_get_edge(i32::try_from(index).map_err(|_| error())?) {
                    if is(callback, "ScriptSomBorderColor") {
                        edge.stroke_set_color(rgb);
                    } else {
                        edge.set_measure(XfaAttribute::Thickness, width, false);
                    }
                }
            }
        } else {
            return Err(error());
        }
        Ok(())
    })
}
