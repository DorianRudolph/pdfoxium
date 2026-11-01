// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Shared wchar32 storage for source WideString copies. Rc<Vec<_>> preserves
//! O(1) copies and takes ownership of parser Vec buffers without copying them.

use std::borrow::Borrow;
use std::cmp::Ordering;
use std::ops::Deref;
use std::rc::Rc;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct XmlString(Rc<Vec<u32>>);

// WideString::Compare calls FXSYS_wmemcmp, which compares signed wchar32
// values on the selected targets. Raw u32 storage must preserve that ordering,
// including code units outside Unicode's range supplied by direct DOM callers.
fn compare_units(left: &[u32], right: &[u32]) -> Ordering {
    left.iter()
        .map(|&unit| unit as i32)
        .cmp(right.iter().map(|&unit| unit as i32))
}

/// An allocation-free ordered view used for BTreeMap lookup. Borrow<[u32]>
/// would violate Borrow's ordering contract because slices use unsigned Ord.
pub trait XmlStringKey {
    fn units(&self) -> &[u32];
}

pub struct XmlStringView<'a>(pub &'a [u32]);

impl XmlStringKey for XmlStringView<'_> {
    fn units(&self) -> &[u32] {
        self.0
    }
}
impl XmlStringKey for XmlString {
    fn units(&self) -> &[u32] {
        self
    }
}
impl PartialEq for dyn XmlStringKey + '_ {
    fn eq(&self, other: &Self) -> bool {
        self.units() == other.units()
    }
}
impl Eq for dyn XmlStringKey + '_ {}
impl PartialOrd for dyn XmlStringKey + '_ {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for dyn XmlStringKey + '_ {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_units(self.units(), other.units())
    }
}
impl PartialOrd for XmlString {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for XmlString {
    fn cmp(&self, other: &Self) -> Ordering {
        compare_units(self, other)
    }
}
impl XmlString {
    pub fn shares_storage_with(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}
impl From<Vec<u32>> for XmlString {
    fn from(value: Vec<u32>) -> Self {
        Self(Rc::new(value))
    }
}
impl From<&[u32]> for XmlString {
    fn from(value: &[u32]) -> Self {
        Self::from(value.to_vec())
    }
}
impl Deref for XmlString {
    type Target = [u32];
    fn deref(&self) -> &[u32] {
        self.0.as_slice()
    }
}
impl AsRef<[u32]> for XmlString {
    fn as_ref(&self) -> &[u32] {
        self
    }
}
impl<'a> Borrow<dyn XmlStringKey + 'a> for XmlString {
    fn borrow(&self) -> &(dyn XmlStringKey + 'a) {
        self
    }
}
