// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fxcrt/bytestring_pool.{h,cpp}`. Interned byte strings share an
//! allocation while the pool lives; callers own their copies afterward.

use std::collections::HashSet;
use std::rc::Rc;

#[derive(Default)]
pub struct ByteStringPool {
    pool: HashSet<Rc<[u8]>>,
}

impl ByteStringPool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn intern(&mut self, value: &[u8]) -> Rc<[u8]> {
        if value.is_empty() {
            return Rc::from(value);
        }
        if let Some(existing) = self.pool.get(value) {
            return Rc::clone(existing);
        }
        let value: Rc<[u8]> = Rc::from(value);
        self.pool.insert(Rc::clone(&value));
        value
    }

    /// Native ByteString inputs already own shared storage. Retain the first
    /// input allocation when the caller supplies that owned representation.
    pub fn intern_shared(&mut self, value: &Rc<[u8]>) -> Rc<[u8]> {
        if value.is_empty() {
            return Rc::clone(value);
        }
        if let Some(existing) = self.pool.get(value.as_ref()) {
            return Rc::clone(existing);
        }
        self.pool.insert(Rc::clone(value));
        Rc::clone(value)
    }
}

#[cfg(test)]
#[path = "bytestring_pool_unittest.rs"]
mod upstream_tests;
