// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Save-creation contracts from `CPDF_SecurityHandler::OnCreate()` and its AES
//! helpers. Literal vectors were calculated independently with Python hashlib,
//! a standalone RC4 implementation and OpenSSL AES-CBC (no padding), using raw
//! random bytes 00..0f followed by permission-suffix bytes 10..13.

use super::*;
use crate::fpdfapi::parser::cpdf_object::retain_object;
use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

const PASSWORD: &[u8] = b"user\0\xff";
const FILE_ID: &[u8] = b"file-id\0\xff";
const FILE_KEY: &str = "be45cb2605bf36bebde684841a28f0fd43c69850a3dce5fedba69928ee3a8991";

fn unhex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let n = |b: u8| if b <= b'9' { b - b'0' } else { b - b'a' + 10 };
            n(pair[0]) * 16 + n(pair[1])
        })
        .collect()
}

fn dict(revision: i32, metadata: bool) -> ObjectRef {
    let mut d = CpdfDictionary::new();
    d.set_new_name_for(b"Filter", b"Standard");
    d.set_new_for(
        b"V",
        CpdfScalarObject::number_from_signed(if revision == 2 {
            1
        } else if revision == 3 {
            2
        } else if revision == 4 {
            4
        } else {
            5
        }),
    );
    d.set_new_for(b"R", CpdfScalarObject::number_from_signed(revision));
    d.set_new_for(b"P", CpdfScalarObject::number_from_signed(-4));
    d.set_new_for(b"EncryptMetadata", CpdfScalarObject::boolean(metadata));
    d.set_new_for(
        b"Length",
        CpdfScalarObject::number_from_signed(if revision == 2 {
            40
        } else if revision <= 4 {
            128
        } else {
            256
        }),
    );
    d.set_new_for(
        b"O",
        CpdfString::from_bytes(
            None,
            &(0..if revision >= 5 { 48 } else { 32 }).collect::<Vec<u8>>(),
        ),
    );
    d.set_new_for(b"OE", CpdfString::from_bytes(None, &[0xee; 32]));
    if revision >= 4 {
        d.set_new_name_for(b"StmF", b"StdCF");
        d.set_new_name_for(b"StrF", b"StdCF");
        let mut filter = CpdfDictionary::new();
        filter.set_new_name_for(b"CFM", if revision == 4 { b"AESV2" } else { b"AESV3" });
        filter.set_new_for(
            b"Length",
            CpdfScalarObject::number_from_signed(if revision == 4 { 16 } else { 32 }),
        );
        let mut filters = CpdfDictionary::new();
        filters.set_for(b"StdCF", Some(retain_object(filter)));
        d.set_for(b"CF", Some(retain_object(filters)));
    }
    retain_object(d)
}
fn id(value: &[u8]) -> ObjectRef {
    let mut a = CpdfArray::new();
    a.append_new(CpdfString::from_bytes(None, value));
    retain_object(a)
}
fn entry(dict: &ObjectRef, key: &[u8]) -> Vec<u8> {
    dict.borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .unwrap()
        .get_byte_string_for(key, b"")
}
fn set_integer(dict: &ObjectRef, key: &[u8], value: i32) {
    dict.borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_new_for(key, CpdfScalarObject::number_from_signed(value));
}
fn create_fixed(
    handler: &mut CpdfSecurityHandler,
    dict: ObjectRef,
    id: Option<&ObjectRef>,
    password: &[u8],
) {
    let mut calls = 0;
    handler.on_create_with_random(dict, id, password, &mut |words| {
        match calls {
            0 => {
                assert_eq!(words.len(), 4);
                for (i, word) in words.iter_mut().enumerate() {
                    let offset = (i * 4) as u8;
                    *word = u32::from_ne_bytes([offset, offset + 1, offset + 2, offset + 3]);
                }
            }
            1 => {
                assert_eq!(words.len(), 1);
                words[0] = u32::from_ne_bytes([0x10, 0x11, 0x12, 0x13]);
            }
            _ => panic!("unexpected random call"),
        }
        calls += 1;
    });
    assert_eq!(calls, 2);
}

