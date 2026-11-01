// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Selection, reference and unit functions from
//! `fxjs/xfa/cfxjse_formcalc_context.cpp` (Choose through UnitValue, excluding
//! Eval). Node default values use the document CJX binding; ordinary objects
//! retain their null default. Named attributes use JavaScript property access.
use super::formcalc_values::{
    argument_mismatch, get_object_default_value, get_simple_value, param_count, property_name,
    simple_value_compare, unfold_args, value_is_null, value_to_float, value_to_utf8_string,
};
use crate::fxcrt::fx_string::{string_to_double, utf16_encode, utf8_decode};
use boa_engine::{
    native_function::NativeFunctionPointer, object::builtins::JsArray, Context, JsResult, JsString,
    JsValue,
};

pub const METHODS: &[(&str, NativeFunctionPointer)] = &[
    ("Choose", choose),
    ("Exists", exists),
    ("HasValue", has_value),
    ("Oneof", oneof),
    ("Within", within),
    ("If", conditional),
    ("Ref", reference),
    ("UnitType", unit_type),
    ("UnitValue", unit_value),
];
fn string(value: &[u8]) -> JsValue {
    JsString::from(utf16_encode(&utf8_decode(value)).as_slice()).into()
}
fn choose(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.len() < 2 {
        return Err(param_count("Choose"));
    }
    if value_is_null(&args[0], context)? {
        return Ok(JsValue::null());
    }
    // Rust saturates source-undefined NaN/out-of-range integer conversions.
    let index = value_to_float(&args[0], context)? as i32;
    if index < 1 {
        return Ok(string(b""));
    }
    let index = index as u64;
    let mut value_index = 0u64;
    for argument in &args[1..] {
        if let Some(object) = argument.as_object().filter(|object| object.is_array()) {
            let array = JsArray::from_object(object)?;
            let length = array.length(context)?;
            // A malformed expansion must not wrap the native unsigned length.
            if length < 2 {
                return Err(argument_mismatch());
            }
            value_index += length - 2;
            if value_index >= index {
                let property = array.get(1, context)?;
                let selected = array.get((length - 1) - (value_index - index), context)?;
                let value = if let Some(object) = selected.as_object() {
                    if property.is_null() {
                        // Nodes read their typed default value; ordinary
                        // objects retain the source null default boundary.
                        get_object_default_value(&object, context)?
                    } else {
                        object.get(property_name(&property, context)?, context)?
                    }
                } else {
                    JsValue::undefined()
                };
                return Ok(string(&value_to_utf8_string(&value, context)?));
            }
            // Native Choose stops after an expansion with multiple references.
            if length > 3 {
                break;
            }
        } else {
            value_index += 1;
            if value_index == index {
                return Ok(string(&value_to_utf8_string(argument, context)?));
            }
        }
    }
    Ok(string(b""))
}
fn exists(_: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    if args.len() != 1 {
        return Err(param_count("Exists"));
    }
    Ok(args[0].is_object().into())
}
fn has_value(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.len() != 1 {
        return Err(param_count("HasValue"));
    }
    let value = get_simple_value(args, 0, context)?;
    if !value.is_string() {
        return Ok(i32::from(value.is_number() || value.is_boolean()).into());
    }
    let value = value_to_utf8_string(&value, context)?;
    Ok(i32::from(value.iter().any(|&c| !whitespace(c))).into())
}
fn oneof(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.len() < 2 {
        return Err(param_count("Oneof"));
    }
    let first = get_simple_value(args, 0, context)?;
    // The source unfolds all arguments before it starts comparing. Getters in
    // later expansions therefore still execute when an earlier value matches.
    for value in unfold_args(args, context)? {
        if simple_value_compare(&first, &value, context)? {
            return Ok(1.into());
        }
    }
    Ok(0.into())
}
fn within(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.len() != 3 {
        return Err(param_count("Within"));
    }
    let value = get_simple_value(args, 0, context)?;
    if value.is_null() {
        return Ok(JsValue::undefined());
    }
    let low = get_simple_value(args, 1, context)?;
    let high = get_simple_value(args, 2, context)?;
    let inside = if value.is_number() {
        let value = value_to_float(&value, context)?;
        let low = value_to_float(&low, context)?;
        let high = value_to_float(&high, context)?;
        value >= low && value <= high
    } else {
        let value = value_to_utf8_string(&value, context)?;
        let low = value_to_utf8_string(&low, context)?;
        let high = value_to_utf8_string(&high, context)?;
        value >= low && value <= high
    };
    Ok(i32::from(inside).into())
}
fn conditional(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.len() != 3 {
        return Err(param_count("If"));
    }
    let condition = get_simple_value(args, 0, context)?.to_boolean();
    get_simple_value(args, if condition { 1 } else { 2 }, context)
}
fn reference(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.len() != 1 {
        return Err(param_count("Ref"));
    }
    let value = &args[0];
    if value.is_boolean() || value.is_string() || value.is_number() {
        return Ok(value.clone());
    }
    let (flag, object) = if value.is_null() {
        (4, JsValue::null())
    } else if let Some(object) = value.as_object().filter(|object| object.is_array()) {
        let array = JsArray::from_object(object)?;
        let property = array.get(1, context)?;
        let object = array.get(2, context)?;
        if !property.is_null() || object.is_null() {
            return Err(argument_mismatch());
        }
        (3, object)
    } else if value.is_object() {
        (3, value.clone())
    } else {
        return Err(argument_mismatch());
    };
    Ok(JsArray::from_iter([flag.into(), JsValue::null(), object], context).into())
}
fn whitespace(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | 0x0b | 0x0c | b'\n' | b'\r')
}
fn number_part(c: u8) -> bool {
    c.is_ascii_digit() || c == b'-' || c == b'.'
}
fn unit_type(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.len() != 1 {
        return Err(param_count("UnitType"));
    }
    let value = get_simple_value(args, 0, context)?;
    if value.is_null() {
        return Ok(JsValue::null());
    }
    let mut text = value_to_utf8_string(&value, context)?;
    if text.is_empty() {
        return Ok(string(b""));
    }
    text.make_ascii_lowercase();
    #[derive(Clone, Copy, PartialEq)]
    enum State {
        Start,
        Invalid,
        Digit,
        DigitWhitespace,
    }
    let mut state = State::Start;
    let mut index = 0;
    while text.get(index).is_some_and(|&c| whitespace(c)) {
        index += 1;
    }
    while let Some(&character) = text.get(index) {
        if whitespace(character) {
            if !matches!(state, State::Digit | State::DigitWhitespace) {
                break;
            }
            state = State::DigitWhitespace;
        } else if number_part(character) {
            if state == State::DigitWhitespace {
                break;
            }
            state = State::Digit;
        } else if matches!(character, b'c' | b'p') && index + 1 < text.len() {
            if state != State::Invalid && !number_part(text[index + 1]) {
                return Ok(string(if character == b'c' { b"cm" } else { b"pt" }));
            }
            state = State::Invalid;
        } else if character == b'm' && index + 1 < text.len() {
            if state != State::Invalid && !number_part(text[index + 1]) {
                return Ok(string(
                    if text[index + 1] == b'p' || text[index..].starts_with(b"millip") {
                        b"mp"
                    } else {
                        b"mm"
                    },
                ));
            }
        } else {
            state = State::Invalid;
        }
        index += 1;
    }
    Ok(string(b"in"))
}
#[derive(Clone, Copy, PartialEq)]
enum Unit {
    Inch,
    Millimeter,
    Centimeter,
    Point,
    Millipoint,
    Unknown,
}
fn unit(value: &[u8]) -> Unit {
    match value {
        b"in" | b"inches" => Unit::Inch,
        b"mm" | b"millimeters" => Unit::Millimeter,
        b"cm" | b"centimeters" => Unit::Centimeter,
        b"pt" | b"points" => Unit::Point,
        b"mp" | b"millipoints" => Unit::Millipoint,
        _ => Unit::Unknown,
    }
}
fn unit_name(text: &[u8], mut index: usize, signed: bool) -> Vec<u8> {
    while text.get(index).is_some_and(|&c| whitespace(c)) {
        index += 1;
    }
    while text.get(index).is_some_and(|&c| {
        if signed {
            number_part(c)
        } else {
            c.is_ascii_digit() || c == b'.'
        }
    }) {
        index += 1;
    }
    while text.get(index).is_some_and(|&c| whitespace(c)) {
        index += 1;
    }
    let end = text[index..]
        .iter()
        .position(|&c| c == b' ')
        .map_or(text.len(), |offset| index + offset);
    let mut name = text[index..end].to_vec();
    name.make_ascii_lowercase();
    name
}
fn unit_value(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.is_empty() || args.len() > 2 {
        return Err(param_count("UnitValue"));
    }
    let value = get_simple_value(args, 0, context)?;
    if value.is_null() {
        return Ok(JsValue::null());
    }
    let text = value_to_utf8_string(&value, context)?;
    let source = unit_name(&text, 0, true);
    let target = if args.len() == 2 {
        let value = get_simple_value(args, 1, context)?;
        unit_name(&value_to_utf8_string(&value, context)?, 0, false)
    } else {
        source.clone()
    };
    let start = text
        .iter()
        .position(|&c| !whitespace(c))
        .unwrap_or(text.len());
    let number = &text[start..];
    let after_sign = if number.first() == Some(&b'-') {
        &number[1..]
    } else {
        number
    };
    let first = if after_sign
        .first()
        .is_some_and(|c| c.is_ascii_digit() || *c == b'.')
    {
        string_to_double(number)
    } else {
        0.0
    };
    // Correct the native dimensional defects (in/cm/mm -> pt/mp divided rather
    // than multiplied), and its missing-unit fallthrough. Bare numbers use
    // inches, as UnitType's default and the disabled UnitValue(6,"pt") vector
    // require. Unknown destination units retain the source's inch fallback.
    let source = if source.is_empty() {
        Unit::Inch
    } else {
        unit(&source)
    };
    let target = match unit(&target) {
        Unit::Unknown => Unit::Inch,
        known => known,
    };
    let result = match source {
        Unit::Inch => match target {
            Unit::Millimeter => first * 25.4,
            Unit::Centimeter => first * 2.54,
            Unit::Point => first * 72.0,
            Unit::Millipoint => first * 72000.0,
            _ => first,
        },
        Unit::Millimeter => match target {
            Unit::Millimeter => first,
            Unit::Centimeter => first / 10.0,
            Unit::Point => first / 25.4 * 72.0,
            Unit::Millipoint => first / 25.4 * 72000.0,
            _ => first / 25.4,
        },
        Unit::Centimeter => match target {
            Unit::Millimeter => first * 10.0,
            Unit::Centimeter => first,
            Unit::Point => first / 2.54 * 72.0,
            Unit::Millipoint => first / 2.54 * 72000.0,
            _ => first / 2.54,
        },
        Unit::Point => match target {
            Unit::Millimeter => first / 72.0 * 25.4,
            Unit::Centimeter => first / 72.0 * 2.54,
            Unit::Point => first,
            Unit::Millipoint => first * 1000.0,
            _ => first / 72.0,
        },
        Unit::Millipoint => match target {
            Unit::Millimeter => first / 72000.0 * 25.4,
            Unit::Centimeter => first / 72000.0 * 2.54,
            Unit::Point => first / 1000.0,
            Unit::Millipoint => first,
            _ => first / 72000.0,
        },
        Unit::Unknown => 0.0,
    };
    Ok(result.into())
}

