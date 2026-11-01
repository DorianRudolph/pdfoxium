// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/parser/cpdf_security_handler.{h,cpp}`: crypt-filter
//! validation, revisions 2-6 password verification, document-load initialization
//! and the pinned save-path `OnCreate()` behavior.

use std::rc::Rc;

use super::cpdf_array::CpdfArray;
use super::cpdf_crypto_handler::{Cipher, CpdfCryptoHandler};
use super::cpdf_dictionary::CpdfDictionary;
use super::cpdf_object::ObjectRef;
use super::cpdf_string::CpdfString;
use crate::fdrm::fx_crypt::{crypt_arc_four_crypt_block, crypt_md5_generate, CryptMd5Context};
use crate::fdrm::fx_crypt_aes::CryptAesContext;
use crate::fdrm::fx_crypt_sha::{
    crypt_sha256_generate, crypt_sha384_generate, crypt_sha512_generate, CryptSha1Context,
    CryptSha2Context,
};
use crate::fxcrt::fx_random;
use crate::fxcrt::fx_string::utf8_decode;

pub const DEFAULT_PASSCODE: [u8; 32] = [
    0x28, 0xbf, 0x4e, 0x5e, 0x4e, 0x75, 0x8a, 0x41, 0x64, 0x00, 0x4e, 0x56, 0xff, 0xfa, 0x01, 0x08,
    0x2e, 0x2e, 0x00, 0xb6, 0xd0, 0x68, 0x3e, 0x80, 0x2f, 0x0c, 0xa9, 0xfe, 0x64, 0x53, 0x69, 0x7a,
];

pub fn get_passcode(password: &[u8]) -> [u8; 32] {
    let mut output = [0u8; 32];
    let len = password.len().min(32);
    output[..len].copy_from_slice(&password[..len]);
    output[len..].copy_from_slice(&DEFAULT_PASSCODE[..32 - len]);
    output
}

pub fn calc_encrypt_key(
    encrypt: &CpdfDictionary,
    password: &[u8],
    key_len: usize,
    ignore_metadata: bool,
    file_id: &[u8],
) -> Vec<u8> {
    assert!(key_len <= 32);
    let mut md5 = CryptMd5Context::start();
    md5.update(&get_passcode(password));
    md5.update(&encrypt.get_byte_string_for(b"O", b""));
    let permission = encrypt.get_integer_for(b"P", 0) as u32;
    md5.update(&permission.to_ne_bytes());
    md5.update(file_id);
    let revision_3_or_greater = encrypt.get_integer_for(b"R", 0) >= 3;
    if !ignore_metadata
        && revision_3_or_greater
        && !encrypt.get_boolean_for(b"EncryptMetadata", true)
    {
        md5.update(&u32::MAX.to_ne_bytes());
    }
    let mut digest = md5.finish();
    let copy_len = key_len.min(digest.len());
    if revision_3_or_greater {
        for _ in 0..50 {
            digest = crypt_md5_generate(&digest[..copy_len]);
        }
    }
    let mut key = vec![0u8; key_len];
    key[..copy_len].copy_from_slice(&digest[..copy_len]);
    key
}

fn valid_key_length(cipher: Cipher, key_len: usize) -> bool {
    match cipher {
        Cipher::Aes => matches!(key_len, 16 | 24 | 32),
        Cipher::Aes2 => key_len == 32,
        Cipher::Rc4 => (5..=16).contains(&key_len),
        Cipher::None => true,
    }
}

pub fn load_crypt_info(encrypt: &CpdfDictionary, name: &[u8]) -> Option<(Cipher, usize)> {
    let version = encrypt.get_integer_for(b"V", 0);
    let mut cipher = Cipher::Rc4;
    let key_len;
    if version >= 4 {
        let crypt_filters = encrypt.get_dict_for(b"CF")?;
        let filters_borrow = crypt_filters.borrow();
        let filters = filters_borrow.as_any().downcast_ref::<CpdfDictionary>()?;
        if crate::fxcrt::bytestring::eq_cstr(name, b"Identity") {
            cipher = Cipher::None;
            key_len = 0;
        } else {
            let def_filter = filters.get_dict_for(name)?;
            let filter_borrow = def_filter.borrow();
            let filter = filter_borrow.as_any().downcast_ref::<CpdfDictionary>()?;
            let mut key_bits = if version == 4 {
                let n = filter.get_integer_for(b"Length", 0);
                if n == 0 {
                    encrypt.get_integer_for(b"Length", 128)
                } else {
                    n
                }
            } else {
                encrypt.get_integer_for(b"Length", 256)
            };
            if key_bits < 0 {
                return None;
            }
            if key_bits < 40 {
                key_bits *= 8;
            }
            key_len = key_bits / 8;
            let cipher_name = filter.get_byte_string_for(b"CFM", b"");
            if crate::fxcrt::bytestring::eq_cstr(&cipher_name, b"AESV2")
                || crate::fxcrt::bytestring::eq_cstr(&cipher_name, b"AESV3")
            {
                cipher = Cipher::Aes;
            }
        }
    } else {
        key_len = if version > 1 {
            encrypt.get_integer_for(b"Length", 40) / 8
        } else {
            5
        };
    }
    if !(0..=32).contains(&key_len) || !valid_key_length(cipher, key_len as usize) {
        return None;
    }
    Some((cipher, key_len as usize))
}

