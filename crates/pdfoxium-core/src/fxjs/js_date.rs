// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Boa boundary for PDF form dates. The context supplies both the clock and
//! timezone, and Boa constructs local dates. PDFium's token/calendar helpers
//! continue to operate on local wall-clock milliseconds.
use super::fx_date_helpers::{self as date, WideCharacterHost};
use boa_engine::{js_string, object::builtins::JsDate, Context, JsResult, JsString, JsValue};

#[derive(Clone, Copy, Debug, Default)]
pub struct JsDateCharacters;
impl WideCharacterHost for JsDateCharacters {
    fn is_wide_alphanumeric(&self, code_unit: u32) -> bool {
        char::from_u32(code_unit).is_some_and(char::is_alphanumeric)
    }
    fn lowercase_wide(&self, code_unit: u32) -> u32 {
        char::from_u32(code_unit)
            .and_then(|c| c.to_lowercase().next())
            .map_or(code_unit, u32::from)
    }
}
/// Same timestamp-specific offset hook as Boa's Date local getters, including
/// floor-to-seconds behavior before the epoch. No separate timezone policy.
pub fn local_time(context: &Context, time: f64) -> f64 {
    if !time.is_finite() {
        return time;
    }
    let seconds = ((time - time.rem_euclid(1000.0)) / 1000.0) as i64;
    time + f64::from(context.host_hooks().local_timezone_offset_seconds(seconds)) * 1000.0
}
pub fn current_date_time(context: &Context) -> f64 {
    local_time(context, context.clock().system_time_millis() as f64)
}
/// Source PDF metadata fields from the same instant and timezone as Boa.
/// Standalone document creation has no runtime yet and uses Boa's default
/// context; active documents pass their retained runtime context.
pub fn pdf_date_fields(
    context: Option<&Context>,
) -> (
    crate::fpdfsdk::fpdf_editpage::PdfDateFields,
    crate::fpdfsdk::fpdf_editpage::PdfDateFields,
) {
    use crate::fpdfsdk::fpdf_editpage::PdfDateFields;
    let default_context;
    let context = match context {
        Some(context) => context,
        None => {
            default_context = Context::default();
            &default_context
        }
    };
    let utc = context.clock().system_time_millis() as f64;
    let fields = |time| PdfDateFields {
        year: date::get_year_from_time(time),
        month: (date::get_month_from_time(time) + 1) as u32,
        day: date::get_day_from_time(time) as u32,
        hour: date::get_hour_from_time(time) as u32,
        minute: date::get_min_from_time(time) as u32,
        second: date::get_sec_from_time(time) as u32,
    };
    (fields(local_time(context, utc)), fields(utc))
}
/// Convert the source parser's local wall-clock result through Boa's intrinsic
/// Date constructor, so local/UTC conversion follows JavaScript, including DST.
pub fn new_local_date(context: &mut Context, time: f64) -> JsResult<JsValue> {
    if !time.is_finite() || time.abs() > 8.64e15 {
        return new_date(context, f64::NAN);
    }
    let year = date::get_year_from_time(time);
    let constructor = context.intrinsics().constructors().date().constructor();
    let value = constructor.construct(
        &[
            // Date's multi-argument constructor maps years 0..99 to 1900..1999.
            // Use an equivalent leap-year cycle, then restore the actual year.
            (if (0..100).contains(&year) {
                year + 400
            } else {
                year
            })
            .into(),
            date::get_month_from_time(time).into(),
            date::get_day_from_time(time).into(),
            date::get_hour_from_time(time).into(),
            date::get_min_from_time(time).into(),
            date::get_sec_from_time(time).into(),
            time.rem_euclid(1000.0).into(),
        ],
        None,
        context,
    )?;
    if (0..100).contains(&year) {
        JsDate::from_object(value.clone())?.set_full_year(&[year.into()], context)?;
    }
    Ok(value.into())
}
pub fn date_parse(context: &mut Context, value: &[u16]) -> JsResult<f64> {
    // JS_DateParse deliberately looks up the visible Date.parse method: PDFs
    // can replace it. Keep property access/call exceptions as opaque values.
    let global = context.global_object();
    let date = global.get(js_string!("Date"), context)?;
    let Some(object) = date.as_object() else {
        return Ok(0.0);
    };
    let parse = object.get(js_string!("parse"), context)?;
    let Some(function) = parse.as_callable() else {
        return Ok(0.0);
    };
    let result = function.call(&global.into(), &[JsString::from(value).into()], context)?;
    // Source requires a Number result; it does not coerce replacement results.
    Ok(result
        .as_number()
        .map_or(0.0, |time| local_time(context, time)))
}
pub fn new_date(context: &mut Context, value: f64) -> JsResult<JsValue> {
    let date = JsDate::new(context);
    date.set_time(value, context)?;
    Ok(date.into())
}
/// Check the genuine Date internal slot before applying JavaScript numeric
/// coercion. User valueOf/Symbol.toPrimitive hooks and their errors are retained.
pub fn date_value(value: &JsValue, context: &mut Context) -> JsResult<Option<f64>> {
    let Some(object) = value.as_object() else {
        return Ok(None);
    };
    if JsDate::from_object(object).is_err() {
        return Ok(None);
    }
    value.to_number(context).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use boa_engine::Source;
    #[test]
    fn metadata_fields_follow_the_actual_boa_clock_and_local_getters() {
        use boa_engine::context::{time::FixedClock, HostHooks};
        use std::rc::Rc;
        #[derive(Debug)]
        struct Hooks(i32);
        impl HostHooks for Hooks {
            fn local_timezone_offset_seconds(&self, _: i64) -> i32 {
                self.0
            }
        }
        let instant = date::make_date(
            date::make_day(2025, 11, 31),
            date::make_time(23, 50, 7, 999),
        );
        for minutes in [0, 90, 345, -210] {
            let mut context = Context::builder()
                .clock(Rc::new(FixedClock::from_millis(instant as u64)))
                .host_hooks(Rc::new(Hooks(minutes * 60)))
                .build()
                .unwrap();
            let (local, utc) = pdf_date_fields(Some(&context));
            let values = [
                local.year,
                local.month as i32,
                local.day as i32,
                local.hour as i32,
                local.minute as i32,
                local.second as i32,
            ];
            for (expression, expected) in [
                "getFullYear()",
                "getMonth()+1",
                "getDate()",
                "getHours()",
                "getMinutes()",
                "getSeconds()",
            ]
            .into_iter()
            .zip(values)
            {
                let script = format!("new Date().{expression}");
                assert_eq!(
                    context
                        .eval(Source::from_bytes(&script))
                        .unwrap()
                        .as_number(),
                    Some(f64::from(expected))
                );
            }
            assert_eq!(
                (utc.year, utc.month, utc.day, utc.hour, utc.minute, utc.second),
                (2025, 12, 31, 23, 50, 7)
            );
            if minutes == 345 {
                assert_eq!(
                    crate::fpdfsdk::fpdf_editpage::format_pdf_date(local, Some(utc)),
                    b"D:20260101053507+05'45'"
                );
                assert_eq!(
                    format!(
                        "D:{}{:02}{:02}{:02}{:02}{:02}",
                        local.year, local.month, local.day, local.hour, local.minute, local.second
                    ),
                    "D:20260101053507"
                );
            }
        }
    }
    #[test]
    fn boa_calendar_and_actual_javascript_dates() {
        let mut context = Context::default();
        let before = context.clock().system_time_millis() as f64;
        let current = current_date_time(&context);
        let after = context.clock().system_time_millis() as f64;
        assert!((local_time(&context, before)..=local_time(&context, after)).contains(&current));
        let value = new_date(&mut context, 1_483_228_800_000.0).unwrap();
        assert_eq!(
            date_value(&value, &mut context).unwrap(),
            Some(1_483_228_800_000.0)
        );
        assert_eq!(
            date_parse(
                &mut context,
                &"2017-01-01T00:00:00Z".encode_utf16().collect::<Vec<_>>()
            )
            .unwrap(),
            local_time(&context, 1_483_228_800_000.0)
        );
        assert!(date_parse(&mut context, &[0xd800]).unwrap().is_nan());
        let fake = context
            .eval(Source::from_bytes("({valueOf(){return 1}})"))
            .unwrap();
        assert_eq!(date_value(&fake, &mut context).unwrap(), None);
    }
    #[test]
    fn pdf_current_time_uses_boas_context_clock() {
        use boa_engine::context::time::FixedClock;
        use std::rc::Rc;
        // Both occurrences of Berlin's repeated 02:30 at the DST overlap.
        let time = date::make_date(date::make_day(2024, 9, 27), date::make_time(0, 30, 0, 123));
        let clock = Rc::new(FixedClock::from_millis(time as u64));
        let mut context = Context::builder().clock(clock.clone()).build().unwrap();
        super::super::cjs_util::register_util(&mut context).unwrap();
        for _ in 0..2 {
            let now = context
                .eval(Source::from_bytes("Date.now()"))
                .unwrap()
                .as_number()
                .unwrap();
            assert_eq!(current_date_time(&context), local_time(&context, now));
            assert_eq!(
                context
                    .eval(Source::from_bytes("+util.scand('yyyy-mm-dd','')"))
                    .unwrap()
                    .as_number(),
                Some(now)
            );
            clock.forward(3_600_000);
        }
    }
    #[test]
    fn visible_date_parse_and_opaque_coercion_errors_are_observed() {
        let mut context = Context::default();
        context
            .eval(Source::from_bytes(
                "Date.parse=function(s){return this===globalThis&&s==='date'?123:456}",
            ))
            .unwrap();
        assert_eq!(
            date_parse(&mut context, &"date".encode_utf16().collect::<Vec<_>>()).unwrap(),
            local_time(&context, 123.0)
        );
        context
            .eval(Source::from_bytes("Date.parse=function(){return '123'}"))
            .unwrap();
        assert_eq!(date_parse(&mut context, &[]).unwrap(), 0.0);
        let sentinel = context
            .eval(Source::from_bytes(
                "globalThis.sentinel={}; Date.parse=function(){throw sentinel}; sentinel",
            ))
            .unwrap();
        assert_eq!(
            date_parse(&mut context, &[])
                .unwrap_err()
                .into_opaque(&mut context)
                .unwrap(),
            sentinel
        );
        let value = context
            .eval(Source::from_bytes(
                "let date=new Date(0);date.valueOf=function(){throw sentinel};date",
            ))
            .unwrap();
        assert_eq!(
            date_value(&value, &mut context)
                .unwrap_err()
                .into_opaque(&mut context)
                .unwrap(),
            sentinel
        );
    }
}
