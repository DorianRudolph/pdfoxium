// Anti-Grain Geometry - Version 2.3
// Copyright (C) 2002-2005 Maxim Shemanarev (http://www.antigrain.com)
//
// Permission to copy, use, modify, sell and distribute this software
// is granted provided this copyright notice appears in all copies.
// This software is provided "as is" without express or implied
// warranty, and with no claim as to its suitability for any purpose.

//! Translation of `third_party/agg23/agg_color_gray.h`.

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Gray8 {
    pub v: u8,
    pub a: u8,
}

impl Gray8 {
    pub const BASE_SHIFT: u32 = 8;
    pub const BASE_SIZE: u32 = 1 << Self::BASE_SHIFT;
    pub const BASE_MASK: u32 = Self::BASE_SIZE - 1;

    pub const fn new(value: u32, alpha: u32) -> Self {
        Self {
            v: value as u8,
            a: alpha as u8,
        }
    }

    pub const fn opaque(value: u32) -> Self {
        Self::new(value, Self::BASE_MASK)
    }
}
