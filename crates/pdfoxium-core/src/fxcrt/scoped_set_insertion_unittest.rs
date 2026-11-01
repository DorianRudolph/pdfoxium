// Copyright 2021 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! core/fxcrt/scoped_set_insertion_unittest.cpp, exercised through the actual
//! selected parser's ScopedSetInsertion equivalent and its actual visited set.
#![allow(non_snake_case)]
use super::CpdfParser;
use std::collections::BTreeSet;

#[test]
fn ScopedSetInsertion() {
    let parser = CpdfParser::new();
    let container = &parser.state.parsing_obj_nums;
    {
        let _insertion = parser.state.guard(5).unwrap();
        assert_eq!(*container.borrow(), BTreeSet::from([5]));
        {
            let _insertion2 = parser.state.guard(6).unwrap();
            assert_eq!(*container.borrow(), BTreeSet::from([5, 6]));
        }
        assert_eq!(*container.borrow(), BTreeSet::from([5]));
    }
    assert!(container.borrow().is_empty());
}

#[test]
fn rejected_recursive_insertion_preserves_the_active_guard() {
    let parser = CpdfParser::new();
    let _active = parser.state.guard(5).unwrap();
    assert!(parser.state.guard(5).is_none());
    assert_eq!(*parser.state.parsing_obj_nums.borrow(), BTreeSet::from([5]));
    assert!(parser.state.guard(5).is_none());
}
