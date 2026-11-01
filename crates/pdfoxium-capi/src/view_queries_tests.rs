// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::*;
use pdfoxium_core::fpdfapi::parser::{
    cpdf_indirect_object_holder::HolderRef, cpdf_name::CpdfName, cpdf_object::retain_object,
    cpdf_reference::CpdfReference, cpdf_scalar_object::CpdfScalarObject, cpdf_string::CpdfString,
};

unsafe fn root(document: *mut c_void) -> ObjectRef {
    unsafe { &*document.cast::<DocumentHandle>() }
        .document
        .borrow()
        .root()
        .unwrap()
}
fn set(dict: &ObjectRef, key: &[u8], value: ObjectRef) {
    dict.borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_for(key, Some(value));
}
fn indirect_stream(holder: &HolderRef, stream: CpdfStream) -> ObjectRef {
    let number = holder
        .borrow_mut()
        .add_indirect_object(retain_object(stream));
    retain_object(CpdfReference::new(holder, number))
}
fn array() -> ObjectRef {
    retain_object(CpdfArray::new())
}

#[test]
fn named_destination_tree_old_dictionary_identity_and_buffer_contract() {
    unsafe {
        let document = FPDF_CreateNewDocument();
        let root = root(document);
        let tree_dest = array();
        let old_dest = array();
        let mut wrapped = CpdfDictionary::new();
        wrapped.set_for(b"D", Some(old_dest.clone()));
        let mut pairs = CpdfArray::new();
        pairs.append_new(CpdfString::from_bytes(None, b"alpha"));
        pairs.append(tree_dest.clone());
        let mut tree = CpdfDictionary::new();
        tree.set_new_for(b"Names", pairs);
        let mut names = CpdfDictionary::new();
        names.set_new_for(b"Dests", tree);
        set(&root, b"Names", retain_object(names));
        let mut old = CpdfDictionary::new();
        old.set_for(b"beta", Some(retain_object(wrapped)));
        old.set_new_for(b"broken", CpdfScalarObject::number_from_signed(7));
        set(&root, b"Dests", retain_object(old));
        assert_eq!(FPDF_CountNamedDests(document), 3); // Native counts even invalid entries.
        let alpha = FPDF_GetNamedDestByName(document, c"alpha".as_ptr());
        let beta = FPDF_GetNamedDestByName(document, c"beta".as_ptr());
        assert!(!alpha.is_null() && !beta.is_null());
        assert!(Rc::ptr_eq(&doc::borrowed(alpha).unwrap(), &tree_dest));
        assert!(Rc::ptr_eq(&doc::borrowed(beta).unwrap(), &old_dest));
        assert!(FPDF_GetNamedDestByName(document, c"broken".as_ptr()).is_null());
        assert!(FPDF_GetNamedDestByName(document, c"".as_ptr()).is_null());
        assert!(FPDF_GetNamedDestByName(document, std::ptr::null()).is_null());
        let mut size = 99;
        assert_eq!(
            FPDF_GetNamedDest(document, 0, std::ptr::null_mut(), &mut size),
            alpha
        );
        assert_eq!(size, 12);
        let mut output = [0xa5u8; 16];
        size = 11;
        assert_eq!(
            FPDF_GetNamedDest(document, 0, output.as_mut_ptr().cast(), &mut size),
            alpha
        );
        assert_eq!(size, -1);
        assert_eq!(output, [0xa5; 16]);
        size = 16;
        assert_eq!(
            FPDF_GetNamedDest(document, 1, output.as_mut_ptr().cast(), &mut size),
            beta
        );
        assert_eq!(size, 10);
        assert_eq!(&output[..10], b"b\0e\0t\0a\0\0\0");
        assert_eq!(&output[10..], &[0xa5; 6]);
        size = 23;
        assert!(FPDF_GetNamedDest(document, 2, output.as_mut_ptr().cast(), &mut size).is_null());
        assert_eq!(size, 23);
        assert!(FPDF_GetNamedDest(document, -1, std::ptr::null_mut(), &mut size).is_null());
        assert_eq!(size, 0);
        assert!(FPDF_GetNamedDest(
            document,
            0,
            output.as_mut_ptr().cast(),
            std::ptr::null_mut()
        )
        .is_null());
        assert_eq!(FPDF_CountNamedDests(std::ptr::null_mut()), 0);
        FPDF_CloseDocument(document);
    }
}