pub struct SecurityState {
    pub revision: i32,
    pub permissions: u32,
    pub key_len: usize,
    pub cipher: Cipher,
    pub file_id: Vec<u8>,
    pub encrypt_key: [u8; 32],
}

impl SecurityState {
    pub fn from_dict(encrypt: &CpdfDictionary, file_id: &[u8]) -> Option<Self> {
        let version = encrypt.get_integer_for(b"V", 0);
        let (cipher, key_len) = if version < 4 {
            load_crypt_info(encrypt, b"")?
        } else {
            let stmf = encrypt.get_byte_string_for(b"StmF", b"");
            let strf = encrypt.get_byte_string_for(b"StrF", b"");
            if stmf != strf {
                return None;
            }
            load_crypt_info(encrypt, &strf)?
        };
        Some(Self {
            revision: encrypt.get_integer_for(b"R", 0),
            permissions: encrypt.get_integer_for(b"P", -1) as u32,
            key_len,
            cipher,
            file_id: file_id.to_vec(),
            encrypt_key: [0; 32],
        })
    }

    pub fn check_user_password(
        &mut self,
        encrypt: &CpdfDictionary,
        password: &[u8],
        ignore_metadata: bool,
    ) -> bool {
        let key = calc_encrypt_key(
            encrypt,
            password,
            self.key_len,
            ignore_metadata,
            &self.file_id,
        );
        self.encrypt_key[..self.key_len].copy_from_slice(&key);
        let ukey = encrypt.get_byte_string_for(b"U", b"");
        if ukey.len() < 16 {
            return false;
        }
        if self.revision == 2 {
            let mut test = DEFAULT_PASSCODE;
            crypt_arc_four_crypt_block(&mut test, &self.encrypt_key[..self.key_len]);
            return ukey[..16] == test[..16];
        }
        let mut test = [0u8; 32];
        let copy_len = ukey.len().min(32);
        test[..copy_len].copy_from_slice(&ukey[..copy_len]);
        for i in (0..=19u8).rev() {
            let mut tmpkey = [0u8; 32];
            for j in 0..self.key_len {
                tmpkey[j] = self.encrypt_key[j] ^ i;
            }
            crypt_arc_four_crypt_block(&mut test, &tmpkey[..self.key_len]);
        }
        let mut md5 = CryptMd5Context::start();
        md5.update(&DEFAULT_PASSCODE);
        md5.update(&self.file_id);
        test[..16] == md5.finish()
    }

    pub fn get_user_password(&self, encrypt: &CpdfDictionary, owner_password: &[u8]) -> Vec<u8> {
        let okey = encrypt.get_byte_string_for(b"O", b"");
        if okey.len() < 32 {
            return Vec::new();
        }
        let mut digest = crypt_md5_generate(&get_passcode(owner_password));
        if self.revision >= 3 {
            for _ in 0..50 {
                digest = crypt_md5_generate(&digest);
            }
        }
        let mut enckey = [0u8; 32];
        enckey[..self.key_len.min(16)].copy_from_slice(&digest[..self.key_len.min(16)]);
        let mut okeybuf = [0u8; 32];
        okeybuf.copy_from_slice(&okey[..32]);
        if self.revision == 2 {
            crypt_arc_four_crypt_block(&mut okeybuf, &enckey[..self.key_len]);
        } else {
            for i in (0..=19u8).rev() {
                let mut tmpkey = [0u8; 32];
                for j in 0..self.key_len {
                    tmpkey[j] = enckey[j] ^ i;
                }
                crypt_arc_four_crypt_block(&mut okeybuf, &tmpkey[..self.key_len]);
            }
        }
        let mut len = 32;
        while len > 0 && DEFAULT_PASSCODE[len - 1] == okeybuf[len - 1] {
            len -= 1;
        }
        okeybuf[..len].to_vec()
    }

