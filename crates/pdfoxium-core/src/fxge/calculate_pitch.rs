// Copyright 2022 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Translation of `core/fxge/calculate_pitch.h` and `.cpp`.

use crate::fxcrt::fx_ceil_div::ceil_div_u32;

fn calculate_pitch8_safely(bpc: u32, components: u32, width: i32) -> Option<u32> {
    let width = u32::try_from(width).ok()?;
    let pitch = bpc.checked_mul(components)?.checked_mul(width)?;
    Some(ceil_div_u32(pitch, 8))
}

fn calculate_pitch32_safely(bpp: i32, width: i32) -> Option<u32> {
    let bpp = u32::try_from(bpp).ok()?;
    let width = u32::try_from(width).ok()?;
    let pitch = bpp.checked_mul(width)?;
    ceil_div_u32(pitch, 32).checked_mul(4)
}

pub fn calculate_pitch8_or_die(bpc: u32, components: u32, width: i32) -> u32 {
    calculate_pitch8_safely(bpc, components, width).expect("pitch8 overflow")
}

pub fn calculate_pitch32_or_die(bpp: i32, width: i32) -> u32 {
    calculate_pitch32_safely(bpp, width).expect("pitch32 overflow")
}

pub fn calculate_pitch8(bpc: u32, components: u32, width: i32) -> Option<u32> {
    calculate_pitch8_safely(bpc, components, width)
}

pub fn calculate_pitch32(bpp: i32, width: i32) -> Option<u32> {
    calculate_pitch32_safely(bpp, width)
}

#[cfg(test)]
mod tests {
    use super::{calculate_pitch32, calculate_pitch8};

    #[test]
    fn pitch_rounding_and_checked_intermediates() {
        assert_eq!(calculate_pitch8(1, 1, 9), Some(2));
        assert_eq!(calculate_pitch32(1, 33), Some(8));
        assert_eq!(calculate_pitch8(32, 4, -1), None);
        assert_eq!(calculate_pitch8(u32::MAX, 2, 1), None);
        assert_eq!(calculate_pitch32(i32::MAX, i32::MAX), None);
    }
}