#[test]
fn xfa_single_stream_filtered_pairs_invalid_entries_and_buffers() {
    unsafe {
        let document = FPDF_CreateNewDocument();
        let root = root(document);
        let holder = (&*document.cast::<DocumentHandle>())
            .document
            .borrow()
            .holder();
        assert_eq!(FPDF_GetXFAPacketCount(document), 0);
        let acro = retain_object(CpdfDictionary::new());
        set(&root, b"AcroForm", acro.clone());
        set(
            &acro,
            b"XFA",
            indirect_stream(&holder, CpdfStream::from_bytes(b"<xdp/>")),
        );
        assert_eq!(FPDF_GetXFAPacketCount(document), 1);
        assert_eq!(
            FPDF_GetXFAPacketName(document, 0, std::ptr::null_mut(), 0),
            1
        );
        let mut name = [0xa5; 16];
        assert_eq!(
            FPDF_GetXFAPacketName(document, 0, name.as_mut_ptr().cast(), 16),
            1
        );
        assert_eq!(name[0], 0);
        assert_eq!(name[1], 0xa5);
        let encoded = CpdfStream::from_bytes(b"3c74656d706c6174652f3e>");
        set(
            &encoded.get_dict(),
            b"Filter",
            retain_object(CpdfName::new(None, b"ASCIIHexDecode")),
        );
        let mut pairs = CpdfArray::new();
        pairs.append_new(CpdfString::from_bytes(None, b"template"));
        pairs.append(indirect_stream(&holder, encoded));
        pairs.append_new(CpdfName::new(None, b"not-a-string"));
        pairs.append(indirect_stream(&holder, CpdfStream::from_bytes(b"ignored")));
        pairs.append_new(CpdfString::from_bytes(None, b"not-a-stream"));
        pairs.append_new(CpdfString::from_bytes(None, b"ignored"));
        pairs.append_new(CpdfString::from_bytes(None, b"datasets"));
        pairs.append(indirect_stream(&holder, CpdfStream::from_bytes(b"")));
        pairs.append_new(CpdfString::from_bytes(None, b"unpaired"));
        set(&acro, b"XFA", retain_object(pairs));
        assert_eq!(FPDF_GetXFAPacketCount(document), 2);
        name.fill(0xa5);
        assert_eq!(
            FPDF_GetXFAPacketName(document, 0, name.as_mut_ptr().cast(), 8),
            9
        );
        assert_eq!(name, [0xa5; 16]);
        assert_eq!(
            FPDF_GetXFAPacketName(document, 0, name.as_mut_ptr().cast(), 9),
            9
        );
        assert_eq!(&name[..9], b"template\0");
        let mut size = 999;
        assert_eq!(
            FPDF_GetXFAPacketContent(document, 0, std::ptr::null_mut(), 0, &mut size),
            1
        );
        assert_eq!(size, 11);
        let mut content = [0xa5; 16];
        assert_eq!(
            FPDF_GetXFAPacketContent(document, 0, content.as_mut_ptr().cast(), 10, &mut size),
            1
        );
        assert_eq!(content, [0xa5; 16]);
        assert_eq!(size, 11);
        assert_eq!(
            FPDF_GetXFAPacketContent(document, 0, content.as_mut_ptr().cast(), 16, &mut size),
            1
        );
        assert_eq!(&content[..11], b"<template/>");
        assert_eq!(content[11], 0xa5); // Raw XML bytes have no added terminator.
        assert_eq!(
            FPDF_GetXFAPacketContent(document, 1, content.as_mut_ptr().cast(), 16, &mut size),
            1
        );
        assert_eq!(size, 0);
        size = 999;
        for index in [-1, 2] {
            assert_eq!(
                FPDF_GetXFAPacketContent(
                    document,
                    index,
                    content.as_mut_ptr().cast(),
                    16,
                    &mut size
                ),
                0
            );
            assert_eq!(size, 999);
            assert_eq!(
                FPDF_GetXFAPacketName(document, index, name.as_mut_ptr().cast(), 16),
                0
            );
        }
        assert_eq!(
            FPDF_GetXFAPacketContent(document, 0, std::ptr::null_mut(), 0, std::ptr::null_mut()),
            0
        );
        assert_eq!(FPDF_GetXFAPacketCount(std::ptr::null_mut()), -1);
        FPDF_CloseDocument(document);
    }
}

