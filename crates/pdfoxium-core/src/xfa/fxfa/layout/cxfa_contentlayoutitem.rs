// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cxfa_contentlayoutitem.{h,cpp}`: weak fragment chains and page-relative bounds.
use super::cxfa_layoutitem::{CxfaLayoutItem, LayoutItemKind, LayoutItemWidget};
use crate::{
    fxcrt::fx_coordinates::{CfxPointF, CfxRectF, CfxSizeF},
    xfa::fxfa::{
        fxfa::XfaWidgetStatus,
        fxfa_basic::{XfaAttribute, XfaElement, XfaUnit},
    },
};
use std::{ops::Deref, rc::Rc};

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub struct CxfaContentLayoutItem(pub(crate) CxfaLayoutItem);
impl Deref for CxfaContentLayoutItem {
    type Target = CxfaLayoutItem;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl CxfaContentLayoutItem {
    pub fn position(&self) -> CfxPointF {
        self.node.with_value(|data| match &data.kind {
            LayoutItemKind::Content { position, .. } => *position,
            _ => unreachable!(),
        })
    }
    pub fn set_position(&self, value: CfxPointF) {
        self.node.with_value_mut(|data| match &mut data.kind {
            LayoutItemKind::Content { position, .. } => *position = value,
            _ => unreachable!(),
        });
    }
    pub fn size(&self) -> CfxSizeF {
        self.node.with_value(|data| match &data.kind {
            LayoutItemKind::Content { size, .. } => *size,
            _ => unreachable!(),
        })
    }
    pub fn set_size(&self, value: CfxSizeF) {
        self.node.with_value_mut(|data| match &mut data.kind {
            LayoutItemKind::Content { size, .. } => *size = value,
            _ => unreachable!(),
        });
    }
    pub fn widget(&self) -> Option<Rc<dyn LayoutItemWidget>> {
        self.node.with_value(|data| match &data.kind {
            LayoutItemKind::Content { widget, .. } => widget.clone(),
            _ => unreachable!(),
        })
    }
    pub fn previous(&self) -> Option<Self> {
        self.node.with_value(|data| match &data.kind {
            LayoutItemKind::Content { previous, .. } => {
                previous.clone().filter(|item| item.is_alive())
            }
            _ => unreachable!(),
        })
    }
    pub fn next(&self) -> Option<Self> {
        self.node.with_value(|data| match &data.kind {
            LayoutItemKind::Content { next, .. } => next.clone().filter(|item| item.is_alive()),
            _ => unreachable!(),
        })
    }
    fn set_previous(&self, value: Option<Self>) {
        self.node.with_value_mut(|data| match &mut data.kind {
            LayoutItemKind::Content { previous, .. } => *previous = value,
            _ => unreachable!(),
        });
    }
    fn set_next(&self, value: Option<Self>) {
        self.node.with_value_mut(|data| match &mut data.kind {
            LayoutItemKind::Content { next, .. } => *next = value,
            _ => unreachable!(),
        });
    }
    pub fn get_first(&self) -> Self {
        let mut item = self.clone();
        while let Some(previous) = item.previous() {
            item = previous;
        }
        item
    }
    pub fn get_last(&self) -> Self {
        let mut item = self.clone();
        while let Some(next) = item.next() {
            item = next;
        }
        item
    }
    pub fn get_index(&self) -> usize {
        let mut count = 0;
        let mut item = self.previous();
        while let Some(previous) = item {
            count += 1;
            item = previous.previous();
        }
        count
    }
    /// Moves a fragment from any existing fragment chain. Tree membership is
    /// independent, exactly as in the native InsertAfter operation.
    pub fn insert_after(&self, item: &Self) {
        assert_ne!(self, item, "fragment self-insertion");
        assert!(
            Rc::ptr_eq(
                &self.store().expect("layout arena expired"),
                &item.store().expect("layout arena expired")
            ),
            "fragment arenas differ"
        );
        item.remove_from_fragment_chain();
        let next = self.next();
        item.set_next(next.clone());
        item.set_previous(Some(self.clone()));
        self.set_next(Some(item.clone()));
        if let Some(next) = next {
            next.set_previous(Some(item.clone()));
        }
    }
    pub(crate) fn remove_from_fragment_chain(&self) {
        let previous = self.previous();
        let next = self.next();
        if let Some(next) = &next {
            next.set_previous(previous.clone());
        }
        if let Some(previous) = previous {
            previous.set_next(next);
        }
        self.set_previous(None);
        self.set_next(None);
    }
    pub fn get_relative_rect(&self) -> CfxRectF {
        CfxRectF::from_point_size(self.position(), self.size())
    }
    pub fn get_absolute_rect(&self) -> CfxRectF {
        let mut rect = self.get_relative_rect();
        let mut parent = self.parent();
        while let Some(item) = parent {
            let node = item.form_node();
            if let Some(content) = item.as_content_layout_item() {
                let position = content.position();
                rect.offset(position.x, position.y);
                if let Some(margin) = node.get_first_child_by_class(XfaElement::Margin) {
                    rect.offset(
                        margin.get_measure_in_unit(XfaAttribute::LeftInset, XfaUnit::Pt),
                        margin.get_measure_in_unit(XfaAttribute::TopInset, XfaUnit::Pt),
                    );
                }
            } else if node.get_element_type() == XfaElement::ContentArea {
                rect.offset(
                    node.get_measure_in_unit(XfaAttribute::X, XfaUnit::Pt),
                    node.get_measure_in_unit(XfaAttribute::Y, XfaUnit::Pt),
                );
                break;
            } else if node.get_element_type() == XfaElement::PageArea {
                break;
            }
            parent = item.parent();
        }
        rect
    }
    pub fn status(&self) -> XfaWidgetStatus {
        self.node.with_value(|data| match &data.kind {
            LayoutItemKind::Content { status, .. } => *status,
            _ => unreachable!(),
        })
    }
    pub fn set_status_bits(&self, value: XfaWidgetStatus) {
        self.node.with_value_mut(|data| match &mut data.kind {
            LayoutItemKind::Content { status, .. } => *status |= value,
            _ => unreachable!(),
        });
    }
    pub fn clear_status_bits(&self, value: XfaWidgetStatus) {
        self.node.with_value_mut(|data| match &mut data.kind {
            LayoutItemKind::Content { status, .. } => *status &= !value,
            _ => unreachable!(),
        });
    }
    pub fn test_status_bits(&self, value: XfaWidgetStatus) -> bool {
        self.status().contains(value)
    }
}
