// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2022 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Translation of `core/fxcrt/fx_2d_size.h` for integral dimensions.

pub fn fx_2d_size_or_die<W, H>(width: W, height: H) -> usize
where
    W: TryInto<usize>,
    H: TryInto<usize>,
{
    let width = width.try_into().unwrap_or_else(|_| panic!("invalid width"));
    let height = height
        .try_into()
        .unwrap_or_else(|_| panic!("invalid height"));
    width.checked_mul(height).expect("2D size overflow")
}

#[cfg(test)]
mod tests {
    use super::fx_2d_size_or_die;

    #[test]
    fn dimensions_are_checked_before_multiplication() {
        assert_eq!(fx_2d_size_or_die(12i32, 3usize), 36);
        assert!(std::panic::catch_unwind(|| fx_2d_size_or_die(-1i32, 3u32)).is_err());
        assert!(std::panic::catch_unwind(|| fx_2d_size_or_die(usize::MAX, 2u8)).is_err());
    }
}
