// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Actual FF widget forwarding from `cxfa_fwladapterwidgetmgr.{h,cpp}`.
use crate::{
    fxcrt::fx_coordinates::CfxRectF,
    xfa::fwl::{cfwl_widget::WidgetRef, cfwl_widgetmgr::WidgetMgrAdapter},
};
#[derive(Default)]
pub struct CxfaFwlAdapterWidgetMgr;
impl CxfaFwlAdapterWidgetMgr {
    pub fn new() -> Self {
        Self
    }
}
impl WidgetMgrAdapter for CxfaFwlAdapterWidgetMgr {
    fn repaint_widget(&self, widget: &WidgetRef) {
        if let Some(ff) = widget
            .base()
            .adapter_iface()
            .and_then(|adapter| adapter.xfa_widget())
        {
            ff.widget().invalidate_rect();
        }
    }
    fn get_popup_pos(
        &self,
        widget: &WidgetRef,
        min: f32,
        max: f32,
        anchor: CfxRectF,
        popup: &mut CfxRectF,
    ) -> bool {
        let Some(ff) = widget
            .base()
            .adapter_iface()
            .and_then(|adapter| adapter.xfa_widget())
        else {
            return false;
        };
        let Some(host) = ff.widget().doc_view() else {
            return false;
        };
        let anchor = ff.get_rotate_matrix().transform_rect(anchor);
        // Native adapter reports that the FF placement path was handled even
        // when the embedding callback itself returns false.
        host.get_popup_pos(ff.as_ref(), min, max, anchor, popup);
        true
    }
}
