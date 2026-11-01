// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Connected public contracts from the pinned structure API and header.
use super::*;
use pdfoxium_core::fpdfapi::parser::{
    cpdf_indirect_object_holder::HolderRef, cpdf_name::CpdfName, cpdf_object::retain_object,
    cpdf_reference::CpdfReference, cpdf_scalar_object::CpdfScalarObject, cpdf_string::CpdfString,
};
fn dictionary() -> ObjectRef {
    retain_object(CpdfDictionary::new())
}
fn set(object: &ObjectRef, key: &[u8], value: ObjectRef) {
    object
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_for(key, Some(value));
}
fn name(value: &[u8]) -> ObjectRef {
    retain_object(CpdfName::new(None, value))
}
fn number(value: i32) -> ObjectRef {
    retain_object(CpdfScalarObject::number_from_signed(value))
}
fn array(values: impl IntoIterator<Item = ObjectRef>) -> ObjectRef {
    let mut array = CpdfArray::new();
    for value in values {
        array.append(value);
    }
    retain_object(array)
}
fn string(value: &[u8]) -> ObjectRef {
    retain_object(CpdfString::from_bytes(None, value))
}
fn indirect(holder: &HolderRef, value: ObjectRef) -> ObjectRef {
    holder.borrow_mut().add_indirect_object(value.clone());
    value
}
fn reference(holder: &HolderRef, value: &ObjectRef) -> ObjectRef {
    retain_object(CpdfReference::new(holder, value.borrow().obj_num()))
}
struct Fixture {
    document: *mut c_void,
    page: *mut c_void,
    tree: *mut c_void,
    top: ObjectRef,
}
impl Fixture {
    unsafe fn new() -> Self {
        let document = FPDF_CreateNewDocument();
        let page = unsafe { FPDFPage_New(document, 0, 100., 100.) };
        let handle = unsafe { &*document.cast::<DocumentHandle>() };
        let holder = handle.document.borrow().holder();
        let catalog = handle.document.borrow().root().unwrap();
        let page_dictionary = unsafe { &*page.cast::<PageHandle>() }
            .page
            .as_ref()
            .unwrap()
            .dict();
        set(&page_dictionary, b"StructParents", number(0));
        let marked = dictionary();
        set(&marked, b"Marked", number(1));
        set(&catalog, b"MarkInfo", marked);
        let root = indirect(&holder, dictionary());
        set(&root, b"Type", name(b"StructTreeRoot"));
        let top = indirect(&holder, dictionary());
        set(&top, b"Type", name(b"StructElem"));
        set(&top, b"S", name(b"Custom"));
        set(&top, b"P", reference(&holder, &root));
        set(&top, b"Pg", reference(&holder, &page_dictionary));
        for (key, value) in [
            (b"Alt".as_slice(), b"alt".as_slice()),
            (b"ActualText", b"actual"),
            (b"E", b"expanded"),
            (b"ID", b""),
        ] {
            set(&top, key, string(value));
        }
        set(
            &top,
            b"T",
            retain_object(CpdfString::from_unicode(None, &[0x1f980])),
        );
        set(&top, b"Lang", name(b"en"));
        let role_map = dictionary();
        set(&role_map, b"Custom", name(b"Figure"));
        set(&root, b"RoleMap", role_map);
        let child = indirect(&holder, dictionary());
        set(&child, b"Type", name(b"StructElem"));
        set(&child, b"S", name(b"P"));
        set(&child, b"P", reference(&holder, &top));
        let mcr = dictionary();
        set(&mcr, b"Type", name(b"MCR"));
        set(&mcr, b"MCID", number(9));
        let foreign_mcr = dictionary();
        set(&foreign_mcr, b"Type", name(b"MCR"));
        set(&foreign_mcr, b"MCID", number(77));
        set(
            &foreign_mcr,
            b"Pg",
            retain_object(CpdfReference::without_holder(u32::MAX - 1)),
        );
        set(
            &top,
            b"K",
            array([reference(&holder, &child), number(4), mcr, foreign_mcr]),
        );
        let number_value = indirect(
            &holder,
            retain_object(CpdfScalarObject::number_from_string(b"2.5")),
        );
        let attributes = dictionary();
        set(&attributes, b"Number", reference(&holder, &number_value));
        set(
            &attributes,
            b"Bool",
            retain_object(CpdfScalarObject::boolean(true)),
        );
        set(&attributes, b"String", string(b"A\0B"));
        set(&attributes, b"Name", name("caf\u{e9}".as_bytes()));
        set(&attributes, b"Color", string(b"blue"));
        set(
            &attributes,
            b"Array",
            array([
                reference(&holder, &number_value),
                retain_object(CpdfScalarObject::boolean(false)),
            ]),
        );
        set(&top, b"A", array([attributes, number(10)]));
        set(&root, b"K", array([reference(&holder, &top)]));
        let parents = dictionary();
        set(
            &parents,
            b"Nums",
            array([
                number(0),
                array([reference(&holder, &child), reference(&holder, &top)]),
            ]),
        );
        set(&root, b"ParentTree", parents);
        set(&catalog, b"StructTreeRoot", reference(&holder, &root));
        let tree = unsafe { FPDF_StructTree_GetForPage(page) };
        assert!(!tree.is_null());
        Self {
            document,
            page,
            tree,
            top,
        }
    }
    unsafe fn element(&self) -> *mut c_void {
        unsafe { FPDF_StructTree_GetChildAtIndex(self.tree, 0) }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        unsafe {
            FPDF_StructTree_Close(self.tree);
            FPDF_ClosePage(self.page);
            FPDF_CloseDocument(self.document);
        }
    }
}
unsafe fn value(attribute: *mut c_void, key: &[u8]) -> *mut c_void {
    unsafe { FPDF_StructElement_Attr_GetValue(attribute, key.as_ptr().cast()) }
}

