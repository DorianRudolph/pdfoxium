// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cxfa_ffapp.{h,cpp}`. The app owns its FWL/theme/font services; their
//! back-references are weak. Embedding capabilities are supplied explicitly.
use super::{
    cxfa_ffdoc::CxfaFfDoc, cxfa_fontmgr::CxfaFontMgr,
    cxfa_fwladapterwidgetmgr::CxfaFwlAdapterWidgetMgr, cxfa_fwltheme::CxfaFwlTheme,
};
use crate::{
    fxcrt::{cfx_timer::TimerHandler, fx_codepage::FxCodePage, fx_stream::SeekableReadStream},
    xfa::{
        fgas::font::{
            cfgas_defaultfontmanager::CfgasDefaultFontManager, cfgas_gefont::CfgasGeFont,
        },
        fwl::{
            cfwl_app::{AppAdapter, CfwlApp},
            cfwl_widgetmgr::WidgetMgrAdapter,
            ifwl_themeprovider::ThemeProvider,
        },
    },
};
use std::{
    cell::{OnceCell, RefCell},
    rc::{Rc, Weak},
};
/// The native application-provider surface, with no default UI or network
/// implementation. Called only after all library interior borrows are released.
pub trait XfaAppProvider {
    fn get_language(&self) -> Vec<u32>;
    fn get_platform(&self) -> Vec<u32>;
    fn get_app_name(&self) -> Vec<u32>;
    fn get_app_title(&self) -> Vec<u32>;
    fn beep(&self, kind: u32);
    fn message_box(&self, message: &[u32], title: &[u32], icon: u32, buttons: u32) -> i32;
    fn response(
        &self,
        question: &[u32],
        title: &[u32],
        default_answer: &[u32],
        masked: bool,
    ) -> Vec<u32>;
    fn download_url(&self, url: &[u32]) -> Option<Rc<dyn SeekableReadStream>>;
    fn post_request_url(
        &self,
        url: &[u32],
        data: &[u32],
        content_type: &[u32],
        encoding: &[u32],
        header: &[u32],
    ) -> Option<Vec<u32>>;
    fn put_request_url(&self, url: &[u32], data: &[u32], encoding: &[u32]) -> bool;
    fn timer_handler(&self) -> Option<Rc<dyn TimerHandler>>;
}
pub struct CxfaFfApp {
    provider: Rc<dyn XfaAppProvider>,
    font_mgr: Rc<CxfaFontMgr>,
    adapter: Rc<CxfaFwlAdapterWidgetMgr>,
    theme: Rc<RefCell<Option<Rc<CxfaFwlTheme>>>>,
    fwl_app: OnceCell<Rc<CfwlApp>>,
    self_ref: Weak<CxfaFfApp>,
}
struct FwlAdapter {
    provider: Rc<dyn XfaAppProvider>,
    adapter: Rc<CxfaFwlAdapterWidgetMgr>,
    theme: Rc<RefCell<Option<Rc<CxfaFwlTheme>>>>,
}
impl AppAdapter for FwlAdapter {
    fn widget_mgr_adapter(&self) -> Rc<dyn WidgetMgrAdapter> {
        self.adapter.clone()
    }
    fn timer_handler(&self) -> Option<Rc<dyn TimerHandler>> {
        self.provider.timer_handler()
    }
    fn theme_provider(&self) -> Rc<dyn ThemeProvider> {
        self.theme
            .borrow()
            .as_ref()
            .expect("FWL theme loaded before widget creation")
            .clone()
    }
}
impl CxfaFfApp {
    /// Like upstream, app creation precedes document/theme font resolution.
    /// A document with embedded fonts must not require installed host fonts.
    pub fn new(provider: Rc<dyn XfaAppProvider>) -> Rc<Self> {
        let app = Rc::new_cyclic(|weak| Self {
            provider,
            font_mgr: Rc::new(CxfaFontMgr::new()),
            adapter: Rc::new(CxfaFwlAdapterWidgetMgr::new()),
            theme: Rc::new(RefCell::new(None)),
            fwl_app: OnceCell::new(),
            self_ref: weak.clone(),
        });
        let fwl = CfwlApp::new(Rc::new(FwlAdapter {
            provider: app.provider.clone(),
            adapter: app.adapter.clone(),
            theme: app.theme.clone(),
        }));
        let _ = app.fwl_app.set(fwl);
        app
    }
    pub fn provider(&self) -> Rc<dyn XfaAppProvider> {
        self.provider.clone()
    }
    pub fn font_mgr(&self) -> Rc<CxfaFontMgr> {
        self.font_mgr.clone()
    }
    pub fn fwl_app(&self) -> Rc<CfwlApp> {
        self.fwl_app.get().expect("initialized FWL app").clone()
    }
    pub fn theme(&self) -> Rc<CxfaFwlTheme> {
        self.theme
            .borrow()
            .as_ref()
            .expect("FWL theme loaded before widget creation")
            .clone()
    }
    pub fn load_fwl_theme(&self, doc: &CxfaFfDoc) -> bool {
        let family: Vec<_> = "Helvetica".chars().map(u32::from).collect();
        let font = CfgasGeFont::load_font(Some(&family), 0, FxCodePage::DefAnsi)
            .or_else(|| CfgasDefaultFontManager::get_font(&family, 0))
            .or_else(|| CfgasDefaultFontManager::get_default_font(0))
            .or_else(|| self.font_mgr.get_font(&doc.pdf_font_mgr(), &family, 0));
        let Some(font) = font else {
            return false;
        };
        let theme = CxfaFwlTheme::new(self.self_ref.clone(), font);
        if !theme.load_calendar_font(doc) {
            return false;
        }
        let old = self.theme.replace(Some(theme));
        drop(old);
        true
    }
    pub fn title(&self) -> Vec<u32> {
        self.provider.get_app_title()
    }
    pub fn message_box(
        &self,
        message: &[u32],
        title: &[u32],
        icon: super::fxfa::AlertIcon,
        buttons: super::fxfa::AlertButton,
    ) -> Option<super::fxfa::AlertReturn> {
        use super::fxfa::AlertReturn;
        match self
            .provider
            .message_box(message, title, icon as u32, buttons as u32)
        {
            1 => Some(AlertReturn::kOK),
            2 => Some(AlertReturn::kCancel),
            3 => Some(AlertReturn::kNo),
            4 => Some(AlertReturn::kYes),
            _ => None,
        }
    }
}
