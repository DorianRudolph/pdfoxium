// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/page/cpdf_sampledfunc.{h,cpp}`.

use super::cpdf_function::{CpdfFunctionData, FunctionType};
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fpdfapi::parser::cpdf_stream_acc::CpdfStreamAcc;
use crate::fxcrt::cfx_bitstream::CfxBitStream;

#[derive(Clone, Copy, Debug, Default)]
pub struct SampleEncodeInfo {
    pub encode_max: f32,
    pub encode_min: f32,
    pub sizes: u32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SampleDecodeInfo {
    pub decode_max: f32,
    pub decode_min: f32,
}

pub struct CpdfSampledFunc {
    base: CpdfFunctionData,
    encode_info: Vec<SampleEncodeInfo>,
    decode_info: Vec<SampleDecodeInfo>,
    bits_per_sample: u32,
    sample_max: u32,
    sample_data: Vec<u8>,
}

impl CpdfSampledFunc {
    pub fn load(object: &ObjectRef) -> Option<Self> {
        let base = CpdfFunctionData::init(object, FunctionType::Sampled)?;
        let borrowed = object.borrow();
        let stream = borrowed.as_any().downcast_ref::<CpdfStream>()?;
        let dict = stream.get_dict();
        let dict_borrowed = dict.borrow();
        let dict = dict_borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
        if dict.get_integer_for(b"FunctionType", -1) != 0 {
            return None;
        }
        let size = dict.get_array_for(b"Size")?;
        let size_borrowed = size.borrow();
        let size = size_borrowed.as_any().downcast_ref::<CpdfArray>()?;
        if size.is_empty() {
            return None;
        }
        let bits_per_sample = dict.get_integer_for(b"BitsPerSample", 0) as u32;
        if !matches!(bits_per_sample, 1 | 2 | 4 | 8 | 12 | 16 | 24 | 32) {
            return None;
        }
        let mut total_bits = bits_per_sample.checked_mul(base.output_count())?;
        let encode = dict.get_array_for(b"Encode");
        let mut encode_info = Vec::with_capacity(base.input_count() as usize);
        for i in 0..base.input_count() as usize {
            let count = size.get_integer_at(i);
            if count <= 0 {
                return None;
            }
            let count = count as u32;
            total_bits = total_bits.checked_mul(count)?;
            let (encode_min, encode_max) = if let Some(encode) = &encode {
                let borrowed = encode.borrow();
                let encode = borrowed.as_any().downcast_ref::<CpdfArray>()?;
                (encode.get_float_at(i * 2), encode.get_float_at(i * 2 + 1))
            } else {
                (0.0, if count == 1 { 1.0 } else { (count - 1) as f32 })
            };
            encode_info.push(SampleEncodeInfo {
                encode_max,
                encode_min,
                sizes: count,
            });
        }
        let total_bytes = total_bits / 8 + u32::from(total_bits % 8 != 0);
        if total_bytes == 0 {
            return None;
        }
        let sample_max = u32::MAX >> (32 - bits_per_sample);
        let mut acc = CpdfStreamAcc::new(stream);
        acc.load_all_data_filtered();
        if total_bytes > acc.get_size() {
            return None;
        }
        let sample_data = acc.detach_data();
        let decode = dict.get_array_for(b"Decode");
        let mut decode_info = Vec::with_capacity(base.output_count() as usize);
        for i in 0..base.output_count() as usize {
            let (decode_min, decode_max) = if let Some(decode) = &decode {
                let borrowed = decode.borrow();
                let decode = borrowed.as_any().downcast_ref::<CpdfArray>()?;
                (decode.get_float_at(i * 2), decode.get_float_at(i * 2 + 1))
            } else {
                (base.range(i * 2), base.range(i * 2 + 1))
            };
            decode_info.push(SampleDecodeInfo {
                decode_max,
                decode_min,
            });
        }
        Some(Self {
            base,
            encode_info,
            decode_info,
            bits_per_sample,
            sample_max,
            sample_data,
        })
    }

    pub fn base(&self) -> &CpdfFunctionData {
        &self.base
    }
    pub fn encode_info(&self) -> &[SampleEncodeInfo] {
        &self.encode_info
    }
    pub fn bits_per_sample(&self) -> u32 {
        self.bits_per_sample
    }
    pub fn call(&self, inputs: &[f32], results: &mut [f32]) -> Option<u32> {
        self.base.call(inputs, results, |inputs, results| {
            self.evaluate(inputs, results)
        })
    }

