// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Node-backed locale queries from `cxfa_nodelocale.{h,cpp}`.
use super::{cxfa_document::CxfaNodeStore, cxfa_node::CxfaNode};
use crate::xfa::{
    fgas::crt::locale_iface::{DateTimeSubcategory, LocaleIface, NumSubcategory},
    fxfa::fxfa_basic::{XfaAttribute, XfaElement},
};
use std::rc::Rc;
fn wide(value: &str) -> Vec<u32> {
    value.chars().map(u32::from).collect()
}

/// XFA_PatternToString: these are formatter patterns, independent of locale data.
pub fn pattern_to_string(category: NumSubcategory) -> Vec<u32> {
    wide(match category {
        NumSubcategory::Percent => "z,zzz,zzz,zzz,zzz,zzz%",
        NumSubcategory::Currency => "$z,zzz,zzz,zzz,zzz,zz9.99",
        NumSubcategory::Decimal => "z,zzz,zzz,zzz,zzz,zz9.zzz",
        NumSubcategory::Integer => "z,zzz,zzz,zzz,zzz,zzz",
    })
}

pub struct CxfaNodeLocale {
    node: Option<CxfaNode>,
    // Retain allocations rather than the document: node payloads retain only weak
    // document/store links, so locale managers cannot create a document cycle.
    _store: Option<Rc<CxfaNodeStore>>,
    timezone: Rc<dyn Fn() -> i32>,
}
impl CxfaNodeLocale {
    pub fn new(node: Option<CxfaNode>, timezone: Rc<dyn Fn() -> i32>) -> Self {
        let store = node.as_ref().and_then(CxfaNode::retain_store);
        Self {
            node: node.filter(|_| store.is_some()),
            _store: store,
            timezone,
        }
    }
    fn node(&self) -> Option<&CxfaNode> {
        self.node.as_ref().filter(|node| node.is_alive())
    }
    fn get_node_by_name(parent: Option<CxfaNode>, name: &[u32]) -> Option<CxfaNode> {
        let mut child = parent.and_then(|node| node.first_child());
        while let Some(node) = child {
            child = node.next_sibling();
            if node.get_attribute_by_enum(XfaAttribute::Name) == name {
                return Some(node);
            }
        }
        None
    }
    fn get_symbol(&self, element: XfaElement, name: &str) -> Vec<u32> {
        let symbols = self
            .node()
            .and_then(|node| node.get_child(0, element, false));
        Self::get_node_by_name(symbols, &wide(name))
            .map_or_else(Vec::new, |node| node.get_content())
    }
    fn get_calendar_symbol(&self, element: XfaElement, index: i32, abbreviated: bool) -> Vec<u32> {
        let Ok(index) = usize::try_from(index) else {
            return Vec::new();
        };
        let Some(calendar) = self
            .node()
            .and_then(|node| node.get_child(0, XfaElement::CalendarSymbols, false))
        else {
            return Vec::new();
        };
        let mut names = calendar.get_first_child_by_class(element);
        while let Some(node) = names {
            if node.get_boolean(XfaAttribute::Abbr) == abbreviated {
                return node
                    .get_child(index, XfaElement::Unknown, false)
                    .map_or_else(Vec::new, |symbol| symbol.get_content());
            }
            names = node.get_next_same_class_sibling(element);
        }
        Vec::new()
    }
    fn pattern_name(category: DateTimeSubcategory) -> &'static str {
        match category {
            DateTimeSubcategory::Short => "short",
            DateTimeSubcategory::Medium | DateTimeSubcategory::Default => "med",
            DateTimeSubcategory::Full => "full",
            DateTimeSubcategory::Long => "long",
        }
    }
}
impl LocaleIface for CxfaNodeLocale {
    fn get_name(&self) -> Vec<u32> {
        self.node()
            .map_or_else(Vec::new, |node| node.get_cdata(XfaAttribute::Name))
    }
    fn get_decimal_symbol(&self) -> Vec<u32> {
        self.get_symbol(XfaElement::NumberSymbols, "decimal")
    }
    fn get_grouping_symbol(&self) -> Vec<u32> {
        self.get_symbol(XfaElement::NumberSymbols, "grouping")
    }
    fn get_percent_symbol(&self) -> Vec<u32> {
        self.get_symbol(XfaElement::NumberSymbols, "percent")
    }
    fn get_minus_symbol(&self) -> Vec<u32> {
        self.get_symbol(XfaElement::NumberSymbols, "minus")
    }
    fn get_currency_symbol(&self) -> Vec<u32> {
        self.get_symbol(XfaElement::CurrencySymbols, "symbol")
    }
    fn get_date_time_symbols(&self) -> Vec<u32> {
        self.node()
            .and_then(|node| node.get_child(0, XfaElement::DateTimeSymbols, false))
            .map_or_else(Vec::new, |node| node.get_content())
    }
    fn get_month_name(&self, month: i32, abbreviated: bool) -> Vec<u32> {
        self.get_calendar_symbol(XfaElement::MonthNames, month, abbreviated)
    }
    fn get_day_name(&self, week: i32, abbreviated: bool) -> Vec<u32> {
        self.get_calendar_symbol(XfaElement::DayNames, week, abbreviated)
    }
    fn get_meridiem_name(&self, am: bool) -> Vec<u32> {
        self.get_calendar_symbol(XfaElement::MeridiemNames, if am { 0 } else { 1 }, false)
    }
    fn get_time_zone_in_minutes(&self) -> i32 {
        (self.timezone)()
    }
    fn get_era_name(&self, ad: bool) -> Vec<u32> {
        self.get_calendar_symbol(XfaElement::EraNames, if ad { 1 } else { 0 }, false)
    }
    fn get_date_pattern(&self, category: DateTimeSubcategory) -> Vec<u32> {
        self.get_symbol(XfaElement::DatePatterns, Self::pattern_name(category))
    }
    fn get_time_pattern(&self, category: DateTimeSubcategory) -> Vec<u32> {
        self.get_symbol(XfaElement::TimePatterns, Self::pattern_name(category))
    }
    fn get_num_pattern(&self, category: NumSubcategory) -> Vec<u32> {
        pattern_to_string(category)
    }
}
