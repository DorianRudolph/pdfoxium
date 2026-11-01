// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Native foreground XFA click/full/dropdown transactions from
//! `cffl_interactiveformfiller.cpp`; no collection borrow spans script dispatch.
use super::*;
use crate::fpdfsdk::cpdfsdk_widget_xfa::XfaAActionType;
impl CfflInteractiveFormFiller {
    pub(crate) fn on_xfa_action(
        &self,
        widget: &Rc<CpdfSdkWidget>,
        view: &Rc<CpdfSdkPageView>,
        kind: XfaAActionType,
        flags: u32,
    ) -> bool {
        if self.notifying.get() || !widget.has_xfa_action(kind) {
            return false;
        }
        let (age, value_age) = (widget.appearance_age(), widget.value_age());
        {
            let _guard = NotifyGuard::new(&self.notifying);
            let mut data = Self::flags(flags);
            widget.on_xfa_action(kind, &mut data, view);
        }
        if !Self::is_valid_annot(view, widget) {
            return true;
        }
        if age == widget.appearance_age() {
            return false;
        }
        if let Some(field) = self.get_form_field(widget) {
            field
                .base()
                .reset_pwl_window_for_value_age(view, widget, value_age);
        }
        kind != XfaAActionType::Click
    }
}