#[test]
fn legacy_create_matches_independent_vectors_and_reopens_password() {
    for (revision, metadata, key, user) in [
        (
            2,
            true,
            "d252bd83ae",
            "0db734a7cbb0252fa16e1f14b338d4594da28cf3515027e949bbda708b5d3d1a",
        ),
        (
            2,
            false,
            "d252bd83ae",
            "0db734a7cbb0252fa16e1f14b338d4594da28cf3515027e949bbda708b5d3d1a",
        ),
        (
            3,
            true,
            "f21a9bd3529af906f1e8ed470f381c4f",
            "b406434346877ee57938ee23b6d4b89971d27063428e8a53bfe116d57e9cc537",
        ),
        (
            3,
            false,
            "6ac83b42c1ad9fe70b847ea041762ec0",
            "3a00dea44b8aa067c9e50406fa7a3ad4cf9869c2dd0adb136677dce3c0951f14",
        ),
        (
            4,
            true,
            "f21a9bd3529af906f1e8ed470f381c4f",
            "b406434346877ee57938ee23b6d4b89971d27063428e8a53bfe116d57e9cc537",
        ),
        (
            4,
            false,
            "6ac83b42c1ad9fe70b847ea041762ec0",
            "3a00dea44b8aa067c9e50406fa7a3ad4cf9869c2dd0adb136677dce3c0951f14",
        ),
    ] {
        let encrypt = dict(revision, metadata);
        let owner = entry(&encrypt, b"O");
        let ids = id(FILE_ID);
        let mut writer = CpdfSecurityHandler::new();
        writer.on_create_with_random(Rc::clone(&encrypt), Some(&ids), PASSWORD, &mut |_| {
            panic!("legacy creation must not request random words")
        });
        assert_eq!(
            writer.encrypt_key().unwrap(),
            unhex(key),
            "revision {revision}, metadata {metadata}"
        );
        assert_eq!(entry(&encrypt, b"U"), unhex(user));
        assert_eq!(entry(&encrypt, b"O"), owner);
        assert!(writer.state.as_ref().unwrap().file_id.is_empty()); // Local ID only.
        let mut reader = CpdfSecurityHandler::new();
        assert!(reader.on_init(Rc::clone(&encrypt), Some(&ids), PASSWORD));
        assert_eq!(reader.encrypt_key(), writer.encrypt_key());
        assert_eq!(reader.get_encoded_password(PASSWORD), PASSWORD);
        let plaintext = b"a PDF string\0with binary\xff";
        let encrypted = writer
            .crypto_handler()
            .unwrap()
            .encrypt_content(7, 2, plaintext);
        assert_eq!(
            reader
                .crypto_handler()
                .unwrap()
                .decrypt_content(7, 2, &encrypted),
            plaintext
        );
        let mut wrong = CpdfSecurityHandler::new();
        assert!(!wrong.on_init(encrypt, Some(&ids), b"wrong"));
    }
}

#[test]
fn aes256_creation_matches_fixed_vectors_and_preserves_pinned_early_return() {
    for (revision, user, user_key) in [
        (5, "3e71e40f05b01f54a32a3108d5c6ab1d99afb28546699bf3256913efd24a6a0262264d440595e3f33eec6cdc5393c6fe", "401bcf46ce8987bcd30bedf4cb35cf53879227bda45620d2dc12cc07eccf6646"),
        (6, "ef24ba706b4244b1ad541c3ebe90edd4b885d503a187dd1fe9aaccce601564b262264d440595e3f33eec6cdc5393c6fe", "21e11ce796c5f02eb5c156b23a2a25f14463b44018ed664d4337ab3b1ccb303a"),
    ] {
        for metadata in [true, false] {
            let encrypt = dict(revision, metadata);
            let owner = entry(&encrypt, b"O");
            let owner_key = entry(&encrypt, b"OE");
            let mut handler = CpdfSecurityHandler::new();
            create_fixed(&mut handler, Rc::clone(&encrypt), None, PASSWORD);
            assert_eq!(handler.encrypt_key().unwrap(), unhex(FILE_KEY));
            assert_eq!(entry(&encrypt, b"U"), unhex(user));
            assert_eq!(entry(&encrypt, b"UE"), unhex(user_key));
            assert_eq!(entry(&encrypt, b"Perms"), unhex(if metadata { "82b72844cd60623668bafccb4ab93f0c" } else { "51574a655a54f830fc90ae42ffa026c8" }));
            assert_eq!(entry(&encrypt, b"O"), owner);
            assert_eq!(entry(&encrypt, b"OE"), owner_key);
            assert!(handler.crypto_handler().is_none());
            let mut reopened = CpdfSecurityHandler::new();
            assert!(reopened.on_init(encrypt, None, PASSWORD));
            assert_eq!(reopened.encrypt_key().unwrap(), unhex(FILE_KEY));
        }
    }
}

#[test]
fn aes256_real_random_creation_keeps_long_binary_passwords_untruncated() {
    for revision in [5, 6] {
        let mut password = vec![b'p'; 160];
        password.extend_from_slice(b"\0\xff\xc0");
        let encrypt = dict(revision, false);
        let mut handler = CpdfSecurityHandler::new();
        handler.on_create(Rc::clone(&encrypt), None, &password);
        let mut reopened = CpdfSecurityHandler::new();
        assert!(reopened.on_init(Rc::clone(&encrypt), None, &password));
        assert_eq!(reopened.encrypt_key(), handler.encrypt_key());
        assert_eq!(reopened.get_encoded_password(&password), password);
        let mut truncated = CpdfSecurityHandler::new();
        assert!(!truncated.on_init(encrypt, None, &password[..127]));
    }
}

