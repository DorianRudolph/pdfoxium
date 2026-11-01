// Adapted from libtiff as bundled by PDFium for pdfoxium.
// Distributed under the original license notice below.
//
/*
 * Copyright (c) 1997 Greg Ward Larson
 * Copyright (c) 1997 Silicon Graphics, Inc.
 *
 * Permission to use, copy, modify, distribute, and sell this software and
 * its documentation for any purpose is hereby granted without fee, provided
 * that (i) the above copyright notices and this permission notice appear in
 * all copies of the software and related documentation, and (ii) the names of
 * Sam Leffler, Greg Larson and Silicon Graphics may not be used in any
 * advertising or publicity relating to the software without the specific,
 * prior written permission of Sam Leffler, Greg Larson and Silicon Graphics.
 *
 * THE SOFTWARE IS PROVIDED "AS-IS" AND WITHOUT WARRANTY OF ANY KIND,
 * EXPRESS, IMPLIED OR OTHERWISE, INCLUDING WITHOUT LIMITATION, ANY
 * WARRANTY OF MERCHANTABILITY OR FITNESS FOR A PARTICULAR PURPOSE.
 *
 * IN NO EVENT SHALL SAM LEFFLER, GREG LARSON OR SILICON GRAPHICS BE LIABLE
 * FOR ANY SPECIAL, INCIDENTAL, INDIRECT OR CONSEQUENTIAL DAMAGES OF ANY KIND,
 * OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
 * WHETHER OR NOT ADVISED OF THE POSSIBILITY OF DAMAGE, AND ON ANY THEORY OF
 * LIABILITY, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE
 * OF THIS SOFTWARE.
 */

