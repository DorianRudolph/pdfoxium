// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Connected save/reopen encryption regressions. Fixtures and password/ID
//! expectations follow `cpdf_security_handler_embeddertest.cpp`; these checks
//! exercise the engine creator and parsed text rather than claiming a complete
//! port of the native C ABI/rendering embedder suite.

use super::*;
use crate::fpdfapi::page::cpdf_page::CpdfPage;
use crate::fpdfapi::page::cpdf_pagemodule::{destroy_page_module, initialize_page_module};
use crate::fpdfapi::page::cpdf_textobject::CpdfTextObject;
use crate::fpdfapi::parser::cpdf_parser::ParserError;
use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;

const AGE_UTF8: &[u8] = b"\xc3\xa2ge";
const AGE_LATIN1: &[u8] = b"\xe2ge";
const HOTEL_UTF8: &[u8] = b"h\xc3\xb4tel";
const HOTEL_LATIN1: &[u8] = b"h\xf4tel";
const PASSWORDS: [&[u8]; 4] = [AGE_UTF8, AGE_LATIN1, HOTEL_UTF8, HOTEL_LATIN1];
const MARKER: &[u8] = b"phase-two saved string\0\xff";

struct PageModule;
impl PageModule {
    fn new() -> Self {
        initialize_page_module();
        Self
    }
}
impl Drop for PageModule {
    fn drop(&mut self) {
        destroy_page_module();
    }
}
#[derive(Default)]
struct Sink(Vec<u8>);
impl WriteStream for Sink {
    fn write_block(&mut self, bytes: &[u8]) -> bool {
        self.0.extend_from_slice(bytes);
        true
    }
}
fn fixture(revision: i32) -> &'static [u8] {
    match revision {
        2 => include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/encrypted_hello_world_r2.pdf"
        )),
        3 => include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/encrypted_hello_world_r3.pdf"
        )),
        5 => include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/encrypted_hello_world_r5.pdf"
        )),
        6 => include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/encrypted_hello_world_r6.pdf"
        )),
        _ => unreachable!(),
    }
}
fn open(bytes: &[u8], password: &[u8]) -> Rc<RefCell<CpdfDocument>> {
    let mut doc = CpdfDocument::new();
    assert_eq!(
        doc.load_doc(
            Rc::new(CfxReadOnlyContainerStream::new(bytes.to_vec())),
            password
        ),
        ParserError::Success
    );
    Rc::new(RefCell::new(doc))
}
fn text_snapshot(doc: &Rc<RefCell<CpdfDocument>>) -> Vec<Vec<u32>> {
    assert_eq!(doc.borrow().page_count(), 1);
    let mut page = CpdfPage::from_document(&mut doc.borrow_mut(), 0).unwrap();
    page.parse_path_content();
    page.holder()
        .iter()
        .filter_map(|o| {
            o.as_any()
                .downcast_ref::<CpdfTextObject>()
                .map(|text| text.char_codes().to_vec())
        })
        .collect()
}
fn mark(doc: &Rc<RefCell<CpdfDocument>>) {
    let root = doc.borrow().root().unwrap();
    root.borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_new_for(b"Phase2SaveMarker", CpdfString::from_bytes(None, MARKER));
}
fn marker(doc: &Rc<RefCell<CpdfDocument>>) -> Vec<u8> {
    let root = doc.borrow().root().unwrap();
    let value = dict(&root, |d| d.get_byte_string_for(b"Phase2SaveMarker", b""));
    value
}
fn ids(doc: &Rc<RefCell<CpdfDocument>>) -> [Vec<u8>; 2] {
    let ids = doc.borrow().parser().id_array().unwrap();
    array(&ids, |a| [a.get_byte_string_at(0), a.get_byte_string_at(1)])
}
fn save(doc: &Rc<RefCell<CpdfDocument>>, flags: u32) -> Vec<u8> {
    let mut sink = Sink::default();
    {
        let mut creator = CpdfCreator::new(Rc::clone(doc), &mut sink);
        assert!(creator.create(flags, 0));
    } // The native archive also flushes at destruction.
    sink.0
}
fn remove_id(doc: &Rc<RefCell<CpdfDocument>>) {
    let trailer = doc.borrow().parser().trailer().unwrap();
    assert!(trailer
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .remove_for(b"ID")
        .is_some());
    assert!(doc.borrow().parser().id_array().is_none());
}