#[cfg(test)]
mod tests {
    use super::super::cfxjse_formcalc_context::CfxJseFormcalcContext;
    use super::*;
    fn source(value: &str) -> Vec<u32> {
        value.chars().map(u32::from).collect()
    }
    /// Every active scalar input/assertion from native Choose, Exists,
    /// HasValue, Oneof, Within, Ref, UnitType and UnitValue embedder cases.
    #[test]
    fn native_selection_reference_and_unit_vectors_execute_translated_formcalc() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        for (input, expected) in [
            (
                r#"Choose(3, "Taxes", "Price", "Person", "Teller")"#,
                "Person",
            ),
            ("Choose(2, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1)", "9"),
            (
                r#"Choose(20/3, "A", "B", "C", "D", "E", "F", "G", "H")"#,
                "F",
            ),
            (r#"Ref("10*3+5*4")"#, "10*3+5*4"),
            (r#"Ref("hello")"#, "hello"),
            (r#"UnitType("36 in")"#, "in"),
            (r#"UnitType("2.54centimeters")"#, "cm"),
            (r#"UnitType("picas")"#, "pt"),
            (r#"UnitType("2.cm")"#, "cm"),
            (r#"UnitType("2.zero cm")"#, "in"),
            (r#"UnitType("kilometers")"#, "in"),
        ] {
            let result = runtime.execute(&source(input)).unwrap();
            assert_eq!(
                result.as_string().unwrap(),
                JsString::from(expected),
                "{input}"
            );
        }
        for (input, expected) in [
            (r#"Exists("hello world")"#, false),
            ("HasValue(2)", true),
            (r#"HasValue(" ")"#, false),
            ("Oneof(3, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1)", true),
            (
                r#"Oneof("John", "Bill", "Gary", "Joan", "John", "Lisa")"#,
                true,
            ),
            ("Oneof(3, 1, 25)", false),
            ("Oneof(3, 3, null)", true),
            ("Oneof(3, null, null)", false),
            (r#"Within("C", "A", "D")"#, true),
            ("Within(1.5, 0, 2)", true),
            ("Within(-1, 0, 2)", false),
        ] {
            let result = runtime.execute(&source(input)).unwrap();
            assert!(
                result.is_boolean() || result.as_number().is_some_and(|n| n.fract() == 0.0),
                "{input}"
            );
            assert_eq!(result.to_boolean(), expected, "{input}");
        }
        for (input, expected) in [
            (r#"UnitValue("2in")"#, 2.0),
            (r#"UnitValue("2in", "cm")"#, 5.08),
        ] {
            let result = runtime
                .execute(&source(input))
                .unwrap()
                .as_number()
                .unwrap() as f32;
            assert!(
                (result - expected).abs() <= f32::EPSILON * expected.abs().max(1.0),
                "{input}"
            );
        }
    }
    #[test]
    fn unit_conversion_matrix_corrects_source_dimension_errors() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        for (input, expected) in [
            (r#"UnitValue("6", "pt")"#, 432.0),
            (r#"UnitValue("2in", "points")"#, 144.0),
            (r#"UnitValue("2in", "millipoints")"#, 144_000.0),
            (r#"UnitValue("25.4mm", "pt")"#, 72.0),
            (r#"UnitValue("25.4mm", "mp")"#, 72_000.0),
            (r#"UnitValue("2.54cm", "pt")"#, 72.0),
            (r#"UnitValue("2.54cm", "mp")"#, 72_000.0),
            (r#"UnitValue("72pt", "mm")"#, 25.4),
            (r#"UnitValue("72pt", "cm")"#, 2.54),
            (r#"UnitValue("72pt", "mp")"#, 72_000.0),
            (r#"UnitValue("72000mp", "pt")"#, 72.0),
            (r#"UnitValue("72000mp", "mm")"#, 25.4),
            (r#"UnitValue("72000mp", "cm")"#, 2.54),
            (r#"UnitValue("5.08cm", "kilograms")"#, 2.0),
            (r#"UnitValue("unknown", "cm")"#, 0.0),
            (r#"UnitValue("--2cm", "mm")"#, 0.0),
            (r#"UnitValue("  -2.54CENTIMETERS  ", "1 inches")"#, -1.0),
        ] {
            let result = runtime
                .execute(&source(input))
                .unwrap()
                .as_number()
                .unwrap();
            assert!(
                (result - expected).abs() < 1e-9 * expected.abs().max(1.0),
                "{input}: {result}"
            );
        }
    }
    #[test]
    fn reference_expansions_preserve_identity_and_getter_errors() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        let result = runtime.engine().eval(&r#"
            (()=>{
                const object={value:'chosen'};object.self=object;
                if(pfm_rt.Choose(1,[3,'value',object])!=='chosen') throw 'named choice';
                if(pfm_rt.Choose(1,[3,true,{true:9}])!=='9') throw 'property coercion';
                if(pfm_rt.Choose(1,[3,null,object])!=='' || pfm_rt.Choose(1,[3,'value',7])!=='') throw 'missing node boundary';
                if(pfm_rt.Choose(3,[3,'value',object,object],'last')!=='') throw 'multi-reference stop';
                if(pfm_rt.Choose(0,'ignored')!=='' || pfm_rt.Choose(4,'ignored')!=='' || pfm_rt.Choose(null,'ignored')!==null) throw 'choice boundaries';
                const reference=pfm_rt.Ref(object);
                if(reference[0]!==3 || reference[1]!==null || reference[2]!==object || reference[2].self!==object) throw 'reference identity';
                if(pfm_rt.Ref(reference)[2]!==object || pfm_rt.Ref(null)[0]!==4 || pfm_rt.Ref(null)[2]!==null) throw 'reference forms';
                if(!pfm_rt.Exists(object) || pfm_rt.HasValue(object)!==0 || pfm_rt.Within(null,0,2)!==undefined) throw 'plain object boundary';
                if(pfm_rt.Within('10','2','30')!==0 || pfm_rt.Within(10,2,30)!==1) throw 'numeric versus lexical';
                if(pfm_rt.HasValue(false)!==1 || pfm_rt.HasValue('\t\r\n')!==0 || pfm_rt.HasValue('\u00a0')!==1) throw 'HasValue types';
                const marker={};
                const poison={get value(){ if(pfm_rt.Within(1,0,2)!==1) throw 'nested call'; throw marker; }};
                for(const call of [()=>pfm_rt.Choose(1,[3,'value',poison]),()=>pfm_rt.Oneof(1,1,[3,'value',poison])]) {
                    try{call();throw 'missing getter error'}catch(e){if(e!==marker)throw e}
                }
                if(pfm_rt.If(1,7,[3,'value',poison])!==7 || pfm_rt.If('',[3,'value',poison],8)!==8) throw 'conditional extraction';
                try{pfm_rt.Ref([3,'value',object]);throw 'bad reference accepted'}catch(e){if(!String(e).includes('Argument mismatch'))throw e}
                return true;
            })()
        "#.encode_utf16().collect::<Vec<_>>()).unwrap();
        assert_eq!(result, JsValue::from(true));
    }
    #[test]
    fn nulls_parser_edges_and_all_method_arity_errors() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        assert_eq!(runtime.engine().eval(&r#"
            (()=>{
                for(const [name,args] of [
                    ['Choose',[]],['Exists',[]],['HasValue',[1,2]],['Oneof',[1]],['Within',[1,2]],
                    ['If',[1,2]],['Ref',[]],['UnitType',[1,2]],['UnitValue',[]],['UnitValue',[1,2,3]]
                ]) {
                    try{pfm_rt[name](...args);throw 'missing arity error'}
                    catch(e){if(!String(e).includes("Incorrect number of parameters calling method '"+name+"'."))throw e}
                }
                if(pfm_rt.UnitType(null)!==null || pfm_rt.UnitValue(null)!==null) throw 'unit null';
                if(pfm_rt.UnitType('')!=='' || pfm_rt.UnitType('   ')!=='in') throw 'empty unit';
                if(pfm_rt.UnitType('1mp')!=='mp' || pfm_rt.UnitType('millipoints')!=='mp' || pfm_rt.UnitType('1mm')!=='mm') throw 'unit parser';
                if(pfm_rt.UnitType('1 2cm')!=='in') throw 'split number';
                try{pfm_rt.Choose(1,[]);throw 'short expansion accepted'}catch(e){if(!String(e).includes('Argument mismatch'))throw e}
                return true;
            })()
        "#.encode_utf16().collect::<Vec<_>>()).unwrap(), JsValue::from(true));
    }
}