//! Decode-only portable SGILog/SGILog24 from the pinned tif_luv.c.
//! Native RGBA requests SGILOGDATAFMT_8BIT; encoding, dithering and alternate
//! floating/integer export formats are not called by that boundary.
#[path = "log_luv_uv.rs"]
mod uv;
use super::super::progressive_decoder_context::{allocate, bytes, CodecError, CodecResult};
fn luminance16(code: u16) -> f64 {
    let exponent = code & 0x7fff;
    if exponent == 0 {
        return 0.0;
    }
    let value = (std::f64::consts::LN_2 / 256.0 * (f64::from(exponent) + 0.5)
        - std::f64::consts::LN_2 * 64.0)
        .exp();
    if code & 0x8000 != 0 {
        -value
    } else {
        value
    }
}
fn display(value: f64) -> u8 {
    if value <= 0.0 {
        0
    } else if value >= 1.0 {
        255
    } else {
        (256.0 * value.sqrt()) as u8
    }
}
fn xyz_to_rgb(xyz: [f32; 3]) -> [u8; 3] {
    let xyz = xyz.map(f64::from);
    [
        [2.690, -1.276, -0.414],
        [-1.022, 1.978, 0.044],
        [0.061, -0.224, 1.163],
    ]
    .map(|row| display(row[0] * xyz[0] + row[1] * xyz[1] + row[2] * xyz[2]))
}
fn uv_decode(code: u16) -> Option<(f64, f64)> {
    if code >= 16289 {
        return None;
    }
    let mut lower = 0;
    let mut upper = uv::UV_ROWS.len();
    while upper - lower > 1 {
        let mid = (lower + upper) >> 1;
        let cumulative = uv::UV_ROWS[mid].2;
        if code > cumulative {
            lower = mid
        } else if code < cumulative {
            upper = mid
        } else {
            lower = mid;
            break;
        }
    }
    let row = uv::UV_ROWS[lower];
    Some((
        f64::from(row.0) + (f64::from(code - row.2) + 0.5) * f64::from(0.0035f32),
        f64::from(0.01694f32) + (lower as f64 + 0.5) * f64::from(0.0035f32),
    ))
}
fn luv_xyz(pixel: u32, short: bool) -> [f32; 3] {
    let (light, u, v) = if short {
        let code = (pixel >> 14) & 0x3ff;
        let light = if code == 0 {
            0.0
        } else {
            (std::f64::consts::LN_2 / 64.0 * (f64::from(code) + 0.5)
                - std::f64::consts::LN_2 * 12.0)
                .exp()
        };
        let (u, v) = uv_decode((pixel & 0x3fff) as u16).unwrap_or((0.210526316, 0.473684211));
        (light, u, v)
    } else {
        (
            luminance16((pixel >> 16) as u16),
            f64::from(((pixel >> 8) & 255) as u8) / 410.0 + 0.5 / 410.0,
            f64::from((pixel & 255) as u8) / 410.0 + 0.5 / 410.0,
        )
    };
    if light <= 0.0 {
        return [0.0; 3];
    }
    let scale = 1.0 / (6.0 * u - 16.0 * v + 12.0);
    let x = 9.0 * u * scale;
    let y = 4.0 * v * scale;
    [
        (x / y * light) as f32,
        light as f32,
        ((1.0 - x - y) / y * light) as f32,
    ]
}
fn row_codes(
    source: &[u8],
    position: &mut usize,
    values: &mut [u32],
    planes: usize,
) -> CodecResult<()> {
    values.fill(0);
    for plane in (0..planes).rev() {
        let mut pixel = 0;
        while pixel < values.len() {
            let code = bytes(source, *position, 1)?[0];
            *position += 1;
            if code >= 128 {
                let value = u32::from(bytes(source, *position, 1)?[0]) << (8 * plane);
                *position += 1;
                let count = (usize::from(code) - 126).min(values.len() - pixel);
                for dest in &mut values[pixel..pixel + count] {
                    *dest |= value;
                }
                pixel += count;
            } else {
                let count = usize::from(code).min(values.len() - pixel);
                for &value in bytes(source, *position, count)? {
                    values[pixel] |= u32::from(value) << (8 * plane);
                    pixel += 1;
                }
                *position += count;
            }
        }
    }
    Ok(())
}
/// Returns native gray/RGB bytes, ready for the existing BGRA output converter.
pub(super) fn decode(
    source: &[u8],
    width: usize,
    height: usize,
    photo: u32,
    compression: u32,
) -> CodecResult<Vec<u8>> {
    let gray = photo == 32844;
    if !((gray && compression == 34676)
        || (!gray && photo == 32845 && matches!(compression, 34676 | 34677)))
    {
        return Err(CodecError::Unsupported);
    }
    let short = compression == 34677;
    let components = if gray { 1 } else { 3 };
    let mut output = allocate(
        width
            .checked_mul(height)
            .and_then(|v| v.checked_mul(components))
            .ok_or(CodecError::InvalidData)?,
    )?;
    let mut codes = Vec::new();
    codes
        .try_reserve_exact(width)
        .map_err(|_| CodecError::Allocation)?;
    codes.resize(width, 0);
    let mut position = 0;
    for row in output.chunks_exact_mut(width * components) {
        if short {
            for value in &mut codes {
                let encoded = bytes(source, position, 3)?;
                position += 3;
                *value = (u32::from(encoded[0]) << 16)
                    | (u32::from(encoded[1]) << 8)
                    | u32::from(encoded[2]);
            }
        } else {
            row_codes(source, &mut position, &mut codes, if gray { 2 } else { 4 })?;
        }
        if gray {
            for (dest, &value) in row.iter_mut().zip(&codes) {
                *dest = display(luminance16(value as u16));
            }
        } else {
            for (dest, &value) in row.chunks_exact_mut(3).zip(&codes) {
                dest.copy_from_slice(&xyz_to_rgb(luv_xyz(value, short)));
            }
        }
    }
    Ok(output)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn logarithmic_gray_literal_run_rows_sign_and_truncation() {
        let source = [3, 0, 0x3f, 0x40, 3, 0, 0, 0, 129, 0xc0, 129, 0];
        assert_eq!(
            decode(&source, 3, 2, 32844, 34676).unwrap(),
            [0, 181, 255, 0, 0, 0]
        );
        for end in 0..source.len() {
            assert!(decode(&source[..end], 3, 2, 32844, 34676).is_err());
        }
    }
    #[test]
    fn logarithmic_color_both_storage_formats_and_uv_table_boundaries() {
        // Zero luminance is black regardless of chroma; neutral high luminance
        // clips each channel. Literal byte planes follow the native row order.
        let source = [2, 0, 0x40, 2, 0, 0, 2, 0, 86, 2, 0, 194];
        let pixels = decode(&source, 2, 1, 32845, 34676).unwrap();
        assert_eq!(&pixels[..3], [0; 3]);
        assert!(pixels[3..].iter().all(|v| *v > 250));
        assert_eq!(decode(&[0, 0, 0], 1, 1, 32845, 34677).unwrap(), [0; 3]);
        for (index, row) in uv::UV_ROWS.iter().enumerate() {
            let (u, v) = uv_decode(row.2).unwrap();
            assert!((u - (f64::from(row.0) + 0.5 * f64::from(0.0035f32))).abs() < 1e-9);
            assert!(
                (v - (f64::from(0.01694f32) + (index as f64 + 0.5) * f64::from(0.0035f32))).abs()
                    < 1e-9
            );
        }
        assert!(uv_decode(16289).is_none());
    }
}
