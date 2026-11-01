// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Choice-item methods and picture/message properties from `cjx_field.{h,cpp}`.
//! Default/raw values share CJX_Object's existing typed value implementation.
use super::{
    cfxjse_engine::{node_data, CfxJseEngine},
    cjx_node::exec_single_event_by_name,
    cjx_subform::{define, exec_deep, parameter_error, readonly, string, to_wide},
};
use crate::{
    fxjs::cfxjs_engine::CfxJsEngine,
    xfa::fxfa::{
        cxfa_eventparam::XfaEventType,
        fxfa::XfaEventError,
        fxfa_basic::XfaElement,
        parser::{cxfa_localevalue::XfaValuePicture, cxfa_node::CxfaNode},
    },
};
use boa_engine::{Context, JsObject, JsResult, JsValue};

pub(crate) fn define_field_methods(object: &JsObject, context: &mut Context) -> JsResult<()> {
    define(
        object,
        context,
        &[
            ("addItem", 1, add_item),
            ("boundItem", 1, bound_item),
            ("clearItems", 0, clear_items),
            ("deleteItem", 1, delete_item),
            ("execCalculate", 0, exec_calculate),
            ("execEvent", 1, exec_event),
            ("execInitialize", 0, exec_initialize),
            ("execValidate", 0, exec_validate),
            ("getDisplayItem", 1, get_display_item),
            ("getItemState", 1, get_item_state),
            ("getSaveItem", 1, get_save_item),
            ("setItemState", 2, set_item_state),
        ],
    )
}
fn exec_calculate(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    exec_deep(this, args, context, XfaEventType::Calculate, false)
}
fn exec_initialize(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    exec_deep(this, args, context, XfaEventType::Initialize, false)
}
fn exec_validate(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    exec_deep(this, args, context, XfaEventType::Validate, false)
}
fn exec_event(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    if args.len() != 1 {
        return Err(parameter_error("Field", "execEvent"));
    }
    let name = to_wide(&args[0], context)?;
    let runtime = binding.runtime()?;
    let result =
        exec_single_event_by_name(&runtime, &binding.node, &name, XfaElement::Field, context)?;
    Ok(if name == [118, 97, 108, 105, 100, 97, 116, 101] {
        (result != XfaEventError::kError).into()
    } else {
        JsValue::undefined()
    })
}
fn clear_items(this: &JsValue, _args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    if binding.node.is_widget_ready() {
        CfxJsEngine::with_host_callback(context, || {
            binding.node.delete_item_with_script(-1, true, true)
        });
    }
    Ok(JsValue::undefined())
}
fn add_item(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    if args.len() != 1 && args.len() != 2 {
        return Err(parameter_error("Field", "addItem"));
    }
    if !binding.node.is_widget_ready() {
        return Ok(JsValue::undefined());
    }
    let label = to_wide(&args[0], context)?;
    let value = if args.len() == 2 {
        to_wide(&args[1], context)?
    } else {
        Vec::new()
    };
    CfxJsEngine::with_host_callback(context, || {
        binding
            .node
            .insert_choice_list_item_with_notify(&label, &value, true)
    });
    Ok(JsValue::undefined())
}
fn delete_item(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    if args.len() != 1 {
        return Err(parameter_error("Field", "deleteItem"));
    }
    if !binding.node.is_widget_ready() {
        return Ok(JsValue::undefined());
    }
    let index = args[0].to_i32(context)?;
    Ok(CfxJsEngine::with_host_callback(context, || {
        binding.node.delete_item_with_script(index, true, true)
    })
    .into())
}
fn get_item(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
    save: bool,
) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    if args.len() != 1 {
        return Err(parameter_error(
            "Field",
            if save {
                "getSaveItem"
            } else {
                "getDisplayItem"
            },
        ));
    }
    let index = args[0].to_i32(context)?;
    if index < 0 || !binding.node.is_widget_ready() {
        return Ok(JsValue::null());
    }
    let value =
        CfxJsEngine::with_host_callback(context, || binding.node.get_choice_list_item(index, save));
    Ok(value.map_or_else(JsValue::null, |value| string(&value)))
}
fn get_display_item(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    get_item(this, args, context, false)
}
fn get_save_item(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    get_item(this, args, context, true)
}
fn bound_item(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    if args.len() != 1 {
        return Err(parameter_error("Field", "boundItem"));
    }
    if !binding.node.is_widget_ready() {
        return Ok(JsValue::undefined());
    }
    let label = to_wide(&args[0], context)?;
    let value = CfxJsEngine::with_host_callback(context, || binding.node.get_item_value(&label));
    Ok(string(&value))
}
fn get_item_state(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    if args.len() != 1 {
        return Err(parameter_error("Field", "getItemState"));
    }
    if !binding.node.is_widget_ready() {
        return Ok(JsValue::undefined());
    }
    let index = args[0].to_i32(context)?;
    Ok(CfxJsEngine::with_host_callback(context, || binding.node.get_item_state(index)).into())
}
fn set_item_state(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    if args.len() != 2 {
        return Err(parameter_error("Field", "setItemState"));
    }
    if !binding.node.is_widget_ready() {
        return Ok(JsValue::undefined());
    }
    let index = args[0].to_i32(context)?;
    let selected = args[1].to_i32(context)? != 0;
    CfxJsEngine::with_host_callback(context, || {
        if selected || binding.node.get_item_state(index) {
            binding
                .node
                .set_item_state_with_script(index, selected, true, true);
        }
    });
    Ok(JsValue::undefined())
}
pub(crate) fn read_property(
    _runtime: &CfxJseEngine,
    node: &CxfaNode,
    callback: &str,
    context: &mut Context,
) -> JsResult<JsValue> {
    match callback {
        "CJX_Field::length" => Ok(JsValue::from(if node.is_widget_ready() {
            node.count_choice_list_items(true) as f64
        } else {
            0.0
        })),
        // This source property explicitly returns null; it does not walk parents.
        "CJX_Field::parentSubform" => Ok(JsValue::null()),
        "CJX_Field::formatMessage" => {
            let validate = CfxJsEngine::with_host_callback(context, || {
                node.get_or_create_property(0, XfaElement::Validate)
            });
            let validate = validate.ok_or_else(|| readonly("Field", "formatMessage"))?;
            let text = CfxJsEngine::with_host_callback(context, || {
                validate.validate_get_message_text(
                    &"formatTest".chars().map(u32::from).collect::<Vec<_>>(),
                )
            });
            Ok(string(&text))
        }
        "CJX_Field::selectedIndex" | "CJX_Field::editValue" | "CJX_Field::formattedValue" => {
            if !node.is_widget_ready() {
                return Ok(JsValue::undefined());
            }
            if callback == "CJX_Field::selectedIndex" {
                return Ok(
                    CfxJsEngine::with_host_callback(context, || node.get_selected_item(0)).into(),
                );
            }
            let picture = if callback == "CJX_Field::editValue" {
                XfaValuePicture::Edit
            } else {
                XfaValuePicture::Display
            };
            let value = CfxJsEngine::with_host_callback(context, || node.get_value(picture));
            Ok(string(&value))
        }
        _ => Err(readonly("Field", callback)),
    }
}
pub(crate) fn write_property(
    runtime: &CfxJseEngine,
    node: &CxfaNode,
    callback: &str,
    value: &JsValue,
    context: &mut Context,
) -> JsResult<()> {
    match callback {
        "CJX_Field::length" | "CJX_Field::parentSubform" => Err(readonly("Field", callback)),
        "CJX_Field::formatMessage" => {
            let is_new = node.get_property(0, XfaElement::Validate).0.is_none();
            let validate = CfxJsEngine::with_host_callback(context, || {
                node.get_or_create_property(0, XfaElement::Validate)
            });
            let Some(validate) = validate else {
                return Ok(());
            };
            let text = to_wide(value, context)?;
            CfxJsEngine::with_host_callback(context, || {
                validate.validate_set_message_text(
                    &"formatTest".chars().map(u32::from).collect::<Vec<_>>(),
                    &text,
                )
            });
            if !is_new {
                if let Some(host) = runtime.widget_script_host() {
                    CfxJsEngine::with_host_callback(context, || host.add_calc_validate(node))?;
                }
            }
            Ok(())
        }
        "CJX_Field::selectedIndex" => {
            if !node.is_widget_ready() {
                return Ok(());
            }
            let index = value.to_i32(context)?;
            CfxJsEngine::with_host_callback(context, || {
                if index == -1 {
                    node.clear_all_selections()
                } else {
                    node.set_item_state_with_script(index, true, true, true)
                }
            });
            Ok(())
        }
        "CJX_Field::editValue" | "CJX_Field::formattedValue" => {
            if !node.is_widget_ready() {
                return Ok(());
            }
            let text = to_wide(value, context)?;
            let picture = if callback == "CJX_Field::editValue" {
                XfaValuePicture::Edit
            } else {
                XfaValuePicture::Display
            };
            CfxJsEngine::with_host_callback(context, || node.set_value(picture, &text));
            Ok(())
        }
        _ => Err(readonly("Field", callback)),
    }
}

#[cfg(test)]
#[path = "cjx_field_tests.rs"]
mod tests;