#[test]
fn null_handles_and_failed_outputs_follow_native_contracts() {
    unsafe {
        let null = std::ptr::null_mut();
        let mut output = [0xa5u8; 32];
        let mut length = 999;
        assert!(FPDF_StructTree_GetForPage(null).is_null());
        FPDF_StructTree_Close(null);
        assert_eq!(FPDF_StructTree_CountChildren(null), -1);
        assert!(FPDF_StructTree_GetChildAtIndex(null, 0).is_null());
        for getter in [
            FPDF_StructElement_GetAltText,
            FPDF_StructElement_GetActualText,
            FPDF_StructElement_GetExpansion,
            FPDF_StructElement_GetID,
            FPDF_StructElement_GetLang,
            FPDF_StructElement_GetType,
            FPDF_StructElement_GetObjType,
            FPDF_StructElement_GetTitle,
        ] {
            assert_eq!(getter(null, output.as_mut_ptr().cast(), 32), 0);
        }
        assert_eq!(
            FPDF_StructElement_GetRawType(null, std::ptr::null_mut(), 0),
            0
        );
        assert_eq!(FPDF_StructElement_CountChildren(null), -1);
        assert!(FPDF_StructElement_GetChildAtIndex(null, 0).is_null());
        assert!(FPDF_StructElement_GetParent(null).is_null());
        assert_eq!(FPDF_StructElement_GetChildMarkedContentID(null, 0), -1);
        assert_eq!(FPDF_StructElement_GetMarkedContentID(null), -1);
        assert_eq!(FPDF_StructElement_GetMarkedContentIdCount(null), -1);
        assert_eq!(FPDF_StructElement_GetMarkedContentIdAtIndex(null, 0), -1);
        assert_eq!(FPDF_StructElement_GetAttributeCount(null), -1);
        assert!(FPDF_StructElement_GetAttributeAtIndex(null, 0).is_null());
        assert_eq!(
            FPDF_StructElement_GetStringAttribute(null, b"Color\0".as_ptr().cast(), null, 0),
            0
        );
        assert_eq!(FPDF_StructElement_Attr_GetCount(null), -1);
        assert_eq!(FPDF_StructElement_Attr_GetType(null), 0);
        assert_eq!(
            FPDF_StructElement_Attr_GetName(null, 0, output.as_mut_ptr().cast(), 32, &mut length),
            0
        );
        assert_eq!(length, 999);
        assert!(value(null, b"Bool\0").is_null());
        assert_eq!(FPDF_StructElement_Attr_CountChildren(null), -1);
        assert!(FPDF_StructElement_Attr_GetChildAtIndex(null, 0).is_null());
        let mut boolean = 7;
        let mut number = 8.;
        assert_eq!(
            FPDF_StructElement_Attr_GetBooleanValue(null, &mut boolean),
            0
        );
        assert_eq!(boolean, 7);
        assert_eq!(FPDF_StructElement_Attr_GetNumberValue(null, &mut number), 0);
        assert_eq!(number, 8.);
        assert_eq!(
            FPDF_StructElement_Attr_GetStringValue(
                null,
                output.as_mut_ptr().cast(),
                32,
                &mut length
            ),
            0
        );
        assert_eq!(
            FPDF_StructElement_Attr_GetBlobValue(null, output.as_mut_ptr().cast(), 32, &mut length),
            0
        );
        assert_eq!(length, 999);
        assert_eq!(output, [0xa5; 32]);
    }
}

