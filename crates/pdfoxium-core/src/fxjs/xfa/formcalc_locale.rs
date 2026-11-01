// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Locale-backed Format/Parse from `cfxjse_formcalc_context.cpp`. Embedders
//! explicitly select the locale; the pending XFA node binding selects its own.
use super::cfxjse_formcalc_context::locale_host;
use super::formcalc_values::{get_simple_value, param_count, value_is_null, value_to_utf8_string};
use crate::fxcrt::fx_string::{utf16_encode, utf8_decode};
use crate::fxjs::js_date;
use crate::xfa::fgas::crt::locale_mgr_iface::LocaleMgrIface;
use crate::xfa::fxfa::parser::{
    cxfa_localemgr::CxfaLocaleMgr,
    cxfa_localevalue::{CxfaLocaleValue, ValueType, XfaValuePicture},
};
use boa_engine::{native_function::NativeFunctionPointer, Context, JsResult, JsString, JsValue};
use std::{cell::Cell, rc::Rc};

pub(super) const METHODS: &[(&str, NativeFunctionPointer)] =
    &[("Format", format), ("Parse", parse)];

#[derive(Clone)]
pub(super) struct FormcalcLocaleHost {
    pub(super) manager: Rc<dyn LocaleMgrIface>,
    pub(super) timezone: Rc<Cell<i32>>,
}
impl FormcalcLocaleHost {
    pub(super) fn new(context: &Context, name: &[u32]) -> Self {
        let timezone = Rc::new(Cell::new(context_timezone_minutes(context)));
        let provider = timezone.clone();
        let manager = Rc::new(CxfaLocaleMgr::new(name, Rc::new(move || provider.get())));
        let host = Self { manager, timezone };
        host.refresh_time_zone(context);
        host
    }
    pub(super) fn refresh_time_zone(&self, context: &Context) {
        // Snapshot the actual context's current offset for this synchronous
        // formatting operation, through the existing Boa Date adapter. The
        // locale itself owns no clock, timezone database or UTC policy.
        self.timezone.set(context_timezone_minutes(context));
    }
}
fn context_timezone_minutes(context: &Context) -> i32 {
    let now = context.clock().system_time_millis() as f64;
    ((js_date::local_time(context, now) - now) / 60_000.0) as i32
}
fn string(value: &[u32]) -> JsValue {
    JsString::from(utf16_encode(value).as_slice()).into()
}
fn starts_with(value: &[u32], prefix: &[u8]) -> bool {
    value.len() >= prefix.len() && value.iter().zip(prefix).all(|(&a, &b)| a == u32::from(b))
}
fn contains(value: &[u32], word: &[u8]) -> bool {
    value
        .windows(word.len())
        .any(|part| starts_with(part, word))
}
/// Source PatternStringType, preserving case-sensitive category prefixes and
/// quoted literal handling before inference from picture symbols.
fn pattern_string_type(pattern: &[u32]) -> (bool, ValueType) {
    if starts_with(pattern, b"datetime") {
        return (true, ValueType::DateTime);
    }
    if starts_with(pattern, b"date") {
        return (
            true,
            if contains(pattern, b"time") {
                ValueType::DateTime
            } else {
                ValueType::Date
            },
        );
    }
    if starts_with(pattern, b"time") {
        return (true, ValueType::Time);
    }
    if starts_with(pattern, b"text") {
        return (true, ValueType::Text);
    }
    if starts_with(pattern, b"num") {
        let suffix = pattern.get(4..).unwrap_or_default();
        return (
            true,
            if starts_with(suffix, b"integer") {
                ValueType::Integer
            } else if starts_with(suffix, b"decimal") {
                ValueType::Decimal
            } else {
                ValueType::Float
            },
        );
    }
    let lowered: Vec<u32> = pattern
        .iter()
        .map(|&ch| {
            char::from_u32(ch)
                .and_then(|ch| ch.to_lowercase().next())
                .map_or(ch, u32::from)
        })
        .collect();
    let mut kind = ValueType::Null;
    let mut quoted = false;
    let mut index = 0;
    while let Some(&ch) = lowered.get(index) {
        index += 1;
        if ch == 39 {
            quoted = !quoted;
            continue;
        }
        if quoted {
            continue;
        }
        match ch {
            104 | 107 => return (false, ValueType::Time),
            120 | 111 | 48 => return (false, ValueType::Text),
            118 | 56 | 36 => return (false, ValueType::Float),
            121 | 106 => {
                while let Some(&next) = lowered.get(index) {
                    index += 1;
                    if next == 39 {
                        quoted = !quoted;
                    } else if !quoted && next == 116 {
                        return (false, ValueType::DateTime);
                    }
                }
                return (false, ValueType::Date);
            }
            97 => kind = ValueType::Text,
            122 | 115 | 101 | 44 | 46 => kind = ValueType::Float,
            _ => {}
        }
    }
    (false, kind)
}
fn wrap(pattern: &[u32], kind: ValueType) -> Option<Vec<u32>> {
    let tag: &[u8] = match kind {
        ValueType::DateTime => {
            let split = pattern.iter().position(|&ch| ch == u32::from(b'T'))?;
            let mut result: Vec<u32> = b"date{".iter().copied().map(u32::from).collect();
            result.extend_from_slice(&pattern[..split]);
            result.extend(b"} time{".iter().copied().map(u32::from));
            result.extend_from_slice(&pattern[split + 1..]);
            result.push(125);
            return Some(result);
        }
        ValueType::Date => b"date{",
        ValueType::Time => b"time{",
        ValueType::Text => b"text{",
        _ => b"num{",
    };
    let mut result: Vec<u32> = tag.iter().copied().map(u32::from).collect();
    result.extend_from_slice(pattern);
    result.push(125);
    Some(result)
}
fn format(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.len() < 2 {
        return Err(param_count("Format"));
    }
    let pattern = get_simple_value(args, 0, context)?;
    let pattern = utf8_decode(&value_to_utf8_string(&pattern, context)?);
    let value = get_simple_value(args, 1, context)?;
    let value = utf8_decode(&value_to_utf8_string(&value, context)?);
    let host = locale_host(this, context)?;
    let manager: Rc<dyn LocaleMgrIface> = host.manager.clone();
    let locale = manager.get_def_locale();
    let (explicit, mut kind) = pattern_string_type(&pattern);
    let pattern = if explicit {
        pattern
    } else {
        if kind == ValueType::Null {
            let numeric = wrap(&pattern, ValueType::Float).expect("numeric wrapper");
            let probe = CxfaLocaleValue::with_pattern(
                ValueType::Float,
                &value,
                &numeric,
                locale.clone(),
                Some(manager.clone()),
            );
            kind = if probe.is_valid() {
                ValueType::Float
            } else {
                ValueType::Text
            };
        }
        let Some(pattern) = wrap(&pattern, kind) else {
            return Ok(string(&[]));
        };
        pattern
    };
    let value =
        CxfaLocaleValue::with_pattern(kind, &value, &pattern, locale.clone(), Some(manager));
    let mut output = Vec::new();
    if !value.format_patterns(&mut output, &pattern, locale, XfaValuePicture::Display) {
        output.clear();
    }
    Ok(string(&output))
}
fn parse(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.len() != 2 {
        return Err(param_count("Parse"));
    }
    let pattern = get_simple_value(args, 0, context)?;
    let value = get_simple_value(args, 1, context)?;
    if value_is_null(&value, context)? {
        return Ok(JsValue::null());
    }
    let pattern = utf8_decode(&value_to_utf8_string(&pattern, context)?);
    let value = utf8_decode(&value_to_utf8_string(&value, context)?);
    let host = locale_host(this, context)?;
    let manager: Rc<dyn LocaleMgrIface> = host.manager.clone();
    let locale = manager.get_def_locale();
    let (explicit, kind) = pattern_string_type(&pattern);
    if explicit {
        let parsed = CxfaLocaleValue::with_pattern(kind, &value, &pattern, locale, Some(manager));
        return Ok(if parsed.is_valid() {
            string(parsed.get_value())
        } else {
            string(&[])
        });
    }
    let kinds: &[ValueType] = match kind {
        ValueType::DateTime => &[ValueType::DateTime],
        ValueType::Date => &[ValueType::Date],
        ValueType::Time => &[ValueType::Time],
        ValueType::Text => &[ValueType::Text],
        ValueType::Float => &[ValueType::Float],
        _ => &[ValueType::Float, ValueType::Text],
    };
    for &kind in kinds {
        let Some(pattern) = wrap(&pattern, kind) else {
            return Ok(string(&[]));
        };
        let parsed = CxfaLocaleValue::with_pattern(
            kind,
            &value,
            &pattern,
            locale.clone(),
            Some(manager.clone()),
        );
        if parsed.is_valid() {
            return Ok(if kind == ValueType::Float {
                parsed.get_double_num().into()
            } else {
                string(parsed.get_value())
            });
        }
    }
    Ok(string(&[]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxjs::xfa::cfxjse_formcalc_context::{
        register_formcalc_with_locale, CfxJseFormcalcContext,
    };
    use boa_engine::{context::HostHooks, js_string, property::Attribute, Source};
    fn wide(value: &str) -> Vec<u32> {
        value.chars().map(u32::from).collect()
    }
    fn run(runtime: &CfxJseFormcalcContext, source: &str) -> JsValue {
        runtime
            .execute(&wide(source))
            .unwrap_or_else(|e| panic!("{source}: {e}"))
    }
    #[test]
    fn source_format_and_parse_programs_run_through_compiler_and_boa() {
        let runtime = CfxJseFormcalcContext::new_with_locale(&wide("en_US")).unwrap();
        // Both intended source cases are DISABLED upstream; execute them here
        // without claiming the XFA document embedder suite is implemented.
        for (program, expected) in [
            (r#"Format("MMM D, YYYY", "20020901")"#, "Sep 1, 2002"),
            (r#"Format("$9,999,999.99", 1234567.89)"#, "$1,234,567.89"),
            (r#"Parse("MMM D, YYYY", "Sep 1, 2002")"#, "2002-09-01"),
            (r#"Format("text{AAA}", "ABC", 1)"#, "ABC"),
            (r#"Parse("num{9.99}", "7.50")"#, "7.50"),
        ] {
            assert_eq!(
                run(&runtime, program).as_string().unwrap(),
                JsString::from(expected),
                "{program}"
            );
        }
        assert_eq!(
            run(&runtime, r#"Parse("$9,999,999.99", "$1,234,567.89")"#).as_number(),
            Some(1234567.89)
        );
        assert!(run(&runtime, r#"Parse("9", null)"#).is_null());
        for source in ["Format()", "Parse()", "Parse(1)", "Parse(1,2,3)"] {
            assert!(runtime.execute(&wide(source)).is_err(), "{source}");
        }
        assert_eq!(run(&runtime, "2+3"), 5.into());
        let german = CfxJseFormcalcContext::new_with_locale(&wide("de_DE")).unwrap();
        assert_eq!(
            run(&german, r#"Format("num{9.99}", 7.5)"#)
                .as_string()
                .unwrap(),
            js_string!("7,50")
        );
        assert_eq!(
            run(&german, r#"Parse("9.99", "7,50")"#).as_number(),
            Some(7.5)
        );
    }
    #[test]
    fn boa_host_timezone_is_refreshed_without_overrides_and_getter_errors_recover() {
        fn nested_format(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
            use crate::fxjs::cfxjs_engine::CfxJsEngine;
            let engine = CfxJsEngine::from_context(context).expect("registered engine");
            CfxJsEngine::with_host_callback(context, || {
                engine.eval(
                    &"pfm_rt.Format('text{X}','R')"
                        .encode_utf16()
                        .collect::<Vec<_>>(),
                )
            })
        }
        #[derive(Default)]
        struct Hooks {
            seconds: Cell<i32>,
        }
        impl HostHooks for Hooks {
            fn local_timezone_offset_seconds(&self, _: i64) -> i32 {
                self.seconds.get()
            }
        }
        let hooks = Rc::new(Hooks::default());
        hooks.seconds.set(5400);
        let mut context = Context::builder()
            .host_hooks(hooks.clone())
            .build()
            .unwrap();
        let object = register_formcalc_with_locale(&mut context, &wide("en_US")).unwrap();
        let host = locale_host(&object.clone().into(), &context).unwrap();
        assert_eq!(
            host.manager
                .get_def_locale()
                .unwrap()
                .get_time_zone_in_minutes(),
            90
        );
        hooks.seconds.set(-12600);
        locale_host(&object.into(), &context).unwrap();
        assert_eq!(
            host.manager
                .get_def_locale()
                .unwrap()
                .get_time_zone_in_minutes(),
            -210
        );
        let runtime = CfxJseFormcalcContext::new_with_locale(&wide("en_US")).unwrap();
        runtime
            .engine()
            .define_global_method(
                &"__nested".encode_utf16().collect::<Vec<_>>(),
                0,
                nested_format,
            )
            .unwrap();
        runtime
            .engine()
            .eval(
                &"globalThis.input=[0,'v',{get v(){return __nested()}}]"
                    .encode_utf16()
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        assert_eq!(
            run(&runtime, r#"Format("text{X}", input)"#)
                .as_string()
                .unwrap(),
            js_string!("R")
        );
        runtime
            .engine()
            .eval(
                &"globalThis.marker={};globalThis.input=[0,'v',{get v(){throw marker}}]"
                    .encode_utf16()
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        let error = runtime
            .execute(&wide(r#"Format("text{X}", input)"#))
            .unwrap_err();
        assert!(runtime
            .engine()
            .with_context(|context| {
                let value = error.into_opaque(context)?;
                let marker = context.global_object().get(js_string!("marker"), context)?;
                Ok(value == marker)
            })
            .unwrap());
        assert_eq!(
            run(&runtime, r#"Format("text{X}", "Q")"#)
                .as_string()
                .unwrap(),
            js_string!("Q")
        );
        assert!(CfxJseFormcalcContext::new()
            .unwrap()
            .execute(&wide(r#"Format("text{X}", "Q")"#))
            .is_err());
        // Keep source native receiver checks and real descriptors.
        context
            .register_global_property(js_string!("other"), 1, Attribute::all())
            .unwrap();
        assert!(context
            .eval(Source::from_bytes(
                "pfm_rt.Format.call(other,'text{X}','Q')"
            ))
            .is_err());
    }
}
