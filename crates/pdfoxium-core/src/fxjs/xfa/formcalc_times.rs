// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Time conversions from `fxjs/xfa/cfxjse_formcalc_context.cpp`. The existing
//! picture layer parses/formats fields and Boa supplies the host offset. The
//! source's float conversion and whole-second Num2* formatting are preserved.
use super::cfxjse_formcalc_context::locale_host;
use super::formcalc_locale::FormcalcLocaleHost;
use super::formcalc_values::{
    get_simple_value, param_count, value_is_null, value_to_float, value_to_utf8_string,
};
use crate::fxcrt::fx_string::{utf16_encode, utf8_decode};
use crate::xfa::fgas::crt::locale_iface::{DateTimeSubcategory, LocaleIface};
use crate::xfa::fgas::crt::locale_mgr_iface::LocaleMgrIface;
use crate::xfa::fxfa::parser::cxfa_localevalue::{CxfaLocaleValue, ValueType, XfaValuePicture};
use boa_engine::{native_function::NativeFunctionPointer, Context, JsResult, JsString, JsValue};
use std::{cell::Cell, rc::Rc};

pub(super) const METHODS: &[(&str, NativeFunctionPointer)] = &[
    ("IsoTime2Num", iso_time_to_num),
    ("Time2Num", time_to_num),
    ("Num2Time", num_to_time),
    ("Num2GMTime", num_to_gm_time),
];

fn wide(value: &str) -> Vec<u32> {
    value.chars().map(u32::from).collect()
}
fn string(value: &[u32]) -> JsValue {
    JsString::from(utf16_encode(value).as_slice()).into()
}
fn select_locale(
    host: &FormcalcLocaleHost,
    name: &[u8],
    fallback: bool,
) -> Option<Rc<dyn LocaleIface>> {
    if !name.is_empty() {
        let locale = host.manager.get_locale_by_name(&utf8_decode(name));
        if locale.is_some() || !fallback {
            return locale;
        }
    }
    // An explicitly selected host locale supplies the source this-node locale
    // until the XFA document/node host is connected.
    host.manager.get_def_locale()
}
fn picture(locale: &dyn LocaleIface, format: &[u8]) -> Vec<u32> {
    let format = if format.is_empty() {
        // IsoTime2Local accidentally uses FormatFromString's date default.
        // Both time parsing and formatting must select the time pattern.
        locale.get_time_pattern(DateTimeSubcategory::Default)
    } else {
        utf8_decode(format)
    };
    let mut result = wide("time{");
    result.extend(format);
    result.push(u32::from(b'}'));
    result
}

/// The source encodes times as UTC milliseconds within the day plus one.
/// Picture parsing has already converted explicit zones into host-local fields.
fn time_number(value: &CxfaLocaleValue, offset_minutes: i32) -> i32 {
    let fields = value.get_time();
    // rem_euclid also fixes IsoTime2Num's source `> 1440` boundary, which
    // otherwise returns the following midnight as 86,400,001 instead of 1.
    let minutes = (i64::from(fields.hour) * 60 + i64::from(fields.minute)
        - i64::from(offset_minutes))
    .rem_euclid(1440);
    (minutes * 60_000 + i64::from(fields.second) * 1000 + i64::from(fields.millisecond) + 1) as i32
}

fn iso_time_to_num(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.len() != 1 {
        return Err(param_count("IsoTime2Num"));
    }
    let value = get_simple_value(args, 0, context)?;
    if value_is_null(&value, context)? {
        return Ok(JsValue::null());
    }
    let value = value_to_utf8_string(&value, context)?;
    // The source requires an uppercase T even though its own intended embedder
    // example is a standalone canonical time. Accept either canonical form.
    let value = value
        .iter()
        .position(|&byte| matches!(byte, b'T' | b't'))
        .map_or(value.as_slice(), |index| &value[index + 1..]);
    if value.is_empty() {
        return Ok(0.into());
    }
    let host = locale_host(this, context)?;
    let manager: Rc<dyn LocaleMgrIface> = host.manager.clone();
    let time = CxfaLocaleValue::with_value(ValueType::Time, &utf8_decode(value), Some(manager));
    if !time.is_valid() {
        return Ok(0.into());
    }
    Ok(time_number(&time, host.timezone.get()).into())
}

