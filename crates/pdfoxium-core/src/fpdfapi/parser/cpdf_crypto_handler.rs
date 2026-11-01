// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/parser/cpdf_crypto_handler.{h,cpp}`.

use super::cpdf_dictionary::CpdfDictionary;
use super::cpdf_object::ObjectRef;
use super::cpdf_object_walker::CpdfObjectWalker;
use super::cpdf_stream::CpdfStream;
use super::cpdf_stream_acc::CpdfStreamAcc;
use super::cpdf_string::CpdfString;
use crate::fdrm::fx_crypt::{crypt_arc_four_crypt_block, crypt_md5_generate};
use crate::fdrm::fx_crypt_aes::CryptAesContext;
use crate::fxcrt::fx_random;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Cipher {
    None,
    Rc4,
    Aes,
    Aes2,
}

pub struct CpdfCryptoHandler {
    key_len: usize,
    cipher: Cipher,
    encrypt_key: [u8; 32],
}

impl super::cpdf_encryptor::CryptoHandler for CpdfCryptoHandler {
    fn encrypt_content(&self, obj_num: u32, gen_num: u32, source: &[u8]) -> Vec<u8> {
        CpdfCryptoHandler::encrypt_content(self, obj_num, gen_num, source)
    }
}

impl CpdfCryptoHandler {
    pub fn new(cipher: Cipher, key: &[u8]) -> Self {
        assert!(cipher != Cipher::Aes || matches!(key.len(), 16 | 24 | 32));
        assert!(cipher != Cipher::Aes2 || key.len() == 32);
        assert!(cipher != Cipher::Rc4 || (5..=16).contains(&key.len()));
        let key_len = key.len().min(32);
        let mut encrypt_key = [0; 32];
        if cipher != Cipher::None {
            encrypt_key[..key_len].copy_from_slice(&key[..key_len]);
        }
        Self {
            key_len,
            cipher,
            encrypt_key,
        }
    }

    pub fn is_cipher_aes(&self) -> bool {
        self.cipher == Cipher::Aes
    }

    pub fn is_signature_dictionary(dictionary: Option<&CpdfDictionary>) -> bool {
        let Some(dictionary) = dictionary else {
            return false;
        };
        let value = dictionary
            .get_direct_object_for(b"Type")
            .or_else(|| dictionary.get_direct_object_for(b"FT"));
        value.is_some_and(|value| {
            crate::fxcrt::bytestring::eq_cstr(&value.borrow().get_string(), b"Sig")
        })
    }

    fn populate_key(&self, obj_num: u32, gen_num: u32) -> [u8; 48] {
        let mut key = [0u8; 48];
        key[..self.key_len].copy_from_slice(&self.encrypt_key[..self.key_len]);
        key[self.key_len] = obj_num as u8;
        key[self.key_len + 1] = (obj_num >> 8) as u8;
        key[self.key_len + 2] = (obj_num >> 16) as u8;
        key[self.key_len + 3] = gen_num as u8;
        key[self.key_len + 4] = (gen_num >> 8) as u8;
        key
    }

    fn derived_key(&self, obj_num: u32, gen_num: u32) -> ([u8; 16], usize) {
        let mut key = self.populate_key(obj_num, gen_num);
        let hash_len = if self.cipher == Cipher::Aes {
            key[self.key_len + 5..self.key_len + 9].copy_from_slice(b"sAlT");
            self.key_len + 9
        } else {
            self.key_len + 5
        };
        let digest = crypt_md5_generate(&key[..hash_len]);
        (digest, (self.key_len + 5).min(16))
    }

