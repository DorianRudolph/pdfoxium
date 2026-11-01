// Copyright 2017 The PDFium Authors
// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cxfa_xmllocale.{h,cpp}`. The XML document owns its node allocations; locale
//! handles remain weak as in the source. The host supplies timezone behavior.
use crate::fxcrt::cfx_read_only_span_stream::CfxReadOnlySpanStream;
use crate::fxcrt::xml::cfx_xmldocument::CfxXmlDocument;
use crate::fxcrt::xml::cfx_xmlnode::{XmlNodeRef, XmlNodeType};
use crate::fxcrt::xml::cfx_xmlparser::CfxXmlParser;
use crate::xfa::fgas::crt::locale_iface::{DateTimeSubcategory, LocaleIface, NumSubcategory};
use std::rc::Rc;

const fn ascii<const N: usize>(text: &[u8; N]) -> [u32; N] {
    let mut result = [0; N];
    let mut index = 0;
    while index < N {
        result[index] = text[index] as u32;
        index += 1;
    }
    result
}

pub struct CxfaXmlLocale {
    _document: CfxXmlDocument,
    locale: XmlNodeRef,
    timezone: Rc<dyn Fn() -> i32>,
}
impl CxfaXmlLocale {
    pub fn create(data: &[u8], timezone: Rc<dyn Fn() -> i32>) -> Option<Self> {
        let stream = Rc::new(CfxReadOnlySpanStream::new(data));
        let document = CfxXmlParser::new(stream).parse()?;
        let mut child = document.root().first_child();
        while let Some(node) = child {
            if node.node_type() == XmlNodeType::Element
                && node.name().as_ref() == ascii(b"locale").as_slice()
            {
                return Some(Self {
                    _document: document,
                    locale: node,
                    timezone,
                });
            }
            child = node.next_sibling();
        }
        None
    }
    fn get_pattern(patterns: &XmlNodeRef, tag: &[u32], name: &[u32]) -> Vec<u32> {
        let mut child = patterns.first_child();
        while let Some(pattern) = child {
            if pattern.node_type() == XmlNodeType::Element
                && pattern.name().as_ref() == tag
                && pattern.get_attribute(&ascii(b"name")).as_ref() == name
            {
                return pattern.text_data();
            }
            child = pattern.next_sibling();
        }
        Vec::new()
    }
    fn get_symbol(&self, group: &[u32], tag: &[u32], name: &[u32]) -> Vec<u32> {
        self.locale
            .first_child_named(group)
            .map_or_else(Vec::new, |patterns| Self::get_pattern(&patterns, tag, name))
    }
    fn get_calendar_symbol(&self, symbol: &[u32], index: i32, abbreviated: bool) -> Vec<u32> {
        let Ok(index) = usize::try_from(index) else {
            return Vec::new();
        };
        let Some(calendar) = self.locale.first_child_named(&ascii(b"calendarSymbols")) else {
            return Vec::new();
        };
        let mut symbol_names = symbol.to_vec();
        symbol_names.extend_from_slice(&ascii(b"Names"));
        let mut child = calendar.first_child();
        while let Some(name) = child {
            if name.node_type() == XmlNodeType::Element
                && name.name().as_ref() == symbol_names.as_slice()
            {
                let abbr = name.get_attribute(&ascii(b"abbr"));
                if (abbr.as_ref() == ascii(b"1").as_slice()) == abbreviated {
                    return name
                        .nth_child_named(symbol, index)
                        .map_or_else(Vec::new, |node| node.text_data());
                }
            }
            child = name.next_sibling();
        }
        Vec::new()
    }
    fn pattern_name(category: DateTimeSubcategory) -> &'static [u32] {
        const MEDIUM: &[u32] = &ascii(b"med");
        const SHORT: &[u32] = &ascii(b"short");
        const FULL: &[u32] = &ascii(b"full");
        const LONG: &[u32] = &ascii(b"long");
        match category {
            DateTimeSubcategory::Default | DateTimeSubcategory::Medium => MEDIUM,
            DateTimeSubcategory::Short => SHORT,
            DateTimeSubcategory::Full => FULL,
            DateTimeSubcategory::Long => LONG,
        }
    }
}
impl LocaleIface for CxfaXmlLocale {
    fn get_name(&self) -> Vec<u32> {
        self.locale.get_attribute(&ascii(b"name")).to_vec()
    }
    fn get_decimal_symbol(&self) -> Vec<u32> {
        self.get_symbol(
            &ascii(b"numberSymbols"),
            &ascii(b"numberSymbol"),
            &ascii(b"decimal"),
        )
    }
    fn get_grouping_symbol(&self) -> Vec<u32> {
        self.get_symbol(
            &ascii(b"numberSymbols"),
            &ascii(b"numberSymbol"),
            &ascii(b"grouping"),
        )
    }
    fn get_percent_symbol(&self) -> Vec<u32> {
        self.get_symbol(
            &ascii(b"numberSymbols"),
            &ascii(b"numberSymbol"),
            &ascii(b"percent"),
        )
    }
    fn get_minus_symbol(&self) -> Vec<u32> {
        self.get_symbol(
            &ascii(b"numberSymbols"),
            &ascii(b"numberSymbol"),
            &ascii(b"minus"),
        )
    }
    fn get_currency_symbol(&self) -> Vec<u32> {
        self.get_symbol(
            &ascii(b"currencySymbols"),
            &ascii(b"currencySymbol"),
            &ascii(b"symbol"),
        )
    }
    fn get_date_time_symbols(&self) -> Vec<u32> {
        self.locale
            .first_child_named(&ascii(b"dateTimeSymbols"))
            .map_or_else(Vec::new, |node| node.text_data())
    }
    fn get_month_name(&self, month: i32, abbreviated: bool) -> Vec<u32> {
        self.get_calendar_symbol(&ascii(b"month"), month, abbreviated)
    }
    fn get_day_name(&self, week: i32, abbreviated: bool) -> Vec<u32> {
        self.get_calendar_symbol(&ascii(b"day"), week, abbreviated)
    }
    fn get_meridiem_name(&self, am: bool) -> Vec<u32> {
        self.get_calendar_symbol(&ascii(b"meridiem"), if am { 0 } else { 1 }, false)
    }
    fn get_time_zone_in_minutes(&self) -> i32 {
        (self.timezone)()
    }
    fn get_era_name(&self, ad: bool) -> Vec<u32> {
        self.get_calendar_symbol(&ascii(b"era"), if ad { 1 } else { 0 }, false)
    }
    fn get_date_pattern(&self, category: DateTimeSubcategory) -> Vec<u32> {
        self.get_symbol(
            &ascii(b"datePatterns"),
            &ascii(b"datePattern"),
            Self::pattern_name(category),
        )
    }
    fn get_time_pattern(&self, category: DateTimeSubcategory) -> Vec<u32> {
        self.get_symbol(
            &ascii(b"timePatterns"),
            &ascii(b"timePattern"),
            Self::pattern_name(category),
        )
    }
    fn get_num_pattern(&self, category: NumSubcategory) -> Vec<u32> {
        // XFA_PatternToString from cxfa_nodelocale.cpp supplies the source's
        // standard numeric patterns; XML only gates their availability.
        if self
            .locale
            .first_child_named(&ascii(b"numberPatterns"))
            .is_none()
        {
            return Vec::new();
        }
        match category {
            NumSubcategory::Percent => ascii(b"z,zzz,zzz,zzz,zzz,zzz%").to_vec(),
            NumSubcategory::Currency => ascii(b"$z,zzz,zzz,zzz,zzz,zz9.99").to_vec(),
            NumSubcategory::Decimal => ascii(b"z,zzz,zzz,zzz,zzz,zz9.zzz").to_vec(),
            NumSubcategory::Integer => ascii(b"z,zzz,zzz,zzz,zzz,zzz").to_vec(),
        }
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use std::cell::Cell;
    const XML_DATA: &[u8] = "<locale name=\"en_US\" desc=\"English(America)\"><calendarSymbols name=\"gregorian\"><monthNames><month>January</month><month>February</month><month>March</month><month>April</month><month>May</month><month>June</month><month>July</month><month>August</month><month>September</month><month>October</month><month>November</month><month>December</month></monthNames><monthNames abbr=\"1\"><month>Jan</month><month>Feb</month><month>Mar</month><month>Apr</month><month>May</month><month>Jun</month><month>Jul</month><month>Aug</month><month>Sep</month><month>Oct</month><month>Nov</month><month>Dec</month></monthNames><dayNames><day>Sunday</day><day>Monday</day><day>Tuesday</day><day>Wednesday</day><day>Thursday</day><day>Friday</day><day>Saturday</day></dayNames><dayNames abbr=\"1\"><day>Sun</day><day>Mon</day><day>Tue</day><day>Wed</day><day>Thu</day><day>Fri</day><day>Sat</day></dayNames><meridiemNames><meridiem>AM</meridiem><meridiem>PM</meridiem></meridiemNames><eraNames><era>BC</era><era>AD</era></eraNames></calendarSymbols><datePatterns><datePattern name=\"full\">EEEE, MMMM D, YYYY</datePattern><datePattern name=\"long\">MMMM D, YYYY</datePattern><datePattern name=\"med\">MMM D, YYYY</datePattern><datePattern name=\"short\">M/D/YY</datePattern></datePatterns><timePatterns><timePattern name=\"full\">h:MM:SS A Z</timePattern><timePattern name=\"long\">h:MM:SS A Z</timePattern><timePattern name=\"med\">h:MM:SS A</timePattern><timePattern name=\"short\">h:MM A</timePattern></timePatterns><dateTimeSymbols>GyMdkHmsSEDFwWahKzZ</dateTimeSymbols><numberPatterns><numberPattern name=\"numeric\">z,zz9.zzz</numberPattern><numberPattern name=\"currency\">$z,zz9.99|($z,zz9.99)</numberPattern><numberPattern name=\"percent\">z,zz9%</numberPattern></numberPatterns><numberSymbols><numberSymbol name=\"decimal\">.</numberSymbol><numberSymbol name=\"grouping\">,</numberSymbol><numberSymbol name=\"percent\">%</numberSymbol><numberSymbol name=\"minus\">-</numberSymbol><numberSymbol name=\"zero\">0</numberSymbol></numberSymbols><currencySymbols><currencySymbol name=\"symbol\">$</currencySymbol><currencySymbol name=\"isoname\">USD</currencySymbol><currencySymbol name=\"decimal\">.</currencySymbol></currencySymbols></locale>".as_bytes();
    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    fn locale() -> CxfaXmlLocale {
        CxfaXmlLocale::create(XML_DATA, Rc::new(|| 0)).unwrap()
    }
    #[test]
    fn Create() {
        assert!(CxfaXmlLocale::create(XML_DATA, Rc::new(|| 0)).is_some());
    }
    #[test]
    fn CreateBadXML() {
        assert!(CxfaXmlLocale::create(&[], Rc::new(|| 0)).is_none());
    }
    #[test]
    fn GetName() {
        assert_eq!(locale().get_name(), wide("en_US"));
    }
    #[test]
    fn GetNumericSymbols() {
        let locale = locale();
        assert_eq!(locale.get_decimal_symbol(), wide("."));
        assert_eq!(locale.get_grouping_symbol(), wide(","));
        assert_eq!(locale.get_percent_symbol(), wide("%"));
        assert_eq!(locale.get_minus_symbol(), wide("-"));
        assert_eq!(locale.get_currency_symbol(), wide("$"));
    }
    #[test]
    fn GetDateTimeSymbols() {
        assert_eq!(
            locale().get_date_time_symbols(),
            wide("GyMdkHmsSEDFwWahKzZ")
        );
    }
    #[test]
    fn GetMonthName() {
        let locale = locale();
        assert_eq!(locale.get_month_name(24, false), wide(""));
        assert_eq!(locale.get_month_name(-5, false), wide(""));
        assert_eq!(locale.get_month_name(1, true), wide("Feb"));
        assert_eq!(locale.get_month_name(1, false), wide("February"));
    }
    #[test]
    fn GetDayName() {
        let locale = locale();
        assert_eq!(locale.get_day_name(24, false), wide(""));
        assert_eq!(locale.get_day_name(-5, false), wide(""));
        assert_eq!(locale.get_day_name(1, true), wide("Mon"));
        assert_eq!(locale.get_day_name(1, false), wide("Monday"));
    }
    #[test]
    fn GetMeridiemName() {
        let locale = locale();
        assert_eq!(locale.get_meridiem_name(true), wide("AM"));
        assert_eq!(locale.get_meridiem_name(false), wide("PM"));
    }
    #[test]
    fn GetEraName() {
        let locale = locale();
        assert_eq!(locale.get_era_name(true), wide("AD"));
        assert_eq!(locale.get_era_name(false), wide("BC"));
    }
    #[test]
    fn GetDatePattern() {
        let locale = locale();
        for (category, expected) in [
            (DateTimeSubcategory::Short, "M/D/YY"),
            (DateTimeSubcategory::Default, "MMM D, YYYY"),
            (DateTimeSubcategory::Medium, "MMM D, YYYY"),
            (DateTimeSubcategory::Full, "EEEE, MMMM D, YYYY"),
            (DateTimeSubcategory::Long, "MMMM D, YYYY"),
        ] {
            assert_eq!(locale.get_date_pattern(category), wide(expected));
        }
    }
    #[test]
    fn GetTimePattern() {
        let locale = locale();
        for (category, expected) in [
            (DateTimeSubcategory::Short, "h:MM A"),
            (DateTimeSubcategory::Default, "h:MM:SS A"),
            (DateTimeSubcategory::Medium, "h:MM:SS A"),
            (DateTimeSubcategory::Full, "h:MM:SS A Z"),
            (DateTimeSubcategory::Long, "h:MM:SS A Z"),
        ] {
            assert_eq!(locale.get_time_pattern(category), wide(expected));
        }
    }
    #[test]
    fn GetNumPattern() {
        let locale = locale();
        for (category, expected) in [
            (NumSubcategory::Percent, "z,zzz,zzz,zzz,zzz,zzz%"),
            (NumSubcategory::Currency, "$z,zzz,zzz,zzz,zzz,zz9.99"),
            (NumSubcategory::Decimal, "z,zzz,zzz,zzz,zzz,zz9.zzz"),
            (NumSubcategory::Integer, "z,zzz,zzz,zzz,zzz,zzz"),
        ] {
            assert_eq!(locale.get_num_pattern(category), wide(expected));
        }
    }
    #[test]
    fn sparse_xml_and_live_timezone_provider_preserve_boundaries() {
        assert!(CxfaXmlLocale::create(b"<notlocale/>", Rc::new(|| 90)).is_none());
        let offset = Rc::new(Cell::new(90));
        let host_offset = offset.clone();
        let locale = CxfaXmlLocale::create(
            b"<locale name='partial'><numberPatterns/></locale>",
            Rc::new(move || host_offset.get()),
        )
        .unwrap();
        assert_eq!(locale.get_name(), wide("partial"));
        assert!(locale.get_decimal_symbol().is_empty());
        assert!(locale.get_day_name(0, false).is_empty());
        assert!(locale
            .get_date_pattern(DateTimeSubcategory::Default)
            .is_empty());
        assert!(!locale.get_num_pattern(NumSubcategory::Integer).is_empty());
        assert_eq!(locale.get_time_zone_in_minutes(), 90);
        offset.set(-330);
        assert_eq!(locale.get_time_zone_in_minutes(), -330);
    }
}
