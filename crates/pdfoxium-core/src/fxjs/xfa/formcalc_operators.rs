// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Scalar/reference operators and expansion plumbing from
//! `fxjs/xfa/cfxjse_formcalc_context.cpp`. XFA SOM resolution stays at the
//! context boundary; ordinary objects never acquire invented default values.
use super::formcalc_values::*;
use boa_engine::{
    native_function::NativeFunctionPointer, object::builtins::JsArray, Context, JsObject, JsResult,
    JsValue,
};

pub(super) const METHODS: &[(&str, NativeFunctionPointer)] = &[
    ("pos_op", positive_operator),
    ("neg_op", negative_operator),
    ("log_or_op", logical_or_operator),
    ("log_and_op", logical_and_operator),
    ("log_not_op", logical_not_operator),
    ("eq_op", equality_operator),
    ("neq_op", notequality_operator),
    ("lt_op", less_operator),
    ("le_op", lessequal_operator),
    ("gt_op", greater_operator),
    ("ge_op", greaterequal_operator),
    ("plus_op", plus_operator),
    ("minus_op", minus_operator),
    ("mul_op", multiple_operator),
    ("div_op", divide_operator),
    ("asgn_val_op", assign_value_operator),
    ("concat_obj", concat_fm_object),
    ("is_obj", is_fm_object),
    ("is_ary", is_fm_array),
    ("get_val", get_fm_value),
    ("get_jsobj", get_fm_jsobj),
    ("var_filter", fm_var_filter),
];
fn count(args: &[JsValue], expected: usize) -> JsResult<()> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(compiler_error())
    }
}
fn assign_value_operator(
    _: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    count(args, 2)?;
    let right = get_simple_value(args, 1, context)?;
    if let Some(array) = as_array(&args[0]) {
        let length = array.length(context)?;
        let property = array.get(1u32, context)?;
        for index in 2..length {
            let value = array.get(index, context)?;
            let object = value
                .as_object()
                .ok_or_else(|| exception("asgn_val_op doesn't have a default property."))?;
            if property.is_null() {
                if !set_object_default_value(&object, &right, context)? {
                    return Err(exception("asgn_val_op doesn't have a default property."));
                }
            } else {
                // Source re-coerces the name for each object. A setter may
                // alter the name object or later expansion elements.
                let name = property_name(&property, context)?;
                object.set(name, right.clone(), false, context)?;
            }
        }
    } else if let Some(object) = args[0].as_object() {
        if !set_object_default_value(&object, &right, context)? {
            return Err(exception("asgn_val_op doesn't have a default property."));
        }
    }
    Ok(right)
}
fn logical(args: &[JsValue], context: &mut Context, and: bool) -> JsResult<JsValue> {
    count(args, 2)?;
    let first = get_simple_value(args, 0, context)?;
    let second = get_simple_value(args, 1, context)?;
    if first.is_null() && second.is_null() {
        return Ok(JsValue::null());
    }
    let first = value_to_float(&first, context)?;
    let second = value_to_float(&second, context)?;
    Ok(i32::from(if and {
        first != 0.0 && second != 0.0
    } else {
        first != 0.0 || second != 0.0
    })
    .into())
}
fn logical_or_operator(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    logical(args, context, false)
}
fn logical_and_operator(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    logical(args, context, true)
}
fn fm_ref_equal(first: &JsValue, second: &JsValue, context: &mut Context) -> JsResult<bool> {
    let (Some(first), Some(second)) = (as_array(first), as_array(second)) else {
        return Ok(false);
    };
    let first_flag = first.get(0u32, context)?;
    let second_flag = second.get(0u32, context)?;
    if first_flag.to_i32(context)? != 3 || second_flag.to_i32(context)? != 3 {
        return Ok(false);
    }
    let first = first.get(2u32, context)?;
    let second = second.get(2u32, context)?;
    if first.is_null() || second.is_null() {
        return Ok(false);
    }
    // Bound nodes have stable cached JS identities. Ordinary objects still
    // compare their own identities rather than two absent native bindings.
    Ok(match (first.as_object(), second.as_object()) {
        (Some(first), Some(second)) => JsObject::equals(&first, &second),
        _ => false,
    })
}
fn equality(args: &[JsValue], context: &mut Context, negate: bool) -> JsResult<JsValue> {
    count(args, 2)?;
    if fm_ref_equal(&args[0], &args[1], context)? {
        return Ok(i32::from(!negate).into());
    }
    let first = get_simple_value(args, 0, context)?;
    let second = get_simple_value(args, 1, context)?;
    let equal = if first.is_null() || second.is_null() {
        first.is_null() && second.is_null()
    } else if first.is_string() && second.is_string() {
        value_to_utf8_string(&first, context)? == value_to_utf8_string(&second, context)?
    } else {
        value_to_double(&first, context)? == value_to_double(&second, context)?
    };
    Ok(i32::from(equal != negate).into())
}
fn equality_operator(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    equality(args, context, false)
}
fn notequality_operator(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    equality(args, context, true)
}
#[derive(Clone, Copy)]
enum Comparison {
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
}
fn comparison(args: &[JsValue], context: &mut Context, kind: Comparison) -> JsResult<JsValue> {
    count(args, 2)?;
    let first = get_simple_value(args, 0, context)?;
    let second = get_simple_value(args, 1, context)?;
    if first.is_null() || second.is_null() {
        let allow_equal = matches!(kind, Comparison::LessEqual | Comparison::GreaterEqual);
        return Ok(i32::from(allow_equal && first.is_null() && second.is_null()).into());
    }
    if first.is_string() && second.is_string() {
        let first = value_to_utf8_string(&first, context)?;
        let second = value_to_utf8_string(&second, context)?;
        // These source string-comparison paths return Boolean, while their
        // numeric and null paths return Number. Keep that visible distinction.
        return Ok(match kind {
            Comparison::Less => first < second,
            Comparison::LessEqual => first <= second,
            Comparison::Greater => first > second,
            Comparison::GreaterEqual => first >= second,
        }
        .into());
    }
    let first = value_to_double(&first, context)?;
    let second = value_to_double(&second, context)?;
    Ok(i32::from(match kind {
        Comparison::Less => first < second,
        Comparison::LessEqual => first <= second,
        Comparison::Greater => first > second,
        Comparison::GreaterEqual => first >= second,
    })
    .into())
}
fn less_operator(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    comparison(args, context, Comparison::Less)
}
fn lessequal_operator(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    comparison(args, context, Comparison::LessEqual)
}
fn greater_operator(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    comparison(args, context, Comparison::Greater)
}
fn greaterequal_operator(
    _: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    comparison(args, context, Comparison::GreaterEqual)
}
#[derive(Clone, Copy)]
enum Arithmetic {
    Plus,
    Minus,
    Multiply,
    Divide,
}
fn arithmetic(args: &[JsValue], context: &mut Context, kind: Arithmetic) -> JsResult<JsValue> {
    count(args, 2)?;
    if matches!(kind, Arithmetic::Plus) {
        // Plus uses ValueIsNull before conversion, so property getters are
        // invoked again by ValueToDouble and short-circuit in source order.
        if value_is_null(&args[0], context)? && value_is_null(&args[1], context)? {
            return Ok(JsValue::null());
        }
        let first = value_to_double(&args[0], context)?;
        let second = value_to_double(&args[1], context)?;
        return Ok((first + second).into());
    }
    let first = get_simple_value(args, 0, context)?;
    let second = get_simple_value(args, 1, context)?;
    if first.is_null() && second.is_null() {
        return Ok(JsValue::null());
    }
    if matches!(kind, Arithmetic::Divide) {
        let second = value_to_double(&second, context)?;
        if second == 0.0 {
            return Err(exception("Divide by zero."));
        }
        return Ok((value_to_double(&first, context)? / second).into());
    }
    let first = value_to_double(&first, context)?;
    let second = value_to_double(&second, context)?;
    Ok(match kind {
        Arithmetic::Minus => first - second,
        Arithmetic::Multiply => first * second,
        _ => unreachable!(),
    }
    .into())
}
fn plus_operator(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    arithmetic(args, context, Arithmetic::Plus)
}
fn minus_operator(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    arithmetic(args, context, Arithmetic::Minus)
}
fn multiple_operator(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    arithmetic(args, context, Arithmetic::Multiply)
}
fn divide_operator(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    arithmetic(args, context, Arithmetic::Divide)
}
fn unary(args: &[JsValue], context: &mut Context, kind: u8) -> JsResult<JsValue> {
    count(args, 1)?;
    let value = get_simple_value(args, 0, context)?;
    if value.is_null() {
        return Ok(JsValue::null());
    }
    let number = value_to_double(&value, context)?;
    Ok(match kind {
        0 => JsValue::from(0.0 + number),
        1 => JsValue::from(0.0 - number),
        _ => JsValue::from(i32::from(number == 0.0)),
    })
}
fn positive_operator(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    unary(args, context, 0)
}
fn negative_operator(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    unary(args, context, 1)
}
fn logical_not_operator(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    unary(args, context, 2)
}
fn is_fm_object(_: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Ok((args.len() == 1 && args[0].is_object()).into())
}
fn is_fm_array(_: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    Ok((args.len() == 1 && as_array(&args[0]).is_some()).into())
}
fn get_fm_value(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    count(args, 1)?;
    if let Some(array) = as_array(&args[0]) {
        // Unlike GetExtractedValue, this exported helper has no length guard.
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
    if let Some(object) = args[0].as_object() {
        return get_object_default_value(&object, context);
    }
    Ok(args[0].clone())
}
fn get_fm_jsobj(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    count(args, 1)?;
    if let Some(array) = as_array(&args[0]) {
        array.get(2u32, context)
    } else {
        Ok(args[0].clone())
    }
}
fn fm_var_filter(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    count(args, 1)?;
    let Some(array) = as_array(&args[0]) else {
        return get_simple_value(args, 0, context);
    };
    let flag = array.get(0u32, context)?.to_i32(context)?;
    if flag != 3 && flag != 4 {
        return get_simple_value(args, 0, context);
    }
    if flag == 4 {
        return Ok(
            JsArray::from_iter([3.into(), JsValue::null(), JsValue::null()], context).into(),
        );
    }
    if array.get(2u32, context)?.is_null() {
        return Err(compiler_error());
    }
    Ok(args[0].clone())
}
fn concat_fm_object(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let mut result = Vec::new();
    for value in args {
        if let Some(array) = as_array(value) {
            let length = array.length(context)?;
            for index in 2..length {
                result.push(array.get(index, context)?);
            }
        } else {
            // Native also appends the expansion array after its members. A
            // foreach then visits its first node twice through get_simple_value.
            // Flatten each expansion once; scalars remain individual entries.
            result.push(value.clone());
        }
    }
    Ok(JsArray::from_iter(result, context).into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use boa_engine::{JsString, NativeFunction, Source};
    fn context() -> Context {
        let mut context = Context::default();
        for &(name, callback) in METHODS {
            context
                .register_global_builtin_callable(
                    JsString::from(name),
                    0,
                    NativeFunction::from_fn_ptr(callback),
                )
                .unwrap();
        }
        context
    }
    fn eval(context: &mut Context, source: &str) -> JsValue {
        context.eval(Source::from_bytes(source)).unwrap()
    }
    #[test]
    fn source_operators_execute_numbers_strings_booleans_and_nulls() {
        let mut context = context();
        assert_eq!(eval(&mut context,"JSON.stringify([plus_op('2.5kg',3),minus_op(9,'2'),mul_op(3,4),div_op(12,3),pos_op('-2'),neg_op(3),log_not_op(0),log_or_op(0,2),log_and_op(2,3)]);").as_string().unwrap().to_std_string().unwrap(),"[5.5,7,12,4,-2,-3,1,1,1]");
        assert_eq!(eval(&mut context,"[plus_op,minus_op,mul_op,div_op,log_or_op,log_and_op].every(f=>f(null,null)===null) && [pos_op,neg_op,log_not_op].every(f=>f(null)===null);").as_boolean(),Some(true));
        assert_eq!(eval(&mut context,"JSON.stringify([eq_op('10','10'),neq_op('10','2'),lt_op('10','2'),le_op(null,null),gt_op(3,2),ge_op(2,2),lt_op(null,1)]);").as_string().unwrap().to_std_string().unwrap(),"[1,1,true,1,1,1,0]");
        assert_eq!(eval(&mut context,"let caught=false;try{div_op(1,0)}catch(e){caught=e instanceof Error&&e.message==='Divide by zero.'}caught;").as_boolean(),Some(true));
        assert_eq!(eval(&mut context,"[()=>plus_op(1),()=>neg_op(1,2),()=>get_val(),()=>var_filter()].every(f=>{try{f();return false}catch(e){return e.message==='Compiler error.'}});").as_boolean(),Some(true));
    }
    #[test]
    fn assignment_and_expansion_helpers_use_real_getters_setters_and_source_iteration_order() {
        let mut context = context();
        assert_eq!(eval(&mut context,"var seen='';var a={},b={};Object.defineProperty(a,'x',{set(v){seen+='a'+v;b.x=v+1}}); asgn_val_op([1,'x',a,b],7); b.x===7&&seen==='a7';").as_boolean(),Some(true));
        assert_eq!(eval(&mut context,"var named={v:12};get_val([1,'v',named])===12&&get_jsobj([1,'v',named])===named&&get_val(named)===null&&is_obj([])&&is_ary([])&&!is_ary({})&&!is_obj(null);").as_boolean(),Some(true));
        assert_eq!(eval(&mut context,"var arr=[3,null,named];var_filter(arr)===arr&&JSON.stringify(var_filter([4,null,{}]))==='[3,null,null]'&&var_filter([1,'v',named])===12;").as_boolean(),Some(true));
        assert_eq!(eval(&mut context,"var group=[1,null,a,b];var combined=concat_obj(group,9);combined.length===3&&combined[0]===a&&combined[1]===b&&combined[2]===9;").as_boolean(),Some(true));
        assert_eq!(eval(&mut context,"var combined=concat_obj([1,null],[1,null,a],b,[1,null,a,b]);combined.length===4&&combined[0]===a&&combined[1]===b&&combined[2]===a&&combined[3]===b;").as_boolean(),Some(true));
        assert_eq!(eval(&mut context,"[()=>asgn_val_op({},3),()=>asgn_val_op([1,null,{}],3),()=>asgn_val_op([1,'x',7],3)].every(f=>{try{f();return false}catch(e){return e.message.includes(\"doesn't have a default property\")}});").as_boolean(),Some(true));
        assert_eq!(eval(&mut context,"var short=[];Object.defineProperty(short,1,{get(){seen+='s';return 'v'}});get_val(short)===undefined&&seen==='a7s';").as_boolean(),Some(true));
    }
    #[test]
    fn reference_identity_and_coercions_preserve_errors_and_source_getter_order() {
        let mut context = context();
        assert_eq!(eval(&mut context,"var a={v:1},b={v:2}; eq_op([3,'v',a],[3,'v',a])===1&&neq_op([3,'v',a],[3,'v',b])===1;").as_boolean(),Some(true));
        assert_eq!(
            eval(
                &mut context,
                "var reads=0;var x=[1,'v',{get v(){reads++;return 2}}];plus_op(x,1)===3&&reads===2;"
            )
            .as_boolean(),
            Some(true)
        );
        assert_eq!(eval(&mut context,"var thrown={tag:21};try{asgn_val_op([1,'v',{set v(value){throw thrown}}],5)}catch(e){e===thrown}").as_boolean(),Some(true));
    }
}
