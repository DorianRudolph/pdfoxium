// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2021 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `xfa/fwl/cfwl_themebackground.{h,cpp}`. Drawing receives the graphics device
//! as a separate mutable argument, so parameters only contain shared borrows.
use super::{
    cfwl_themepart::{CfwlThemePart, Part},
    cfwl_widget::FwlWidget,
};
use crate::xfa::fgas::graphics::cfgas_gepath::CfgasGePath;
use std::ops::{Deref, DerefMut};
#[derive(Clone)]
pub struct CfwlThemeBackground<'a> {
    pub base: CfwlThemePart<'a>,
    pub path: Option<&'a CfgasGePath>,
}
impl<'a> CfwlThemeBackground<'a> {
    pub fn new(part: Part, widget: &'a dyn FwlWidget) -> Self {
        Self {
            base: CfwlThemePart::new(part, widget),
            path: None,
        }
    }
    pub fn get_path(&self) -> Option<&CfgasGePath> {
        self.path
    }
    pub fn set_path(&mut self, path: Option<&'a CfgasGePath>) {
        self.path = path;
    }
}
impl<'a> Deref for CfwlThemeBackground<'a> {
    type Target = CfwlThemePart<'a>;
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}
impl DerefMut for CfwlThemeBackground<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.base
    }
}
