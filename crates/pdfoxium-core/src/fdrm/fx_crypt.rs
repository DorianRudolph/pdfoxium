// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fdrm/fx_crypt.{h,cpp}`. MD5's four rounds are expressed as a loop
//! over the same constants and word rotations as PDFium's expanded macros.

pub struct CryptRc4Context {
    x: usize,
    y: usize,
    m: [u8; 256],
}

impl CryptRc4Context {
    pub fn setup(key: &[u8]) -> Self {
        let mut context = Self {
            x: 0,
            y: 0,
            m: std::array::from_fn(|i| i as u8),
        };
        let mut j = 0usize;
        for i in 0..256 {
            let key_byte = if key.is_empty() {
                0
            } else {
                key[i % key.len()]
            };
            j = (j + context.m[i] as usize + key_byte as usize) & 0xff;
            context.m.swap(i, j);
        }
        context
    }

    pub fn crypt(&mut self, data: &mut [u8]) {
        for datum in data {
            self.x = (self.x + 1) & 0xff;
            self.y = (self.y + self.m[self.x] as usize) & 0xff;
            self.m.swap(self.x, self.y);
            let index = (self.m[self.x] as usize + self.m[self.y] as usize) & 0xff;
            *datum ^= self.m[index];
        }
    }
}

pub fn crypt_arc_four_crypt_block(data: &mut [u8], key: &[u8]) {
    CryptRc4Context::setup(key).crypt(data);
}

const MD5_K: [u32; 64] = [
    0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
    0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
    0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
    0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed, 0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
    0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
    0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
    0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
    0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
];
const MD5_SHIFTS: [[u32; 4]; 4] = [
    [7, 12, 17, 22],
    [5, 9, 14, 20],
    [4, 11, 16, 23],
    [6, 10, 15, 21],
];

pub struct CryptMd5Context {
    total: [u32; 2],
    state: [u32; 4],
    buffer: [u8; 64],
}

impl CryptMd5Context {
    pub fn start() -> Self {
        Self {
            total: [0, 0],
            state: [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476],
            buffer: [0; 64],
        }
    }

    fn process(&mut self, data: &[u8]) {
        debug_assert_eq!(data.len(), 64);
        let x: [u32; 16] =
            std::array::from_fn(|i| u32::from_le_bytes(data[i * 4..i * 4 + 4].try_into().unwrap()));
        let [mut a, mut b, mut c, mut d] = self.state;
        for i in 0..64 {
            let (f, g) = match i {
                0..=15 => ((b & c) | (!b & d), i),
                16..=31 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let next = a.wrapping_add(f).wrapping_add(x[g]).wrapping_add(MD5_K[i]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(next.rotate_left(MD5_SHIFTS[i / 16][i % 4]));
        }
        self.state[0] = self.state[0].wrapping_add(a);
        self.state[1] = self.state[1].wrapping_add(b);
        self.state[2] = self.state[2].wrapping_add(c);
        self.state[3] = self.state[3].wrapping_add(d);
    }

    pub fn update(&mut self, mut data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let mut left = ((self.total[0] >> 3) & 0x3f) as usize;
        let fill = 64 - left;
        let len = data.len() as u64;
        let bits = (len << 3) as u32;
        self.total[0] = self.total[0].wrapping_add(bits);
        self.total[1] = self.total[1]
            .wrapping_add((len >> 29) as u32)
            .wrapping_add(u32::from(u64::from(self.total[0]) < (len << 3)));
        if left != 0 && data.len() >= fill {
            self.buffer[left..].copy_from_slice(&data[..fill]);
            let block = self.buffer;
            self.process(&block);
            data = &data[fill..];
            left = 0;
        }
        while data.len() >= 64 {
            self.process(&data[..64]);
            data = &data[64..];
        }
        if !data.is_empty() {
            self.buffer[left..left + data.len()].copy_from_slice(data);
        }
    }

    pub fn finish(mut self) -> [u8; 16] {
        let mut length = [0u8; 8];
        length[..4].copy_from_slice(&self.total[0].to_le_bytes());
        length[4..].copy_from_slice(&self.total[1].to_le_bytes());
        let last = ((self.total[0] >> 3) & 0x3f) as usize;
        let pad = if last < 56 { 56 - last } else { 120 - last };
        let mut padding = [0u8; 64];
        padding[0] = 0x80;
        self.update(&padding[..pad]);
        self.update(&length);
        let mut digest = [0u8; 16];
        for (i, word) in self.state.iter().enumerate() {
            digest[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
        }
        digest
    }
}

pub fn crypt_md5_generate(data: &[u8]) -> [u8; 16] {
    let mut context = CryptMd5Context::start();
    context.update(data);
    context.finish()
}

#[cfg(test)]
mod tests {
    use super::{crypt_arc_four_crypt_block, crypt_md5_generate, CryptMd5Context};

    #[test]
    fn rc4_known_vector() {
        let mut data = b"Plaintext".to_vec();
        crypt_arc_four_crypt_block(&mut data, b"Key");
        assert_eq!(data, [0xbb, 0xf3, 0x16, 0xe8, 0xd9, 0x40, 0xaf, 0x0a, 0xd3]);
    }

    #[test]
    fn md5_known_vectors_and_chunked_update() {
        assert_eq!(
            crypt_md5_generate(b""),
            [
                0xd4, 0x1d, 0x8c, 0xd9, 0x8f, 0x00, 0xb2, 0x04, 0xe9, 0x80, 0x09, 0x98, 0xec, 0xf8,
                0x42, 0x7e
            ]
        );
        let expected = [
            0x90, 0x01, 0x50, 0x98, 0x3c, 0xd2, 0x4f, 0xb0, 0xd6, 0x96, 0x3f, 0x7d, 0x28, 0xe1,
            0x7f, 0x72,
        ];
        assert_eq!(crypt_md5_generate(b"abc"), expected);
        let mut context = CryptMd5Context::start();
        context.update(b"a");
        context.update(b"bc");
        assert_eq!(context.finish(), expected);
    }
}

#[cfg(test)]
#[path = "fx_crypt_unittest.rs"]
mod upstream_tests;