    pub fn check_owner_password(&mut self, encrypt: &CpdfDictionary, password: &[u8]) -> bool {
        let user_password = self.get_user_password(encrypt, password);
        self.check_user_password(encrypt, &user_password, false)
            || self.check_user_password(encrypt, &user_password, true)
    }
}

fn big_order_64_bits_mod3(data: &[u8]) -> usize {
    assert!(data.len() >= 16);
    data[..16]
        .iter()
        .fold(0usize, |value, &byte| (value * 256 + byte as usize) % 3)
}

pub fn revision6_hash(password: &[u8], salt: &[u8; 8], vector: Option<&[u8; 48]>) -> [u8; 32] {
    let mut sha = CryptSha2Context::start_256();
    sha.update(password);
    sha.update(salt);
    if let Some(vector) = vector {
        sha.update(vector);
    }
    let mut digest = sha.finish_256().to_vec();
    let mut i = 0usize;
    loop {
        let round_size = password.len() + digest.len() + vector.map_or(0, |vector| vector.len());
        let mut content = Vec::with_capacity(round_size * 64);
        for _ in 0..64 {
            content.extend_from_slice(password);
            content.extend_from_slice(&digest);
            if let Some(vector) = vector {
                content.extend_from_slice(vector);
            }
        }
        let mut aes = CryptAesContext::new(&digest[..16]);
        aes.set_iv(digest[16..32].try_into().unwrap());
        let mut encrypted = vec![0; content.len()];
        aes.encrypt(&mut encrypted, &content);
        digest = match big_order_64_bits_mod3(&encrypted) {
            0 => crypt_sha256_generate(&encrypted),
            1 => crypt_sha384_generate(&encrypted),
            _ => crypt_sha512_generate(&encrypted),
        };
        i += 1;
        if i >= 64 && i - 32 >= encrypted[encrypted.len() - 1] as usize {
            break;
        }
    }
    digest[..32].try_into().unwrap()
}

