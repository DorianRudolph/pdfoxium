// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `xfa/fgas/crt/locale_iface.h`. Locale strings preserve wchar32 code units.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DateTimeSubcategory {
    Default,
    Short,
    Medium,
    Full,
    Long,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NumSubcategory {
    Percent,
    Currency,
    Decimal,
    Integer,
}

/// The source's LocaleIface and GCedLocaleIface share this interface. Rc owners
/// replace cppgc reachability; locale objects do not hold JavaScript values.
pub trait LocaleIface {
    fn get_name(&self) -> Vec<u32>;
    fn get_decimal_symbol(&self) -> Vec<u32>;
    fn get_grouping_symbol(&self) -> Vec<u32>;
    fn get_percent_symbol(&self) -> Vec<u32>;
    fn get_minus_symbol(&self) -> Vec<u32>;
    fn get_currency_symbol(&self) -> Vec<u32>;
    fn get_date_time_symbols(&self) -> Vec<u32>;
    fn get_month_name(&self, month: i32, abbreviated: bool) -> Vec<u32>;
    fn get_day_name(&self, week: i32, abbreviated: bool) -> Vec<u32>;
    fn get_meridiem_name(&self, am: bool) -> Vec<u32>;
    fn get_time_zone_in_minutes(&self) -> i32;
    fn get_era_name(&self, ad: bool) -> Vec<u32>;
    fn get_date_pattern(&self, category: DateTimeSubcategory) -> Vec<u32>;
    fn get_time_pattern(&self, category: DateTimeSubcategory) -> Vec<u32>;
    fn get_num_pattern(&self, category: NumSubcategory) -> Vec<u32>;
}
