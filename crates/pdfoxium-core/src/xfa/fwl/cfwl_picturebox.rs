// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `xfa/fwl/cfwl_picturebox.{h,cpp}`. The source widget draws its optional
//! border; its XFA host supplies the image itself.
use super::{
    cfwl_app::CfwlApp,
    cfwl_themepart::Part,
    cfwl_widget::{CfwlWidget, FwlType, FwlWidget, Properties, WidgetRef},
};
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxRectF},
    xfa::fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
};
use std::{cell::Cell, rc::Rc};

pub struct CfwlPictureBox {
    base: CfwlWidget,
    client_rect: Cell<CfxRectF>,
}
impl CfwlPictureBox {
    pub fn new(app: &Rc<CfwlApp>) -> Rc<Self> {
        Self::with_properties(app, Properties::default(), None)
    }
    pub fn with_properties(
        app: &Rc<CfwlApp>,
        properties: Properties,
        outer: Option<WidgetRef>,
    ) -> Rc<Self> {
        let control = Rc::new(Self {
            base: CfwlWidget::new(app, properties, outer),
            client_rect: Cell::new(CfxRectF::default()),
        });
        app.register_widget(control.clone() as WidgetRef);
        control
    }
    pub fn client_rect(&self) -> CfxRectF {
        self.client_rect.get()
    }
}
impl FwlWidget for CfwlPictureBox {
    fn base(&self) -> &CfwlWidget {
        &self.base
    }
    fn get_class_id(&self) -> FwlType {
        FwlType::PictureBox
    }
    fn update(&self) {
        if !self.base.is_locked() {
            self.client_rect.set(self.get_client_rect());
        }
    }
    fn draw_widget(&self, graphics: &mut CfgasGeGraphics<'_, '_, '_>, matrix: CfxMatrix) {
        if self.base.has_border() {
            self.base.draw_border(graphics, Part::Border, matrix);
        }
    }
}
