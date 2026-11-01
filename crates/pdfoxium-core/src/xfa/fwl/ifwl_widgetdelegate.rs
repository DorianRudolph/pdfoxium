// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Replaceable widget callbacks; the caller retains the small owner before
//! invoking host code, without a widget-manager/delegate RefCell borrow.
use super::{cfwl_event::FwlEvent, cfwl_message::FwlMessage};
use crate::{
    fxcrt::fx_coordinates::CfxMatrix, xfa::fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
};
pub trait WidgetDelegate {
    fn on_process_message(&self, message: &mut FwlMessage);
    fn on_process_event(&self, event: &mut FwlEvent);
    fn on_draw_widget(&self, graphics: &mut CfgasGeGraphics<'_, '_, '_>, matrix: CfxMatrix);
}