    pub fn encrypt_content(&self, obj_num: u32, gen_num: u32, source: &[u8]) -> Vec<u8> {
        if self.cipher == Cipher::None {
            return source.to_vec();
        }
        if self.cipher == Cipher::Aes {
            let mut aes = if self.key_len == 32 {
                CryptAesContext::new(&self.encrypt_key)
            } else {
                assert_eq!(
                    self.key_len, 16,
                    "PDFium rejects 24-byte AES keys while encrypting"
                );
                let (key, _) = self.derived_key(obj_num, gen_num);
                CryptAesContext::new(&key)
            };
            let mut words = [0u32; 4];
            fx_random::fill(&mut words);
            let mut iv = [0u8; 16];
            for (chunk, word) in iv.chunks_exact_mut(4).zip(words) {
                chunk.copy_from_slice(&word.to_ne_bytes());
            }
            aes.set_iv(&iv);
            let source_data_size = source.len() & !15;
            let mut dest = vec![0u8; 16 + source_data_size + 16];
            dest[..16].copy_from_slice(&iv);
            aes.encrypt(
                &mut dest[16..16 + source_data_size],
                &source[..source_data_size],
            );
            let remaining = &source[source_data_size..];
            let mut padding = [16u8; 16];
            padding[..remaining.len()].copy_from_slice(remaining);
            padding[remaining.len()..].fill((16 - remaining.len()) as u8);
            aes.encrypt(&mut dest[16 + source_data_size..], &padding);
            return dest;
        }
        let (key, key_len) = self.derived_key(obj_num, gen_num);
        let mut dest = source.to_vec();
        crypt_arc_four_crypt_block(&mut dest, &key[..key_len]);
        dest
    }

    pub fn decrypt_content(&self, obj_num: u32, gen_num: u32, source: &[u8]) -> Vec<u8> {
        if self.cipher == Cipher::None {
            return source.to_vec();
        }
        if self.cipher != Cipher::Aes {
            let (key, key_len) = self.derived_key(obj_num, gen_num);
            let mut dest = source.to_vec();
            crypt_arc_four_crypt_block(&mut dest, &key[..key_len]);
            return dest;
        }
        let mut aes = if self.key_len == 32 {
            CryptAesContext::new(&self.encrypt_key)
        } else {
            let (key, _) = self.derived_key(obj_num, gen_num);
            CryptAesContext::new(&key)
        };
        let Some(iv_bytes) = source.get(..16) else {
            return Vec::new();
        };
        let iv: [u8; 16] = iv_bytes.try_into().unwrap();
        aes.set_iv(&iv);
        let payload = &source[16..];
        let full_blocks = payload.len() / 16;
        let mut dest = Vec::with_capacity(payload.len());
        for (index, block) in payload.chunks_exact(16).enumerate() {
            let mut plain = [0u8; 16];
            aes.decrypt(&mut plain, block);
            // PDFium holds the last complete block until DecryptFinish().
            // A partial trailing block makes the previous one non-final.
            let is_final = index + 1 == full_blocks && payload.len() % 16 == 0;
            if is_final {
                let padding = plain[15];
                if padding < 16 {
                    dest.extend_from_slice(&plain[..16 - padding as usize]);
                }
            } else {
                dest.extend_from_slice(&plain);
            }
        }
        dest
    }

