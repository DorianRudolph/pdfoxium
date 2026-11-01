/*
 * Copyright (c) 1988-1997 Sam Leffler
 * Copyright (c) 1991-1997 Silicon Graphics, Inc.
 *
 * Permission to use, copy, modify, distribute, and sell this software and
 * its documentation for any purpose is hereby granted without fee, provided
 * that (i) the above copyright notices and this permission notice appear in
 * all copies of the software and related documentation, and (ii) the names of
 * Sam Leffler and Silicon Graphics may not be used in any advertising or
 * publicity relating to the software without the specific, prior written
 * permission of Sam Leffler and Silicon Graphics.
 *
 * THE SOFTWARE IS PROVIDED "AS-IS" AND WITHOUT WARRANTY OF ANY KIND,
 * EXPRESS, IMPLIED OR OTHERWISE, INCLUDING WITHOUT LIMITATION, ANY
 * WARRANTY OF MERCHANTABILITY OR FITNESS FOR A PARTICULAR PURPOSE.
 *
 * IN NO EVENT SHALL SAM LEFFLER OR SILICON GRAPHICS BE LIABLE FOR
 * ANY SPECIAL, INCIDENTAL, INDIRECT OR CONSEQUENTIAL DAMAGES OF ANY KIND,
 * OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
 * WHETHER OR NOT ADVISED OF THE POSSIBILITY OF DAMAGE, AND ON ANY THEORY OF
 * LIABILITY, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE
 * OF THIS SOFTWARE.
 */

//! Safe direct translation of the pinned libtiff YCbCr and CIE Lab converters.
//! Integer lookup rounding, reference ranges and the native display matrix are
//! retained. This is color conversion after published Rust decompression.
use super::super::progressive_decoder_context::{CodecError, CodecResult};

pub(super) struct YCbCr {
    y: [i32; 256],
    cr_r: [i32; 256],
    cb_b: [i32; 256],
    cr_g: [i32; 256],
    cb_g: [i32; 256],
}
impl YCbCr {
    pub fn new(luma: &[f32], reference: &[f32]) -> CodecResult<Self> {
        if luma.len() != 3
            || reference.len() != 6
            || luma.iter().any(|v| !v.is_finite())
            || luma[1] == 0.0
            || reference.iter().any(|v| {
                !v.is_finite()
                    || *v <= (-0x7fff_ffffi64 + 128) as f32
                    || *v >= 0x7fff_ffffi64 as f32
            })
        {
            return Err(CodecError::InvalidData);
        }
        let fix = |v: f32| (f64::from(v.clamp(0.0, 2.0)) * 65536.0 + 0.5) as i32;
        let f1 = 2.0 - 2.0 * luma[0];
        let f3 = 2.0 - 2.0 * luma[2];
        let d1 = fix(f1);
        let d2 = -fix(luma[0] * f1 / luma[1]);
        let d3 = fix(f3);
        let d4 = -fix(luma[2] * f3 / luma[1]);
        let code = |c: i32, black: f32, white: f32, range: f32| {
            ((c - (black as i32)) as f32 * range / if white != black { white - black } else { 1.0 })
                .clamp(-4096.0, 4096.0) as i32
        };
        let mut result = Self {
            y: [0; 256],
            cr_r: [0; 256],
            cb_b: [0; 256],
            cr_g: [0; 256],
            cb_g: [0; 256],
        };
        for i in 0..256 {
            let cr = code(
                i as i32 - 128,
                reference[4] - 128.0,
                reference[5] - 128.0,
                127.0,
            );
            let cb = code(
                i as i32 - 128,
                reference[2] - 128.0,
                reference[3] - 128.0,
                127.0,
            );
            result.cr_r[i] = (d1 * cr + 32768) >> 16;
            result.cb_b[i] = (d3 * cb + 32768) >> 16;
            result.cr_g[i] = d2 * cr;
            result.cb_g[i] = d4 * cb + 32768;
            result.y[i] = code(i as i32, reference[0], reference[1], 255.0);
        }
        Ok(result)
    }
    pub fn convert(&self, y: u8, cb: u8, cr: u8) -> [u8; 3] {
        let y = self.y[y as usize];
        let cb = cb as usize;
        let cr = cr as usize;
        [
            (y + self.cr_r[cr]).clamp(0, 255) as u8,
            (y + ((self.cb_g[cb] + self.cr_g[cr]) >> 16)).clamp(0, 255) as u8,
            (y + self.cb_b[cb]).clamp(0, 255) as u8,
        ]
    }
}

pub(super) struct CieLab {
    white: [f32; 3],
    table: Vec<u8>,
}
impl CieLab {
    pub fn new(white: &[f32]) -> CodecResult<Self> {
        if white.len() != 2 || white.iter().any(|v| !v.is_finite()) || white[1] == 0.0 {
            return Err(CodecError::InvalidData);
        }
        // display_sRGB has identical 2.4 gamma, 1.0 black and 100.0 white
        // luminance for each channel. Native conversion indexes a 1500-step
        // table, rather than applying the transfer function per output pixel.
        let table = (0..=1500)
            .map(|index| {
                let value =
                    255.0f32 * ((f64::from(index) / 1500.0).powf(1.0 / f64::from(2.4f32)) as f32);
                (value + 0.5).min(255.0) as u8
            })
            .collect();
        Ok(Self {
            white: [
                white[0] / white[1] * 100.0,
                100.0,
                (1.0 - white[0] - white[1]) / white[1] * 100.0,
            ],
            table,
        })
    }
    pub fn convert(&self, values: [u16; 4], bits: u8) -> [u8; 3] {
        let (light, a, b) = if bits == 8 {
            (
                u32::from(values[0]) * 257,
                i32::from(values[1] as u8 as i8) * 256,
                i32::from(values[2] as u8 as i8) * 256,
            )
        } else {
            (
                u32::from(values[0]),
                i32::from(values[1] as i16),
                i32::from(values[2] as i16),
            )
        };
        let light = light as f32 * 100.0 / 65535.0;
        let (y, cby) = if light < 8.856 {
            let y = light * self.white[1] / 903.292;
            (y, 7.787 * (y / self.white[1]) + 16.0 / 116.0)
        } else {
            let cby = (light + 16.0) / 116.0;
            (self.white[1] * cby * cby * cby, cby)
        };
        let component = |value: f32, white: f32| {
            if value < 0.2069 {
                white * (value - 0.13793) / 7.787
            } else {
                white * value * value * value
            }
        };
        let x = component(a as f32 / 256.0 / 500.0 + cby, self.white[0]);
        let z = component(cby - b as f32 / 256.0 / 200.0, self.white[2]);
        let matrix = [
            [3.2410, -1.5374, -0.4986],
            [-0.9692, 1.8760, 0.0416],
            [0.0556, -0.2040, 1.0570],
        ];
        matrix.map(|row| {
            let luminance = (row[0] * x + row[1] * y + row[2] * z).clamp(1.0, 100.0);
            let index = ((luminance - 1.0) / ((100.0 - 1.0) / 1500.0)) as usize;
            self.table[index.min(1500)]
        })
    }
}
