// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Common function state from `core/fpdfapi/page/cpdf_function.{h,cpp}`.
//! PostScript function loading follows with its interpreter.

use std::collections::HashSet;
use std::rc::Rc;

use super::cpdf_expintfunc::CpdfExpIntFunc;
use super::cpdf_psfunc::CpdfPsFunc;
use super::cpdf_sampledfunc::CpdfSampledFunc;
use super::cpdf_stitchfunc::CpdfStitchFunc;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::{get_dict_object, ObjectRef};
use crate::fpdfapi::parser::fpdf_parser_utility::read_array_elements_to_vector;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum FunctionType {
    Invalid = -1,
    Sampled = 0,
    ExponentialInterpolation = 2,
    Stitching = 3,
    PostScript = 4,
}

impl FunctionType {
    pub fn from_integer(value: i32) -> Self {
        match value {
            0 => Self::Sampled,
            2 => Self::ExponentialInterpolation,
            3 => Self::Stitching,
            4 => Self::PostScript,
            _ => Self::Invalid,
        }
    }
}

pub struct CpdfFunctionData {
    function_type: FunctionType,
    inputs: u32,
    outputs: u32,
    domains: Vec<f32>,
    ranges: Vec<f32>,
}

impl CpdfFunctionData {
    pub fn init(object: &ObjectRef, function_type: FunctionType) -> Option<Self> {
        let dict = get_dict_object(object)?;
        let borrowed = dict.borrow();
        let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
        let domain = dict.get_array_for(b"Domain")?;
        let borrowed = domain.borrow();
        let domain = borrowed.as_any().downcast_ref::<CpdfArray>()?;
        let inputs = u32::try_from(domain.size() / 2).ok()?;
        if inputs == 0 {
            return None;
        }
        let domains = read_array_elements_to_vector(domain, inputs as usize * 2);
        let (outputs, ranges) = if let Some(range) = dict.get_array_for(b"Range") {
            let borrowed = range.borrow();
            let range = borrowed.as_any().downcast_ref::<CpdfArray>()?;
            let outputs = u32::try_from(range.size() / 2).ok()?;
            (
                outputs,
                read_array_elements_to_vector(range, outputs as usize * 2),
            )
        } else {
            (0, Vec::new())
        };
        if matches!(
            function_type,
            FunctionType::Sampled | FunctionType::PostScript
        ) && outputs == 0
        {
            return None;
        }
        Some(Self {
            function_type,
            inputs,
            outputs,
            domains,
            ranges,
        })
    }

    pub fn function_type(&self) -> FunctionType {
        self.function_type
    }
    pub fn input_count(&self) -> u32 {
        self.inputs
    }
    pub fn output_count(&self) -> u32 {
        self.outputs
    }
    pub fn set_output_count(&mut self, count: u32) {
        self.outputs = count;
    }
    pub fn domain(&self, index: usize) -> f32 {
        self.domains[index]
    }
    pub fn range(&self, index: usize) -> f32 {
        self.ranges[index]
    }
    pub fn resize_ranges_for_outputs(&mut self) {
        if !self.ranges.is_empty() {
            self.ranges.resize(self.outputs as usize * 2, 0.0);
        }
    }

    pub fn call(
        &self,
        inputs: &[f32],
        results: &mut [f32],
        evaluate: impl FnOnce(&[f32], &mut [f32]) -> bool,
    ) -> Option<u32> {
        if inputs.len() != self.inputs as usize || results.len() < self.outputs as usize {
            return None;
        }
        let mut clamped = Vec::with_capacity(inputs.len());
        for (i, &input) in inputs.iter().enumerate() {
            let min = self.domains[i * 2];
            let max = self.domains[i * 2 + 1];
            if min > max {
                return None;
            }
            clamped.push(clamp_like_cpp(input, min, max));
        }
        if !evaluate(&clamped, results) {
            return None;
        }
        if self.ranges.is_empty() {
            return Some(self.outputs);
        }
        for (i, result) in results.iter_mut().take(self.outputs as usize).enumerate() {
            let min = self.ranges[i * 2];
            let max = self.ranges[i * 2 + 1];
            if min > max {
                return None;
            }
            *result = clamp_like_cpp(*result, min, max);
        }
        Some(self.outputs)
    }

    pub fn interpolate(x: f32, xmin: f32, xmax: f32, ymin: f32, ymax: f32) -> f32 {
        let divisor = xmax - xmin;
        ymin + if divisor != 0.0 {
            (x - xmin) * (ymax - ymin) / divisor
        } else {
            0.0
        }
    }
}

fn clamp_like_cpp(value: f32, min: f32, max: f32) -> f32 {
    if value < min {
        min
    } else if max < value {
        max
    } else {
        value
    }
}

pub enum CpdfFunction {
    Sampled(CpdfSampledFunc),
    Exponential(CpdfExpIntFunc),
    Stitching(CpdfStitchFunc),
    PostScript(CpdfPsFunc),
}

impl CpdfFunction {
    pub fn load(object: &ObjectRef) -> Option<Self> {
        Self::load_with_visited(object, &mut HashSet::new())
    }

    pub(crate) fn load_with_visited(
        object: &ObjectRef,
        visited: &mut HashSet<usize>,
    ) -> Option<Self> {
        let identity = Rc::as_ptr(object) as *const () as usize;
        if !visited.insert(identity) {
            return None;
        }
        let result = (|| {
            let dict = get_dict_object(object)?;
            let borrowed = dict.borrow();
            let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
            let function_type =
                FunctionType::from_integer(dict.get_integer_for(b"FunctionType", -1));
            drop(borrowed);
            match function_type {
                FunctionType::Sampled => Some(Self::Sampled(CpdfSampledFunc::load(object)?)),
                FunctionType::ExponentialInterpolation => {
                    Some(Self::Exponential(CpdfExpIntFunc::load(object)?))
                }
                FunctionType::Stitching => {
                    Some(Self::Stitching(CpdfStitchFunc::load(object, visited)?))
                }
                FunctionType::PostScript => Some(Self::PostScript(CpdfPsFunc::load(object)?)),
                FunctionType::Invalid => None,
            }
        })();
        visited.remove(&identity);
        result
    }

    pub fn base(&self) -> &CpdfFunctionData {
        match self {
            Self::Sampled(value) => value.base(),
            Self::Exponential(value) => value.base(),
            Self::Stitching(value) => value.base(),
            Self::PostScript(value) => value.base(),
        }
    }
    pub fn input_count(&self) -> u32 {
        self.base().input_count()
    }
    pub fn output_count(&self) -> u32 {
        self.base().output_count()
    }
    pub fn call(&self, inputs: &[f32], results: &mut [f32]) -> Option<u32> {
        match self {
            Self::Sampled(value) => value.call(inputs, results),
            Self::Exponential(value) => value.call(inputs, results),
            Self::Stitching(value) => value.call(inputs, results),
            Self::PostScript(value) => value.call(inputs, results),
        }
    }
}

#[cfg(test)]
#[path = "cpdf_function_unittest.rs"]
mod upstream_tests;
