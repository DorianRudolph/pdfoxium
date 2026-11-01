// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Document/current-node locale selection for the FormCalc runtime. Actual
//! locale data stays in the document's CxfaLocaleMgr. A per-call view selects
//! the current node default without mutating document-global state. Timezone
//! reads use the existing Boa offset snapshot, with no second implementation.
use super::{cfxjse_engine::CfxJseEngine, formcalc_locale::FormcalcLocaleHost};
use crate::xfa::{
    fgas::crt::{
        locale_iface::{DateTimeSubcategory, LocaleIface, NumSubcategory},
        locale_mgr_iface::LocaleMgrIface,
    },
    fxfa::parser::cxfa_localemgr::CxfaLocaleMgr,
};
use boa_engine::Context;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
struct DocumentLocaleMgr {
    manager: Rc<CxfaLocaleMgr>,
    selected: RefCell<Option<Rc<dyn LocaleIface>>>,
    timezone: Rc<Cell<i32>>,
}
impl DocumentLocaleMgr {
    fn wrap(&self, locale: Rc<dyn LocaleIface>) -> Rc<dyn LocaleIface> {
        Rc::new(ContextLocale {
            locale,
            timezone: self.timezone.clone(),
        })
    }
}
impl LocaleMgrIface for DocumentLocaleMgr {
    fn get_def_locale(&self) -> Option<Rc<dyn LocaleIface>> {
        self.selected
            .borrow()
            .clone()
            .map(|locale| self.wrap(locale))
    }
    fn get_locale_by_name(&self, name: &[u32]) -> Option<Rc<dyn LocaleIface>> {
        self.manager
            .get_locale_by_name(name)
            .map(|locale| self.wrap(locale))
    }
    fn set_def_locale(&self, locale: Option<Rc<dyn LocaleIface>>) {
        let old = self.selected.replace(locale);
        drop(old);
    }
}
struct ContextLocale {
    locale: Rc<dyn LocaleIface>,
    timezone: Rc<Cell<i32>>,
}
impl LocaleIface for ContextLocale {
    fn get_name(&self) -> Vec<u32> {
        self.locale.get_name()
    }
    fn get_decimal_symbol(&self) -> Vec<u32> {
        self.locale.get_decimal_symbol()
    }
    fn get_grouping_symbol(&self) -> Vec<u32> {
        self.locale.get_grouping_symbol()
    }
    fn get_percent_symbol(&self) -> Vec<u32> {
        self.locale.get_percent_symbol()
    }
    fn get_minus_symbol(&self) -> Vec<u32> {
        self.locale.get_minus_symbol()
    }
    fn get_currency_symbol(&self) -> Vec<u32> {
        self.locale.get_currency_symbol()
    }
    fn get_date_time_symbols(&self) -> Vec<u32> {
        self.locale.get_date_time_symbols()
    }
    fn get_month_name(&self, month: i32, abbreviated: bool) -> Vec<u32> {
        self.locale.get_month_name(month, abbreviated)
    }
    fn get_day_name(&self, week: i32, abbreviated: bool) -> Vec<u32> {
        self.locale.get_day_name(week, abbreviated)
    }
    fn get_meridiem_name(&self, am: bool) -> Vec<u32> {
        self.locale.get_meridiem_name(am)
    }
    fn get_time_zone_in_minutes(&self) -> i32 {
        self.timezone.get()
    }
    fn get_era_name(&self, ad: bool) -> Vec<u32> {
        self.locale.get_era_name(ad)
    }
    fn get_date_pattern(&self, category: DateTimeSubcategory) -> Vec<u32> {
        self.locale.get_date_pattern(category)
    }
    fn get_time_pattern(&self, category: DateTimeSubcategory) -> Vec<u32> {
        self.locale.get_time_pattern(category)
    }
    fn get_num_pattern(&self, category: NumSubcategory) -> Vec<u32> {
        self.locale.get_num_pattern(category)
    }
}
pub(super) fn locale_host(context: &Context) -> Option<FormcalcLocaleHost> {
    let runtime = CfxJseEngine::from_context(context)?;
    let manager = runtime.document().get_locale_mgr()?;
    let selected = match runtime.current_this_node() {
        Some(node) => node.get_locale(),
        None => manager.get_def_locale(),
    };
    let timezone = Rc::new(Cell::new(0));
    let manager = Rc::new(DocumentLocaleMgr {
        manager,
        selected: RefCell::new(selected),
        timezone: timezone.clone(),
    });
    let host = FormcalcLocaleHost { manager, timezone };
    host.refresh_time_zone(context);
    Some(host)
}
#[cfg(test)]
#[path = "formcalc_document_tests.rs"]
mod tests;
