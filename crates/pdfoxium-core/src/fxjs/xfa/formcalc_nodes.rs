// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Dot/dotdot SOM expansions and default node values from
//! `cfxjse_formcalc_context.cpp`, using the existing document engine and Boa.
use super::{
    cfxjse_engine::{resolve_error, CfxJseEngine},
    cfxjse_resolveprocessor::{ObjectResolveResult, ResolveResultType, XfaResolveFlags},
    formcalc_iso::generate_som_expression,
    formcalc_values::{
        argument_mismatch, as_array, compiler_error, exception, property_name, value_to_integer,
    },
};
use crate::{
    fxcrt::fx_string::{utf8_decode, utf8_encode},
    fxjs::{cfxjs_engine::CfxJsEngine, cjs_event_context::wide_from_utf16},
    xfa::fxfa::{
        fxfa_basic::{XfaAttribute, XfaScriptType},
        parser::xfa_basic_data::ScriptAttributeDescriptor,
    },
};
use boa_engine::{
    native_function::NativeFunctionPointer, object::builtins::JsArray, Context, JsObject, JsResult,
    JsString, JsValue,
};
use std::rc::Rc;

pub(super) const METHODS: &[(&str, NativeFunctionPointer)] =
    &[("dot_acc", dot_accessor), ("dotdot_acc", dotdot_accessor)];
