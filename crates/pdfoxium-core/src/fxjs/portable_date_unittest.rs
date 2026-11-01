// Copyright 2021 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Native date/TZ/format vectors through the selected actual Boa boundary.
use crate::fxjs::{cjs_util, fx_date_helpers as date, js_date};
#[cfg(feature = "xfa")]
use crate::xfa::fgas::crt::locale_iface::LocaleIface;
#[cfg(feature = "xfa")]
use crate::xfa::fxfa::parser::cxfa_xmllocale::CxfaXmlLocale;
use boa_engine::{
    context::{time::FixedClock, HostHooks},
    Context, Source,
};
use std::rc::Rc;
#[derive(Debug)]
struct Offset(i32);
impl HostHooks for Offset {
    fn local_timezone_offset_seconds(&self, _: i64) -> i32 {
        self.0
    }
}
fn context(milliseconds: u64, offset_seconds: i32) -> Context {
    Context::builder()
        .clock(Rc::new(FixedClock::from_millis(milliseconds)))
        .host_hooks(Rc::new(Offset(offset_seconds)))
        .build()
        .unwrap()
}
#[test]
fn native_now() {
    // The native FakeTime fixture selects this instant and local fields. Boa's
    // supplied FixedClock/HostHooks replace native time_t/tm function overrides.
    let time = date::make_date(date::make_day(2020, 3, 23), date::make_time(15, 5, 21, 0));
    let context = context(time as u64, 0);
    let (local, _utc) = js_date::pdf_date_fields(Some(&context));
    assert_eq!(local.year, 2020);
    assert_eq!(local.month, 4);
    assert_eq!(local.day, 23);
    assert_eq!(local.hour, 15);
    assert_eq!(local.minute, 5);
    assert_eq!(local.second, 21);
    assert_eq!(js_date::local_time(&context, time).rem_euclid(1000.0), 0.0);
}
#[cfg(feature = "xfa")]
fn assert_timezone_case(native_tz: &str, expected_minutes: i32) {
    // POSIX process-global TZ strings are deliberately represented by Boa's
    // host hook. The real XML locale forwards that context's offset, as FFDoc
    // supplies it in production; no native timezone cache/parser is retained.
    let context = Rc::new(context(1_500_000_000_000, expected_minutes * 60));
    let timezone: Rc<dyn Fn() -> i32> = Rc::new(move || {
        let now = context.clock().system_time_millis() as f64;
        ((js_date::local_time(&context, now) - now) / 60_000.) as i32
    });
    let locale = CxfaXmlLocale::create(br#"<locale name="en_US"/>"#, timezone).unwrap();
    assert_eq!(
        locale.get_time_zone_in_minutes(),
        expected_minutes,
        "{native_tz}"
    );
}
#[test]
#[cfg(feature = "xfa")]
fn native_hour_offsets() {
    for (input, expected) in [
        ("UTC", 0),
        ("UTC+1", -60),
        ("UTC-1", 60),
        ("UTC+14", -840),
        ("UTC-14", 840),
    ] {
        assert_timezone_case(input, expected);
    }
}
#[test]
#[cfg(feature = "xfa")]
fn native_half_hour_offsets() {
    for (input, expected) in [
        ("UTC+0:30", -30),
        ("UTC-0:30", 30),
        ("UTC+1:30", -90),
        ("UTC-1:30", 90),
        ("UTC+9:30", -570),
        ("UTC-9:30", 570),
    ] {
        assert_timezone_case(input, expected);
    }
}
#[test]
fn native_wcsftime_selected_caller_vectors() {
    // FXSYS_wcsftime's only production caller is CJS_Util::printd. Exercise
    // every source field input and the whole no-crash year sweep there. Boa
    // normalizes invalid raw calendar fields; printd may reject negative years.
    // There is no second C tm-based formatter or native invalid-parameter hook.
    let mut context = context(0, 0);
    cjs_util::register_util(&mut context).unwrap();
    context.eval(Source::from_bytes(r#"
        function format(year,month,day,hour,minute,second,pattern='yyyy-mm-ddTHH:MM:ss') {
            let value=new Date(0);
            value.setFullYear(year,month,day);
            value.setHours(hour,minute,second,0);
            return util.printd(pattern,value);
        }
        let good=format(1974,7,9,11,59,59);
        if(good.length!==19 || good!=='1974-08-09T11:59:59') throw Error('native format value/length');
    "#)).unwrap();
    // A separate evaluation per native vector unwinds Boa's script frame and
    // temporary registers after each call, including rejected negative years.
    // Keep the real formatter/context and every source year; engine limits are
    // unchanged and an uncatchable engine error still fails this test.
    for tm_year in -2500..=8500 {
        let year = tm_year + 1900;
        let source =
            format!("try{{format({year},7,9,11,59,59)}}catch(error){{if({year}>=0)throw error;}}");
        context
            .eval(Source::from_bytes(&source))
            .unwrap_or_else(|error| {
                panic!("native tm_year={tm_year}: {error:?}");
            });
    }
    // Native assertions for malformed fields and Feb 30 require safe
    // completion only. Feed the same fields through the actual Date host.
    context
        .eval(Source::from_bytes(
            "format(1899,-1,-1,-1,-1,-1,'yy-mm-ddTHH:MM:ss')",
        ))
        .unwrap();
    context
        .eval(Source::from_bytes(
            "format(2015,1,30,12,0,0,'yy-mm-ddTHH:MM:ss')",
        ))
        .unwrap();
}
