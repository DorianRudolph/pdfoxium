// Copyright 2024 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/parser/cpdf_number_unittest.cpp`.
#![allow(non_snake_case)]

use super::CpdfScalarObject;
use crate::fpdfapi::parser::cpdf_object::PdfObject;
use crate::fxcrt::fx_stream::{ArchiveStream, WriteStream};

#[derive(Default)]
struct ByteStringArchiveStream {
    str_: Vec<u8>,
}
impl WriteStream for ByteStringArchiveStream {
    fn write_block(&mut self, buffer: &[u8]) -> bool {
        self.str_.extend_from_slice(buffer);
        true
    }
}
impl ArchiveStream for ByteStringArchiveStream {
    fn current_offset(&self) -> i64 {
        unreachable!("native fixture CurrentOffset is NOTREACHED")
    }
}

#[test]
fn WriteToFloat() {
    for (value, expected) in [
        (0.0, " 0"),
        (1.0, " 1"),
        (-7.5, " -7.5"),
        (38.895285, " 38.895287"),
        (-77.037232, " -77.03723"),
        (f32::MAX, " 340282350000000000000000000000000000000"),
        (
            f32::MIN_POSITIVE,
            " .000000000000000000000000000000000000011754944",
        ),
    ] {
        let mut output_stream = ByteStringArchiveStream::default();
        let number = CpdfScalarObject::number_from_float(value);
        assert!(PdfObject::write_to(&number, &mut output_stream, None));
        assert_eq!(expected.as_bytes(), output_stream.str_);
    }
}

#[test]
fn WriteToInt() {
    for (value, expected) in [
        (0, " 0"),
        (1, " 1"),
        (-99, " -99"),
        (1234, " 1234"),
        (-54321, " -54321"),
        (i32::MAX, " 2147483647"),
        (i32::MIN, " -2147483648"),
    ] {
        let mut output_stream = ByteStringArchiveStream::default();
        let number = CpdfScalarObject::number_from_signed(value);
        assert!(PdfObject::write_to(&number, &mut output_stream, None));
        assert_eq!(expected.as_bytes(), output_stream.str_);
    }
}
