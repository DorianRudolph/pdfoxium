// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Shared value and expansion helpers from `cfxjse_formcalc_context.cpp`.
//! Ordinary JS objects have no XFA node default property: reading yields null,
//! and writing fails. Bound XFA nodes use the actual document CJX default value.
//! Named attribute expansions use actual JS properties.
use crate::fxcrt::fx_string::{string_to_double, string_to_float, string_to_int};
use boa_engine::{
    object::builtins::JsArray, Context, JsError, JsNativeError, JsObject, JsResult, JsString,
    JsValue,
};
use std::collections::HashSet;

pub(super) fn exception(message: &str) -> JsError {
    JsNativeError::error()
        .with_message(message.to_owned())
        .into()
}
pub(super) fn compiler_error() -> JsError {
    exception("Compiler error.")
}
pub(super) fn argument_mismatch() -> JsError {
    exception("Argument mismatch in property or function argument.")
}
pub(super) fn param_count(method: &str) -> JsError {
    exception(&format!(
        "Incorrect number of parameters calling method '{method}'."
    ))
}
pub(super) use super::formcalc_nodes::{get_object_default_value, set_object_default_value};
pub(super) fn as_array(value: &JsValue) -> Option<JsArray> {
    value
        .as_object()
        .filter(JsObject::is_array)
        .and_then(|object| JsArray::from_object(object).ok())
}
/// Source GetExtractedValue (one expansion step, not recursive conversion).
pub(super) fn get_extracted_value(value: &JsValue, context: &mut Context) -> JsResult<JsValue> {
    if let Some(array) = as_array(value) {
        if array.length(context)? < 3 {
            return Ok(JsValue::undefined());
        }
        let property = array.get(1u32, context)?;
        let value = array.get(2u32, context)?;
        let Some(object) = value.as_object() else {
            return Ok(JsValue::undefined());
        };
        if property.is_null() {
            return get_object_default_value(&object, context);
        }
        let name = property_name(&property, context)?;
        return object.get(name, context);
    }
    if let Some(object) = value.as_object() {
        return get_object_default_value(&object, context);
    }
    Ok(value.clone())
}
pub(super) fn get_simple_value(
    args: &[JsValue],
    index: usize,
    context: &mut Context,
) -> JsResult<JsValue> {
    get_extracted_value(args.get(index).unwrap_or(&JsValue::undefined()), context)
}
pub(super) fn value_is_null(value: &JsValue, context: &mut Context) -> JsResult<bool> {
    Ok(get_extracted_value(value, context)?.is_null())
}
fn scalar_value(value: &JsValue, context: &mut Context) -> JsResult<JsValue> {
    let mut current = value.clone();
    let mut seen = HashSet::new();
    loop {
        if let Some(object) = current.as_object() {
            if !seen.insert(object) {
                return Err(exception("Cyclic FormCalc value."));
            }
        }
        current = get_extracted_value(&current, context)?;
        if !current.is_object() {
            return Ok(current);
        }
    }
}
pub(super) fn value_to_integer(value: &JsValue, context: &mut Context) -> JsResult<i32> {
    let value = scalar_value(value, context)?;
    if value.is_string() {
        return Ok(string_to_int(&value_to_utf8_string(&value, context)?));
    }
    value.to_i32(context)
}
pub(super) fn value_to_double(value: &JsValue, context: &mut Context) -> JsResult<f64> {
    let value = scalar_value(value, context)?;
    if value.is_undefined() {
        return Ok(0.0);
    }
    if value.is_string() {
        return Ok(string_to_double(&value_to_utf8_string(&value, context)?));
    }
    value.to_number(context)
}
pub(super) fn value_to_float(value: &JsValue, context: &mut Context) -> JsResult<f32> {
    let value = scalar_value(value, context)?;
    if value.is_undefined() {
        return Ok(0.0);
    }
    if value.is_string() {
        return Ok(string_to_float(&value_to_utf8_string(&value, context)?));
    }
    Ok(value.to_number(context)? as f32)
}
pub(super) fn extract_double(value: &JsValue, context: &mut Context) -> JsResult<Option<f64>> {
    let Some(array) = as_array(value) else {
        return value_to_double(value, context).map(Some);
    };
    if array.length(context)? < 3 {
        return Ok(None);
    }
    let property = array.get(1u32, context)?;
    let value = array.get(2u32, context)?;
    if property.is_null() || !value.is_object() {
        return value_to_double(&value, context).map(Some);
    }
    let name = property_name(&property, context)?;
    let value = value
        .as_object()
        .expect("checked object")
        .get(name, context)?;
    value_to_double(&value, context).map(Some)
}
/// This helper deliberately does not extract array/default values. Its source
/// callers decide whether GetSimpleValue is required first. V8's UTF-8 boundary
/// replaces isolated UTF-16 surrogates; embedded NUL bytes remain meaningful.
pub(super) fn value_to_utf8_string(value: &JsValue, context: &mut Context) -> JsResult<Vec<u8>> {
    if value.is_null() || value.is_undefined() {
        return Ok(Vec::new());
    }
    if let Some(value) = value.as_boolean() {
        return Ok(if value { b"1" } else { b"0" }.to_vec());
    }
    let text = value.to_string(context)?;
    let mut bytes = Vec::new();
    for character in char::decode_utf16(text.iter()) {
        let character = character.unwrap_or(char::REPLACEMENT_CHARACTER);
        let mut encoded = [0; 4];
        bytes.extend_from_slice(character.encode_utf8(&mut encoded).as_bytes());
    }
    Ok(bytes)
}
pub(super) fn property_name(value: &JsValue, context: &mut Context) -> JsResult<JsString> {
    // ReentrantToByteStringHelper uses ordinary JS ToString here, including
    // null/undefined and booleans (unlike ValueToUTF8String's special cases).
    let text = value.to_string(context)?;
    let replaced: String = char::decode_utf16(text.iter())
        .map(|character| character.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect();
    Ok(JsString::from(replaced.as_str()))
}
pub(super) fn simple_value_compare(
    first: &JsValue,
    second: &JsValue,
    context: &mut Context,
) -> JsResult<bool> {
    if first.is_string() {
        return Ok(value_to_utf8_string(first, context)? == value_to_utf8_string(second, context)?);
    }
    if first.is_number() {
        return Ok(value_to_float(first, context)? == value_to_float(second, context)?);
    }
    if first.is_boolean() {
        return Ok(first.to_boolean() == second.to_boolean());
    }
    Ok(first.is_null() && second.is_null())
}
pub(super) fn unfold_args(args: &[JsValue], context: &mut Context) -> JsResult<Vec<JsValue>> {
    let mut results = Vec::new();
    for arg in args.iter().skip(1) {
        if let Some(array) = as_array(arg) {
            let length = array.length(context)?;
            if length < 3 {
                continue;
            }
            let property = array.get(1u32, context)?;
            for index in 2..length {
                let value = array.get(index, context)?;
                let extracted = if let Some(object) = value.as_object() {
                    if property.is_null() {
                        get_object_default_value(&object, context)?
                    } else {
                        let name = property_name(&property, context)?;
                        object.get(name, context)?
                    }
                } else {
                    JsValue::undefined()
                };
                results.push(extracted);
            }
        } else if let Some(object) = arg.as_object() {
            results.push(get_object_default_value(&object, context)?);
        } else {
            results.push(arg.clone());
        }
    }
    Ok(results)
}
pub(super) fn apply_to_expansion(
    args: &[JsValue],
    strict: bool,
    context: &mut Context,
    mut visitor: impl FnMut(JsValue, &mut Context) -> JsResult<()>,
) -> JsResult<bool> {
    for value in args {
        if let Some(array) = as_array(value) {
            if !apply_to_array(&array, context, &mut visitor)? && strict {
                return Err(argument_mismatch());
            }
        } else if let Some(object) = value.as_object() {
            apply_to_object(&object, context, &mut visitor)?;
        } else if !value.is_null() {
            visitor(value.clone(), context)?;
        }
    }
    Ok(true)
}
pub(super) fn apply_to_array(
    array: &JsObject,
    context: &mut Context,
    mut visitor: impl FnMut(JsValue, &mut Context) -> JsResult<()>,
) -> JsResult<bool> {
    let array = JsArray::from_object(array.clone())?;
    let length = array.length(context)?;
    if length < 3 {
        return Ok(false);
    }
    let property = array.get(1u32, context)?;
    let name = if property.is_null() {
        None
    } else {
        Some(property_name(&property, context)?)
    };
    for index in 2..length {
        let value = array.get(index, context)?;
        let Some(object) = value.as_object() else {
            continue;
        };
        let value = if let Some(name) = &name {
            object.get(name.clone(), context)?
        } else {
            get_object_default_value(&object, context)?
        };
        if !value.is_null() {
            visitor(value, context)?;
        }
    }
    Ok(true)
}
pub(super) fn apply_to_object(
    object: &JsObject,
    context: &mut Context,
    mut visitor: impl FnMut(JsValue, &mut Context) -> JsResult<()>,
) -> JsResult<()> {
    let value = get_object_default_value(object, context)?;
    if !value.is_null() {
        visitor(value, context)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use boa_engine::{js_string, Source};
    fn eval(context: &mut Context, source: &str) -> JsValue {
        context.eval(Source::from_bytes(source)).unwrap()
    }
    #[test]
    fn source_scalar_and_named_attribute_conversions_preserve_null_and_numeric_prefix_rules() {
        let mut context = Context::default();
        for (source, expected) in [
            ("null", 0.0),
            ("undefined", 0.0),
            ("false", 0.0),
            ("true", 1.0),
            ("'12.5suffix'", 12.5),
            ("'invalid'", 0.0),
            ("({valueOf(){throw 17}})", 0.0),
            ("[1,'value',{value:'3.5kg'}]", 3.5),
            ("[1,null,{}]", 0.0),
            ("[1,'v']", 0.0),
        ] {
            let value = eval(&mut context, source);
            assert_eq!(
                value_to_double(&value, &mut context).unwrap(),
                expected,
                "{source}"
            );
        }
        let integer = eval(&mut context, "'-12.9suffix'");
        assert_eq!(value_to_integer(&integer, &mut context).unwrap(), -12);
        let undefined = JsValue::undefined();
        assert!(!value_is_null(&undefined, &mut context).unwrap());
        let object = eval(&mut context, "({})");
        assert!(value_is_null(&object, &mut context).unwrap());
        let short = eval(&mut context, "[1,'v']");
        assert!(extract_double(&short, &mut context).unwrap().is_none());
        let primitive = eval(&mut context, "[1,'v',7]");
        assert_eq!(extract_double(&primitive, &mut context).unwrap(), Some(7.0));
        assert_eq!(value_to_double(&primitive, &mut context).unwrap(), 0.0);
    }
    #[test]
    fn direct_utf8_conversion_booleans_nulls_nul_and_surrogates_follow_source_boundary() {
        let mut context = Context::default();
        for (source, expected) in [
            ("null", b"".as_slice()),
            ("undefined", b""),
            ("true", b"1"),
            ("false", b"0"),
            ("'a\\u0000b'", b"a\0b"),
            ("'\\ud800'", b"\xef\xbf\xbd"),
        ] {
            let value = eval(&mut context, source);
            assert_eq!(
                value_to_utf8_string(&value, &mut context).unwrap(),
                expected
            );
        }
        let object = eval(&mut context, "({toString(){return 'direct'}})");
        assert_eq!(
            value_to_utf8_string(&object, &mut context).unwrap(),
            b"direct"
        );
        assert!(get_extracted_value(&object, &mut context)
            .unwrap()
            .is_null());
    }
    #[test]
    fn expansion_and_unfold_keep_order_array_shape_null_and_undefined_rules() {
        let mut context = Context::default();
        let value = eval(
            &mut context,
            "var seen=''; [1,{get a(){seen+='a';return 2},get b(){seen+='b';return null}}]",
        );
        // Build argument handles without cloning an object graph.
        let array = eval(
            &mut context,
            "[1,'n',{get n(){seen+='1';return 4}},{get n(){seen+='2';return null}},{},9]",
        );
        let short = eval(&mut context, "[1]");
        let mut values = Vec::new();
        assert!(apply_to_expansion(
            &[
                JsValue::null(),
                JsValue::undefined(),
                value,
                array.clone(),
                short.clone()
            ],
            false,
            &mut context,
            |value, _| {
                values.push(value);
                Ok(())
            }
        )
        .unwrap());
        assert_eq!(values.len(), 3); // undefined scalar, n=4, missing n=undefined.
        assert!(values[0].is_undefined());
        assert_eq!(values[1].as_number(), Some(4.0));
        assert!(values[2].is_undefined());
        assert_eq!(
            eval(&mut context, "seen").as_string().unwrap(),
            js_string!("12")
        );
        assert!(apply_to_expansion(&[short], true, &mut context, |_, _| Ok(())).is_err());
        let unfolded = unfold_args(
            &[0.into(), array, JsValue::null(), JsValue::undefined()],
            &mut context,
        )
        .unwrap();
        assert_eq!(unfolded.len(), 6); // 4 expansion slots, null, undefined.
        assert_eq!(unfolded[0].as_number(), Some(4.0));
        assert!(unfolded[1].is_null());
        assert!(unfolded[2].is_undefined());
        assert!(unfolded[3].is_undefined());
    }
    #[test]
    fn cyclic_extraction_is_rejected_without_recursing_the_host_stack_and_getters_can_throw() {
        let mut context = Context::default();
        let cyclic = eval(
            &mut context,
            "var holder={}; var cyclic=[1,'value',holder]; holder.value=cyclic; cyclic;",
        );
        assert!(value_to_double(&cyclic, &mut context).is_err());
        let throwing = eval(
            &mut context,
            "var thrown={marker:19}; [1,'value',{get value(){throw thrown}}];",
        );
        let error = get_extracted_value(&throwing, &mut context).unwrap_err();
        assert_eq!(error.as_opaque(), Some(&eval(&mut context, "thrown")));
    }
}
