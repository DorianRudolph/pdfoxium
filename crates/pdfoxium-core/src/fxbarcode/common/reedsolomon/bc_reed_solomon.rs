// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
// Original code is licensed as follows:
/*
 * Copyright 2007 ZXing authors
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

//! Direct `BC_ReedSolomon.{h,cpp}` cached generator construction and systematic
//! parity encoding. Source data coefficients are checked before field indexing.
use super::{
    bc_reed_solomon_gf256::CbcReedSolomonGf256, bc_reed_solomon_gf256_poly::CbcReedSolomonGf256Poly,
};

pub struct CbcReedSolomonEncoder<'a> {
    field: &'a CbcReedSolomonGf256,
    cached_generators: Vec<CbcReedSolomonGf256Poly<'a>>,
}
impl<'a> CbcReedSolomonEncoder<'a> {
    pub fn new(field: &'a CbcReedSolomonGf256) -> Self {
        Self {
            field,
            cached_generators: vec![CbcReedSolomonGf256Poly::new(field, &[1]).unwrap()],
        }
    }
    fn build_generator(&mut self, degree: usize) -> Option<&CbcReedSolomonGf256Poly<'a>> {
        if degree > 255 {
            return None;
        }
        if degree >= self.cached_generators.len() {
            self.cached_generators
                .try_reserve(degree + 1 - self.cached_generators.len())
                .ok()?;
            for d in self.cached_generators.len()..=degree {
                let temp = CbcReedSolomonGf256Poly::new(self.field, &[1, self.field.exp(d - 1)])?;
                let next = self.cached_generators.last()?.multiply(&temp)?;
                self.cached_generators.push(next);
            }
        }
        self.cached_generators.get(degree)
    }
    pub fn encode(&mut self, to_encode: &mut [i32], ec_bytes: usize) -> bool {
        if ec_bytes == 0 || ec_bytes > 255 || to_encode.len() <= ec_bytes {
            return false;
        }
        let data_bytes = to_encode.len() - ec_bytes;
        // Existing parity slots are overwritten; only data values enter GF256.
        if to_encode[..data_bytes]
            .iter()
            .any(|value| !(0..256).contains(value))
        {
            return false;
        }
        let field = self.field;
        let Some(generator) = self.build_generator(ec_bytes) else {
            return false;
        };
        let Some(info) = CbcReedSolomonGf256Poly::new(field, &to_encode[..data_bytes]) else {
            return false;
        };
        let Some(info) = info.multiply_by_monomial(ec_bytes as i32, 1) else {
            return false;
        };
        let Some(remainder) = info.divide(generator) else {
            return false;
        };
        let coefficients = remainder.get_coefficients();
        let Some(zero_coefficients) = ec_bytes.checked_sub(coefficients.len()) else {
            return false;
        };
        to_encode[data_bytes..data_bytes + zero_coefficients].fill(0);
        to_encode[data_bytes + zero_coefficients..].copy_from_slice(coefficients);
        true
    }
}
#[cfg(test)]
mod tests {
    use super::super::bc_reed_solomon_gf256::QR_CODE_FIELD;
    use super::*;
    #[test]
    fn parity_vectors_zero_syndromes_cached_generators_and_invalid_inputs() {
        let mut encoder = CbcReedSolomonEncoder::new(&QR_CODE_FIELD);
        let mut simple = [1, 2, 3, 999, 999];
        assert!(encoder.encode(&mut simple, 2));
        assert_eq!(simple, [1, 2, 3, 4, 4]);
        for parity in [7, 2, 10] {
            let data = [32, 91, 11, 120, 209, 114, 220, 77, 67, 64];
            let mut encoded = data.to_vec();
            encoded.resize(data.len() + parity, 0);
            assert!(encoder.encode(&mut encoded, parity));
            assert_eq!(&encoded[..data.len()], data);
            // An independent Horner evaluation verifies that all generator
            // roots are zeros of the encoded message, including cached degrees.
            for exponent in 0..parity {
                let x = QR_CODE_FIELD.exp(exponent);
                let syndrome = encoded
                    .iter()
                    .fold(0, |sum, &value| QR_CODE_FIELD.multiply(sum, x) ^ value);
                assert_eq!(syndrome, 0);
            }
        }
        for (mut values, parity) in [
            (vec![1, 2], 0),
            (vec![1, 2], 2),
            (vec![-1, 0], 1),
            (vec![256, 0], 1),
            (vec![1; 257], 256),
        ] {
            let previous = values.clone();
            assert!(!encoder.encode(&mut values, parity));
            assert_eq!(values, previous);
        }
    }
}
