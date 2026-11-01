// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Structure/row/column tab traversal from `fpdfsdk/cpdfsdk_annotiterator.cpp`.
use super::cpdfsdk_annot::CpdfSdkAnnot;
use super::cpdfsdk_pageview::CpdfSdkPageView;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfdoc::cpdf_annot::Subtype;
use std::cmp::Ordering;
use std::rc::{Rc, Weak};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TabOrder {
    Structure,
    Row,
    Column,
}
pub struct CpdfSdkAnnotIterator {
    annots: Vec<Weak<CpdfSdkAnnot>>,
}
impl CpdfSdkAnnotIterator {
    pub fn new(view: &CpdfSdkPageView, subtypes: &[Subtype]) -> Self {
        let tab_order = view
            .page()
            .and_then(|page| {
                let dict = page.pdf_dict()?;
                let borrowed = dict.borrow();
                let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
                Some(match dict.get_byte_string_for(b"Tabs", b"").as_slice() {
                    b"R" => TabOrder::Row,
                    b"C" => TabOrder::Column,
                    _ => TabOrder::Structure,
                })
            })
            .unwrap_or(TabOrder::Structure);
        let annots = view
            .annot_list()
            .into_iter()
            .filter(|annot| {
                subtypes.contains(&annot.subtype())
                    && annot.optional_content_visible()
                    && !annot
                        .widget()
                        .is_some_and(|widget| widget.is_signature_widget())
            })
            .collect();
        Self {
            annots: order_annots(annots, tab_order)
                .iter()
                .map(Rc::downgrade)
                .collect(),
        }
    }
    pub fn first_annot(&self) -> Option<Rc<CpdfSdkAnnot>> {
        self.annots.first()?.upgrade().filter(|a| a.is_alive())
    }
    pub fn last_annot(&self) -> Option<Rc<CpdfSdkAnnot>> {
        self.annots.last()?.upgrade().filter(|a| a.is_alive())
    }
    pub fn next_annot(&self, annot: &Rc<CpdfSdkAnnot>) -> Option<Rc<CpdfSdkAnnot>> {
        let index = self
            .annots
            .iter()
            .position(|a| a.ptr_eq(&Rc::downgrade(annot)))?;
        self.annots
            .get(index + 1)?
            .upgrade()
            .filter(|a| a.is_alive())
    }
    pub fn prev_annot(&self, annot: &Rc<CpdfSdkAnnot>) -> Option<Rc<CpdfSdkAnnot>> {
        let index = self
            .annots
            .iter()
            .position(|a| a.ptr_eq(&Rc::downgrade(annot)))?;
        self.annots
            .get(index.checked_sub(1)?)?
            .upgrade()
            .filter(|a| a.is_alive())
    }
}
fn order_annots(mut remaining: Vec<Rc<CpdfSdkAnnot>>, order: TabOrder) -> Vec<Rc<CpdfSdkAnnot>> {
    if order == TabOrder::Structure {
        return remaining;
    }
    remaining.sort_unstable_by(|a, b| {
        match order {
            TabOrder::Row => a.rect().left.partial_cmp(&b.rect().left),
            _ => b.rect().top.partial_cmp(&a.rect().top),
        }
        .unwrap_or(Ordering::Equal)
    });
    let mut annots = Vec::with_capacity(remaining.len());
    while !remaining.is_empty() {
        // The native 0/-1 coordinate sentinels can loop forever or choose a
        // different column for valid negative-coordinate pages. Track whether
        // a candidate exists rather than assuming coordinates are positive.
        let mut selected = None;
        let mut coordinate = 0.0;
        for index in (0..remaining.len()).rev() {
            let rect = remaining[index].rect();
            let value = if order == TabOrder::Row {
                rect.top
            } else {
                rect.left
            };
            if selected.is_none()
                || if order == TabOrder::Row {
                    value > coordinate
                } else {
                    value < coordinate
                }
            {
                selected = Some(index);
                coordinate = value;
            }
        }
        let first = remaining.remove(selected.expect("nonempty annotations"));
        let rect = first.rect();
        annots.push(first);
        let selected: Vec<usize> = remaining
            .iter()
            .enumerate()
            .filter_map(|(index, annot)| {
                let candidate = annot.rect();
                let within = if order == TabOrder::Row {
                    let middle = (candidate.top + candidate.bottom) / 2.0;
                    middle > rect.bottom && middle < rect.top
                } else {
                    let middle = (candidate.left + candidate.right) / 2.0;
                    middle > rect.left && middle < rect.right
                };
                within.then_some(index)
            })
            .collect();
        for &index in &selected {
            annots.push(remaining[index].clone());
        }
        for index in selected.into_iter().rev() {
            remaining.remove(index);
        }
    }
    annots
}