#[test]
fn full_save_preserves_all_password_encodings_cipher_and_decrypted_contents() {
    let _module = PageModule::new();
    for revision in [2, 3, 5, 6] {
        for opening_password in [AGE_UTF8, HOTEL_LATIN1] {
            let doc = open(fixture(revision), opening_password);
            let original_text = text_snapshot(&doc);
            assert_eq!(original_text.len(), 2);
            let original_ids = ids(&doc);
            mark(&doc);
            let saved = save(&doc, CpdfCreator::NONE);
            for password in PASSWORDS {
                let reopened = open(&saved, password);
                assert_eq!(
                    reopened
                        .borrow()
                        .parser()
                        .security_handler_ref()
                        .unwrap()
                        .revision(),
                    Some(revision)
                );
                assert_eq!(ids(&reopened)[0], original_ids[0]);
                assert_eq!(marker(&reopened), MARKER);
                assert_eq!(text_snapshot(&reopened), original_text);
            }
            assert!(doc.borrow().parser().security_handler_ref().is_some());
        }
    }
}

#[test]
fn remove_security_save_reopens_without_password_and_preserves_source_security() {
    let _module = PageModule::new();
    for revision in [2, 3, 5, 6] {
        for flags in [
            CpdfCreator::REMOVE_SECURITY,
            CpdfCreator::REMOVE_SECURITY_DEPRECATED,
        ] {
            let doc = open(fixture(revision), HOTEL_UTF8);
            let original_text = text_snapshot(&doc);
            let original_security = doc.borrow().parser().security_handler_ref().unwrap();
            mark(&doc);
            let saved = save(&doc, flags);
            let reopened = open(&saved, b"");
            assert!(reopened.borrow().parser().encrypt_dict().is_none());
            assert!(reopened.borrow().parser().security_handler_ref().is_none());
            assert_eq!(marker(&reopened), MARKER);
            assert_eq!(text_snapshot(&reopened), original_text);
            assert!(Rc::ptr_eq(
                &doc.borrow().parser().security_handler_ref().unwrap(),
                &original_security
            ));
        }
    }
}

#[test]
fn encrypted_incremental_save_preserves_original_bytes_and_both_ids() {
    let _module = PageModule::new();
    for revision in [2, 3, 5, 6] {
        let original = fixture(revision);
        let doc = open(original, HOTEL_UTF8);
        let original_text = text_snapshot(&doc);
        let original_ids = ids(&doc);
        let original_xref = doc.borrow().parser().last_xref_offset();
        mark(&doc);
        let saved = save(&doc, CpdfCreator::INCREMENTAL);
        assert!(saved.starts_with(original));
        let reopened = open(&saved, AGE_LATIN1);
        assert_eq!(ids(&reopened), original_ids);
        // Read the saved revision itself: native trailer merging consumes
        // /Prev as it follows the chain back to the original revision.
        let trailer = super::roundtrip_tests::saved_table_trailer(&saved);
        assert_eq!(
            dict(&trailer, |d| d.get_integer_for(b"Prev", 0)),
            original_xref as i32
        );
        assert_eq!(marker(&reopened), MARKER);
        assert_eq!(text_snapshot(&reopened), original_text);
        assert_eq!(
            reopened
                .borrow()
                .parser()
                .security_handler_ref()
                .unwrap()
                .revision(),
            Some(revision)
        );
    }
}

#[test]
fn missing_id_legacy_recreation_uses_encoded_password_and_keeps_owner_entry() {
    let _module = PageModule::new();
    for revision in [2, 3, 5, 6] {
        for (opening_password, is_owner) in [(AGE_UTF8, true), (HOTEL_LATIN1, false)] {
            let doc = open(fixture(revision), opening_password);
            let original_text = text_snapshot(&doc);
            let original_encrypt = doc.borrow().parser().encrypt_dict().unwrap();
            let owner_entry = dict(&original_encrypt, |d| d.get_byte_string_for(b"O", b""));
            remove_id(&doc);
            mark(&doc);
            let saved = save(&doc, CpdfCreator::NONE);
            let valid_passwords: &[&[u8]] = if revision <= 3 && is_owner {
                &PASSWORDS[..2]
            } else {
                &PASSWORDS
            };
            for &password in valid_passwords {
                let reopened = open(&saved, password);
                let new_encrypt = reopened.borrow().parser().encrypt_dict().unwrap();
                assert_eq!(
                    dict(&new_encrypt, |d| d.get_byte_string_for(b"O", b"")),
                    owner_entry
                );
                assert_eq!(ids(&reopened)[0], ids(&reopened)[1]);
                assert_eq!(marker(&reopened), MARKER);
                assert_eq!(text_snapshot(&reopened), original_text);
            }
            if revision <= 3 && is_owner {
                let mut no_user = CpdfDocument::new();
                assert_eq!(
                    no_user.load_doc(Rc::new(CfxReadOnlyContainerStream::new(saved)), HOTEL_UTF8),
                    ParserError::PasswordError
                );
            }
            assert!(Rc::ptr_eq(
                &doc.borrow().parser().encrypt_dict().unwrap(),
                &original_encrypt
            ));
        }
    }
}