const DEFAULT_VALUE: ScriptAttributeDescriptor = ScriptAttributeDescriptor {
    attribute: XfaAttribute::Unknown,
    script_type: XfaScriptType::Basic,
    callback_name: "CJX_Object::ScriptSomDefaultValue",
};
pub(super) fn get_object_default_value(
    object: &JsObject,
    context: &mut Context,
) -> JsResult<JsValue> {
    let Some(runtime) = CfxJseEngine::from_context(context) else {
        return Ok(JsValue::null());
    };
    let Some(node) = runtime.node_from_value(&object.clone().into()) else {
        return Ok(JsValue::null());
    };
    runtime.read_script_attribute(&node, DEFAULT_VALUE, context)
}
pub(super) fn set_object_default_value(
    object: &JsObject,
    value: &JsValue,
    context: &mut Context,
) -> JsResult<bool> {
    let Some(runtime) = CfxJseEngine::from_context(context) else {
        return Ok(false);
    };
    let Some(node) = runtime.node_from_value(&object.clone().into()) else {
        return Ok(false);
    };
    runtime.write_script_attribute(&node, DEFAULT_VALUE, value, context)?;
    Ok(true)
}
fn byte_string(value: &JsValue, context: &mut Context) -> JsResult<Vec<u8>> {
    // Accessor names use JavaScript ToString, unlike ValueToUTF8String's
    // special FormCalc Boolean/null conversion. Preserve source UTF-8 spelling.
    Ok(utf8_encode(&wide_from_utf16(
        &property_name(value, context)?.to_vec(),
    )))
}
fn missing_property(name: &[u8], expression: &[u8]) -> boa_engine::JsError {
    exception(&format!(
        "An attempt was made to reference property '{}' of a non-object in SOM expression {}.",
        String::from_utf8_lossy(name),
        String::from_utf8_lossy(expression)
    ))
}
fn get_object_for_name(
    runtime: &Rc<CfxJseEngine>,
    name: &[u8],
    context: &mut Context,
) -> JsResult<Option<JsValue>> {
    let reference = runtime.current_this_object();
    let expression = utf8_decode(name);
    let result = CfxJsEngine::with_host_callback(context, || {
        runtime.resolve_object_values_from(
            reference.as_ref(),
            &expression,
            XfaResolveFlags::CHILDREN
                | XfaResolveFlags::PROPERTIES
                | XfaResolveFlags::SIBLINGS
                | XfaResolveFlags::PARENT,
        )
    })
    .map_err(resolve_error)?;
    let Some(result) = result.filter(|result| result.result_type == ResolveResultType::Nodes)
    else {
        return Ok(None);
    };
    let Some(node) = result.objects.first() else {
        return Ok(None);
    };
    Ok(Some(
        runtime.get_or_create_object_binding(node, context)?.into(),
    ))
}
fn resolve_objects(
    runtime: &Rc<CfxJseEngine>,
    reference: &JsValue,
    expression: &[u8],
    dot: bool,
    no_name: bool,
    context: &mut Context,
) -> JsResult<Option<ObjectResolveResult>> {
    let mut expression = utf8_decode(expression);
    let (node, flags) = if dot {
        if reference.is_null() {
            (
                runtime.current_this_object(),
                XfaResolveFlags::SIBLINGS | XfaResolveFlags::PARENT,
            )
        } else {
            let Some(node) = runtime.resolved_object_from_value(reference) else {
                return Ok(None);
            };
            if no_name {
                let mut name = node
                    .as_node()
                    .and_then(|node| node.try_attribute(XfaAttribute::Name, false))
                    .unwrap_or_default();
                if name.is_empty() {
                    name.push(35);
                    name.extend(
                        crate::xfa::fxfa::parser::xfa_basic_data::element_to_name(node.element())
                            .bytes()
                            .map(u32::from),
                    );
                } else {
                    // This name is one SOM component. Escape literal dots just
                    // as CXFA_Node::GetNameExpression does for indexed siblings.
                    let mut escaped = Vec::with_capacity(name.len());
                    for ch in name {
                        if ch == 46 {
                            escaped.push(92);
                        }
                        escaped.push(ch);
                    }
                    name = escaped;
                }
                name.extend(expression);
                expression = name;
                (Some(node), XfaResolveFlags::SIBLINGS)
            } else {
                let flags = if expression == [42] {
                    XfaResolveFlags::CHILDREN
                } else {
                    XfaResolveFlags::CHILDREN
                        | XfaResolveFlags::ATTRIBUTES
                        | XfaResolveFlags::PROPERTIES
                };
                (Some(node), flags)
            }
        }
    } else {
        (
            runtime.resolved_object_from_value(reference),
            XfaResolveFlags::ANY_CHILD,
        )
    };
    CfxJsEngine::with_host_callback(context, || {
        runtime.resolve_object_values_from(node.as_ref(), &expression, flags)
    })
    .map_err(resolve_error)
}
fn parse_resolve_result(
    runtime: &CfxJseEngine,
    result: ObjectResolveResult,
    parent: &JsValue,
    context: &mut Context,
) -> JsResult<(Vec<JsValue>, bool)> {
    if result.result_type == ResolveResultType::Nodes {
        let mut values = Vec::with_capacity(result.objects.len());
        for node in result.objects {
            values.push(runtime.get_or_create_object_binding(&node, context)?.into());
        }
        return Ok((values, false));
    }
    let descriptor = result.script_attribute;
    if let Some(descriptor) =
        descriptor.filter(|descriptor| descriptor.script_type == XfaScriptType::Object)
    {
        let mut values = Vec::with_capacity(result.objects.len());
        for node in result.objects {
            values.push(runtime.read_resolved_attribute(&node, descriptor, context)?);
        }
        return Ok((values, false));
    }
    // Basic attributes are lazy references. Subsequent extraction/assignment
    // uses ordinary JS property access, retaining getters, setters and reentry.
    Ok((
        if parent.is_object() {
            vec![parent.clone()]
        } else {
            Vec::new()
        },
        true,
    ))
}
fn dot_accessor(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    dot_accessor_common(args, context, true)
}
fn dotdot_accessor(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    dot_accessor_common(args, context, false)
}
fn dot_accessor_common(args: &[JsValue], context: &mut Context, dot: bool) -> JsResult<JsValue> {
    if !(4..=5).contains(&args.len()) {
        return Err(compiler_error());
    }
    let star = args.len() == 4;
    let index = if star {
        0
    } else {
        value_to_integer(&args[4], context)?
    };
    let name = byte_string(&args[2], context)?;
    let no_name = dot && name.is_empty();
    let expression = generate_som_expression(&name, args[3].to_i32(context)?, index, star);
    let runtime = CfxJseEngine::from_context(context);
    if let Some(array) = as_array(&args[0]) {
        let length = array.length(context)?;
        if length < 3 {
            return Err(argument_mismatch());
        }
        let Some(runtime) = runtime else {
            return Err(missing_property(&name, &expression));
        };
        let mut values = Vec::new();
        let mut attribute = false;
        for i in 2..length {
            let reference = array.get(i, context)?;
            if let Some(result) =
                resolve_objects(&runtime, &reference, &expression, dot, no_name, context)?
            {
                let (mut result, is_attribute) =
                    parse_resolve_result(&runtime, result, &reference, context)?;
                attribute = is_attribute;
                values.append(&mut result);
            }
        }
        if values.is_empty() {
            return Err(missing_property(&name, &expression));
        }
        let mut expanded = Vec::with_capacity(values.len() + 2);
        expanded.push(1.into());
        expanded.push(if attribute {
            JsString::from(String::from_utf8_lossy(&name).as_ref()).into()
        } else {
            JsValue::null()
        });
        expanded.extend(values);
        return Ok(JsArray::from_iter(expanded, context).into());
    }
    let accessor_name = byte_string(&args[1], context)?;
    let Some(runtime) = runtime else {
        return Err(missing_property(&name, &expression));
    };
    let mut reference = args[0].clone();
    let result = if reference.is_object() || (reference.is_null() && accessor_name.is_empty()) {
        resolve_objects(&runtime, &reference, &expression, dot, no_name, context)?
    } else if !reference.is_object() && !accessor_name.is_empty() {
        if let Some(object) = get_object_for_name(&runtime, &accessor_name, context)? {
            reference = object;
            resolve_objects(&runtime, &reference, &expression, dot, no_name, context)?
        } else {
            None
        }
    } else {
        None
    };
    let Some(result) = result else {
        return Err(missing_property(&name, &expression));
    };
    let (values, attribute) = parse_resolve_result(&runtime, result, &reference, context)?;
    let mut expanded = Vec::with_capacity(values.len() + 2);
    expanded.push(1.into());
    expanded.push(if attribute {
        JsString::from(String::from_utf8_lossy(&name).as_ref()).into()
    } else {
        JsValue::null()
    });
    expanded.extend(values);
    Ok(JsArray::from_iter(expanded, context).into())
}

#[cfg(test)]
#[path = "formcalc_nodes_tests.rs"]
mod tests;
