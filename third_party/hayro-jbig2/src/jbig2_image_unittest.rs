// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in ../LICENSE-PDFIUM.

//! Close translation of core/fxcodec/jbig2/jbig2_image_unittest.cpp.
//! Native byte spans become row indices into actual packed-word Bitmaps, so
//! multiple mutable rows never alias. Literal external fixtures are converted
//! from big-endian bytes; the codec owns words and emits through PackedSink,
//! rather than retaining caller byte pointers. Invalid raw external-layout
//! constructor cases are inventoried separately, not simulated here.
#![allow(non_snake_case, non_upper_case_globals)]

use super::{Bitmap, WORD_BYTES};
use crate::decode::CombinationOperator;
use core::ops::Range;

const kWidthPixels: i32 = 80;
const kWidthBytes: usize = 10;
const kStrideBytes: usize = kWidthBytes + 2;
const kHeightLines: i32 = 20;
const kLargerHeightLines: i32 = 100;
const kTooLargeHeightLines: i32 = 40000000;

// An index, not a borrow: native tests retain simultaneous mutable row spans.
#[derive(Clone, Copy)]
struct Line(i32);
struct Image(Bitmap);
impl Image {
    fn new(w: i32, h: i32) -> Self {
        Self(Bitmap::new_pdfium(w as u32, h as u32))
    }
    fn external(w: i32, h: i32, stride: usize, bytes: &[u8]) -> Self {
        let mut image = Self::new(w, h);
        if image.0.data.is_empty() {
            return image;
        }
        // Explicit fixture representation conversion, preserving every padding
        // byte. These are valid external fixtures; the decoder never borrows
        // byte buffers as word storage. Oversized dimensions above use the
        // real production allocation predicate before touching the short buf.
        assert_eq!(stride % WORD_BYTES as usize, 0);
        assert!(stride >= (w as usize).div_ceil(8));
        let bytes = &bytes[..stride * h as usize];
        image.0.stride = (stride / WORD_BYTES as usize) as u32;
        image.0.data = bytes
            .chunks_exact(4)
            .map(|word| u32::from_be_bytes(word.try_into().unwrap()))
            .collect();
        image
    }
    fn width(&self) -> i32 {
        self.0.width as i32
    }
    fn height(&self) -> i32 {
        self.0.height as i32
    }
    fn has_data(&self) -> bool {
        !self.0.data.is_empty()
    }
    fn span(&self) -> &[u32] {
        &self.0.data
    }
    fn GetLine(&self, y: i32) -> Line {
        Line(y)
    }
    fn range(&self, line: Line) -> Range<usize> {
        if line.0 < 0 || line.0 >= self.height() || !self.has_data() {
            return 0..0;
        }
        let start = line.0 as usize * self.0.stride as usize;
        start..start + self.0.stride as usize
    }
    fn empty(&self, line: Line) -> bool {
        self.range(line).is_empty()
    }
    fn line_size(&self, line: Line) -> usize {
        self.range(line).len() * 4
    }
    fn byte(&self, line: Line, x: usize) -> u8 {
        self.0.data[self.range(line).start + x / 4].to_be_bytes()[x % 4]
    }
    fn GetPixel(&self, x: i32, line: Line) -> u8 {
        self.0.get_pixel(x as u32, line.0 as u32)
    }
    fn SetPixel(&mut self, x: i32, line: Line, value: bool) {
        self.0.set_pixel(x as u32, line.0 as u32, u8::from(value));
    }
    fn SubImage(&self, x: i32, y: i32, w: i32, h: i32) -> Self {
        Self(self.0.pdfium_subimage(x, y, w as u32, h as u32).unwrap())
    }
    fn Expand(&mut self, height: i32, value: bool) {
        // Production striped-page expansion runs before composition. A zero
        // row, zero width region requests only the expansion operation.
        self.0.expand_background = Some(value);
        self.0
            .combine_completed_rows(&Bitmap::default(), 0, height, CombinationOperator::Or, 0);
    }
    fn CopyLine(&mut self, to: Line, from: Line) {
        let dest = self.range(to);
        if dest.is_empty() {
            return;
        }
        let src = self.range(from);
        if src.is_empty() {
            self.0.data[dest].fill(0);
        } else {
            self.0.data.copy_within(src, dest.start);
        }
    }
}

