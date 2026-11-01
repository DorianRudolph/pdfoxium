// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2024 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fdrm/fx_crypt_sha.{h,cpp}`. RustCrypto's SHA contexts replace the
//! source's compression loops while retaining its incremental and one-shot
//! SHA-1, SHA-256, SHA-384, and SHA-512 behavior.

use sha1::{Digest, Sha1};
use sha2::{Sha256, Sha384, Sha512};

pub struct CryptSha1Context(Sha1);

impl CryptSha1Context {
    pub fn start() -> Self {
        Self(Sha1::new())
    }
    pub fn update(&mut self, data: &[u8]) {
        self.0.update(data);
    }
    pub fn finish(self) -> [u8; 20] {
        self.0.finalize().into()
    }
}

pub fn crypt_sha1_generate(data: &[u8]) -> Vec<u8> {
    Sha1::digest(data).to_vec()
}

enum Sha2State {
    Sha256(Sha256),
    Sha384(Sha384),
    Sha512(Sha512),
}

pub struct CryptSha2Context(Sha2State);

impl CryptSha2Context {
    pub fn start_256() -> Self {
        Self(Sha2State::Sha256(Sha256::new()))
    }
    pub fn start_384() -> Self {
        Self(Sha2State::Sha384(Sha384::new()))
    }
    pub fn start_512() -> Self {
        Self(Sha2State::Sha512(Sha512::new()))
    }

    pub fn update(&mut self, data: &[u8]) {
        match &mut self.0 {
            Sha2State::Sha256(hash) => hash.update(data),
            Sha2State::Sha384(hash) => hash.update(data),
            Sha2State::Sha512(hash) => hash.update(data),
        }
    }

    pub fn finish_256(self) -> [u8; 32] {
        match self.0 {
            Sha2State::Sha256(hash) => hash.finalize().into(),
            _ => panic!("SHA-256 finish requires a SHA-256 context"),
        }
    }

    pub fn finish_384(self) -> [u8; 48] {
        match self.0 {
            Sha2State::Sha384(hash) => hash.finalize().into(),
            _ => panic!("SHA-384 finish requires a SHA-384 context"),
        }
    }

    pub fn finish_512(self) -> [u8; 64] {
        match self.0 {
            Sha2State::Sha512(hash) => hash.finalize().into(),
            _ => panic!("SHA-512 finish requires a SHA-512 context"),
        }
    }
}

pub fn crypt_sha256_generate(data: &[u8]) -> Vec<u8> {
    Sha256::digest(data).to_vec()
}
pub fn crypt_sha384_generate(data: &[u8]) -> Vec<u8> {
    Sha384::digest(data).to_vec()
}
pub fn crypt_sha512_generate(data: &[u8]) -> Vec<u8> {
    Sha512::digest(data).to_vec()
}

#[cfg(test)]
mod tests {
    use super::{
        crypt_sha1_generate, crypt_sha256_generate, crypt_sha384_generate, crypt_sha512_generate,
        CryptSha2Context,
    };

    #[test]
    fn known_vectors_and_incremental_sha256() {
        assert_eq!(
            crypt_sha1_generate(b"abc"),
            hex("a9993e364706816aba3e25717850c26c9cd0d89d")
        );
        assert_eq!(
            crypt_sha256_generate(b"abc"),
            hex("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
        assert_eq!(crypt_sha384_generate(b"abc"), hex("cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7"));
        assert_eq!(crypt_sha512_generate(b"abc"), hex("ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"));
        let mut hash = CryptSha2Context::start_256();
        hash.update(b"a");
        hash.update(b"bc");
        assert_eq!(hash.finish_256().to_vec(), crypt_sha256_generate(b"abc"));
    }

    fn hex(input: &str) -> Vec<u8> {
        input
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }
}