    pub fn decrypt_object_tree(&self, object: Option<ObjectRef>) -> bool {
        let Some(root) = object else { return false };
        let (obj_num, gen_num) = {
            let borrowed = root.borrow();
            (borrowed.obj_num(), borrowed.gen_num())
        };
        let mut may_be_sign_dictionaries: Vec<(ObjectRef, ObjectRef)> = Vec::new();
        let mut object_to_decrypt = Some(root);
        while let Some(object) = object_to_decrypt.take() {
            let mut walker = CpdfObjectWalker::new(object);
            while let Some(child) = walker.get_next() {
                let parent = walker.parent();
                if walker.dictionary_key() == b"Contents" {
                    if let Some(parent_dict) = parent.and_then(|parent| {
                        let borrowed = parent.borrow();
                        borrowed
                            .as_any()
                            .downcast_ref::<CpdfDictionary>()
                            .map(|_| parent.clone())
                    }) {
                        let has_type = {
                            let borrowed = parent_dict.borrow();
                            let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
                            dict.key_exist(b"Type") || dict.key_exist(b"FT")
                        };
                        if has_type {
                            may_be_sign_dictionaries.push((parent_dict, child));
                            walker.skip_walk_into_current_object();
                            continue;
                        }
                    }
                }
                let is_string = child.borrow().as_any().is::<CpdfString>();
                if is_string {
                    let decrypted = {
                        let borrowed = child.borrow();
                        let string = borrowed.as_any().downcast_ref::<CpdfString>().unwrap();
                        self.decrypt_content(obj_num, gen_num, string.get_string())
                    };
                    child
                        .borrow_mut()
                        .as_any_mut()
                        .downcast_mut::<CpdfString>()
                        .unwrap()
                        .set_string(&decrypted);
                }
                let is_stream = child.borrow().as_any().is::<CpdfStream>();
                if is_stream {
                    let decrypted = {
                        let borrowed = child.borrow();
                        let stream = borrowed.as_any().downcast_ref::<CpdfStream>().unwrap();
                        let mut acc = CpdfStreamAcc::new(stream);
                        acc.load_all_data_raw();
                        if self.is_cipher_aes() && acc.get_size() < 16 {
                            Vec::new()
                        } else {
                            self.decrypt_content(obj_num, gen_num, acc.get_span())
                        }
                    };
                    child
                        .borrow_mut()
                        .as_any_mut()
                        .downcast_mut::<CpdfStream>()
                        .unwrap()
                        .take_data(decrypted);
                }
            }
            drop(walker);
            while let Some((parent, contents)) = may_be_sign_dictionaries.pop() {
                let is_signature = {
                    let borrowed = parent.borrow();
                    Self::is_signature_dictionary(
                        borrowed.as_any().downcast_ref::<CpdfDictionary>(),
                    )
                };
                if !is_signature {
                    object_to_decrypt = Some(contents);
                    break;
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{Cipher, CpdfCryptoHandler};
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_string::CpdfString;

    #[test]
    fn rc4_object_key_round_trip_and_signature_contents() {
        let handler = CpdfCryptoHandler::new(Cipher::Rc4, b"secret");
        let encrypted = handler.encrypt_content(7, 0, b"message");
        assert_ne!(encrypted, b"message");
        assert_eq!(handler.decrypt_content(7, 0, &encrypted), b"message");
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Type", b"Sig\0tail");
        dict.set_new_for(b"Contents", CpdfString::from_bytes(None, &encrypted));
        dict.set_new_for(b"Reason", CpdfString::from_bytes(None, &encrypted));
        let object = retain_object(dict);
        object.borrow_mut().set_obj_num(7);
        assert!(handler.decrypt_object_tree(Some(object.clone())));
        let borrowed = object.borrow();
        let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
        assert_eq!(
            dict.get_object_for(b"Contents")
                .unwrap()
                .borrow()
                .get_string(),
            encrypted
        );
        assert_eq!(
            dict.get_object_for(b"Reason")
                .unwrap()
                .borrow()
                .get_string(),
            b"message"
        );
    }

    #[test]
    fn aes_object_encryption_round_trip_with_padding() {
        for key in [
            b"0123456789abcdef".as_slice(),
            b"0123456789abcdef0123456789abcdef".as_slice(),
        ] {
            let handler = CpdfCryptoHandler::new(Cipher::Aes, key);
            for plain in [
                b"short".as_slice(),
                b"exactly16bytes!".as_slice(),
                b"two full blocks and another byte!".as_slice(),
            ] {
                let encrypted = handler.encrypt_content(3, 1, plain);
                assert_eq!(encrypted.len() % 16, 0);
                assert_eq!(handler.decrypt_content(3, 1, &encrypted), plain);
            }
        }
    }
}