fn CheckImageEq(img1: &Image, img2: &Image, line: u32) {
    assert_eq!(img1.width(), img2.width());
    assert_eq!(img1.height(), img2.height());
    for y in 0..img1.height() {
        let img1_line = img1.GetLine(y);
        let img2_line = img2.GetLine(y);
        for x in 0..img1.width() {
            assert_eq!(
                img1.GetPixel(x, img1_line),
                img2.GetPixel(x, img2_line),
                "at {x} {y} actual line {line}"
            );
        }
    }
}

#[test]
fn EmptyImage() {
    let mut empty = Image::new(0, 0);
    assert_eq!(empty.width(), 0);
    assert_eq!(empty.height(), 0);

    // Out-of-bounds GetLine() returns empty.
    let line0 = empty.GetLine(0);
    let line1 = empty.GetLine(1);
    assert!(empty.empty(line0));
    assert!(empty.empty(line1));

    // Out-of-bounds SetPixel() is silent no-op.
    empty.SetPixel(0, line0, true);
    empty.SetPixel(1, line1, true);

    // Out-of-bounds GetPixel returns 0.
    assert_eq!(empty.GetPixel(0, line0), 0);
    assert_eq!(empty.GetPixel(1, line1), 0);
}

#[test]
fn JBig2ImageCreate() {
    let mut img = Image::new(kWidthPixels, kHeightLines);
    assert_eq!(kWidthPixels, img.width());
    assert_eq!(kHeightLines, img.height());
    let line0 = img.GetLine(0);
    assert_eq!(0, img.GetPixel(0, line0));
    assert_eq!(kStrideBytes, img.line_size(line0));
    assert_eq!(0, img.byte(line0, 0));
    let line_last = img.GetLine(kHeightLines - 1);
    assert_eq!(kStrideBytes, img.line_size(line_last));
    assert_eq!(0, img.byte(line_last, kWidthBytes - 1));

    img.SetPixel(0, line0, true);
    img.SetPixel(kWidthPixels - 1, line_last, true);
    assert_eq!(1, img.GetPixel(0, line0));
    assert_eq!(1, img.GetPixel(kWidthPixels - 1, line_last));
    assert_eq!(0x80, img.byte(line0, 0));
    assert_eq!(0x01, img.byte(line_last, kWidthBytes - 1));

    // Out-of-bounds SetPixel() is silent no-op.
    let line1 = img.GetLine(1);
    img.SetPixel(-1, line1, true);
    let line_oob = img.GetLine(kHeightLines);
    img.SetPixel(kWidthPixels, line_oob, true);

    // Out-of-bounds GetPixel returns 0.
    assert_eq!(0, img.GetPixel(-1, img.GetLine(-1)));
    assert_eq!(0, img.GetPixel(kWidthPixels, img.GetLine(kHeightLines)));

    // Out-of-bounds GetLine() returns empty.
    assert!(img.empty(img.GetLine(-1)));
    assert!(img.empty(img.GetLine(kHeightLines)));
}

#[test]
fn JBig2ImageCreateTooBig() {
    let img = Image::new(kWidthPixels, kTooLargeHeightLines);
    assert_eq!(0, img.width());
    assert_eq!(0, img.height());
    assert!(!img.has_data());
    assert!(img.span().is_empty());
}

#[test]
fn JBig2ImageCreateExternal() {
    let buf = [0u8; kHeightLines as usize * kStrideBytes];
    let mut img = Image::external(kWidthPixels, kHeightLines, kStrideBytes, &buf);
    let line0 = img.GetLine(0);
    let line_last = img.GetLine(kHeightLines - 1);
    img.SetPixel(0, line0, true);
    img.SetPixel(kWidthPixels - 1, line_last, false);
    assert_eq!(kWidthPixels, img.width());
    assert_eq!(kHeightLines, img.height());
    assert!(img.GetPixel(0, line0) != 0);
    assert!(img.GetPixel(kWidthPixels - 1, line_last) == 0);
}

#[test]
fn JBig2ImageCreateExternalTooBig() {
    let buf = [0u8; kHeightLines as usize * kStrideBytes];
    let img = Image::external(kWidthPixels, kTooLargeHeightLines, kStrideBytes, &buf);
    assert_eq!(0, img.width());
    assert_eq!(0, img.height());
    assert!(!img.has_data());
    assert!(img.span().is_empty());
}