#[test]
fn native_trailer_offsets_and_short_output_preservation() {
    unsafe {
        for (name, expected) in [
            ("two_signatures.pdf", &[633, 1703, 2781][..]),
            ("hello_world.pdf", &[840][..]),
            ("annotation_stamp_with_ap.pdf", &[441, 7945, 101719][..]),
            ("linearized.pdf", &[474, 11384][..]),
            ("trailer_end_trailing_space.pdf", &[1193][..]),
        ] {
            let file = std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../upstream/pdfium/testing/resources")
                    .join(name),
            )
            .unwrap();
            let document =
                FPDF_LoadMemDocument64(file.as_ptr().cast(), file.len(), std::ptr::null());
            assert!(!document.is_null(), "{name}");
            let count = FPDF_GetTrailerEnds(document, std::ptr::null_mut(), 0);
            assert_eq!(count, expected.len() as c_ulong, "{name}");
            let mut ends = vec![0; expected.len()];
            assert_eq!(
                FPDF_GetTrailerEnds(document, ends.as_mut_ptr(), count),
                count
            );
            assert_eq!(ends, expected, "{name}");
            ends.fill(0xa5);
            assert_eq!(
                FPDF_GetTrailerEnds(document, ends.as_mut_ptr(), count - 1),
                count
            );
            assert_eq!(ends, vec![0xa5; expected.len()]);
            FPDF_CloseDocument(document);
        }
        assert_eq!(
            FPDF_GetTrailerEnds(std::ptr::null_mut(), std::ptr::null_mut(), 0),
            0
        );
        let empty = FPDF_CreateNewDocument();
        assert_eq!(FPDF_GetTrailerEnds(empty, std::ptr::null_mut(), 0), 0);
        FPDF_CloseDocument(empty);
    }
}

#[test]
fn trailer_scan_releases_document_borrow_before_host_read() {
    struct Source {
        bytes: Vec<u8>,
        document: Cell<*mut c_void>,
        called: Cell<bool>,
    }
    extern "C" fn read(
        param: *mut c_void,
        offset: c_ulong,
        buffer: *mut u8,
        length: c_ulong,
    ) -> c_int {
        let source = unsafe { &*param.cast::<Source>() };
        let document = source.document.get();
        if !document.is_null() {
            source.called.set(true);
            assert_eq!(unsafe { FPDF_GetPageCount(document) }, 1);
            assert!(unsafe { &*document.cast::<DocumentHandle>() }
                .document
                .try_borrow_mut()
                .is_ok());
        }
        let Some(bytes) = (offset as usize)
            .checked_add(length as usize)
            .and_then(|end| source.bytes.get(offset as usize..end))
        else {
            return 0;
        };
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, bytes.len());
        }
        1
    }
    unsafe {
        let mut source = Source {
            bytes: std::fs::read(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../upstream/pdfium/testing/resources/hello_world.pdf"),
            )
            .unwrap(),
            document: Cell::new(std::ptr::null_mut()),
            called: Cell::new(false),
        };
        let api = FPDF_FILEACCESS {
            m_FileLen: source.bytes.len() as c_ulong,
            m_GetBlock: Some(read),
            m_Param: std::ptr::from_mut(&mut source).cast(),
        };
        let document = FPDF_LoadCustomDocument(&api, std::ptr::null());
        assert!(!document.is_null());
        source.document.set(document);
        let mut end = 0;
        assert_eq!(FPDF_GetTrailerEnds(document, &mut end, 1), 1);
        assert_eq!(end, 840);
        assert!(source.called.get());
        source.document.set(std::ptr::null_mut());
        FPDF_CloseDocument(document);
    }
}
