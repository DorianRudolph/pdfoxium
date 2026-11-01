// Adapted from libtiff as bundled by PDFium for pdfoxium.
// Distributed under the original license notice below.
//
/*
 * Copyright (c) 1996-1997 Sam Leffler
 * Copyright (c) 1996 Pixar
 *
 * Permission to use, copy, modify, distribute, and sell this software and
 * its documentation for any purpose is hereby granted without fee, provided
 * that (i) the above copyright notices and this permission notice appear in
 * all copies of the software and related documentation, and (ii) the names of
 * Pixar, Sam Leffler and Silicon Graphics may not be used in any advertising or
 * publicity relating to the software without the specific, prior written
 * permission of Pixar, Sam Leffler and Silicon Graphics.
 *
 * THE SOFTWARE IS PROVIDED "AS-IS" AND WITHOUT WARRANTY OF ANY KIND,
 * EXPRESS, IMPLIED OR OTHERWISE, INCLUDING WITHOUT LIMITATION, ANY
 * WARRANTY OF MERCHANTABILITY OR FITNESS FOR A PARTICULAR PURPOSE.
 *
 * IN NO EVENT SHALL PIXAR, SAM LEFFLER OR SILICON GRAPHICS BE LIABLE FOR
 * ANY SPECIAL, INCIDENTAL, INDIRECT OR CONSEQUENTIAL DAMAGES OF ANY KIND,
 * OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
 * WHETHER OR NOT ADVISED OF THE POSSIBILITY OF DAMAGE, AND ON ANY THEORY OF
 * LIABILITY, ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE
 * OF THIS SOFTWARE.
 */

//! Decode-only PixarLog: existing Rust zlib decompression, native 11-bit
//! horizontal accumulation and the pinned linear display sample tables.
use super::super::progressive_decoder_context::{allocate, CodecError, CodecResult};
pub(super) enum Pixels {
    Bytes(Vec<u8>),
    Words(Vec<u16>),
}
fn linear_table() -> &'static [f32] {
    static TABLE: std::sync::OnceLock<Vec<f32>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let nlinear = (1.0 / 1.004f64.ln()) as usize;
        let c = 1.0 / nlinear as f64;
        let b = (-c * 1250.0).exp();
        let step = b * c * 1.0f64.exp();
        (0..2048)
            .map(|i| {
                if i < nlinear {
                    (i as f64 * step) as f32
                } else {
                    (b * (c * i as f64).exp()) as f32
                }
            })
            .collect()
    })
}

pub(super) fn decode(
    data: &[u8],
    width: usize,
    height: usize,
    channels: usize,
    bits: u8,
    little: bool,
) -> CodecResult<Pixels> {
    if channels == 0 || !matches!(bits, 8 | 16) {
        return Err(CodecError::Unsupported);
    }
    let count = width
        .checked_mul(height)
        .and_then(|n| n.checked_mul(channels))
        .ok_or(CodecError::InvalidData)?;
    let mut encoded = allocate(count.checked_mul(2).ok_or(CodecError::InvalidData)?)?;
    let (decoded, status) =
        zlib_rs::decompress_slice(&mut encoded, data, zlib_rs::InflateConfig::default());
    if status != zlib_rs::ReturnCode::Ok || decoded.len() != count * 2 {
        return Err(CodecError::InvalidData);
    }
    let mut words = Vec::new();
    words
        .try_reserve_exact(count)
        .map_err(|_| CodecError::Allocation)?;
    words.extend(decoded.chunks_exact(2).map(|v| {
        if little {
            u16::from_le_bytes([v[0], v[1]])
        } else {
            u16::from_be_bytes([v[0], v[1]])
        }
    }));
    let row_samples = width * channels;
    for row in words.chunks_exact_mut(row_samples) {
        for sample in channels..row.len() {
            row[sample] = row[sample].wrapping_add(row[sample - channels]);
        }
    }
    let linear = linear_table();
    if bits == 8 {
        Ok(Pixels::Bytes(
            words
                .into_iter()
                .map(|value| {
                    (f64::from(linear[(value & 2047) as usize]) * 255.0 + 0.5).min(255.0) as u8
                })
                .collect(),
        ))
    } else {
        for value in &mut words {
            *value =
                (f64::from(linear[(*value & 2047) as usize]) * 65535.0 + 0.5).min(65535.0) as u16;
        }
        Ok(Pixels::Words(words))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn compressed(words: &[u16], little: bool) -> Vec<u8> {
        let data = words
            .iter()
            .flat_map(|v| {
                if little {
                    v.to_le_bytes()
                } else {
                    v.to_be_bytes()
                }
            })
            .collect::<Vec<_>>();
        let mut output = vec![0; 1024];
        let (result, status) =
            zlib_rs::compress_slice(&mut output, &data, zlib_rs::DeflateConfig::default());
        assert_eq!(status, zlib_rs::ReturnCode::Ok);
        result.to_vec()
    }
    #[test]
    fn both_sample_depths_byte_orders_and_horizontal_wrap() {
        for little in [true, false] {
            // First RGB pixel: 0,1000,1250. Second is encoded as horizontal
            // differences, including a negative green delta in an unsigned word.
            let bytes = compressed(&[0, 1000, 1250, 1250, 0u16.wrapping_sub(1000), 0], little);
            let Pixels::Bytes(decoded) = decode(&bytes, 2, 1, 3, 8, little).unwrap() else {
                panic!()
            };
            assert_eq!(decoded, [0, 94, 255, 255, 0, 255]);
            let Pixels::Words(decoded) = decode(&bytes, 2, 1, 3, 16, little).unwrap() else {
                panic!()
            };
            assert_eq!(decoded[0], 0);
            assert_eq!(decoded[2], 65535);
            assert_eq!(decoded[3], 65535);
            assert_eq!(decoded[4], 0);
            assert!((24000..24200).contains(&decoded[1]));
        }
    }
    #[test]
    fn predictor_resets_at_each_row_and_truncated_deflate_is_rejected() {
        let bytes = compressed(&[1250, 0, 0, 1250], true);
        let Pixels::Bytes(decoded) = decode(&bytes, 2, 2, 1, 8, true).unwrap() else {
            panic!()
        };
        assert_eq!(decoded, [255, 255, 0, 255]);
        for end in 0..bytes.len() {
            assert!(decode(&bytes[..end], 2, 2, 1, 8, true).is_err());
        }
        assert!(decode(&bytes, 2, 2, 1, 32, true).is_err());
    }
}