#[test]
fn JBig2ImageExpand() {
    let mut img = Image::new(kWidthPixels, kHeightLines);
    let mut line0 = img.GetLine(0);
    let mut line_last = img.GetLine(kHeightLines - 1);
    img.SetPixel(0, line0, true);
    img.SetPixel(kWidthPixels - 1, line_last, false);
    img.Expand(kLargerHeightLines, true);
    assert_eq!(kWidthPixels, img.width());
    assert_eq!(kLargerHeightLines, img.height());
    line0 = img.GetLine(0);
    line_last = img.GetLine(kHeightLines - 1);
    assert!(img.GetPixel(0, line0) != 0);
    assert!(img.GetPixel(kWidthPixels - 1, line_last) == 0);
    assert!(img.GetPixel(kWidthPixels - 1, img.GetLine(kLargerHeightLines - 1)) != 0);
}

#[test]
fn JBig2ImageExpandTooBig() {
    let mut img = Image::new(kWidthPixels, kHeightLines);
    let mut line0 = img.GetLine(0);
    let mut line_last = img.GetLine(kHeightLines - 1);
    img.SetPixel(0, line0, true);
    img.SetPixel(kWidthPixels - 1, line_last, false);
    img.Expand(kTooLargeHeightLines, true);
    assert_eq!(kWidthPixels, img.width());
    assert_eq!(kHeightLines, img.height());
    line0 = img.GetLine(0);
    line_last = img.GetLine(kHeightLines - 1);
    assert!(img.GetPixel(0, line0) != 0);
    assert!(img.GetPixel(kWidthPixels - 1, line_last) == 0);
}

#[test]
fn JBig2ImageExpandExternal() {
    let buf = [0u8; kHeightLines as usize * kStrideBytes];
    let mut img = Image::external(kWidthPixels, kHeightLines, kStrideBytes, &buf);
    let mut line0 = img.GetLine(0);
    let mut line_last = img.GetLine(kHeightLines - 1);
    img.SetPixel(0, line0, true);
    img.SetPixel(kWidthPixels - 1, line_last, false);
    img.Expand(kLargerHeightLines, true);
    assert_eq!(kWidthPixels, img.width());
    assert_eq!(kLargerHeightLines, img.height());
    line0 = img.GetLine(0);
    line_last = img.GetLine(kHeightLines - 1);
    assert!(img.GetPixel(0, line0) != 0);
    assert!(img.GetPixel(kWidthPixels - 1, line_last) == 0);
    assert!(img.GetPixel(kWidthPixels - 1, img.GetLine(kLargerHeightLines - 1)) != 0);
}

#[test]
fn JBig2ImageExpandExternalTooBig() {
    let buf = [0u8; kHeightLines as usize * kStrideBytes];
    let mut img = Image::external(kWidthPixels, kHeightLines, kStrideBytes, &buf);
    let mut line0 = img.GetLine(0);
    let mut line_last = img.GetLine(kHeightLines - 1);
    img.SetPixel(0, line0, true);
    img.SetPixel(kWidthPixels - 1, line_last, false);
    img.Expand(kTooLargeHeightLines, true);
    assert_eq!(kWidthPixels, img.width());
    assert_eq!(kHeightLines, img.height());
    line0 = img.GetLine(0);
    line_last = img.GetLine(kHeightLines - 1);
    assert!(img.GetPixel(0, line0) != 0);
    assert!(img.GetPixel(kWidthPixels - 1, line_last) == 0);
}

#[test]
fn JBig2EmptyImage() {
    let empty = Image::new(0, 0);

    // Empty subimage.
    let sub1 = empty.SubImage(0, 0, 0, 0);
    assert_eq!(sub1.width(), 0);
    assert_eq!(sub1.height(), 0);

    // Larger dimensions are zero-padded.
    let mut sub2 = empty.SubImage(0, 0, 1, 1);
    assert_eq!(1, sub2.width());
    assert_eq!(1, sub2.height());
    assert_eq!(0, sub2.GetPixel(0, sub2.GetLine(0)));

    // Bad dimensions give an empty image.
    sub2 = empty.SubImage(0, 0, -1, -1);
    assert_eq!(sub2.width(), 0);
    assert_eq!(sub2.height(), 0);

    // Bad offsets zero pad the image.
    let mut sub3 = empty.SubImage(-1, -1, 2, 2);
    assert_eq!(sub3.width(), 2);
    assert_eq!(sub3.height(), 2);

    // Bad dimensions and bad offsets give an empty image.
    sub3 = empty.SubImage(-1, -1, -100, -100);
    assert_eq!(sub3.width(), 0);
    assert_eq!(sub3.height(), 0);
}