pub fn aes256_check_password(
    encrypt: &CpdfDictionary,
    revision: i32,
    permissions: u32,
    password: &[u8],
    owner: bool,
) -> Option<[u8; 32]> {
    assert!(revision >= 5);
    let okey = encrypt.get_byte_string_for(b"O", b"");
    let ukey = encrypt.get_byte_string_for(b"U", b"");
    if okey.len() < 48 || ukey.len() < 48 {
        return None;
    }
    let pkey = if owner { &okey } else { &ukey };
    let vector: Option<&[u8; 48]> = owner.then(|| ukey[..48].try_into().unwrap());
    let digest = if revision >= 6 {
        revision6_hash(password, pkey[32..40].try_into().unwrap(), vector)
    } else {
        let mut sha = CryptSha2Context::start_256();
        sha.update(password);
        sha.update(&pkey[32..40]);
        if let Some(vector) = vector {
            sha.update(vector);
        }
        sha.finish_256()
    };
    if digest != pkey[..32] {
        return None;
    }
    let digest = if revision >= 6 {
        revision6_hash(password, pkey[40..48].try_into().unwrap(), vector)
    } else {
        let mut sha = CryptSha2Context::start_256();
        sha.update(password);
        sha.update(&pkey[40..48]);
        if let Some(vector) = vector {
            sha.update(vector);
        }
        sha.finish_256()
    };
    let ekey = encrypt.get_byte_string_for(if owner { b"OE" } else { b"UE" }, b"");
    if ekey.len() < 32 {
        return None;
    }
    let zero_iv = [0u8; 16];
    let mut aes = CryptAesContext::new(&digest);
    aes.set_iv(&zero_iv);
    let mut encrypt_key = [0u8; 32];
    aes.decrypt(&mut encrypt_key, &ekey[..32]);
    aes.set_key(&encrypt_key);
    aes.set_iv(&zero_iv);
    let perms = encrypt.get_byte_string_for(b"Perms", b"");
    if perms.is_empty() {
        return None;
    }
    let mut perms_buf = [0u8; 16];
    perms_buf[..perms.len().min(16)].copy_from_slice(&perms[..perms.len().min(16)]);
    let mut result = [0u8; 16];
    aes.decrypt(&mut result, &perms_buf);
    if &result[9..12] != b"adb"
        || u32::from_le_bytes(result[..4].try_into().unwrap()) != permissions
    {
        return None;
    }
    if result[8] != b'F' && !encrypt.get_boolean_for(b"EncryptMetadata", true) {
        return None;
    }
    Some(encrypt_key)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PasswordEncodingConversion {
    Unknown,
    None,
    Latin1ToUtf8,
    Utf8ToLatin1,
}

pub struct CpdfSecurityHandler {
    owner_unlocked: bool,
    version: i32,
    password_encoding_conversion: PasswordEncodingConversion,
    state: Option<SecurityState>,
    encrypt_dict: Option<ObjectRef>,
    crypto_handler: Option<CpdfCryptoHandler>,
}

impl Default for CpdfSecurityHandler {
    fn default() -> Self {
        Self {
            owner_unlocked: false,
            version: 0,
            password_encoding_conversion: PasswordEncodingConversion::Unknown,
            state: None,
            encrypt_dict: None,
            crypto_handler: None,
        }
    }
}

impl CpdfSecurityHandler {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn on_init(
        &mut self,
        encrypt_dict: ObjectRef,
        id_array: Option<&ObjectRef>,
        password: &[u8],
    ) -> bool {
        let file_id = id_array
            .and_then(|array| {
                let borrowed = array.borrow();
                borrowed
                    .as_any()
                    .downcast_ref::<CpdfArray>()
                    .map(|array| array.get_byte_string_at(0))
            })
            .unwrap_or_default();
        let state = {
            let borrowed = encrypt_dict.borrow();
            let Some(dict) = borrowed.as_any().downcast_ref::<CpdfDictionary>() else {
                return false;
            };
            SecurityState::from_dict(dict, &file_id)
        };
        let Some(state) = state else { return false };
        self.version = {
            let borrowed = encrypt_dict.borrow();
            borrowed
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .unwrap()
                .get_integer_for(b"V", 0)
        };
        self.owner_unlocked = false;
        self.password_encoding_conversion = PasswordEncodingConversion::Unknown;
        self.crypto_handler = None;
        self.encrypt_dict = Some(encrypt_dict);
        self.state = Some(state);
        if self.state.as_ref().unwrap().cipher == Cipher::None {
            return true;
        }
        if !self.check_security(password) {
            return false;
        }
        let state = self.state.as_ref().unwrap();
        self.crypto_handler = Some(CpdfCryptoHandler::new(
            state.cipher,
            &state.encrypt_key[..state.key_len],
        ));
        true
    }

    /// `CPDF_SecurityHandler::OnCreate()`. The caller supplies the password in
    /// the encoding previously selected by `GetEncodedPassword()`; this method
    /// performs no extra conversion or password truncation.
    pub fn on_create(
        &mut self,
        encrypt_dict: ObjectRef,
        id_array: Option<&ObjectRef>,
        password: &[u8],
    ) {
        self.on_create_with_random(encrypt_dict, id_array, password, &mut fx_random::fill);
    }

    // The random operation is supplied explicitly so deterministic regression
    // vectors can exercise the exact native call sizes/order. Production uses
    // FX_Random::Fill above; this is not an alternate encryption algorithm.
    fn on_create_with_random(
        &mut self,
        encrypt_dict: ObjectRef,
        id_array: Option<&ObjectRef>,
        password: &[u8],
        fill_random: &mut impl FnMut(&mut [u32]),
    ) {
        // This is the save overload of LoadDict(): retain the supplied dictionary
        // and update V/R/P even if crypt-filter validation subsequently fails.
        // It does not reset owner-unlocked, encoding-conversion, file ID or keys.
        self.encrypt_dict = Some(Rc::clone(&encrypt_dict));
        {
            let borrowed = encrypt_dict.borrow();
            let dict = borrowed
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .expect("OnCreate requires an encryption dictionary");
            self.version = dict.get_integer_for(b"V", 0);
            let state = self.state.get_or_insert_with(|| SecurityState {
                revision: 0,
                permissions: 0,
                key_len: 0,
                cipher: Cipher::None,
                file_id: Vec::new(),
                encrypt_key: [0; 32],
            });
            state.revision = dict.get_integer_for(b"R", 0);
            state.permissions = dict.get_integer_for(b"P", -1) as u32;
            let mut filter_name = Vec::new();
            if self.version >= 4 {
                let stream_name = dict.get_byte_string_for(b"StmF", b"");
                filter_name = dict.get_byte_string_for(b"StrF", b"");
                if stream_name != filter_name {
                    return;
                }
            }
            let Some((cipher, key_len)) = load_crypt_info(dict, &filter_name) else {
                return;
            };
            state.cipher = cipher;
            state.key_len = key_len;
        }

        if self.state.as_ref().unwrap().revision >= 5 {
            let mut random = [0u32; 4];
            fill_random(&mut random);
            let mut sha = CryptSha2Context::start_256();
            for word in random {
                sha.update(&word.to_ne_bytes());
            }
            self.state.as_mut().unwrap().encrypt_key = sha.finish_256();
            self.aes256_set_password(&encrypt_dict, password);
            self.aes256_set_perms(&encrypt_dict, fill_random);
            // Preserve the pinned early return: it neither creates/replaces the
            // crypto handler nor updates O/OE in this AES-256 save branch.
            return;
        }

        let file_id = id_array
            .map(|array| {
                let borrowed = array.borrow();
                borrowed
                    .as_any()
                    .downcast_ref::<CpdfArray>()
                    .expect("OnCreate ID must be an array")
                    .get_byte_string_at(0)
            })
            .unwrap_or_default();
        let key = {
            let borrowed = encrypt_dict.borrow();
            let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
            calc_encrypt_key(
                dict,
                password,
                self.state.as_ref().unwrap().key_len,
                false,
                &file_id,
            )
        };
        let state = self.state.as_mut().unwrap();
        state.encrypt_key[..state.key_len].copy_from_slice(&key);
        let user_entry = if state.revision < 3 {
            let mut passcode = DEFAULT_PASSCODE;
            crypt_arc_four_crypt_block(&mut passcode, &state.encrypt_key[..state.key_len]);
            passcode
        } else {
            let mut md5 = CryptMd5Context::start();
            md5.update(&DEFAULT_PASSCODE);
            md5.update(&file_id);
            let mut digest = md5.finish();
            crypt_arc_four_crypt_block(&mut digest, &state.encrypt_key[..state.key_len]);
            let mut temporary_key = [0u8; 32];
            for i in 1..=19u8 {
                for (temporary, &key) in temporary_key[..state.key_len]
                    .iter_mut()
                    .zip(&state.encrypt_key[..state.key_len])
                {
                    *temporary = key ^ i;
                }
                crypt_arc_four_crypt_block(&mut digest, &temporary_key[..state.key_len]);
            }
            let mut user_entry = [0u8; 32];
            user_entry[..16].copy_from_slice(&digest);
            user_entry[16..].copy_from_slice(&crypt_md5_generate(&digest));
            user_entry
        };
        encrypt_dict
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"U", CpdfString::from_bytes(None, &user_entry));
        self.init_crypto_handler();
    }

    fn aes256_set_password(&mut self, encrypt_dict: &ObjectRef, password: &[u8]) {
        let state = self.state.as_ref().unwrap();
        let mut sha = CryptSha1Context::start();
        sha.update(&state.encrypt_key);
        sha.update(b"hello");
        let digest = sha.finish();
        let mut user_entry = [0u8; 48];
        let hash = if state.revision >= 6 {
            revision6_hash(password, digest[..8].try_into().unwrap(), None)
        } else {
            let mut sha = CryptSha2Context::start_256();
            sha.update(password);
            sha.update(&digest[..8]);
            sha.finish_256()
        };
        user_entry[..32].copy_from_slice(&hash);
        user_entry[32..].copy_from_slice(&digest[..16]);
        encrypt_dict
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"U", CpdfString::from_bytes(None, &user_entry));
        let hash = if state.revision >= 6 {
            revision6_hash(password, digest[8..16].try_into().unwrap(), None)
        } else {
            let mut sha = CryptSha2Context::start_256();
            sha.update(password);
            sha.update(&digest[8..16]);
            sha.finish_256()
        };
        let mut aes = CryptAesContext::new(&hash);
        aes.set_iv(&[0; 16]);
        let mut encrypted_key = [0u8; 32];
        aes.encrypt(&mut encrypted_key, &state.encrypt_key);
        encrypt_dict
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"UE", CpdfString::from_bytes(None, &encrypted_key));
    }

    fn aes256_set_perms(
        &mut self,
        encrypt_dict: &ObjectRef,
        fill_random: &mut impl FnMut(&mut [u32]),
    ) {
        let state = self.state.as_ref().unwrap();
        let mut plain = [0u8; 16];
        plain[..4].copy_from_slice(&state.permissions.to_le_bytes());
        plain[4..8].fill(0xff);
        let metadata = encrypt_dict
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .get_boolean_for(b"EncryptMetadata", true);
        plain[8] = if metadata { b'T' } else { b'F' };
        plain[9..12].copy_from_slice(b"adb");
        // Algorithm 3.10 requires bytes 12..16 to be random data. Native fills
        // a single uint32_t there, preserving native-endian byte order.
        let mut random = [0u32; 1];
        fill_random(&mut random);
        plain[12..].copy_from_slice(&random[0].to_ne_bytes());
        let mut aes = CryptAesContext::new(&state.encrypt_key);
        aes.set_iv(&[0; 16]);
        let mut encrypted = [0u8; 16];
        aes.encrypt(&mut encrypted, &plain);
        encrypt_dict
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"Perms", CpdfString::from_bytes(None, &encrypted));
    }

    fn init_crypto_handler(&mut self) {
        let state = self.state.as_ref().unwrap();
        self.crypto_handler = Some(CpdfCryptoHandler::new(
            state.cipher,
            &state.encrypt_key[..state.key_len],
        ));
    }

    fn check_security(&mut self, password: &[u8]) -> bool {
        if !password.is_empty() && self.check_password(password, true) {
            self.owner_unlocked = true;
            return true;
        }
        self.check_password(password, false)
    }

    fn check_password(&mut self, password: &[u8], owner: bool) -> bool {
        assert_eq!(
            self.password_encoding_conversion,
            PasswordEncodingConversion::Unknown
        );
        if self.check_password_impl(password, owner) {
            self.password_encoding_conversion = PasswordEncodingConversion::None;
            return true;
        }
        if password.is_ascii() {
            return false;
        }
        if self.state.as_ref().unwrap().revision >= 5 {
            let utf8 = latin1_to_utf8(password);
            if !self.check_password_impl(&utf8, owner) {
                return false;
            }
            self.password_encoding_conversion = PasswordEncodingConversion::Latin1ToUtf8;
        } else {
            let latin1 = utf8_to_latin1(password);
            if !self.check_password_impl(&latin1, owner) {
                return false;
            }
            self.password_encoding_conversion = PasswordEncodingConversion::Utf8ToLatin1;
        }
        true
    }

    fn check_password_impl(&mut self, password: &[u8], owner: bool) -> bool {
        let Some(encrypt_dict) = &self.encrypt_dict else {
            return false;
        };
        let borrowed = encrypt_dict.borrow();
        let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
        let state = self.state.as_mut().unwrap();
        if state.revision >= 5 {
            let key =
                aes256_check_password(dict, state.revision, state.permissions, password, owner);
            if let Some(key) = key {
                state.encrypt_key = key;
                return true;
            }
            return false;
        }
        if owner {
            state.check_owner_password(dict, password)
        } else {
            state.check_user_password(dict, password, false)
                || state.check_user_password(dict, password, true)
        }
    }

    pub fn get_permissions(&self, get_owner_perms: bool) -> u32 {
        let Some(state) = &self.state else { return 0 };
        let mut permissions = if self.owner_unlocked && get_owner_perms {
            u32::MAX
        } else {
            state.permissions
        };
        if let Some(dict) = &self.encrypt_dict {
            let borrowed = dict.borrow();
            let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
            if crate::fxcrt::bytestring::eq_cstr(
                &dict.get_byte_string_for(b"Filter", b""),
                b"Standard",
            ) {
                permissions &= 0xffff_fffc;
                permissions |= 0xffff_f0c0;
            }
        }
        permissions
    }

    pub fn is_metadata_encrypted(&self) -> bool {
        let dict = self
            .encrypt_dict
            .as_ref()
            .expect("security handler must be initialized");
        let borrowed = dict.borrow();
        borrowed
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .get_boolean_for(b"EncryptMetadata", true)
    }

    pub fn get_encoded_password(&self, password: &[u8]) -> Vec<u8> {
        match self.password_encoding_conversion {
            PasswordEncodingConversion::None => password.to_vec(),
            PasswordEncodingConversion::Latin1ToUtf8 => latin1_to_utf8(password),
            PasswordEncodingConversion::Utf8ToLatin1 => utf8_to_latin1(password),
            PasswordEncodingConversion::Unknown => {
                panic!("password encoding is unknown before authentication")
            }
        }
    }

    pub fn crypto_handler(&self) -> Option<&CpdfCryptoHandler> {
        self.crypto_handler.as_ref()
    }
    pub fn cipher(&self) -> Option<Cipher> {
        self.state.as_ref().map(|state| state.cipher)
    }
    pub fn key_len(&self) -> Option<usize> {
        self.state.as_ref().map(|state| state.key_len)
    }
    pub fn version(&self) -> i32 {
        self.version
    }
    pub fn revision(&self) -> Option<i32> {
        self.state.as_ref().map(|state| state.revision)
    }
    pub fn encrypt_key(&self) -> Option<&[u8]> {
        self.state
            .as_ref()
            .map(|state| &state.encrypt_key[..state.key_len])
    }
}