    fn evaluate(&self, inputs: &[f32], results: &mut [f32]) -> bool {
        let mut pos = 0u32;
        let mut encoded_input = vec![0.0f32; inputs.len()];
        let mut index = vec![0u32; inputs.len()];
        let mut blocksize = vec![0u32; inputs.len()];
        for i in 0..inputs.len() {
            blocksize[i] = if i == 0 {
                1
            } else {
                let Some(value) = blocksize[i - 1].checked_mul(self.encode_info[i - 1].sizes)
                else {
                    return false;
                };
                value
            };
            encoded_input[i] = CpdfFunctionData::interpolate(
                inputs[i],
                self.base.domain(i * 2),
                self.base.domain(i * 2 + 1),
                self.encode_info[i].encode_min,
                self.encode_info[i].encode_max,
            );
            index[i] = (encoded_input[i] as u32).min(self.encode_info[i].sizes - 1);
            let Some(next) = index[i]
                .checked_mul(blocksize[i])
                .and_then(|value| pos.checked_add(value))
            else {
                return false;
            };
            pos = next;
        }
        let Some(bits_to_output) = self.base.output_count().checked_mul(self.bits_per_sample)
        else {
            return false;
        };
        if bits_to_output > i32::MAX as u32 {
            return false;
        }
        let Some(bitpos) = pos.checked_mul(bits_to_output) else {
            return false;
        };
        if bitpos > i32::MAX as u32
            || bitpos
                .checked_add(bits_to_output)
                .is_none_or(|value| value > i32::MAX as u32)
        {
            return false;
        }
        if self.sample_data.is_empty() {
            return false;
        }
        let mut bits = CfxBitStream::new(&self.sample_data);
        bits.skip_bits(bitpos as usize);
        for i in 0..self.base.output_count() as usize {
            let sample = bits.get_bits(self.bits_per_sample);
            let mut encoded = sample as f32;
            for j in 0..inputs.len() {
                // A terminal grid point has no neighbor to interpolate with.
                // This also makes a one-point dimension constant, as required
                // by sampled-function interpolation. Pinned PDFium instead
                // multiplies its sole sample by the encoded input here.
                if index[j] < self.encode_info[j].sizes - 1 {
                    let Some(bitpos2) = blocksize[j]
                        .checked_add(pos)
                        .and_then(|value| value.checked_mul(self.base.output_count()))
                        .and_then(|value| value.checked_add(i as u32))
                        .and_then(|value| value.checked_mul(self.bits_per_sample))
                    else {
                        return false;
                    };
                    if bitpos2 > i32::MAX as u32 {
                        return false;
                    }
                    let mut next_bits = CfxBitStream::new(&self.sample_data);
                    next_bits.skip_bits(bitpos2 as usize);
                    let sample2 = next_bits.get_bits(self.bits_per_sample) as f32;
                    encoded += (encoded_input[j] - index[j] as f32) * (sample2 - sample as f32);
                }
            }
            results[i] = CpdfFunctionData::interpolate(
                encoded,
                0.0,
                self.sample_max as f32,
                self.decode_info[i].decode_min,
                self.decode_info[i].decode_max,
            );
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::CpdfSampledFunc;
    use crate::fpdfapi::parser::cpdf_array::CpdfArray;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fpdfapi::parser::cpdf_stream::CpdfStream;

    fn numbers(values: &[i32]) -> CpdfArray {
        let mut array = CpdfArray::new();
        for &value in values {
            array.append_new(CpdfScalarObject::number_from_signed(value));
        }
        array
    }

    #[test]
    fn sampled_function_interpolates_bitstream_samples() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"FunctionType", CpdfScalarObject::number_from_signed(0));
        dict.set_new_for(b"Domain", numbers(&[0, 1]));
        dict.set_new_for(b"Range", numbers(&[0, 1]));
        dict.set_new_for(b"Size", numbers(&[2]));
        dict.set_new_for(b"BitsPerSample", CpdfScalarObject::number_from_signed(8));
        let stream = retain_object(CpdfStream::from_data_and_dict(
            vec![0, 255],
            retain_object(dict),
        ));
        let function = CpdfSampledFunc::load(&stream).unwrap();
        let mut result = [0.0];
        assert_eq!(function.call(&[0.5], &mut result), Some(1));
        assert_eq!(result, [0.5]);
        assert_eq!(function.call(&[1.0], &mut result), Some(1));
        assert_eq!(result, [1.0]);
    }

    #[test]
    fn single_point_sample_grid_preserves_nonzero_constants() {
        for encode in [None, Some([0, 0]), Some([1, 1])] {
            let mut dict = CpdfDictionary::new();
            dict.set_new_for(b"FunctionType", CpdfScalarObject::number_from_signed(0));
            dict.set_new_for(b"Domain", numbers(&[0, 1]));
            dict.set_new_for(b"Range", numbers(&[0, 1, 0, 1, 0, 1]));
            dict.set_new_for(b"Size", numbers(&[1]));
            dict.set_new_for(b"BitsPerSample", CpdfScalarObject::number_from_signed(8));
            if let Some(encode) = encode {
                dict.set_new_for(b"Encode", numbers(&encode));
            }
            let stream = retain_object(CpdfStream::from_data_and_dict(
                vec![17, 129, 250],
                retain_object(dict),
            ));
            let function = CpdfSampledFunc::load(&stream).unwrap();
            for input in [-1.0, 0.0, 0.25, 0.5, 1.0, 2.0] {
                let mut result = [0.0; 3];
                assert_eq!(function.call(&[input], &mut result), Some(3));
                for (actual, sample) in result.into_iter().zip([17.0, 129.0, 250.0]) {
                    assert!((actual - sample / 255.0).abs() < 0.000001);
                }
            }
        }
    }
}
