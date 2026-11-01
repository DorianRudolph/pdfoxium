// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cxfa_viewlayoutitem.{h,cpp}`.
use super::{
    cxfa_layoutitem::{CxfaLayoutItem, LayoutItemKind, LayoutItemPageView},
    cxfa_layoutprocessor::CxfaLayoutProcessor,
};
use crate::{
    fxcrt::fx_coordinates::CfxSizeF,
    xfa::fxfa::{
        fxfa_basic::{XfaAttribute, XfaAttributeValue, XfaElement, XfaUnit},
        parser::cxfa_node::CxfaNode,
    },
};
use std::{ops::Deref, rc::Rc};
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct CxfaViewLayoutItem(pub(crate) CxfaLayoutItem);
impl Deref for CxfaViewLayoutItem {
    type Target = CxfaLayoutItem;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl CxfaViewLayoutItem {
    pub fn page_view(&self) -> Option<Rc<dyn LayoutItemPageView>> {
        self.node.with_value(|data| match &data.kind {
            LayoutItemKind::View { page_view, .. } => page_view.clone(),
            _ => unreachable!(),
        })
    }
    pub fn set_old_subform(&self, node: Option<&CxfaNode>) {
        let store = node.and_then(CxfaNode::retain_store);
        self.node.with_value_mut(|data| match &mut data.kind {
            LayoutItemKind::View {
                old_subform,
                old_store,
                ..
            } => {
                *old_subform = node.cloned();
                *old_store = store;
            }
            _ => unreachable!(),
        });
    }
    pub fn old_subform(&self) -> Option<CxfaNode> {
        self.node.with_value(|data| match &data.kind {
            LayoutItemKind::View { old_subform, .. } => old_subform.clone(),
            _ => unreachable!(),
        })
    }
    pub fn get_layout(&self) -> Option<Rc<CxfaLayoutProcessor>> {
        let document = self.form_node().document()?;
        CxfaLayoutProcessor::from_document(&document)
    }
    pub fn get_page_index(&self) -> i32 {
        self.get_layout()
            .map_or(-1, |layout| layout.get_page_index(self))
    }
    pub fn get_page_size(&self) -> CfxSizeF {
        let Some(medium) = self
            .form_node()
            .get_first_child_by_class(XfaElement::Medium)
        else {
            return CfxSizeF::default();
        };
        let mut size = CfxSizeF::new(
            medium.get_measure_in_unit(XfaAttribute::Short, XfaUnit::Pt),
            medium.get_measure_in_unit(XfaAttribute::Long, XfaUnit::Pt),
        );
        if medium.get_enum(XfaAttribute::Orientation) == XfaAttributeValue::Landscape {
            std::mem::swap(&mut size.width, &mut size.height);
        }
        size
    }
    pub fn get_master_page(&self) -> CxfaNode {
        self.form_node()
    }
}