#[test]
fn real_page_structure_identity_text_buffers_and_scoped_mcids() {
    unsafe {
        let fixture = Fixture::new();
        let element = fixture.element();
        assert!(!element.is_null());
        assert_eq!(FPDF_StructTree_CountChildren(fixture.tree), 1);
        assert_eq!(fixture.element(), element);
        assert!(FPDF_StructTree_GetChildAtIndex(fixture.tree, -1).is_null());
        assert!(FPDF_StructTree_GetChildAtIndex(fixture.tree, 1).is_null());
        let child = FPDF_StructElement_GetChildAtIndex(element, 0);
        assert!(!child.is_null());
        assert_eq!(FPDF_StructElement_GetParent(child), element);
        assert!(FPDF_StructElement_GetParent(element).is_null());
        assert_eq!(FPDF_StructElement_CountChildren(element), 4);
        assert!(FPDF_StructElement_GetChildAtIndex(element, 1).is_null());
        assert!(FPDF_StructElement_GetChildAtIndex(element, -1).is_null());
        assert!(FPDF_StructElement_GetChildAtIndex(element, 4).is_null());
        assert_eq!(
            (0..5)
                .map(|i| FPDF_StructElement_GetChildMarkedContentID(element, i))
                .collect::<Vec<_>>(),
            [-1, 4, 9, -1, -1]
        );
        assert_eq!(FPDF_StructElement_GetMarkedContentID(element), -1);
        assert_eq!(FPDF_StructElement_GetMarkedContentIdCount(element), 3);
        assert_eq!(
            (0..5)
                .map(|i| FPDF_StructElement_GetMarkedContentIdAtIndex(element, i))
                .collect::<Vec<_>>(),
            [4, 9, 77, -1, -1]
        );
        assert_eq!(
            FPDF_StructElement_GetMarkedContentIdAtIndex(element, -1),
            -1
        );
        let mut output = [0xa5u8; 32];
        assert_eq!(
            FPDF_StructElement_GetAltText(element, std::ptr::null_mut(), 0),
            8
        );
        assert_eq!(
            FPDF_StructElement_GetAltText(element, output.as_mut_ptr().cast(), 7),
            8
        );
        assert_eq!(output, [0xa5; 32]);
        assert_eq!(
            FPDF_StructElement_GetAltText(element, output.as_mut_ptr().cast(), 8),
            8
        );
        assert_eq!(&output[..8], &[b'a', 0, b'l', 0, b't', 0, 0, 0]);
        assert_eq!(
            FPDF_StructElement_GetActualText(element, std::ptr::null_mut(), 0),
            14
        );
        assert_eq!(
            FPDF_StructElement_GetExpansion(element, std::ptr::null_mut(), 0),
            18
        );
        assert_eq!(
            FPDF_StructElement_GetID(element, std::ptr::null_mut(), 0),
            2
        );
        assert_eq!(
            FPDF_StructElement_GetLang(element, std::ptr::null_mut(), 0),
            0
        );
        output.fill(0xa5);
        assert_eq!(
            FPDF_StructElement_GetAltText(child, output.as_mut_ptr().cast(), 32),
            0
        );
        assert_eq!(output, [0xa5; 32]);
        assert_eq!(
            FPDF_StructElement_GetTitle(element, output.as_mut_ptr().cast(), 6),
            6
        );
        assert_eq!(&output[..6], &[0x3e, 0xd8, 0x80, 0xdd, 0, 0]);
        assert_eq!(
            FPDF_StructElement_GetType(element, std::ptr::null_mut(), 0),
            14
        );
        assert_eq!(
            FPDF_StructElement_GetRawType(element, std::ptr::null_mut(), 0),
            14
        );
        assert_eq!(
            FPDF_StructElement_GetObjType(element, std::ptr::null_mut(), 0),
            22
        );
        // The raw-type length is measured in bytes despite the u16 pointer.
        let mut raw_type = [0xa5a5u16; 7];
        assert_eq!(
            FPDF_StructElement_GetRawType(element, raw_type.as_mut_ptr(), 7),
            14
        );
        assert_eq!(raw_type, [0xa5a5; 7]);
        assert_eq!(
            FPDF_StructElement_GetRawType(element, raw_type.as_mut_ptr(), 14),
            14
        );
        assert_eq!(raw_type, [67, 117, 115, 116, 111, 109, 0]);
        let mcr = dictionary();
        set(&mcr, b"Type", name(b"MCR"));
        set(&mcr, b"MCID", number(31));
        set(&fixture.top, b"K", mcr);
        assert_eq!(FPDF_StructElement_GetMarkedContentIdCount(element), 1);
        assert_eq!(FPDF_StructElement_GetMarkedContentIdAtIndex(element, 0), 31);
        assert_eq!(
            FPDF_StructElement_GetMarkedContentIdAtIndex(element, -1),
            -1
        );
        assert_eq!(FPDF_StructElement_GetMarkedContentIdAtIndex(element, 1), -1);
        let owner = tree(fixture.tree).unwrap();
        let weak = Rc::downgrade(&owner);
        drop(owner);
        drop(fixture);
        assert!(weak.upgrade().is_none());
    }
}