fn time_to_num(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if !(1..=3).contains(&args.len()) {
        return Err(param_count("Time2Num"));
    }
    let value = get_simple_value(args, 0, context)?;
    if value_is_null(&value, context)? {
        return Ok(JsValue::null());
    }
    let value = value_to_utf8_string(&value, context)?;
    let Some((format, name)) = optional_format_locale(args, context)? else {
        return Ok(JsValue::null());
    };
    // Obtain the host only after extraction/coercion: getters can enter the
    // runtime again and Boa's host timezone may have changed in that callback.
    let host = locale_host(this, context)?;
    let Some(locale) = select_locale(&host, &name, true) else {
        return Ok(0.into());
    };
    let pattern = picture(locale.as_ref(), &format);
    let manager: Rc<dyn LocaleMgrIface> = host.manager.clone();
    let time = CxfaLocaleValue::with_pattern(
        ValueType::Time,
        &utf8_decode(&value),
        &pattern,
        Some(locale),
        Some(manager),
    );
    if !time.is_valid() {
        return Ok(0.into());
    }
    Ok(time_number(&time, host.timezone.get()).into())
}

fn optional_format_locale(
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<Option<(Vec<u8>, Vec<u8>)>> {
    let mut format = Vec::new();
    let mut locale = Vec::new();
    for (index, target) in [(1, &mut format), (2, &mut locale)] {
        if args.len() > index {
            let value = get_simple_value(args, index, context)?;
            if value_is_null(&value, context)? {
                return Ok(None);
            }
            *target = value_to_utf8_string(&value, context)?;
        }
    }
    Ok(Some((format, locale)))
}

fn num_to_gm_time(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    num_to_all_time(this, args, context, true)
}
fn num_to_time(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    num_to_all_time(this, args, context, false)
}

/// GMT formatting explicitly requests a zero offset. This changes only the
/// picture formatter's numeric provider, never the Boa clock or host hooks.
/// There are no JavaScript callbacks while this scope is active.
struct ScopedFormatterOffset<'a> {
    offset: &'a Cell<i32>,
    previous: i32,
}
impl<'a> ScopedFormatterOffset<'a> {
    fn new(offset: &'a Cell<i32>, value: i32) -> Self {
        Self {
            previous: offset.replace(value),
            offset,
        }
    }
}
impl Drop for ScopedFormatterOffset<'_> {
    fn drop(&mut self) {
        self.offset.set(self.previous);
    }
}

