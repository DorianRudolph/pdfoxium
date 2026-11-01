// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! A declaration shared by the selectors in a source style rule.
use super::cfx_cssdeclaration::CfxCssDeclaration;
use super::cfx_cssselector::CfxCssSelector;
use std::rc::Rc;

#[derive(Debug, Default)]
pub struct CfxCssStyleRule {
    pub selectors: Vec<Rc<CfxCssSelector>>,
    pub declaration: Rc<CfxCssDeclaration>,
}
impl CfxCssStyleRule {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn count_selector_lists(&self) -> usize {
        self.selectors.len()
    }
    pub fn get_selector_list(&self, index: usize) -> Option<&CfxCssSelector> {
        self.selectors.get(index).map(Rc::as_ref)
    }
    pub fn get_declaration(&self) -> &CfxCssDeclaration {
        &self.declaration
    }
    pub fn get_declaration_mut(&mut self) -> &mut CfxCssDeclaration {
        Rc::make_mut(&mut self.declaration)
    }
    pub fn set_selector(&mut self, selectors: &mut Vec<Rc<CfxCssSelector>>) {
        debug_assert!(self.selectors.is_empty());
        std::mem::swap(&mut self.selectors, selectors);
    }
}
