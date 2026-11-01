// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Native named lookup from `cxfa_treelist.{h,cpp}`. `fxjs::xfa::cjx_list`
//! supplies its script wrapper; native lookup retains source hash equality.
use super::{
    cxfa_list::CxfaList,
    cxfa_node::{hash_name, CxfaNode},
};
pub trait CxfaTreeList: CxfaList {
    fn named_item(&self, name: &[u32]) -> Option<CxfaNode> {
        let hash = hash_name(name);
        for index in 0..self.get_length() {
            if let Some(node) = self.item(index) {
                if node.get_name_hash() == hash {
                    return Some(node);
                }
            }
        }
        None
    }
}
impl<T: CxfaList + ?Sized> CxfaTreeList for T {}
