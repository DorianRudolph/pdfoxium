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

//! Immutable source GF(256) exponent/log tables. Static fields replace native
//! initialization/destruction; zero polynomials are values with no owner cycle.
use super::bc_reed_solomon_gf256_poly::CbcReedSolomonGf256Poly;

#[derive(Debug)]
pub struct CbcReedSolomonGf256 {
    primitive: i32,
    exp_table: [i32; 256],
    log_table: [i32; 256],
}
pub static QR_CODE_FIELD: CbcReedSolomonGf256 = CbcReedSolomonGf256::new(0x11d);
pub static DATA_MATRIX_FIELD: CbcReedSolomonGf256 = CbcReedSolomonGf256::new(0x12d);
impl CbcReedSolomonGf256 {
    pub const fn new(primitive: i32) -> Self {
        assert!(primitive >= 0x100 && primitive < 0x200);
        let mut exp_table = [0; 256];
        let mut log_table = [0; 256];
        let mut x = 1;
        let mut j = 0;
        while j < 256 {
            exp_table[j] = x;
            x <<= 1;
            if x >= 256 {
                x ^= primitive;
            }
            j += 1;
        }
        let mut i = 0;
        while i < 255 {
            log_table[exp_table[i] as usize] = i as i32;
            i += 1;
        }
        Self {
            primitive,
            exp_table,
            log_table,
        }
    }
    pub fn get_zero(&self) -> CbcReedSolomonGf256Poly<'_> {
        CbcReedSolomonGf256Poly::new(self, &[0]).unwrap()
    }
    pub fn build_monomial(
        &self,
        degree: i32,
        coefficient: i32,
    ) -> Option<CbcReedSolomonGf256Poly<'_>> {
        if degree < 0 || !(0..256).contains(&coefficient) {
            return None;
        }
        if coefficient == 0 {
            return Some(self.get_zero());
        }
        let length = (degree as usize).checked_add(1)?;
        if length > i32::MAX as usize {
            return None;
        }
        let mut coefficients = Vec::new();
        coefficients.try_reserve_exact(length).ok()?;
        coefficients.resize(length, 0);
        coefficients[0] = coefficient;
        CbcReedSolomonGf256Poly::from_owned(self, coefficients)
    }
    pub const fn add_or_subtract(a: i32, b: i32) -> i32 {
        a ^ b
    }
    pub fn exp(&self, exponent: usize) -> i32 {
        self.exp_table[exponent]
    }
    pub fn inverse(&self, a: i32) -> Option<i32> {
        if !(1..256).contains(&a) {
            return None;
        }
        Some(self.exp_table[255 - self.log_table[a as usize] as usize])
    }
    pub fn multiply(&self, a: i32, b: i32) -> i32 {
        assert!((0..256).contains(&a) && (0..256).contains(&b));
        if a == 0 || b == 0 {
            return 0;
        }
        if a == 1 {
            return b;
        }
        if b == 1 {
            return a;
        }
        self.exp_table[((self.log_table[a as usize] + self.log_table[b as usize]) % 255) as usize]
    }
    pub(super) fn same_field(&self, other: &Self) -> bool {
        self.primitive == other.primitive
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn both_fields_have_complete_power_cycles_and_nonzero_inverses() {
        for field in [&QR_CODE_FIELD, &DATA_MATRIX_FIELD] {
            let mut seen = [false; 256];
            for exponent in 0..255 {
                let value = field.exp(exponent);
                assert_ne!(value, 0);
                assert!(!seen[value as usize]);
                seen[value as usize] = true;
                assert_eq!(field.multiply(value, field.inverse(value).unwrap()), 1);
            }
            assert_eq!(field.exp(0), 1);
            assert_eq!(field.exp(255), 1);
            assert_eq!(field.inverse(0), None);
            assert_eq!(field.multiply(0, 91), 0);
            assert_eq!(field.multiply(1, 91), 91);
        }
        assert_eq!(QR_CODE_FIELD.exp(8), 29);
        assert_eq!(DATA_MATRIX_FIELD.exp(8), 45);
    }
}
