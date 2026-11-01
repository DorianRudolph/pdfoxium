// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Safe counterpart of CPDFSDK_PauseAdapter. The C boundary captures the
//! IFSDK_PAUSE pointer; core/SDK code receives only an ordinary Rust callback.
use crate::fxcrt::pauseindicator_iface::PauseIndicatorIface;
pub struct CpdfsdkPauseAdapter<'a> {
    callback: Option<&'a mut dyn FnMut() -> bool>,
}
impl<'a> CpdfsdkPauseAdapter<'a> {
    pub fn new(callback: Option<&'a mut dyn FnMut() -> bool>) -> Self {
        Self { callback }
    }
}
impl PauseIndicatorIface for CpdfsdkPauseAdapter<'_> {
    fn need_to_pause_now(&mut self) -> bool {
        self.callback
            .as_deref_mut()
            .is_some_and(|callback| callback())
    }
}
