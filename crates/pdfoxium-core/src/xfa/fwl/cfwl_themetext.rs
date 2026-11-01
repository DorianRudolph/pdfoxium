// Copyright 2021 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `xfa/fwl/cfwl_themetext.{h,cpp}`.
use super::{
    cfwl_themepart::{CfwlThemePart, Part},
    cfwl_widget::FwlWidget,
};
use crate::xfa::fde::cfde_data::{FdeTextAlignment, FdeTextStyle};
use std::ops::{Deref, DerefMut};
#[derive(Clone)]
pub struct CfwlThemeText<'a> {
    pub base: CfwlThemePart<'a>,
    pub tto_align: FdeTextAlignment,
    pub tto_styles: FdeTextStyle,
    pub text: Vec<u32>,
}
impl<'a> CfwlThemeText<'a> {
    pub fn new(part: Part, widget: &'a dyn FwlWidget) -> Self {
        Self {
            base: CfwlThemePart::new(part, widget),
            tto_align: FdeTextAlignment::TopLeft,
            tto_styles: FdeTextStyle::default(),
            text: Vec::new(),
        }
    }
}
impl<'a> Deref for CfwlThemeText<'a> {
    type Target = CfwlThemePart<'a>;
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}
impl DerefMut for CfwlThemeText<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.base
    }
}