#[test]
fn typed_attributes_canonical_children_and_output_preservation() {
    unsafe {
        let fixture = Fixture::new();
        let element = fixture.element();
        assert_eq!(FPDF_StructElement_GetAttributeCount(element), 2);
        let attributes = FPDF_StructElement_GetAttributeAtIndex(element, 0);
        assert!(!attributes.is_null());
        assert_eq!(
            FPDF_StructElement_GetAttributeAtIndex(element, 0),
            attributes
        );
        assert!(FPDF_StructElement_GetAttributeAtIndex(element, -1).is_null());
        assert!(FPDF_StructElement_GetAttributeAtIndex(element, 1).is_null());
        assert!(FPDF_StructElement_GetAttributeAtIndex(element, 2).is_null());
        assert_eq!(FPDF_StructElement_Attr_GetCount(attributes), 6);
        let mut length = 999;
        let mut bytes = [0xa5u8; 32];
        assert_eq!(
            FPDF_StructElement_Attr_GetName(
                attributes,
                0,
                bytes.as_mut_ptr().cast(),
                5,
                &mut length
            ),
            1
        );
        assert_eq!(length, 6);
        assert_eq!(bytes, [0xa5; 32]);
        assert_eq!(
            FPDF_StructElement_Attr_GetName(
                attributes,
                0,
                bytes.as_mut_ptr().cast(),
                6,
                &mut length
            ),
            1
        );
        assert_eq!(&bytes[..6], b"Array\0");
        length = 999;
        assert_eq!(
            FPDF_StructElement_Attr_GetName(attributes, -1, std::ptr::null_mut(), 0, &mut length),
            0
        );
        assert_eq!(length, 999);
        assert_eq!(
            FPDF_StructElement_Attr_GetName(
                attributes,
                0,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut()
            ),
            0
        );
        assert!(FPDF_StructElement_Attr_GetValue(attributes, std::ptr::null()).is_null());
        assert!(value(attributes, b"missing\0").is_null());
        let boolean = value(attributes, b"Bool\0");
        let number = value(attributes, b"Number\0");
        let text = value(attributes, b"String\0");
        let named = value(attributes, b"Name\0");
        let list = value(attributes, b"Array\0");
        assert_eq!(FPDF_StructElement_Attr_GetType(boolean), 1);
        assert_eq!(FPDF_StructElement_Attr_GetType(number), 2);
        assert_eq!(FPDF_StructElement_Attr_GetType(text), 3);
        assert_eq!(FPDF_StructElement_Attr_GetType(named), 4);
        assert_eq!(FPDF_StructElement_Attr_GetType(list), 5);
        assert_eq!(FPDF_StructElement_Attr_CountChildren(list), 2);
        assert_eq!(FPDF_StructElement_Attr_CountChildren(number), -1);
        assert_eq!(FPDF_StructElement_Attr_GetChildAtIndex(list, 0), number);
        assert!(FPDF_StructElement_Attr_GetChildAtIndex(list, -1).is_null());
        assert!(FPDF_StructElement_Attr_GetChildAtIndex(list, 2).is_null());
        let mut out_boolean = 7;
        let mut out_number = 8.;
        assert_eq!(
            FPDF_StructElement_Attr_GetBooleanValue(number, &mut out_boolean),
            0
        );
        assert_eq!(out_boolean, 7);
        assert_eq!(
            FPDF_StructElement_Attr_GetBooleanValue(boolean, &mut out_boolean),
            1
        );
        assert_eq!(out_boolean, 1);
        assert_eq!(
            FPDF_StructElement_Attr_GetNumberValue(boolean, &mut out_number),
            0
        );
        assert_eq!(out_number, 8.);
        assert_eq!(
            FPDF_StructElement_Attr_GetNumberValue(number, &mut out_number),
            1
        );
        assert_eq!(out_number, 2.5);
        assert_eq!(
            FPDF_StructElement_Attr_GetBooleanValue(boolean, std::ptr::null_mut()),
            0
        );
        assert_eq!(
            FPDF_StructElement_Attr_GetNumberValue(number, std::ptr::null_mut()),
            0
        );
        bytes.fill(0xa5);
        length = 999;
        assert_eq!(
            FPDF_StructElement_Attr_GetStringValue(
                number,
                bytes.as_mut_ptr().cast(),
                32,
                &mut length
            ),
            0
        );
        assert_eq!(length, 999);
        assert_eq!(bytes, [0xa5; 32]);
        assert_eq!(
            FPDF_StructElement_Attr_GetStringValue(
                named,
                bytes.as_mut_ptr().cast(),
                9,
                &mut length
            ),
            1
        );
        assert_eq!(length, 10);
        assert_eq!(bytes, [0xa5; 32]);
        assert_eq!(
            FPDF_StructElement_Attr_GetStringValue(
                named,
                bytes.as_mut_ptr().cast(),
                10,
                &mut length
            ),
            1
        );
        assert_eq!(&bytes[..10], &[b'c', 0, b'a', 0, b'f', 0, 0xe9, 0, 0, 0]);
        bytes.fill(0xa5);
        assert_eq!(
            FPDF_StructElement_Attr_GetBlobValue(text, bytes.as_mut_ptr().cast(), 2, &mut length),
            1
        );
        assert_eq!(length, 3);
        assert_eq!(bytes, [0xa5; 32]);
        assert_eq!(
            FPDF_StructElement_Attr_GetBlobValue(text, bytes.as_mut_ptr().cast(), 3, &mut length),
            1
        );
        assert_eq!(&bytes[..4], &[b'A', 0, b'B', 0xa5]);
        length = 999;
        assert_eq!(
            FPDF_StructElement_Attr_GetBlobValue(named, std::ptr::null_mut(), 0, &mut length),
            0
        );
        assert_eq!(length, 999);
        assert_eq!(
            FPDF_StructElement_Attr_GetStringValue(
                text,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut()
            ),
            0
        );
        assert_eq!(
            FPDF_StructElement_Attr_GetBlobValue(
                text,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut()
            ),
            0
        );
        assert_eq!(
            FPDF_StructElement_GetStringAttribute(
                element,
                b"Color\0".as_ptr().cast(),
                std::ptr::null_mut(),
                0
            ),
            10
        );
        let map = attribute(attributes).unwrap().1;
        set(&fixture.top, b"A", map);
        assert_eq!(FPDF_StructElement_GetAttributeCount(element), 1);
        assert_eq!(
            FPDF_StructElement_GetAttributeAtIndex(element, 0),
            attributes
        );
        assert!(FPDF_StructElement_GetAttributeAtIndex(element, 1).is_null());
        assert_eq!(
            FPDF_StructElement_GetStringAttribute(
                element,
                b"Color\0".as_ptr().cast(),
                std::ptr::null_mut(),
                0
            ),
            0
        );
    }
}
