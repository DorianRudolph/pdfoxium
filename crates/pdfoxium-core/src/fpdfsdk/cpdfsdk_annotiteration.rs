// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Stable layout/drawing ordering from `fpdfsdk/cpdfsdk_annotiteration.cpp`.
use super::cpdfsdk_annot::CpdfSdkAnnot;
use super::cpdfsdk_pageview::CpdfSdkPageView;
use std::rc::{Rc, Weak};
pub struct CpdfSdkAnnotIteration {
    list: Vec<Weak<CpdfSdkAnnot>>,
}
impl CpdfSdkAnnotIteration {
    pub fn new(view: &CpdfSdkPageView) -> Self {
        Self::ordered(view, false)
    }
    pub fn for_drawing(view: &CpdfSdkPageView) -> Self {
        Self::ordered(view, true)
    }
    fn ordered(view: &CpdfSdkPageView, focused_at_end: bool) -> Self {
        let mut list = view.annot_list();
        list.sort_by_key(|annot| annot.layout_order());
        if !focused_at_end {
            // Hit-test the painter's frontmost annotation first. Native uses
            // the drawing order here too, targeting covered widgets despite
            // the public z-order query identifying the visible front widget.
            list.reverse();
        }
        if let Some(focused) = view.focus_annot() {
            if let Some(index) = list.iter().position(|annot| Rc::ptr_eq(annot, &focused)) {
                let focused = list.remove(index);
                let index = if focused_at_end { list.len() } else { 0 };
                list.insert(index, focused);
            }
        }
        Self {
            list: list.iter().map(Rc::downgrade).collect(),
        }
    }
    pub fn iter(&self) -> std::slice::Iter<'_, Weak<CpdfSdkAnnot>> {
        self.list.iter()
    }
}
