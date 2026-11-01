// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxcrt/cfx_bitstream_unittest.cpp` at the pinned PDFium revision.

#![allow(non_snake_case, non_upper_case_globals)]

use super::CfxBitStream;

fn ReferenceGetBits32(data: &[u8], bitpos: usize, nbits: u32) -> u32 {
    let mut result = 0;
    for i in 0..nbits {
        if data[(bitpos + i as usize) / 8] & (1 << (7 - (bitpos + i as usize) % 8)) != 0 {
            result |= 1 << (nbits - i - 1);
        }
    }
    result
}

#[test]
fn BitStream() {
    const kData: [u8; 8] = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77];
    let mut bitstream = CfxBitStream::new(&kData);

    // Initial state.
    assert!(!(bitstream.is_eof()));
    assert_eq!(0, bitstream.pos());
    assert_eq!(64, bitstream.bits_remaining());

    // Read, read, read!
    assert_eq!(0x00, bitstream.get_bits(8));
    assert_eq!(8, bitstream.pos());
    assert_eq!(56, bitstream.bits_remaining());

    assert_eq!(0x00, bitstream.get_bits(1));
    assert_eq!(9, bitstream.pos());
    assert_eq!(55, bitstream.bits_remaining());

    assert_eq!(0x00, bitstream.get_bits(2));
    assert_eq!(11, bitstream.pos());
    assert_eq!(53, bitstream.bits_remaining());

    assert_eq!(0x04, bitstream.get_bits(3));
    assert_eq!(14, bitstream.pos());
    assert_eq!(50, bitstream.bits_remaining());

    assert_eq!(0x04, bitstream.get_bits(4));
    assert_eq!(18, bitstream.pos());
    assert_eq!(46, bitstream.bits_remaining());

    assert_eq!(0x44, bitstream.get_bits(7));
    assert_eq!(25, bitstream.pos());
    assert_eq!(39, bitstream.bits_remaining());

    assert_eq!(0xCD, bitstream.get_bits(9));
    assert_eq!(34, bitstream.pos());
    assert_eq!(30, bitstream.bits_remaining());

    assert_eq!(0x08AA, bitstream.get_bits(15));
    assert_eq!(49, bitstream.pos());
    assert_eq!(15, bitstream.bits_remaining());

    // Cannot advance past the end.
    assert_eq!(0x00, bitstream.get_bits(16));
    assert_eq!(49, bitstream.pos());
    assert_eq!(15, bitstream.bits_remaining());

    // Make sure SkipBits() works.
    bitstream.skip_bits(14);
    assert_eq!(63, bitstream.pos());
    assert_eq!(1, bitstream.bits_remaining());
    bitstream.skip_bits(2);
    assert_eq!(65, bitstream.pos());
    assert_eq!(0, bitstream.bits_remaining());
    assert!(bitstream.is_eof());

    // Make sure Rewind() works.
    bitstream.rewind();
    assert!(!(bitstream.is_eof()));
    assert_eq!(0, bitstream.pos());
    assert_eq!(64, bitstream.bits_remaining());

    // Read some more.
    bitstream.skip_bits(5);
    assert_eq!(5, bitstream.pos());
    assert_eq!(59, bitstream.bits_remaining());

    assert_eq!(0x0448, bitstream.get_bits(17));
    assert_eq!(22, bitstream.pos());
    assert_eq!(42, bitstream.bits_remaining());

    // Make sure ByteAlign() works.
    bitstream.byte_align();
    assert_eq!(24, bitstream.pos());
    assert_eq!(40, bitstream.bits_remaining());

    assert_eq!(0x19A22AB3, bitstream.get_bits(31));
    assert_eq!(55, bitstream.pos());
    assert_eq!(9, bitstream.bits_remaining());

    // Do some bigger reads.
    bitstream.rewind();
    assert_eq!(0x112233, bitstream.get_bits(32));
    assert_eq!(32, bitstream.pos());
    assert_eq!(32, bitstream.bits_remaining());

    bitstream.rewind();
    bitstream.skip_bits(31);
    assert_eq!(0xA22AB33B, bitstream.get_bits(32));
    assert_eq!(63, bitstream.pos());
    assert_eq!(1, bitstream.bits_remaining());

    // Skip past the end.
    bitstream.skip_bits(1000);
    assert!(bitstream.is_eof());
    assert_eq!(1063, bitstream.pos());
    assert_eq!(0, bitstream.bits_remaining());
    assert_eq!(0, bitstream.get_bits(4));

    // Align past the end.
    bitstream.byte_align();
    assert!(bitstream.is_eof());
    assert_eq!(1064, bitstream.pos());
    assert_eq!(0, bitstream.bits_remaining());
    assert_eq!(0, bitstream.get_bits(4));
}

#[test]
fn BitStreamEmpty() {
    let mut bitstream = CfxBitStream::new(&[]);
    assert!(bitstream.is_eof());
    assert_eq!(0, bitstream.pos());
    assert_eq!(0, bitstream.bits_remaining());

    // Getting bits returns zero and doesn't advance.
    assert_eq!(0, bitstream.get_bits(4));
    assert_eq!(0, bitstream.pos());

    // Skip past the end.
    bitstream.skip_bits(63);
    assert!(bitstream.is_eof());
    assert_eq!(63, bitstream.pos());
    assert_eq!(0, bitstream.bits_remaining());
    assert_eq!(0, bitstream.get_bits(4));

    // Align past the end.
    bitstream.byte_align();
    assert!(bitstream.is_eof());
    assert_eq!(64, bitstream.pos());
    assert_eq!(0, bitstream.bits_remaining());
    assert_eq!(0, bitstream.get_bits(4));
}

#[test]
fn BitStreamBig() {
    // Native fabricates a huge span over 32 bytes and never reads it. Keep its
    // logical bit-size fixture directly, without an invalid Rust slice.
    let kNotReallyBigEnough = [0u8; 32];
    const kAllocationBytes: usize = usize::MAX / 8;
    const kAllocationBits: usize = kAllocationBytes * 8;
    let mut bitstream = CfxBitStream {
        bit_pos: 0,
        bit_size: kAllocationBits,
        data: &kNotReallyBigEnough,
    };
    assert!(!bitstream.is_eof());
    assert_eq!(0, bitstream.pos());
    assert_eq!(kAllocationBits, bitstream.bits_remaining());
    bitstream.skip_bits(kAllocationBits - 1023);
    assert!(!bitstream.is_eof());
    assert_eq!(kAllocationBits - 1023, bitstream.pos());
    assert_eq!(1023, bitstream.bits_remaining());
    bitstream.byte_align();
    assert!(!bitstream.is_eof());
    assert_eq!(kAllocationBits - 1016, bitstream.pos());
    assert_eq!(1016, bitstream.bits_remaining());
}

#[test]
fn BitStreamSameAsReferenceGetBits32() {
    const kData: [u8; 8] = [0xDE, 0x3F, 0xB1, 0x7C, 0x12, 0x9A, 0x04, 0x56];
    let mut bitstream = CfxBitStream::new(&kData);
    for nbits in 1..=32 {
        for bitpos in 0..kData.len() * 8 - nbits as usize {
            bitstream.rewind();
            bitstream.skip_bits(bitpos);
            assert_eq!(
                bitstream.get_bits(nbits),
                ReferenceGetBits32(&kData, bitpos, nbits)
            );
        }
    }
}