fn latin1_to_utf8(password: &[u8]) -> Vec<u8> {
    let mut output = String::new();
    for &byte in password {
        output.push(char::from(byte));
    }
    output.into_bytes()
}

fn utf8_to_latin1(password: &[u8]) -> Vec<u8> {
    utf8_decode(password)
        .into_iter()
        .map(|ch| ch as u8)
        .collect()
}

#[cfg(test)]
#[path = "cpdf_security_handler_create_tests.rs"]
mod create_tests;

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::{
        aes256_check_password, calc_encrypt_key, get_passcode, CpdfSecurityHandler, SecurityState,
        DEFAULT_PASSCODE,
    };
    use crate::fdrm::fx_crypt::{crypt_arc_four_crypt_block, crypt_md5_generate};
    use crate::fdrm::fx_crypt_aes::CryptAesContext;
    use crate::fdrm::fx_crypt_sha::crypt_sha256_generate;
    use crate::fpdfapi::parser::cpdf_array::CpdfArray;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fpdfapi::parser::cpdf_string::CpdfString;
    use crate::fpdfapi::parser::cpdf_syntax_parser::{CpdfSyntaxParser, ParseType};
    use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;

    #[test]
    fn revision_two_accepts_user_and_owner_passwords() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"V", CpdfScalarObject::number_from_signed(1));
        dict.set_new_for(b"R", CpdfScalarObject::number_from_signed(2));
        dict.set_new_for(b"P", CpdfScalarObject::number_from_signed(-4));
        let mut owner_entry = get_passcode(b"user");
        let owner_hash = crypt_md5_generate(&get_passcode(b"owner"));
        crypt_arc_four_crypt_block(&mut owner_entry, &owner_hash[..5]);
        dict.set_new_for(b"O", CpdfString::from_bytes(None, &owner_entry));
        let key = calc_encrypt_key(&dict, b"user", 5, false, b"file-id");
        let mut user_entry = DEFAULT_PASSCODE;
        crypt_arc_four_crypt_block(&mut user_entry, &key);
        dict.set_new_for(b"U", CpdfString::from_bytes(None, &user_entry));

        let mut security = SecurityState::from_dict(&dict, b"file-id").unwrap();
        assert!(security.check_user_password(&dict, b"user", false));
        assert_eq!(&security.encrypt_key[..5], key);
        assert!(!security.check_user_password(&dict, b"wrong", false));
        assert_eq!(
            security.get_user_password(&dict, b"owner"),
            get_passcode(b"user")
        );
        assert!(security.check_owner_password(&dict, b"owner"));

        let mut id = CpdfArray::new();
        id.append_new(CpdfString::from_bytes(None, b"file-id"));
        let id = retain_object(id);
        let dict = retain_object(dict);
        let mut handler = CpdfSecurityHandler::new();
        assert!(handler.on_init(dict.clone(), Some(&id), b"owner"));
        assert_eq!(handler.get_encoded_password(b"owner"), b"owner");
        assert_eq!(handler.encrypt_key().unwrap(), key);
        assert!(handler.crypto_handler().is_some());
    }

    #[test]
    fn revision_five_recovers_file_key_for_both_passwords() {
        let file_key = [42u8; 32];
        let permissions: u32 = 0xffff_fffc;
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"V", CpdfScalarObject::number_from_signed(5));
        dict.set_new_for(b"R", CpdfScalarObject::number_from_signed(5));
        dict.set_new_for(
            b"P",
            CpdfScalarObject::number_from_signed(permissions as i32),
        );
        dict.set_new_name_for(b"Filter", b"Standard");
        dict.set_new_name_for(b"StmF", b"StdCF");
        dict.set_new_name_for(b"StrF", b"StdCF");
        let mut filter = CpdfDictionary::new();
        filter.set_new_name_for(b"CFM", b"AESV3");
        let mut filters = CpdfDictionary::new();
        filters.set_for(b"StdCF", Some(retain_object(filter)));
        dict.set_for(b"CF", Some(retain_object(filters)));
        let mut user = crypt_sha256_generate(b"user12345678");
        user.extend_from_slice(b"12345678abcdefgh");
        let mut owner_data = b"ownerABCDEFGH".to_vec();
        owner_data.extend_from_slice(&user);
        let mut owner = crypt_sha256_generate(&owner_data);
        owner.extend_from_slice(b"ABCDEFGH87654321");
        dict.set_new_for(b"U", CpdfString::from_bytes(None, &user));
        dict.set_new_for(b"O", CpdfString::from_bytes(None, &owner));
        for (name, password, salt, vector) in [
            (
                b"UE".as_slice(),
                b"user".as_slice(),
                b"abcdefgh".as_slice(),
                None,
            ),
            (
                b"OE".as_slice(),
                b"owner".as_slice(),
                b"87654321".as_slice(),
                Some(user.as_slice()),
            ),
        ] {
            let mut key_material = Vec::from(password);
            key_material.extend_from_slice(salt);
            if let Some(vector) = vector {
                key_material.extend_from_slice(vector);
            }
            let digest = crypt_sha256_generate(&key_material);
            let mut aes = CryptAesContext::new(&digest);
            let mut ciphertext = [0u8; 32];
            aes.encrypt(&mut ciphertext, &file_key);
            dict.set_new_for(name, CpdfString::from_bytes(None, &ciphertext));
        }
        let mut perms = [0xffu8; 16];
        perms[..4].copy_from_slice(&permissions.to_le_bytes());
        perms[8..12].copy_from_slice(b"Tadb");
        let mut aes = CryptAesContext::new(&file_key);
        let mut encrypted_perms = [0u8; 16];
        aes.encrypt(&mut encrypted_perms, &perms);
        dict.set_new_for(b"Perms", CpdfString::from_bytes(None, &encrypted_perms));
        assert_eq!(
            aes256_check_password(&dict, 5, permissions, b"user", false),
            Some(file_key)
        );
        assert_eq!(
            aes256_check_password(&dict, 5, permissions, b"owner", true),
            Some(file_key)
        );
        assert_eq!(
            aes256_check_password(&dict, 5, permissions, b"wrong", false),
            None
        );
        let dict = retain_object(dict);
        let mut handler = CpdfSecurityHandler::new();
        assert!(handler.on_init(dict.clone(), None, b"user"));
        assert_eq!(handler.encrypt_key().unwrap(), file_key);
        assert_eq!(
            handler.cipher(),
            Some(crate::fpdfapi::parser::cpdf_crypto_handler::Cipher::Aes)
        );
        let mut owner_handler = CpdfSecurityHandler::new();
        assert!(owner_handler.on_init(dict, None, b"owner"));
        assert_eq!(owner_handler.get_permissions(true), 0xffff_fffc);
    }

    #[test]
    fn opens_pdfium_revision_six_fixture_with_utf8_and_latin1_passwords() {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/encrypted_hello_world_r6.pdf"
        ));
        let file = Rc::new(CfxReadOnlyContainerStream::new(bytes.to_vec()));
        let mut syntax = CpdfSyntaxParser::from_file(file);
        syntax.set_pos(635);
        let encrypt = syntax.get_indirect_object(None, ParseType::Loose).unwrap();
        let trailer_pos = bytes
            .windows(7)
            .rposition(|word| word == b"trailer")
            .unwrap();
        syntax.set_pos(trailer_pos as i64);
        let trailer = crate::fpdfapi::parser::cpdf_parser::load_trailer(&mut syntax, None).unwrap();
        let id = {
            let borrowed = trailer.borrow();
            borrowed
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .unwrap()
                .get_array_for(b"ID")
                .unwrap()
        };
        for (password, encoded) in [
            (b"\xc3\xa2ge".as_slice(), b"\xc3\xa2ge".as_slice()),
            (b"\xe2ge".as_slice(), b"\xc3\xa2ge".as_slice()),
            (b"h\xc3\xb4tel".as_slice(), b"h\xc3\xb4tel".as_slice()),
            (b"h\xf4tel".as_slice(), b"h\xc3\xb4tel".as_slice()),
        ] {
            let mut handler = CpdfSecurityHandler::new();
            assert!(handler.on_init(encrypt.clone(), Some(&id), password));
            assert_eq!(handler.revision(), Some(6));
            assert_eq!(handler.get_encoded_password(password), encoded);
        }
    }
}
