// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Adapter from the real FFDoc/App/Notify services to CJX_HostPseudoModel.
//! Engine host ownership is weak back to FFDoc, preventing a runtime cycle.
use super::{cxfa_ffdoc::CxfaFfDoc, fxfa::XfaPrintOpt, parser::cxfa_node::CxfaNode};
use crate::fxjs::xfa::cjx_hostpseudomodel::XfaHost;
use boa_engine::{JsNativeError, JsResult};
use std::rc::{Rc, Weak};
pub struct CxfaFfHost {
    document: Weak<CxfaFfDoc>,
}
impl CxfaFfHost {
    pub fn new(document: Weak<CxfaFfDoc>) -> Self {
        Self { document }
    }
    fn doc(&self) -> JsResult<Rc<CxfaFfDoc>> {
        self.document.upgrade().ok_or_else(|| {
            JsNativeError::reference()
                .with_message("XFA form document has expired")
                .into()
        })
    }
}
impl XfaHost for CxfaFfHost {
    fn calculations_enabled(&self) -> JsResult<bool> {
        Ok(self.doc()?.is_calculations_enabled())
    }
    fn set_calculations_enabled(&self, enabled: bool) -> JsResult<()> {
        self.doc()?.set_calculations_enabled(enabled);
        Ok(())
    }
    fn validations_enabled(&self) -> JsResult<bool> {
        Ok(self.doc()?.is_validations_enabled())
    }
    fn set_validations_enabled(&self, enabled: bool) -> JsResult<()> {
        self.doc()?.set_validations_enabled(enabled);
        Ok(())
    }
    fn current_page(&self) -> JsResult<i32> {
        Ok(self.doc()?.get_current_page())
    }
    fn set_current_page(&self, page: i32) -> JsResult<()> {
        self.doc()?.set_current_page(page);
        Ok(())
    }
    fn num_pages(&self) -> JsResult<i32> {
        Ok(self.doc()?.count_pages())
    }
    fn language(&self) -> JsResult<Vec<u32>> {
        Ok(self.doc()?.app().provider().get_language())
    }
    fn platform(&self) -> JsResult<Vec<u32>> {
        Ok(self.doc()?.app().provider().get_platform())
    }
    fn app_name(&self) -> JsResult<Vec<u32>> {
        Ok(self.doc()?.app().provider().get_app_name())
    }
    fn title(&self) -> JsResult<Vec<u32>> {
        Ok(self.doc()?.get_title())
    }
    fn set_title(&self, title: &[u32]) -> JsResult<()> {
        self.doc()?.set_title(title);
        Ok(())
    }
    fn goto_url(&self, url: &[u32]) -> JsResult<()> {
        self.doc()?.goto_url(url);
        Ok(())
    }
    fn open_list(&self, node: &CxfaNode) -> JsResult<()> {
        self.doc()?.notify().open_drop_down_list(node)
    }
    fn get_focus(&self) -> JsResult<Option<CxfaNode>> {
        Ok(self.doc()?.notify().get_focus_widget_node())
    }
    fn set_focus(&self, node: Option<&CxfaNode>) -> JsResult<()> {
        self.doc()?
            .notify()
            .set_focus_widget_node(node)
            .map_err(|error| {
                JsNativeError::typ()
                    .with_message(format!("XFA focus failed: {error:?}"))
                    .into()
            })
    }
    fn reset_data(&self, node: Option<&CxfaNode>) -> JsResult<()> {
        self.doc()?.notify().reset_data(node);
        Ok(())
    }
    fn beep(&self, kind: u32) -> JsResult<()> {
        self.doc()?.app().provider().beep(kind);
        Ok(())
    }
    fn message_box(
        &self,
        message: &[u32],
        title: &[u32],
        icon: u32,
        buttons: u32,
    ) -> JsResult<i32> {
        Ok(self
            .doc()?
            .app()
            .provider()
            .message_box(message, title, icon, buttons))
    }
    fn response(
        &self,
        question: &[u32],
        title: &[u32],
        answer: &[u32],
        masked: bool,
    ) -> JsResult<Vec<u32>> {
        Ok(self
            .doc()?
            .app()
            .provider()
            .response(question, title, answer, masked))
    }
    fn print(&self, start: i32, end: i32, options: XfaPrintOpt) -> JsResult<()> {
        self.doc()?.print(start, end, options);
        Ok(())
    }
    fn export_data(&self, path: &[u32], xdp: bool) -> JsResult<()> {
        self.doc()?.export_data(path, xdp);
        Ok(())
    }
}

impl crate::fxjs::xfa::formcalc_network::FormcalcNetworkHost for CxfaFfHost {
    fn download_url(
        &self,
        url: &[u32],
    ) -> JsResult<Option<Rc<dyn crate::fxcrt::fx_stream::SeekableReadStream>>> {
        Ok(self.doc()?.app().provider().download_url(url))
    }
    fn post_request_url(
        &self,
        url: &[u32],
        data: &[u32],
        kind: &[u32],
        encoding: &[u32],
        header: &[u32],
    ) -> JsResult<Option<Vec<u32>>> {
        Ok(self
            .doc()?
            .app()
            .provider()
            .post_request_url(url, data, kind, encoding, header))
    }
    fn put_request_url(&self, url: &[u32], data: &[u32], encoding: &[u32]) -> JsResult<bool> {
        Ok(self
            .doc()?
            .app()
            .provider()
            .put_request_url(url, data, encoding))
    }
}
