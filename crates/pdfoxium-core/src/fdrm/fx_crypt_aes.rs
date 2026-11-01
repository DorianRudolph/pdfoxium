// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fdrm/fx_crypt_aes.{h,cpp}`. RustCrypto provides the AES block
//! permutation; the CBC chaining and context IV updates follow PDFium.

use aes::cipher::{Block, BlockCipherDecrypt, BlockCipherEncrypt, KeyInit};
use aes::{Aes128, Aes192, Aes256};

enum AesCipher {
    Aes128(Aes128),
    Aes192(Aes192),
    Aes256(Aes256),
}

impl AesCipher {
    fn encrypt_block(&self, block: &mut Block<Aes128>) {
        match self {
            Self::Aes128(cipher) => cipher.encrypt_block(block),
            Self::Aes192(cipher) => cipher.encrypt_block(block),
            Self::Aes256(cipher) => cipher.encrypt_block(block),
        }
    }

    fn decrypt_block(&self, block: &mut Block<Aes128>) {
        match self {
            Self::Aes128(cipher) => cipher.decrypt_block(block),
            Self::Aes192(cipher) => cipher.decrypt_block(block),
            Self::Aes256(cipher) => cipher.decrypt_block(block),
        }
    }
}

pub struct CryptAesContext {
    cipher: AesCipher,
    iv: [u8; 16],
}

impl CryptAesContext {
    pub fn new(key: &[u8]) -> Self {
        let cipher = match key.len() {
            16 => AesCipher::Aes128(Aes128::new_from_slice(key).unwrap()),
            24 => AesCipher::Aes192(Aes192::new_from_slice(key).unwrap()),
            32 => AesCipher::Aes256(Aes256::new_from_slice(key).unwrap()),
            _ => panic!("PDF AES key must be 16, 24, or 32 bytes"),
        };
        Self {
            cipher,
            iv: [0; 16],
        }
    }

    pub fn set_key(&mut self, key: &[u8]) {
        self.cipher = Self::new(key).cipher;
    }

    pub fn set_iv(&mut self, iv: &[u8; 16]) {
        self.iv = *iv;
    }

    pub fn iv(&self) -> &[u8; 16] {
        &self.iv
    }

    pub fn encrypt(&mut self, dest: &mut [u8], src: &[u8]) {
        assert_eq!(dest.len(), src.len());
        assert_eq!(src.len() & 15, 0);
        for (output, input) in dest.chunks_exact_mut(16).zip(src.chunks_exact(16)) {
            let mut block = Block::<Aes128>::try_from(input).unwrap();
            for i in 0..16 {
                block[i] ^= self.iv[i];
            }
            self.cipher.encrypt_block(&mut block);
            output.copy_from_slice(&block);
            self.iv.copy_from_slice(&block);
        }
    }

    pub fn decrypt(&mut self, dest: &mut [u8], src: &[u8]) {
        assert_eq!(dest.len(), src.len());
        assert_eq!(src.len() & 15, 0);
        for (output, input) in dest.chunks_exact_mut(16).zip(src.chunks_exact(16)) {
            let mut block = Block::<Aes128>::try_from(input).unwrap();
            self.cipher.decrypt_block(&mut block);
            for i in 0..16 {
                output[i] = block[i] ^ self.iv[i];
            }
            self.iv.copy_from_slice(input);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CryptAesContext;

    #[test]
    fn aes_128_cbc_known_vector_and_chaining() {
        let key = hex("2b7e151628aed2a6abf7158809cf4f3c");
        let iv: [u8; 16] = hex("000102030405060708090a0b0c0d0e0f").try_into().unwrap();
        let first = hex("6bc1bee22e409f96e93d7e117393172a");
        let second = hex("ae2d8a571e03ac9c9eb76fac45af8e51");
        let mut context = CryptAesContext::new(&key);
        context.set_iv(&iv);
        let mut out = [0; 16];
        context.encrypt(&mut out, &first);
        assert_eq!(out.as_slice(), hex("7649abac8119b246cee98e9b12e9197d"));
        context.encrypt(&mut out, &second);
        assert_eq!(out.as_slice(), hex("5086cb9b507219ee95db113a917678b2"));
        let mut decrypt = CryptAesContext::new(&key);
        decrypt.set_iv(&iv);
        let mut plain = [0; 16];
        decrypt.decrypt(&mut plain, &hex("7649abac8119b246cee98e9b12e9197d"));
        assert_eq!(plain.as_slice(), first);
        decrypt.decrypt(&mut plain, &out);
        assert_eq!(plain.as_slice(), second);
    }

    #[test]
    fn aes_192_and_256_cbc_known_vectors() {
        let iv: [u8; 16] = hex("000102030405060708090a0b0c0d0e0f").try_into().unwrap();
        let plain = hex("6bc1bee22e409f96e93d7e117393172a");
        for (key, expected) in [
            (
                "8e73b0f7da0e6452c810f32b809079e562f8ead2522c6b7b",
                "4f021db243bc633d7178183a9fa071e8",
            ),
            (
                "603deb1015ca71be2b73aef0857d77811f352c073b6108d72d9810a30914dff4",
                "f58c4c04d6e5f1ba779eabfb5f7bfbd6",
            ),
        ] {
            let mut context = CryptAesContext::new(&hex(key));
            context.set_iv(&iv);
            let mut out = [0; 16];
            context.encrypt(&mut out, &plain);
            assert_eq!(out.as_slice(), hex(expected));
            context.set_iv(&iv);
            let mut recovered = [0; 16];
            context.decrypt(&mut recovered, &out);
            assert_eq!(recovered.as_slice(), plain);
        }
    }

    fn hex(input: &str) -> Vec<u8> {
        input
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }
}
