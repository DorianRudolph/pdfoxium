// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/parser/cpdf_document_unittest.cpp`.
//! CpdfDocument::new owns the selected page/render module state; native
//! TestWithPageModule setup/teardown is automatic in these fixtures.
#![allow(non_snake_case, non_upper_case_globals)]
use super::*;
use crate::fpdfapi::parser::{
    cpdf_linearized_header::CpdfLinearizedHeader, cpdf_object::retain_object,
    cpdf_string::CpdfString,
};
const kNumTestPages: usize = 7;
fn CreatePageTreeNode(kids: ObjectRef, doc: &mut CpdfDocument, count: i32) -> ObjectRef {
    let holder = doc.holder();
    let new_objnum = holder.borrow_mut().add_indirect_object(kids.clone());
    let pageNode = holder.borrow_mut().new_indirect_dictionary();
    with_dict_mut(&pageNode, |dict| {
        dict.set_new_name_for(b"Type", b"Pages");
        dict.set_new_for(b"Kids", CpdfReference::new(&holder, new_objnum));
        dict.set_new_for(b"Count", CpdfScalarObject::number_from_signed(count));
    })
    .unwrap();
    for i in 0..with_array(&kids, CpdfArray::size).unwrap() {
        let child = with_array(&kids, |array| array.get_dict_at(i))
            .unwrap()
            .unwrap();
        with_dict_mut(&child, |dict| {
            dict.set_new_for(
                b"Parent",
                CpdfReference::new(&holder, pageNode.borrow().obj_num()),
            );
        })
        .unwrap();
    }
    pageNode
}
fn CreateNumberedPage(number: usize) -> ObjectRef {
    let mut page = CpdfDictionary::new();
    page.set_new_name_for(b"Type", b"Page");
    page.set_new_for(
        b"PageNumbering",
        CpdfScalarObject::number_from_signed(number as i32),
    );
    retain_object(page)
}
fn SetRootForTesting(doc: &mut CpdfDocument, root: ObjectRef) {
    doc.root_dict = Some(root);
}
fn ResizePageListForTesting(doc: &mut CpdfDocument, size: usize) {
    doc.page_list.resize(size, 0);
}
fn CPDF_TestDocumentForPages() -> CpdfDocument {
    let mut doc = CpdfDocument::new();
    let holder = doc.holder();
    let mut zeroToTwo = CpdfArray::new();
    zeroToTwo.append_new(CpdfReference::new(
        &holder,
        holder
            .borrow_mut()
            .add_indirect_object(CreateNumberedPage(0)),
    ));
    zeroToTwo.append_new(CpdfReference::new(
        &holder,
        holder
            .borrow_mut()
            .add_indirect_object(CreateNumberedPage(1)),
    ));
    zeroToTwo.append_new(CpdfReference::new(
        &holder,
        holder
            .borrow_mut()
            .add_indirect_object(CreateNumberedPage(2)),
    ));
    let branch1 = CreatePageTreeNode(retain_object(zeroToTwo), &mut doc, 3);
    let mut zeroToThree = CpdfArray::new();
    zeroToThree.append_new(CpdfReference::new(&holder, branch1.borrow().obj_num()));
    zeroToThree.append_new(CpdfReference::new(
        &holder,
        holder
            .borrow_mut()
            .add_indirect_object(CreateNumberedPage(3)),
    ));
    let branch2 = CreatePageTreeNode(retain_object(zeroToThree), &mut doc, 4);
    let mut fourFive = CpdfArray::new();
    fourFive.append_new(CpdfReference::new(
        &holder,
        holder
            .borrow_mut()
            .add_indirect_object(CreateNumberedPage(4)),
    ));
    fourFive.append_new(CpdfReference::new(
        &holder,
        holder
            .borrow_mut()
            .add_indirect_object(CreateNumberedPage(5)),
    ));
    let branch3 = CreatePageTreeNode(retain_object(fourFive), &mut doc, 2);
    let mut justSix = CpdfArray::new();
    justSix.append_new(CpdfReference::new(
        &holder,
        holder
            .borrow_mut()
            .add_indirect_object(CreateNumberedPage(6)),
    ));
    let branch4 = CreatePageTreeNode(retain_object(justSix), &mut doc, 1);
    let mut allPages = CpdfArray::new();
    allPages.append_new(CpdfReference::new(&holder, branch2.borrow().obj_num()));
    allPages.append_new(CpdfReference::new(&holder, branch3.borrow().obj_num()));
    allPages.append_new(CpdfReference::new(&holder, branch4.borrow().obj_num()));
    let pagesDict = CreatePageTreeNode(retain_object(allPages), &mut doc, kNumTestPages as i32);
    SetRootForTesting(&mut doc, holder.borrow_mut().new_indirect_dictionary());
    with_dict_mut(&doc.root().unwrap(), |dict| {
        dict.set_new_for(
            b"Pages",
            CpdfReference::new(&holder, pagesDict.borrow().obj_num()),
        );
    })
    .unwrap();
    ResizePageListForTesting(&mut doc, kNumTestPages);
    doc
}
fn SetTreeSize(doc: &mut CpdfDocument, size: usize) {
    with_dict_mut(&doc.root().unwrap(), |dict| {
        dict.set_new_for(b"Count", CpdfScalarObject::number_from_signed(size as i32));
    })
    .unwrap();
    ResizePageListForTesting(doc, size);
}
fn CPDF_TestDocumentWithPageWithoutPageNum() -> (CpdfDocument, ObjectRef) {
    let mut doc = CpdfDocument::new();
    let holder = doc.holder();
    let mut allPages = CpdfArray::new();
    allPages.append_new(CpdfReference::new(
        &holder,
        holder
            .borrow_mut()
            .add_indirect_object(CreateNumberedPage(0)),
    ));
    allPages.append_new(CpdfReference::new(
        &holder,
        holder
            .borrow_mut()
            .add_indirect_object(CreateNumberedPage(1)),
    ));
    // Page without pageNum.
    let inlined_page = CreateNumberedPage(2);
    allPages.append(inlined_page.clone());
    let pagesDict = CreatePageTreeNode(retain_object(allPages), &mut doc, 3);
    SetRootForTesting(&mut doc, holder.borrow_mut().new_indirect_dictionary());
    with_dict_mut(&doc.root().unwrap(), |dict| {
        dict.set_new_for(
            b"Pages",
            CpdfReference::new(&holder, pagesDict.borrow().obj_num()),
        );
    })
    .unwrap();
    ResizePageListForTesting(&mut doc, 3);
    (doc, inlined_page)
}
fn TestLinearized(dict: &CpdfDictionary) -> CpdfLinearizedHeader {
    CpdfLinearizedHeader::from_dict(dict, 0)
}
fn CPDF_TestDocPagesWithoutKids() -> CpdfDocument {
    let mut doc = CpdfDocument::new();
    let holder = doc.holder();
    let pagesDict = holder.borrow_mut().new_indirect_dictionary();
    with_dict_mut(&pagesDict, |dict| {
        dict.set_new_name_for(b"Type", b"Pages");
        dict.set_new_for(b"Count", CpdfScalarObject::number_from_signed(3));
    })
    .unwrap();
    ResizePageListForTesting(&mut doc, 10);
    SetRootForTesting(&mut doc, holder.borrow_mut().new_indirect_dictionary());
    with_dict_mut(&doc.root().unwrap(), |dict| {
        dict.set_new_for(
            b"Pages",
            CpdfReference::new(&holder, pagesDict.borrow().obj_num()),
        );
    })
    .unwrap();
    doc
}
#[test]
fn GetPages() {
    let mut document = CPDF_TestDocumentForPages();
    for i in 0..kNumTestPages {
        let page = document.page_dictionary(i);
        assert!(page.is_some());
        let page = page.unwrap();
        assert!(with_dict(&page, |dict| dict.key_exist(b"PageNumbering")).unwrap());
        assert_eq!(
            i as i32,
            with_dict(&page, |dict| dict.get_integer_for(b"PageNumbering", 0)).unwrap()
        );
    }
    let page = document.page_dictionary(kNumTestPages);
    assert!(page.is_none());
}
#[test]
fn GetPageWithoutObjNumTwice() {
    let (mut document, inlined_page) = CPDF_TestDocumentWithPageWithoutPageNum();
    let page = document.page_dictionary(2);
    assert!(page.is_some());
    let page = page.unwrap();
    assert!(Rc::ptr_eq(&inlined_page, &page));
    let second_call_page = document.page_dictionary(2);
    assert!(second_call_page.is_some());
    assert!(Rc::ptr_eq(&page, &second_call_page.unwrap()));
    // Native derived inlined_page member is destroyed before the base document.
    drop(inlined_page);
}
#[test]
fn GetPagesReverseOrder() {
    let mut document = CPDF_TestDocumentForPages();
    for i in (0..=6).rev() {
        let page = document.page_dictionary(i);
        assert!(page.is_some());
        let page = page.unwrap();
        assert!(with_dict(&page, |dict| dict.key_exist(b"PageNumbering")).unwrap());
        assert_eq!(
            i as i32,
            with_dict(&page, |dict| dict.get_integer_for(b"PageNumbering", 0)).unwrap()
        );
    }
    let page = document.page_dictionary(kNumTestPages);
    assert!(page.is_none());
}
#[test]
fn GetPagesInDisorder() {
    let mut document = CPDF_TestDocumentForPages();
    let page = document.page_dictionary(1);
    assert!(page.is_some());
    let mut page = page.unwrap();
    assert!(with_dict(&page, |dict| dict.key_exist(b"PageNumbering")).unwrap());
    assert_eq!(
        1,
        with_dict(&page, |dict| dict.get_integer_for(b"PageNumbering", 0)).unwrap()
    );
    let next_page = document.page_dictionary(3);
    drop(page);
    assert!(next_page.is_some());
    page = next_page.unwrap();
    assert!(with_dict(&page, |dict| dict.key_exist(b"PageNumbering")).unwrap());
    assert_eq!(
        3,
        with_dict(&page, |dict| dict.get_integer_for(b"PageNumbering", 0)).unwrap()
    );
    let next_page = document.page_dictionary(kNumTestPages);
    drop(page);
    assert!(next_page.is_none());
    let page = document.page_dictionary(6);
    assert!(page.is_some());
    let page = page.unwrap();
    assert!(with_dict(&page, |dict| dict.key_exist(b"PageNumbering")).unwrap());
    assert_eq!(
        6,
        with_dict(&page, |dict| dict.get_integer_for(b"PageNumbering", 0)).unwrap()
    );
}
#[test]
fn IsValidPageObject() {
    let document = CPDF_TestDocumentForPages();
    let holder = document.holder();
    let mut dict_type_name_page = CpdfDictionary::new();
    dict_type_name_page.set_new_name_for(b"Type", b"Page");
    let dict_type_name_page = retain_object(dict_type_name_page);
    holder
        .borrow_mut()
        .add_indirect_object(dict_type_name_page.clone());
    assert!(CpdfDocument::is_valid_page_object(&dict_type_name_page));
    let mut dict_type_string_page = CpdfDictionary::new();
    dict_type_string_page.set_new_for(b"Type", CpdfString::from_bytes(None, b"Page"));
    let dict_type_string_page = retain_object(dict_type_string_page);
    holder
        .borrow_mut()
        .add_indirect_object(dict_type_string_page.clone());
    assert!(!CpdfDocument::is_valid_page_object(&dict_type_string_page));
    let mut dict_type_name_font = CpdfDictionary::new();
    dict_type_name_font.set_new_name_for(b"Type", b"Font");
    let dict_type_name_font = retain_object(dict_type_name_font);
    holder
        .borrow_mut()
        .add_indirect_object(dict_type_name_font.clone());
    assert!(!CpdfDocument::is_valid_page_object(&dict_type_name_font));
    let obj_no_type = holder.borrow_mut().new_indirect_dictionary();
    assert!(!CpdfDocument::is_valid_page_object(&obj_no_type));
}
#[test]
fn UseCachedPageObjNumIfHaveNotPagesDict() {
    // Hint-table page availability can cache object numbers without a Pages dict.
    const kPageCount: usize = 100;
    const kTestPageNum: usize = 33;
    let mut linearization_dict = CpdfDictionary::new();
    let mut document = CpdfDocument::new();
    {
        let first_page = CreateNumberedPage(0);
        // Retained construction is non-null by type in Rust.
        let first_page_obj_num = document
            .holder()
            .borrow_mut()
            .add_indirect_object(first_page.clone());
        assert_ne!(kTestPageNum as i32, first_page_obj_num as i32);
        linearization_dict.set_new_for(b"Linearized", CpdfScalarObject::boolean(true));
        linearization_dict.set_new_for(
            b"N",
            CpdfScalarObject::number_from_signed(kPageCount as i32),
        );
        linearization_dict.set_new_for(
            b"O",
            CpdfScalarObject::number_from_signed(first_page_obj_num as i32),
        );
        // Native parser belongs to the document holder; Rust separates storage.
        let mut parser = CpdfParser::with_holder(document.holder());
        parser.set_linearized_header_for_testing(TestLinearized(&linearization_dict));
        document.parser = parser;
    }
    document.load_pages();
    assert_eq!(kPageCount, document.page_count());
    let page_stub = document.holder().borrow_mut().new_indirect_dictionary();
    let obj_num = page_stub.borrow().obj_num();
    assert!(!document.is_page_loaded(kTestPageNum));
    assert!(document.page_dictionary(kTestPageNum).is_none());
    document.set_page_obj_num(kTestPageNum, obj_num);
    assert!(document.is_page_loaded(kTestPageNum));
    assert!(Rc::ptr_eq(
        &page_stub,
        &document.page_dictionary(kTestPageNum).unwrap()
    ));
    // Native dictionary is constructed before document; it outlives its parser.
    drop(document);
    drop(linearization_dict);
}
#[test]
fn CountGreaterThanPageTree() {
    let mut document = CPDF_TestDocumentForPages();
    SetTreeSize(&mut document, kNumTestPages + 3);
    for i in 0..kNumTestPages {
        assert!(document.page_dictionary(i).is_some());
    }
    for i in kNumTestPages..kNumTestPages + 4 {
        assert!(document.page_dictionary(i).is_none());
    }
    assert!(document.page_dictionary(kNumTestPages - 1).is_some());
}
#[test]
fn PagesWithoutKids() {
    let mut doc = CPDF_TestDocPagesWithoutKids();
    for i in 0..5 {
        assert!(doc.page_dictionary(i).is_none());
    }
    assert!(doc.page_dictionary(0).is_none());
}