#[test]
fn JBig2SubImage() {
    // clang-format off
    // 1-px wide rectangle in image.
    let pattern: [u8; 40] = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0xff, 0x80, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x01, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0xff, 0x80, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    // 1-px wide rectangle in image, offset 2 in x.
    let pattern20: [u8; 40] = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0xfe, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x04, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0xfe, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    // 1-px wide rectangle in image, offset 2 in x and y, padded.
    let pattern22: [u8; 40] = [
        0x00, 0x04, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0xfe, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    // 1-px wide rectangle in image, offset 16 in x, 1 in y, padded.
    let pattern161: [u8; 40] = [
        0xff, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0xff, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    // clang-format on

    // Image size a nice clean power of two.
    let img32 = Image::external(32, 5, 8, &pattern);

    // Image size not a nice clean value.
    let img37 = Image::external(37, 5, 8, &pattern);

    // Expected results to check against.
    let expected20 = Image::external(30, 5, 8, &pattern20);
    let expected22 = Image::external(30, 5, 8, &pattern22);
    let expected161 = Image::external(25, 5, 8, &pattern161);
    let expected_zeros = Image::new(32, 5);

    // Empty subimage.
    let mut sub = img32.SubImage(0, 0, 0, 0);
    assert_eq!(sub.width(), 0);
    assert_eq!(sub.height(), 0);

    // Full sub-image.
    sub = img32.SubImage(0, 0, 32, 5);
    assert_eq!(sub.width(), 32);
    assert_eq!(sub.height(), 5);
    CheckImageEq(&img32, &sub, line!());

    sub = img37.SubImage(0, 0, 32, 5);
    assert_eq!(sub.width(), 32);
    assert_eq!(sub.height(), 5);
    CheckImageEq(&img32, &sub, line!());

    // Actual bit manipulations.
    sub = img32.SubImage(2, 0, 30, 5);
    CheckImageEq(&expected20, &sub, line!());

    sub = img37.SubImage(2, 2, 30, 5);
    CheckImageEq(&expected22, &sub, line!());

    // Fast path.
    sub = img37.SubImage(16, 1, 25, 5);
    CheckImageEq(&expected161, &sub, line!());

    // Aligned Sub-image including cruft in stride beyond width.
    sub = img37.SubImage(32, 0, 32, 5);
    CheckImageEq(&expected_zeros, &sub, line!());

    // Sub-image waaaaay beyond width.
    sub = img37.SubImage(2000, 0, 32, 5);
    CheckImageEq(&expected_zeros, &sub, line!());

    // Sub-image waaaaay beyond height.
    sub = img37.SubImage(0, 2000, 32, 5);
    CheckImageEq(&expected_zeros, &sub, line!());

    // Sub-image with negative x offset.
    sub = img37.SubImage(-1, 0, 32, 5);
    CheckImageEq(&expected_zeros, &sub, line!());

    // Sub-image with negative y offset.
    sub = img37.SubImage(0, -1, 32, 5);
    CheckImageEq(&expected_zeros, &sub, line!());

    // Sub-image with negative width.
    sub = img37.SubImage(-1, 0, 32, 5);
    CheckImageEq(&expected_zeros, &sub, line!());

    // Sub-image with negative height.
    sub = img37.SubImage(0, -1, 32, 5);
    CheckImageEq(&expected_zeros, &sub, line!());

    // Sub-image wider than original.
    sub = img37.SubImage(0, 0, 128, 5);
    assert_eq!(128, sub.width());
    assert_eq!(5, sub.height());

    // Sub-image higher than original.
    sub = img37.SubImage(0, 0, 32, 40);
    assert_eq!(32, sub.width());
    assert_eq!(40, sub.height());
}

#[test]
fn JBig2CopyLine() {
    // clang-format off
    // Horizontal line in image.
    let pattern: [u8; 24] = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0xff, 0x80, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];

    let expected_pattern: [u8; 24] = [
        0x00, 0x01, 0xff, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    // clang-format on

    let mut img = Image::external(37, 3, 8, &pattern);
    let expected = Image::external(37, 3, 8, &expected_pattern);

    // Shuffle.
    img.CopyLine(img.GetLine(2), img.GetLine(1));
    img.CopyLine(img.GetLine(1), img.GetLine(0));
    img.CopyLine(img.GetLine(0), img.GetLine(2));

    // Clear top line via invalid |from| offset.
    img.CopyLine(img.GetLine(2), img.GetLine(3));

    // Copies with invalid |to|s don't mess with things.
    img.CopyLine(img.GetLine(-1), img.GetLine(0));
    img.CopyLine(img.GetLine(4), img.GetLine(0));
    img.CopyLine(img.GetLine(-1), img.GetLine(-1));
    img.CopyLine(img.GetLine(4), img.GetLine(4));

    CheckImageEq(&expected, &img, line!());
}