#[test]
fn legacy_missing_id_and_empty_id_array_match_without_changing_stored_id() {
    let empty_ids = retain_object(CpdfArray::new());
    for revision in [2, 3, 4] {
        let a = dict(revision, true);
        let b = dict(revision, true);
        let mut first = CpdfSecurityHandler::new();
        first.on_create(Rc::clone(&a), None, PASSWORD);
        let mut second = CpdfSecurityHandler::new();
        second.on_create(Rc::clone(&b), Some(&empty_ids), PASSWORD);
        assert_eq!(first.encrypt_key(), second.encrypt_key());
        assert_eq!(entry(&a, b"U"), entry(&b, b"U"));
        let mut reopened = CpdfSecurityHandler::new();
        assert!(reopened.on_init(a, None, PASSWORD));
    }
}

#[test]
fn invalid_create_retains_source_partial_load_state_and_old_crypto() {
    let ids = id(FILE_ID);
    let original = dict(2, true);
    let mut handler = CpdfSecurityHandler::new();
    handler.on_create(Rc::clone(&original), Some(&ids), PASSWORD);
    let original_key = handler.encrypt_key().unwrap().to_vec();
    let original_ciphertext = handler
        .crypto_handler()
        .unwrap()
        .encrypt_content(3, 0, b"state");
    for (version, revision, length) in [(2, 3, -8), (2, 3, 8), (4, 4, 136), (5, 5, 248)] {
        let invalid = dict(revision, false);
        set_integer(&invalid, b"V", version);
        set_integer(&invalid, b"Length", length);
        // V4 obtains Length from StdCF before top-level Length; make it invalid too.
        if version == 4 {
            let filters = invalid
                .borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .unwrap()
                .get_dict_for(b"CF")
                .unwrap();
            let filter = filters
                .borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .unwrap()
                .get_dict_for(b"StdCF")
                .unwrap();
            set_integer(&filter, b"Length", length);
        }
        let owner = entry(&invalid, b"O");
        handler.on_create_with_random(Rc::clone(&invalid), None, PASSWORD, &mut |_| {
            panic!("failed validation must not request randomness")
        });
        assert_eq!(handler.version(), version);
        assert_eq!(handler.revision(), Some(revision));
        assert_eq!(handler.cipher(), Some(Cipher::Rc4));
        assert_eq!(handler.encrypt_key().unwrap(), original_key);
        assert!(Rc::ptr_eq(handler.encrypt_dict.as_ref().unwrap(), &invalid));
        assert!(entry(&invalid, b"U").is_empty());
        assert_eq!(entry(&invalid, b"O"), owner);
        assert_eq!(
            handler
                .crypto_handler()
                .unwrap()
                .encrypt_content(3, 0, b"state"),
            original_ciphertext
        );
    }
    let invalid = dict(4, true);
    invalid
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_new_name_for(b"StmF", b"DifferentFromStrF");
    handler.on_create_with_random(invalid, None, PASSWORD, &mut |_| {
        panic!("mismatched filters must fail before randomness")
    });
    assert_eq!(handler.revision(), Some(4));
    assert_eq!(handler.encrypt_key().unwrap(), original_key);
}

#[test]
fn aes256_recreation_keeps_old_crypto_authentication_and_file_id_fields() {
    let ids = id(FILE_ID);
    let original = dict(2, true);
    let mut initial = CpdfSecurityHandler::new();
    initial.on_create(Rc::clone(&original), Some(&ids), PASSWORD);
    let mut handler = CpdfSecurityHandler::new();
    assert!(handler.on_init(original, Some(&ids), PASSWORD));
    handler.owner_unlocked = true;
    let old_ciphertext =
        handler
            .crypto_handler()
            .unwrap()
            .encrypt_content(8, 0, b"keep old handler");
    for revision in [5, 6] {
        create_fixed(
            &mut handler,
            dict(revision, false),
            Some(&id(b"different ID")),
            PASSWORD,
        );
        assert_eq!(handler.state.as_ref().unwrap().file_id, FILE_ID);
        assert!(handler.owner_unlocked);
        assert_eq!(handler.get_encoded_password(PASSWORD), PASSWORD);
        assert_eq!(handler.encrypt_key().unwrap(), unhex(FILE_KEY));
        assert_eq!(handler.cipher(), Some(Cipher::Aes));
        assert!(!handler.crypto_handler().unwrap().is_cipher_aes());
        assert_eq!(
            handler
                .crypto_handler()
                .unwrap()
                .encrypt_content(8, 0, b"keep old handler"),
            old_ciphertext
        );
    }
}
