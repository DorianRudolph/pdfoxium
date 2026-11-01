// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Security lifetime across document-initialization recovery. The ordinary
//! parser initializer is a real callback; these tests control its failure or
//! mutation without making impossible read/allocation states.
use super::super::cpdf_string::CpdfString;
use super::*;
use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;

fn original(revision: i32) -> Vec<u8> {
    std::fs::read(format!(
        "{}/../../upstream/pdfium/testing/resources/encrypted_hello_world_r{revision}.pdf",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}
fn parse(parser: &mut CpdfParser, bytes: Vec<u8>) -> ParserError {
    parser.start_parse(Rc::new(CfxReadOnlyContainerStream::new(bytes)), b"h\xf4tel")
}
fn check_decrypted_content(parser: &CpdfParser) {
    assert_eq!(parser.root_obj_num(), 1);
    let object = parser
        .parse_indirect_object(6)
        .expect("Recovered encrypted content stream");
    let borrowed = object.borrow();
    let mut data = CpdfStreamAcc::new(borrowed.as_any().downcast_ref::<CpdfStream>().unwrap());
    data.load_all_data_filtered();
    for expected in [
        b"(Hello, world!)".as_slice(),
        b"(Goodbye, world!)".as_slice(),
    ] {
        assert!(data
            .get_span()
            .windows(expected.len())
            .any(|part| part == expected));
    }
}
fn rewrite_filter(parser: &CpdfParser) {
    let encrypt = parser.encrypt_dict().unwrap();
    encrypt
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_new_name_for(b"Filter", b"UnavailableSecurityHandler");
}
fn remove_root(parser: &CpdfParser) {
    let trailer = parser.trailer().unwrap();
    trailer
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .remove_for(b"Root");
}

#[test]
fn initialization_retry_recreates_the_handler_without_changing_its_authenticated_key() {
    for revision in [2, 3] {
        let seen = Rc::new(RefCell::new(Vec::<Rc<CpdfSecurityHandler>>::new()));
        let observed = seen.clone();
        let mut parser = CpdfParser::new();
        parser.set_try_init(Some(Rc::new(move |parser| {
            observed.borrow_mut().push(
                parser
                    .security_handler_ref()
                    .expect("Security precedes initialization"),
            );
            observed.borrow().len() > 1
        })));
        assert_eq!(parse(&mut parser, original(revision)), ParserError::Success);
        let handlers = seen.borrow();
        assert_eq!(handlers.len(), 2);
        // Retain the first allocation so allocator address reuse cannot fake
        // replacement. Recovery must clear it before recreating encryption.
        assert!(!Rc::ptr_eq(&handlers[0], &handlers[1]));
        assert_eq!(handlers[0].encrypt_key(), handlers[1].encrypt_key());
        assert!(Rc::ptr_eq(
            &parser.security_handler_ref().unwrap(),
            &handlers[1]
        ));
        assert_eq!(
            parser.permissions(false),
            if revision == 2 {
                0xffffffc0
            } else {
                0xfffff0c0
            }
        );
        check_decrypted_content(&parser);
    }
}

#[test]
fn a_root_invalidated_by_the_initializer_is_rebuilt_before_further_decryption() {
    for revision in [2, 3] {
        let seen = Rc::new(RefCell::new(None));
        let observed = seen.clone();
        let calls = Rc::new(Cell::new(0usize));
        let callback_calls = calls.clone();
        let mut parser = CpdfParser::new();
        parser.set_try_init(Some(Rc::new(move |parser| {
            callback_calls.set(callback_calls.get() + 1);
            *observed.borrow_mut() = parser.security_handler_ref();
            remove_root(parser);
            true
        })));
        assert_eq!(parse(&mut parser, original(revision)), ParserError::Success);
        assert_eq!(calls.get(), 1);
        let previous = seen.borrow();
        let current = parser.security_handler_ref().unwrap();
        assert!(!Rc::ptr_eq(previous.as_ref().unwrap(), &current));
        assert_eq!(
            previous.as_ref().unwrap().encrypt_key(),
            current.encrypt_key()
        );
        check_decrypted_content(&parser);
    }
}

#[test]
fn a_handler_error_during_reauthentication_does_not_leave_the_original_handler_installed() {
    for revision in [2, 3] {
        let calls = Rc::new(Cell::new(0usize));
        let observed = calls.clone();
        let mut parser = CpdfParser::new();
        parser.set_try_init(Some(Rc::new(move |parser| {
            observed.set(observed.get() + 1);
            assert!(parser.security_handler_ref().is_some());
            rewrite_filter(parser);
            false
        })));
        assert_eq!(
            parse(&mut parser, original(revision)),
            ParserError::HandlerError
        );
        assert_eq!(calls.get(), 1);
        assert!(parser.security_handler_ref().is_none());
        let mut healthy = CpdfParser::new();
        assert_eq!(
            parse(&mut healthy, original(revision)),
            ParserError::Success
        );
        check_decrypted_content(&healthy);
    }
}

#[test]
fn failed_reauthentication_of_changed_credentials_does_not_keep_a_stale_decryption_key() {
    for revision in [2, 3] {
        let calls = Rc::new(Cell::new(0usize));
        let observed = calls.clone();
        let mut parser = CpdfParser::new();
        parser.set_try_init(Some(Rc::new(move |parser| {
            observed.set(observed.get() + 1);
            let encrypt = parser.encrypt_dict().unwrap();
            encrypt
                .borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap()
                .set_new_for(b"U", CpdfString::from_bytes(None, b""));
            false
        })));
        assert_eq!(
            parse(&mut parser, original(revision)),
            ParserError::PasswordError
        );
        assert_eq!(calls.get(), 1);
        assert!(parser.security_handler_ref().is_none());
    }
}

#[test]
fn invalid_root_repair_also_propagates_a_reauthentication_handler_error() {
    for revision in [2, 3] {
        let calls = Rc::new(Cell::new(0usize));
        let observed = calls.clone();
        let mut parser = CpdfParser::new();
        parser.set_try_init(Some(Rc::new(move |parser| {
            observed.set(observed.get() + 1);
            remove_root(parser);
            rewrite_filter(parser);
            true
        })));
        assert_eq!(
            parse(&mut parser, original(revision)),
            ParserError::HandlerError
        );
        assert_eq!(calls.get(), 1);
        assert!(parser.security_handler_ref().is_none());
    }
}

#[test]
fn an_already_rebuilt_cross_reference_does_not_repeat_a_failed_initializer() {
    for revision in [2, 3] {
        let mut bytes = original(revision);
        let marker = b"startxref\n";
        let at = bytes
            .windows(marker.len())
            .rposition(|part| part == marker)
            .unwrap()
            + marker.len();
        let end = at + bytes[at..].iter().position(|&c| c == b'\n').unwrap();
        bytes[at..end].fill(b'0');
        let calls = Rc::new(Cell::new(0usize));
        let observed = calls.clone();
        let mut parser = CpdfParser::new();
        parser.set_try_init(Some(Rc::new(move |_| {
            observed.set(observed.get() + 1);
            false
        })));
        assert_eq!(parse(&mut parser, bytes), ParserError::FormatError);
        assert_eq!(calls.get(), 1);
    }
}
