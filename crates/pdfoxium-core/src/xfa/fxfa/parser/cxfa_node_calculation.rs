// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! The per-object CalcData and calculation recursion members of CJX_Object.
//! Node handles are non-owning; the document arena retains the actual nodes.
use super::*;
impl CxfaNode {
    pub fn calculation_dependents(&self) -> Option<Vec<Self>> {
        self.0.with_value(|node| node.calc_data.clone())
    }
    pub fn add_calculation_dependent(&self, dependent: &Self) {
        self.0.with_value_mut(|node| {
            let dependencies = node.calc_data.get_or_insert_with(Vec::new);
            if !dependencies.contains(dependent) {
                dependencies.push(dependent.clone());
            }
        });
    }
    /// Native TakeCalcDataFrom replaces the destination, even when the source
    /// has no CalcData. Ordinary Clone/MergeAllData do not copy this state.
    pub fn take_calculation_data_from(&self, source: &Self) -> bool {
        if !self.is_alive() || !source.is_alive() {
            return false;
        }
        if self == source {
            return true;
        }
        let data = source.0.with_value_mut(|node| node.calc_data.take());
        let old = self
            .0
            .with_value_mut(|node| std::mem::replace(&mut node.calc_data, data));
        drop(old);
        true
    }
    pub fn get_calc_recursion_count(&self) -> usize {
        self.0.with_value(|node| node.calc_recursion_count)
    }
    pub fn set_calc_recursion_count(&self, count: usize) {
        self.0
            .with_value_mut(|node| node.calc_recursion_count = count);
    }
}
#[cfg(test)]
#[path = "cxfa_node_calculation_tests.rs"]
mod tests;
