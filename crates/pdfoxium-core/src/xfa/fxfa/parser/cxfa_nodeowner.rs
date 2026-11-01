// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cxfa_nodeowner.{h,cpp}`. Retained Rust list owners replace cppgc tracing.
use super::cxfa_list::CxfaList;
use std::{cell::RefCell, rc::Rc};
#[derive(Default)]
pub struct CxfaNodeOwner {
    lists: RefCell<Vec<Rc<dyn CxfaList>>>,
}
impl CxfaNodeOwner {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn persist_list(&self, list: Rc<dyn CxfaList>) {
        self.lists.borrow_mut().push(list);
    }
    pub fn persisted_list_count(&self) -> usize {
        self.lists.borrow().len()
    }
}
