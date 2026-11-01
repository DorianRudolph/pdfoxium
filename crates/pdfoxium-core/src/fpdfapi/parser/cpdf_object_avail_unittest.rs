// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/parser/cpdf_object_avail_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]
use super::*;
use crate::fpdfapi::parser::availability_test_support::{
    ObjectState::{Available, Unavailable},
    TestHolder,
};
use crate::fpdfapi::parser::{
    cpdf_array::CpdfArray, cpdf_dictionary::CpdfDictionary, cpdf_object::retain_object,
    cpdf_string::CpdfString,
};

#[test]
fn OneObject() {
    let holder = TestHolder::new();
    holder.AddObject(
        1,
        retain_object(CpdfString::from_bytes(None, b"string")),
        Unavailable,
    );
    let mut avail = CpdfObjectAvail::from_obj_num(holder.GetValidator(), &holder.holder, 1);
    assert_eq!(DocAvailStatus::DataNotAvailable, avail.check_avail());
    holder.SetObjectState(1, Available);
    assert_eq!(DocAvailStatus::DataAvailable, avail.check_avail());
}
#[test]
fn OneReferencedObject() {
    let holder = TestHolder::new();
    holder.AddObject(
        1,
        retain_object(CpdfReference::new(&holder.holder, 2)),
        Unavailable,
    );
    holder.AddObject(
        2,
        retain_object(CpdfString::from_bytes(None, b"string")),
        Unavailable,
    );
    let mut avail = CpdfObjectAvail::from_obj_num(holder.GetValidator(), &holder.holder, 1);
    assert_eq!(DocAvailStatus::DataNotAvailable, avail.check_avail());
    holder.SetObjectState(1, Available);
    assert_eq!(DocAvailStatus::DataNotAvailable, avail.check_avail());
    holder.SetObjectState(2, Available);
    assert_eq!(DocAvailStatus::DataAvailable, avail.check_avail());
}
#[test]
fn CycledReferences() {
    let holder = TestHolder::new();
    holder.AddObject(
        1,
        retain_object(CpdfReference::new(&holder.holder, 2)),
        Unavailable,
    );
    holder.AddObject(
        2,
        retain_object(CpdfReference::new(&holder.holder, 3)),
        Unavailable,
    );
    holder.AddObject(
        3,
        retain_object(CpdfReference::new(&holder.holder, 1)),
        Unavailable,
    );
    let mut avail = CpdfObjectAvail::from_obj_num(holder.GetValidator(), &holder.holder, 1);
    assert_eq!(DocAvailStatus::DataNotAvailable, avail.check_avail());
    holder.SetObjectState(1, Available);
    assert_eq!(DocAvailStatus::DataNotAvailable, avail.check_avail());
    holder.SetObjectState(2, Available);
    assert_eq!(DocAvailStatus::DataNotAvailable, avail.check_avail());
    holder.SetObjectState(3, Available);
    assert_eq!(DocAvailStatus::DataAvailable, avail.check_avail());
}
#[test]
fn DoNotCheckParent() {
    let holder = TestHolder::new();
    holder.AddObject(1, retain_object(CpdfDictionary::new()), Unavailable);
    holder.AddObject(2, retain_object(CpdfDictionary::new()), Unavailable);
    holder.SetReference(2, b"Parent", 1);
    let mut avail = CpdfObjectAvail::from_obj_num(holder.GetValidator(), &holder.holder, 2);
    assert_eq!(DocAvailStatus::DataNotAvailable, avail.check_avail());
    holder.SetObjectState(2, Available);
    // Object should be available when the Parent object is unavailable.
    assert_eq!(DocAvailStatus::DataAvailable, avail.check_avail());
}
#[test]
fn Generic() {
    let holder = TestHolder::new();
    const kDepth: u32 = 100;
    for i in 1..kDepth {
        holder.AddObject(i, retain_object(CpdfDictionary::new()), Unavailable);
        // Add ref to next dictionary.
        holder.SetReference(i, b"Child", i + 1);
    }
    // Add final object.
    holder.AddObject(kDepth, retain_object(CpdfDictionary::new()), Unavailable);
    let mut avail = CpdfObjectAvail::from_obj_num(holder.GetValidator(), &holder.holder, 1);
    for i in 1..=kDepth {
        assert_eq!(DocAvailStatus::DataNotAvailable, avail.check_avail());
        holder.SetObjectState(i, Available);
    }
    assert_eq!(DocAvailStatus::DataAvailable, avail.check_avail());
}
fn FailOnExclude(_: &ObjectRef) -> bool {
    panic!("NOTREACHED: root must not be excluded")
}
#[test]
fn NotExcludeRoot() {
    let holder = TestHolder::new();
    holder.AddObject(1, retain_object(CpdfDictionary::new()), Available);
    let mut avail = CpdfObjectAvail::from_obj_num(holder.GetValidator(), &holder.holder, 1);
    avail.set_exclude_object(Some(Rc::new(FailOnExclude)));
    assert_eq!(DocAvailStatus::DataAvailable, avail.check_avail());
}
#[test]
fn NotExcludeReferedRoot() {
    let holder = TestHolder::new();
    holder.AddObject(
        1,
        retain_object(CpdfReference::new(&holder.holder, 2)),
        Available,
    );
    holder.AddObject(2, retain_object(CpdfDictionary::new()), Available);
    let mut avail = CpdfObjectAvail::from_obj_num(holder.GetValidator(), &holder.holder, 1);
    avail.set_exclude_object(Some(Rc::new(FailOnExclude)));
    assert_eq!(DocAvailStatus::DataAvailable, avail.check_avail());
}
#[test]
fn Exclude() {
    let holder = TestHolder::new();
    holder.AddObject(1, retain_object(CpdfDictionary::new()), Available);
    holder.SetReference(1, b"ArrayRef", 2);
    holder.AddObject(2, retain_object(CpdfArray::new()), Available);
    holder.WithArray(2, |array| {
        array.append_new(CpdfReference::new(&holder.holder, 2));
    });
    // Native comment calls this the array item; preserve its actual self-reference 2.
    holder.AddObject(
        3,
        retain_object(CpdfString::from_bytes(None, b"Not available string")),
        Unavailable,
    );
    let mut avail = CpdfObjectAvail::from_obj_num(holder.GetValidator(), &holder.holder, 1);
    avail.set_exclude_object(Some(Rc::new(|object| {
        object.borrow().as_any().is::<CpdfArray>()
    })));
    assert_eq!(DocAvailStatus::DataAvailable, avail.check_avail());
}
#[test]
fn ReadErrorOnExclude() {
    let holder = TestHolder::new();
    holder.AddObject(1, retain_object(CpdfDictionary::new()), Available);
    holder.SetReference(1, b"DictRef", 2);
    holder.AddObject(2, retain_object(CpdfDictionary::new()), Available);
    holder.SetReference(2, b"Type", 3);
    // The value of Type is not available at start.
    holder.AddObject(
        3,
        retain_object(CpdfString::from_bytes(None, b"Exclude me")),
        Unavailable,
    );
    holder.SetReference(2, b"OtherData", 4);
    // This dictionary item must not be checked once the dictionary is skipped.
    holder.AddObject(
        4,
        retain_object(CpdfString::from_bytes(None, b"Not available string")),
        Unavailable,
    );
    let mut avail = CpdfObjectAvail::from_obj_num(holder.GetValidator(), &holder.holder, 1);
    avail.set_exclude_object(Some(Rc::new(|object| {
        // An unavailable indirect Type value must cause exclusion to be retried.
        object
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .is_some_and(|dict| dict.get_byte_string_for(b"Type", b"") == b"Exclude me")
    })));
    assert_eq!(DocAvailStatus::DataNotAvailable, avail.check_avail());
    holder.SetObjectState(3, Available);
    // Object 4 remains unavailable but belongs to the skipped dictionary.
    assert_eq!(DocAvailStatus::DataAvailable, avail.check_avail());
}
#[test]
fn IgnoreNotExistsObject() {
    let holder = TestHolder::new();
    holder.AddObject(1, retain_object(CpdfDictionary::new()), Available);
    holder.SetReference(1, b"NotExistsObjRef", 2);
    let mut avail = CpdfObjectAvail::from_obj_num(holder.GetValidator(), &holder.holder, 1);
    // Missing object 2 does not prevent all existing related data being checked.
    assert_eq!(DocAvailStatus::DataAvailable, avail.check_avail());
}
#[test]
fn CheckTwice() {
    let holder = TestHolder::new();
    holder.AddObject(
        1,
        retain_object(CpdfString::from_bytes(None, b"string")),
        Unavailable,
    );
    let mut avail = CpdfObjectAvail::from_obj_num(holder.GetValidator(), &holder.holder, 1);
    assert_eq!(avail.check_avail(), avail.check_avail());
    holder.SetObjectState(1, Available);
    assert_eq!(avail.check_avail(), avail.check_avail());
}
#[test]
fn SelfReferedInlinedObject() {
    let holder = TestHolder::new();
    holder.AddObject(1, retain_object(CpdfDictionary::new()), Available);
    holder.SetReference(1, b"Data", 2);
    let root = holder.WithDict(1, |dict| dict.set_new_for(b"Dict", CpdfDictionary::new()));
    root.borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_new_for(b"Self", CpdfReference::new(&holder.holder, 1));
    holder.AddObject(
        2,
        retain_object(CpdfString::from_bytes(None, b"Data")),
        Unavailable,
    );
    let mut avail = CpdfObjectAvail::from_root(holder.GetValidator(), &holder.holder, root);
    assert_eq!(DocAvailStatus::DataNotAvailable, avail.check_avail());
    holder.SetObjectState(2, Available);
    assert_eq!(DocAvailStatus::DataAvailable, avail.check_avail());
}