fn num_to_all_time(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
    gm: bool,
) -> JsResult<JsValue> {
    let method = if gm { "Num2GMTime" } else { "Num2Time" };
    if !(1..=3).contains(&args.len()) {
        return Err(param_count(method));
    }
    let value = get_simple_value(args, 0, context)?;
    if value.is_null() {
        return Ok(JsValue::null());
    }
    // Preserve ValueToFloat and the source's signed integer conversion, but
    // reject non-finite values rather than invoking C++ conversion UB.
    let value = value_to_float(&value, context)?;
    if !value.is_finite() || value.abs() < 1.0 {
        return Ok(JsValue::null());
    }
    let value = value as i32;
    let Some((format, name)) = optional_format_locale(args, context)? else {
        return Ok(JsValue::null());
    };
    let host = locale_host(this, context)?;
    let Some(locale) = select_locale(&host, &name, false) else {
        return Ok(string(&[]));
    };
    let pattern = picture(locale.as_ref(), &format);
    // Num2AllTime decomposes only whole seconds; retain that float/precision
    // boundary. Normalize combined seconds, fixing minute carries, negative
    // offsets and day rollover instead of constructing invalid ISO fields.
    let seconds = (i64::from(value) / 1000
        + if gm {
            0
        } else {
            i64::from(host.timezone.get()) * 60
        })
    .rem_euclid(86_400);
    let canonical = wide(&format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    ));
    let _offset = gm.then(|| ScopedFormatterOffset::new(host.timezone.as_ref(), 0));
    let manager: Rc<dyn LocaleMgrIface> = host.manager.clone();
    let time = CxfaLocaleValue::with_value(ValueType::Time, &canonical, Some(manager));
    let mut output = Vec::new();
    // IsoTime2Local returns the formatter's output even on failure, including
    // its canonical fallback. Format's separate empty-on-failure rule is not
    // part of this source helper.
    time.format_patterns(
        &mut output,
        &pattern,
        Some(locale),
        XfaValuePicture::Display,
    );
    Ok(string(&output))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxjs::xfa::cfxjse_formcalc_context::{
        execute_in_context, register_formcalc_with_locale,
    };
    use boa_engine::{context::HostHooks, js_string, Source};

    #[derive(Default)]
    struct Hooks {
        seconds: Cell<i32>,
    }
    impl HostHooks for Hooks {
        fn local_timezone_offset_seconds(&self, _: i64) -> i32 {
            self.seconds.get()
        }
    }
    fn context(offset: i32) -> (Context, Rc<Hooks>) {
        let hooks = Rc::new(Hooks {
            seconds: Cell::new(offset),
        });
        let mut context = Context::builder()
            .host_hooks(hooks.clone())
            .build()
            .unwrap();
        register_formcalc_with_locale(&mut context, &wide("en_US")).unwrap();
        (context, hooks)
    }
    fn run(context: &mut Context, program: &str) -> JsValue {
        execute_in_context(context, &wide(program)).unwrap_or_else(|e| panic!("{program}: {e}"))
    }
    fn number(context: &mut Context, program: &str, expected: i32) {
        assert_eq!(
            run(context, program).as_number(),
            Some(f64::from(expected)),
            "{program}"
        );
    }
    fn text(context: &mut Context, program: &str, expected: &str) {
        assert_eq!(
            run(context, program).as_string().unwrap(),
            JsString::from(expected),
            "{program}"
        );
    }

    // cfxjse_formcalc_context_embeddertest.cpp: Time2Num, every value/assertion
    // through the source compiler and Boa; an explicit locale replaces opening
    // simple_xfa.pdf, whose document-bound host is still pending.
    #[test]
    fn source_time2num_programs() {
        let (mut context, _) = context(0);
        for (program, expected) in [
            (r#"Time2Num("00:00:00 GMT", "HH:MM:SS Z")"#, 1),
            (r#"Time2Num("00:00:01 GMT", "HH:MM:SS Z")"#, 1001),
            (r#"Time2Num("00:01:00 GMT", "HH:MM:SS Z")"#, 60001),
            (r#"Time2Num("01:00:00 GMT", "HH:MM:SS Z")"#, 3600001),
            (r#"Time2Num("23:59:59 GMT", "HH:MM:SS Z")"#, 86399001),
            (r#"Time2Num("", "", 1)"#, 0),
            (
                r#"Time2Num("13:13:13 GMT", "HH:MM:SS Z", "fr_FR")"#,
                47593001,
            ),
        ] {
            number(&mut context, program, expected);
        }
    }

    // Time2NumWithTZ: retain every native offset (POSIX TZ signs reversed)
    // through Boa HostHooks rather than mutating a process-wide timezone.
    #[test]
    fn source_time2num_timezone_programs_use_boas_host() {
        let (mut context, hooks) = context(0);
        for minutes in [-840, 840, -570, 30, -30, 1, -1] {
            hooks.seconds.set(minutes * 60);
            for (time, expected) in [
                ("00:00:00", 1),
                ("11:59:59", 43199001),
                ("12:00:00", 43200001),
                ("23:59:59", 86399001),
            ] {
                number(
                    &mut context,
                    &format!(r#"Time2Num("{time} GMT", "HH:MM:SS Z")"#),
                    expected,
                );
            }
        }
        hooks.seconds.set(3 * 3600);
        number(&mut context, r#"Time2Num("1:13:13 PM")"#, 36793001);
        number(
            &mut context,
            r#"Time2Num("13:13:13 GMT", "HH:MM:SS Z") - Time2Num("13:13:13", "HH:MM:SS")"#,
            10800000,
        );
    }

    #[test]
    fn intended_disabled_iso_and_num_time_examples_execute() {
        let (mut context, hooks) = context(0);
        number(&mut context, r#"IsoTime2Num("00:00:00Z")"#, 1);
        for (program, expected) in [
            (r#"Num2GMTime(1, "HH:MM:SS")"#, "00:00:00"),
            (r#"Num2GMTime(65593001, "HH:MM:SS Z")"#, "18:13:13 GMT"),
            (
                r#"Num2GMTime(43993001, TimeFmt(4, "de_DE"), "de_DE")"#,
                "12:13 Uhr GMT",
            ),
            (r#"Num2Time(1, "HH:MM:SS")"#, "00:00:00"),
        ] {
            text(&mut context, program, expected);
        }
        // GMT's explicitly requested zone stays GMT under a nonzero Boa host;
        // the following ordinary conversion still uses that same host offset.
        hooks.seconds.set(90 * 60);
        text(
            &mut context,
            r#"Num2GMTime(65593001, "HH:MM:SS Z")"#,
            "18:13:13 GMT",
        );
        text(
            &mut context,
            r#"Num2Time(1, "HH:MM:SS Z")"#,
            "01:30:00 GMT+01:30",
        );
        hooks.seconds.set(-210 * 60);
        text(
            &mut context,
            r#"Num2Time(1, "HH:MM:SS Z")"#,
            "20:30:00 GMT-03:30",
        );
        text(
            &mut context,
            r#"Num2Time(Time2Num("23:59:59 GMT", "HH:MM:SS Z"), "HH:MM:SS")"#,
            "20:29:59",
        );
    }

    #[test]
    fn canonical_offsets_milliseconds_wrap_and_bad_inputs() {
        let (mut context, hooks) = context(0);
        for offset in [-840, 840, -570, 30, -30, 1, -1, 0] {
            hooks.seconds.set(offset * 60);
            for (program, expected) in [
                (r#"IsoTime2Num("2024-02-29T00:00:00Z")"#, 1),
                (r#"IsoTime2Num("2024-02-29t19:30:22.123+05:30")"#, 50422124),
                (
                    r#"Time2Num("23:59:59.999 GMT", "HH:MM:SS.FFF Z")"#,
                    86400000,
                ),
                (r#"IsoTime2Num("24:00:00Z")"#, 0),
                (r#"IsoTime2Num("")"#, 0),
                (r#"IsoTime2Num("20:00:00Ztrailing")"#, 0),
                (r#"IsoTime2Num("2024-01-01T")"#, 0),
                (r#"Time2Num("25:00:00", "HH:MM:SS")"#, 0),
            ] {
                number(&mut context, program, expected);
            }
        }
        for method in ["IsoTime2Num", "Time2Num", "Num2Time", "Num2GMTime"] {
            assert!(execute_in_context(&mut context, &wide(&format!("{method}()"))).is_err());
            assert!(run(&mut context, &format!("{method}(null)")).is_null());
        }
        for method in ["Time2Num", "Num2Time", "Num2GMTime"] {
            assert!(
                execute_in_context(&mut context, &wide(&format!("{method}(1,2,3,4)"))).is_err()
            );
            assert!(run(&mut context, &format!("{method}(1,null)")).is_null());
            assert!(run(&mut context, &format!(r#"{method}(1,"",null)"#)).is_null());
        }
        assert!(execute_in_context(&mut context, &wide("IsoTime2Num(1,2)")).is_err());
        assert!(run(&mut context, "Num2Time(0)").is_null());
        assert!(run(&mut context, "Num2GMTime(0.5)").is_null());
        hooks.seconds.set(0);
        // No format defaults to time, correcting IsoTime2Local's date fallback.
        text(&mut context, "Num2Time(1)", "12:00:00 AM");
        text(
            &mut context,
            r#"Num2GMTime(86400001, "HH:MM:SS")"#,
            "00:00:00",
        );
    }

    #[test]
    fn getter_reentry_and_opaque_errors_release_all_host_borrows() {
        let (mut context, hooks) = context(90 * 60);
        context
            .eval(Source::from_bytes(
                "var input=[0,'v',{get v(){return pfm_rt.Num2GMTime(1,'HH:MM:SS')+' GMT'}}];",
            ))
            .unwrap();
        number(&mut context, r#"Time2Num(input, "HH:MM:SS Z")"#, 1);
        let marker = context
            .eval(Source::from_bytes(
                "var marker={};input=[0,'v',{get v(){throw marker}}];marker",
            ))
            .unwrap();
        let error =
            execute_in_context(&mut context, &wide(r#"Time2Num(input,"HH:MM:SS")"#)).unwrap_err();
        assert_eq!(error.into_opaque(&mut context).unwrap(), marker);
        hooks.seconds.set(-30 * 60);
        number(&mut context, r#"Time2Num("00:00:00", "HH:MM:SS")"#, 1800001);
        assert_eq!(
            context
                .eval(Source::from_bytes("typeof pfm_rt.Time2Num"))
                .unwrap()
                .as_string()
                .unwrap(),
            js_string!("function")
        );
    }
}
