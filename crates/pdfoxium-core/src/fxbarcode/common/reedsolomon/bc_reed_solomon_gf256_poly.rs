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

//! Direct `BC_ReedSolomonGF256Poly.{h,cpp}`. Coefficients run highest degree
//! first. A borrowed immutable field replaces the native non-owning pointer.
use super::bc_reed_solomon_gf256::CbcReedSolomonGf256;

#[derive(Clone, Debug)]
pub struct CbcReedSolomonGf256Poly<'a> {
    field: &'a CbcReedSolomonGf256,
    coefficients: Vec<i32>,
}
impl<'a> CbcReedSolomonGf256Poly<'a> {
    pub fn new(field: &'a CbcReedSolomonGf256, coefficients: &[i32]) -> Option<Self> {
        let mut owned = Vec::new();
        owned.try_reserve_exact(coefficients.len()).ok()?;
        owned.extend_from_slice(coefficients);
        Self::from_owned(field, owned)
    }
    pub(super) fn from_owned(
        field: &'a CbcReedSolomonGf256,
        mut coefficients: Vec<i32>,
    ) -> Option<Self> {
        if coefficients.is_empty()
            || coefficients.len() > i32::MAX as usize
            || coefficients.iter().any(|value| !(0..256).contains(value))
        {
            return None;
        }
        let first = coefficients
            .iter()
            .position(|&coefficient| coefficient != 0)
            .unwrap_or(coefficients.len() - 1);
        coefficients.drain(..first);
        Some(Self {
            field,
            coefficients,
        })
    }
    pub fn get_coefficients(&self) -> &[i32] {
        &self.coefficients
    }
    pub fn get_coefficient(&self, degree: i32) -> Option<i32> {
        let degree = usize::try_from(degree).ok()?;
        self.coefficients
            .get(
                self.coefficients
                    .len()
                    .checked_sub(degree.checked_add(1)?)?,
            )
            .copied()
    }
    pub fn get_degree(&self) -> i32 {
        self.coefficients.len() as i32 - 1
    }
    pub fn is_zero(&self) -> bool {
        self.coefficients[0] == 0
    }
    pub fn add_or_subtract(&self, other: &Self) -> Option<Self> {
        if !self.field.same_field(other.field) {
            return None;
        }
        if self.is_zero() {
            return Some(other.clone());
        }
        if other.is_zero() {
            return Some(self.clone());
        }
        let (smaller, larger) = if self.coefficients.len() <= other.coefficients.len() {
            (&self.coefficients, &other.coefficients)
        } else {
            (&other.coefficients, &self.coefficients)
        };
        let length_diff = larger.len() - smaller.len();
        let mut sum_diff = Vec::new();
        sum_diff.try_reserve_exact(larger.len()).ok()?;
        sum_diff.extend_from_slice(&larger[..length_diff]);
        for (a, b) in smaller.iter().zip(&larger[length_diff..]) {
            sum_diff.push(CbcReedSolomonGf256::add_or_subtract(*a, *b));
        }
        Self::from_owned(self.field, sum_diff)
    }
    pub fn multiply(&self, other: &Self) -> Option<Self> {
        if !self.field.same_field(other.field) {
            return None;
        }
        if self.is_zero() || other.is_zero() {
            return Some(self.field.get_zero());
        }
        let length = self
            .coefficients
            .len()
            .checked_add(other.coefficients.len())?
            .checked_sub(1)?;
        if length > i32::MAX as usize {
            return None;
        }
        let mut product = Vec::new();
        product.try_reserve_exact(length).ok()?;
        product.resize(length, 0);
        for (i, &a) in self.coefficients.iter().enumerate() {
            for (j, &b) in other.coefficients.iter().enumerate() {
                product[i + j] ^= self.field.multiply(a, b);
            }
        }
        Self::from_owned(self.field, product)
    }
    pub fn multiply_by_monomial(&self, degree: i32, coefficient: i32) -> Option<Self> {
        if degree < 0 || !(0..256).contains(&coefficient) {
            return None;
        }
        if coefficient == 0 {
            return Some(self.field.get_zero());
        }
        let length = self.coefficients.len().checked_add(degree as usize)?;
        if length > i32::MAX as usize {
            return None;
        }
        let mut product = Vec::new();
        product.try_reserve_exact(length).ok()?;
        product.resize(length, 0);
        for (destination, &source) in product.iter_mut().zip(&self.coefficients) {
            *destination = self.field.multiply(source, coefficient);
        }
        Self::from_owned(self.field, product)
    }
    /// The native Divide() returns only the remainder. Keep its leading-term
    /// cancellation loop; its unreturned quotient needs no allocation.
    pub fn divide(&self, other: &Self) -> Option<Self> {
        if !self.field.same_field(other.field) || other.is_zero() {
            return None;
        }
        let mut remainder = self.clone();
        let inverse = self.field.inverse(other.coefficients[0])?;
        while remainder.get_degree() >= other.get_degree() && !remainder.is_zero() {
            let old_degree = remainder.get_degree();
            let degree = old_degree - other.get_degree();
            let scale = self.field.multiply(remainder.coefficients[0], inverse);
            let term = other.multiply_by_monomial(degree, scale)?;
            remainder = remainder.add_or_subtract(&term)?;
            if !remainder.is_zero() && remainder.get_degree() >= old_degree {
                return None;
            }
        }
        Some(remainder)
    }
}
#[cfg(test)]
mod tests {
    use super::super::bc_reed_solomon_gf256::{DATA_MATRIX_FIELD, QR_CODE_FIELD};
    use super::*;
    #[test]
    fn canonical_coefficients_polynomial_products_and_remainders() {
        let field = &QR_CODE_FIELD;
        assert_eq!(
            CbcReedSolomonGf256Poly::new(field, &[0, 0, 0])
                .unwrap()
                .get_coefficients(),
            [0]
        );
        let x_plus_one = CbcReedSolomonGf256Poly::new(field, &[0, 1, 1]).unwrap();
        let x_plus_two = CbcReedSolomonGf256Poly::new(field, &[1, 2]).unwrap();
        let product = x_plus_one.multiply(&x_plus_two).unwrap();
        assert_eq!(product.get_coefficients(), [1, 3, 2]);
        assert_eq!(product.get_degree(), 2);
        assert_eq!(product.get_coefficient(0), Some(2));
        assert_eq!(product.get_coefficient(3), None);
        assert!(product.divide(&x_plus_one).unwrap().is_zero());
        let dividend = CbcReedSolomonGf256Poly::new(field, &[1, 2, 3, 0, 0]).unwrap();
        assert_eq!(
            dividend.divide(&product).unwrap().get_coefficients(),
            [4, 4]
        );
        assert!(dividend.divide(&field.get_zero()).is_none());
        assert!(field.build_monomial(-1, 1).is_none());
        assert!(field.build_monomial(2, 256).is_none());
        assert_eq!(
            field.build_monomial(2, 3).unwrap().get_coefficients(),
            [3, 0, 0]
        );
        assert!(CbcReedSolomonGf256Poly::new(field, &[]).is_none());
        assert!(CbcReedSolomonGf256Poly::new(field, &[-1]).is_none());
        assert!(product.multiply(&DATA_MATRIX_FIELD.get_zero()).is_none());
    }
}
