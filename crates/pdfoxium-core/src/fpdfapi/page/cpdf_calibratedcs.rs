// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! CalGray, CalRGB, Lab and XYZ helpers from `core/fpdfapi/page/cpdf_colorspace.cpp`.

use super::cpdf_colorspace::{ColorSpaceFamily, CpdfColorSpace, CpdfColorSpaceData};
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fxge::dib::fx_dib::FxRgbStruct;

const SRGB_SAMPLES_1: [u8; 192] = [
    0, 3, 6, 10, 13, 15, 18, 20, 22, 23, 25, 27, 28, 30, 31, 32, 34, 35, 36, 37, 38, 39, 40, 41,
    42, 43, 44, 45, 46, 47, 48, 49, 49, 50, 51, 52, 53, 53, 54, 55, 56, 56, 57, 58, 58, 59, 60, 61,
    61, 62, 62, 63, 64, 64, 65, 66, 66, 67, 67, 68, 68, 69, 70, 70, 71, 71, 72, 72, 73, 73, 74, 74,
    75, 76, 76, 77, 77, 78, 78, 79, 79, 79, 80, 80, 81, 81, 82, 82, 83, 83, 84, 84, 85, 85, 85, 86,
    86, 87, 87, 88, 88, 88, 89, 89, 90, 90, 91, 91, 91, 92, 92, 93, 93, 93, 94, 94, 95, 95, 95, 96,
    96, 97, 97, 97, 98, 98, 98, 99, 99, 99, 100, 100, 101, 101, 101, 102, 102, 102, 103, 103, 103,
    104, 104, 104, 105, 105, 106, 106, 106, 107, 107, 107, 108, 108, 108, 109, 109, 109, 110, 110,
    110, 110, 111, 111, 111, 112, 112, 112, 113, 113, 113, 114, 114, 114, 115, 115, 115, 115, 116,
    116, 116, 117, 117, 117, 118, 118, 118, 118, 119, 119, 119, 120,
];

const SRGB_SAMPLES_2: [u8; 208] = [
    120, 121, 122, 124, 125, 126, 127, 128, 129, 130, 131, 132, 133, 134, 135, 136, 137, 138, 139,
    140, 141, 142, 143, 144, 145, 146, 147, 148, 148, 149, 150, 151, 152, 153, 154, 155, 155, 156,
    157, 158, 159, 159, 160, 161, 162, 163, 163, 164, 165, 166, 167, 167, 168, 169, 170, 170, 171,
    172, 173, 173, 174, 175, 175, 176, 177, 178, 178, 179, 180, 180, 181, 182, 182, 183, 184, 185,
    185, 186, 187, 187, 188, 189, 189, 190, 190, 191, 192, 192, 193, 194, 194, 195, 196, 196, 197,
    197, 198, 199, 199, 200, 200, 201, 202, 202, 203, 203, 204, 205, 205, 206, 206, 207, 208, 208,
    209, 209, 210, 210, 211, 212, 212, 213, 213, 214, 214, 215, 215, 216, 216, 217, 218, 218, 219,
    219, 220, 220, 221, 221, 222, 222, 223, 223, 224, 224, 225, 226, 226, 227, 227, 228, 228, 229,
    229, 230, 230, 231, 231, 232, 232, 233, 233, 234, 234, 235, 235, 236, 236, 237, 237, 238, 238,
    238, 239, 239, 240, 240, 241, 241, 242, 242, 243, 243, 244, 244, 245, 245, 246, 246, 246, 247,
    247, 248, 248, 249, 249, 250, 250, 251, 251, 251, 252, 252, 253, 253, 254, 254, 255, 255,
];

fn read_point(dict: &CpdfDictionary, key: &[u8]) -> Option<[f32; 3]> {
    let object = dict.get_array_for(key)?;
    let borrowed = object.borrow();
    let array = borrowed.as_any().downcast_ref::<CpdfArray>()?;
    (array.size() == 3).then(|| {
        [
            array.get_float_at(0),
            array.get_float_at(1),
            array.get_float_at(2),
        ]
    })
}

fn get_white_point(dict: &CpdfDictionary) -> Option<[f32; 3]> {
    let point = read_point(dict, b"WhitePoint")?;
    (point[0] > 0.0 && point[1] == 1.0 && point[2] > 0.0).then_some(point)
}

fn get_black_point(dict: &CpdfDictionary) -> [f32; 3] {
    let Some(object) = dict.get_array_for(b"BlackPoint") else {
        return [0.0; 3];
    };
    let borrowed = object.borrow();
    let Some(array) = borrowed.as_any().downcast_ref::<CpdfArray>() else {
        return [0.0; 3];
    };
    if array.size() != 3 {
        return [0.0; 3];
    }
    let mut point = [0.0; 3];
    for (index, value) in point.iter_mut().enumerate() {
        *value = array.get_float_at(index);
        if *value < 0.0 {
            return [0.0; 3];
        }
    }
    point
}

fn dict_at_one(array: &CpdfArray) -> Option<crate::fpdfapi::parser::cpdf_object::ObjectRef> {
    array.get_dict_at(1)
}

#[derive(Clone, Copy)]
struct Matrix3([f32; 9]);

impl Matrix3 {
    fn inverse(self) -> Self {
        let [a, b, c, d, e, f, g, h, i] = self.0;
        let det = a * (e * i - f * h) - b * (i * d - f * g) + c * (d * h - e * g);
        if det.abs() < f32::EPSILON {
            return Self([0.0; 9]);
        }
        Self([
            (e * i - f * h) / det,
            -(b * i - c * h) / det,
            (b * f - c * e) / det,
            -(d * i - f * g) / det,
            (a * i - c * g) / det,
            -(a * f - c * d) / det,
            (d * h - e * g) / det,
            -(a * h - b * g) / det,
            (a * e - b * d) / det,
        ])
    }

    fn multiply(self, rhs: Self) -> Self {
        let [a, b, c, d, e, f, g, h, i] = self.0;
        let [ma, mb, mc, md, me, mf, mg, mh, mi] = rhs.0;
        Self([
            a * ma + b * md + c * mg,
            a * mb + b * me + c * mh,
            a * mc + b * mf + c * mi,
            d * ma + e * md + f * mg,
            d * mb + e * me + f * mh,
            d * mc + e * mf + f * mi,
            g * ma + h * md + i * mg,
            g * mb + h * me + i * mh,
            g * mc + h * mf + i * mi,
        ])
    }

    fn transform_vector(self, vector: [f32; 3]) -> [f32; 3] {
        let [a, b, c, d, e, f, g, h, i] = self.0;
        let [x, y, z] = vector;
        [
            a * x + b * y + c * z,
            d * x + e * y + f * z,
            g * x + h * y + i * z,
        ]
    }
}

fn rgb_conversion(component: f32) -> f32 {
    let scale = ((component.clamp(0.0, 1.0) * 1023.0) as i32).max(0) as usize;
    if scale < 192 {
        f32::from(SRGB_SAMPLES_1[scale]) / 255.0
    } else {
        f32::from(SRGB_SAMPLES_2[scale / 4 - 48]) / 255.0
    }
}

fn xyz_to_srgb(x: f32, y: f32, z: f32) -> FxRgbStruct<f32> {
    let r = 3.2410 * x - 1.5374 * y - 0.4986 * z;
    let g = -0.9692 * x + 1.8760 * y + 0.0416 * z;
    let b = 0.0556 * x - 0.2040 * y + 1.0570 * z;
    FxRgbStruct {
        red: rgb_conversion(r),
        green: rgb_conversion(g),
        blue: rgb_conversion(b),
    }
}

fn xyz_to_srgb_white_point(x: f32, y: f32, z: f32, white: [f32; 3]) -> FxRgbStruct<f32> {
    const RX: f32 = 0.64;
    const RY: f32 = 0.33;
    const GX: f32 = 0.30;
    const GY: f32 = 0.60;
    const BX: f32 = 0.15;
    const BY: f32 = 0.06;
    let rgb_xyz = Matrix3([
        RX,
        GX,
        BX,
        RY,
        GY,
        BY,
        1.0 - RX - RY,
        1.0 - GX - GY,
        1.0 - BX - BY,
    ]);
    let sum = rgb_xyz.inverse().transform_vector(white);
    let diagonal = Matrix3([sum[0], 0.0, 0.0, 0.0, sum[1], 0.0, 0.0, 0.0, sum[2]]);
    let matrix = rgb_xyz.multiply(diagonal);
    let rgb = matrix.inverse().transform_vector([x, y, z]);
    FxRgbStruct {
        red: rgb_conversion(rgb[0]),
        green: rgb_conversion(rgb[1]),
        blue: rgb_conversion(rgb[2]),
    }
}

pub struct CpdfCalGray {
    data: CpdfColorSpaceData,
    gamma: f32,
    white_point: [f32; 3],
    black_point: [f32; 3],
}

impl Default for CpdfCalGray {
    fn default() -> Self {
        Self {
            data: CpdfColorSpaceData::new(ColorSpaceFamily::CalGray, 0),
            gamma: 1.0,
            white_point: [1.0; 3],
            black_point: [0.0; 3],
        }
    }
}

impl CpdfCalGray {
    pub fn load(array: &CpdfArray) -> Option<Self> {
        let dict = dict_at_one(array)?;
        let borrowed = dict.borrow();
        let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
        let white_point = get_white_point(dict)?;
        let black_point = get_black_point(dict);
        let gamma = match dict.get_float_for(b"Gamma") {
            0.0 => 1.0,
            value => value,
        };
        Some(Self {
            data: CpdfColorSpaceData::new(ColorSpaceFamily::CalGray, 1),
            gamma,
            white_point,
            black_point,
        })
    }

    pub fn gamma(&self) -> f32 {
        self.gamma
    }
    pub fn white_point(&self) -> [f32; 3] {
        self.white_point
    }
    pub fn black_point(&self) -> [f32; 3] {
        self.black_point
    }
}

impl CpdfColorSpace for CpdfCalGray {
    fn data(&self) -> &CpdfColorSpaceData {
        &self.data
    }

    fn get_rgb(&self, values: &[f32]) -> Option<FxRgbStruct<f32>> {
        let gray = *values.first()?;
        Some(FxRgbStruct {
            red: gray,
            green: gray,
            blue: gray,
        })
    }

    fn translate_image_line(
        &self,
        dest: &mut [u8],
        src: &[u8],
        pixels: usize,
        _width: i32,
        _height: i32,
        trans_mask: bool,
    ) {
        assert!(!trans_mask && src.len() >= pixels && dest.len() >= pixels * 3);
        for (gray, pixel) in src.iter().zip(dest.chunks_exact_mut(3)).take(pixels) {
            pixel.copy_from_slice(&[*gray, *gray, *gray]);
        }
    }
}

pub struct CpdfCalRgb {
    data: CpdfColorSpaceData,
    white_point: [f32; 3],
    black_point: [f32; 3],
    gamma: Option<[f32; 3]>,
    matrix: Option<[f32; 9]>,
}

impl Default for CpdfCalRgb {
    fn default() -> Self {
        Self {
            data: CpdfColorSpaceData::new(ColorSpaceFamily::CalRgb, 0),
            white_point: [1.0; 3],
            black_point: [0.0; 3],
            gamma: None,
            matrix: None,
        }
    }
}

impl CpdfCalRgb {
    pub fn load(array: &CpdfArray) -> Option<Self> {
        let dict = dict_at_one(array)?;
        let borrowed = dict.borrow();
        let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
        let white_point = get_white_point(dict)?;
        let black_point = get_black_point(dict);
        let gamma = dict.get_array_for(b"Gamma").map(|object| {
            let borrowed = object.borrow();
            let array = borrowed
                .as_any()
                .downcast_ref::<CpdfArray>()
                .expect("typed array");
            std::array::from_fn(|i| array.get_float_at(i))
        });
        let matrix = dict.get_array_for(b"Matrix").map(|object| {
            let borrowed = object.borrow();
            let array = borrowed
                .as_any()
                .downcast_ref::<CpdfArray>()
                .expect("typed array");
            std::array::from_fn(|i| array.get_float_at(i))
        });
        Some(Self {
            data: CpdfColorSpaceData::new(ColorSpaceFamily::CalRgb, 3),
            white_point,
            black_point,
            gamma,
            matrix,
        })
    }

    pub fn black_point(&self) -> [f32; 3] {
        self.black_point
    }
}

impl CpdfColorSpace for CpdfCalRgb {
    fn data(&self) -> &CpdfColorSpaceData {
        &self.data
    }

    fn get_rgb(&self, values: &[f32]) -> Option<FxRgbStruct<f32>> {
        let [mut a, mut b, mut c] = [*values.first()?, *values.get(1)?, *values.get(2)?];
        if let Some(gamma) = self.gamma {
            a = a.powf(gamma[0]);
            b = b.powf(gamma[1]);
            c = c.powf(gamma[2]);
        }
        let [x, y, z] = if let Some(matrix) = self.matrix {
            [
                matrix[0] * a + matrix[3] * b + matrix[6] * c,
                matrix[1] * a + matrix[4] * b + matrix[7] * c,
                matrix[2] * a + matrix[5] * b + matrix[8] * c,
            ]
        } else {
            [a, b, c]
        };
        Some(xyz_to_srgb_white_point(x, y, z, self.white_point))
    }

    fn translate_image_line(
        &self,
        dest: &mut [u8],
        src: &[u8],
        pixels: usize,
        _width: i32,
        _height: i32,
        trans_mask: bool,
    ) {
        assert!(!trans_mask && src.len() >= pixels * 3 && dest.len() >= pixels * 3);
        for (source, target) in src
            .chunks_exact(3)
            .zip(dest.chunks_exact_mut(3))
            .take(pixels)
        {
            target.copy_from_slice(&[source[2], source[1], source[0]]);
        }
    }
}

pub struct CpdfLabCs {
    data: CpdfColorSpaceData,
    white_point: [f32; 3],
    black_point: [f32; 3],
    ranges: [f32; 4],
}

impl CpdfLabCs {
    pub fn load(array: &CpdfArray) -> Option<Self> {
        let dict = dict_at_one(array)?;
        let borrowed = dict.borrow();
        let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
        let white_point = get_white_point(dict)?;
        let black_point = get_black_point(dict);
        let ranges = if let Some(object) = dict.get_array_for(b"Range") {
            let borrowed = object.borrow();
            let array = borrowed.as_any().downcast_ref::<CpdfArray>()?;
            std::array::from_fn(|i| array.get_float_at(i))
        } else {
            [-100.0, 100.0, -100.0, 100.0]
        };
        Some(Self {
            data: CpdfColorSpaceData::new(ColorSpaceFamily::Lab, 3),
            white_point,
            black_point,
            ranges,
        })
    }

    pub fn white_point(&self) -> [f32; 3] {
        self.white_point
    }
    pub fn black_point(&self) -> [f32; 3] {
        self.black_point
    }
}

impl CpdfColorSpace for CpdfLabCs {
    fn data(&self) -> &CpdfColorSpaceData {
        &self.data
    }

    fn get_default_value(&self, component: usize) -> (f32, f32, f32) {
        assert!(component < 3);
        if component > 0 {
            let min = self.ranges[component * 2 - 2];
            let max = self.ranges[component * 2 - 1];
            if min <= max {
                return (0.0_f32.clamp(min, max), min, max);
            }
        }
        (0.0, 0.0, 100.0)
    }

    fn get_rgb(&self, values: &[f32]) -> Option<FxRgbStruct<f32>> {
        let [lstar, astar, bstar] = [*values.first()?, *values.get(1)?, *values.get(2)?];
        let m = (lstar + 16.0) / 116.0;
        let l = m + astar / 500.0;
        let n = m - bstar / 200.0;
        let x = if l < 0.2069 {
            0.957 * 0.12842 * (l - 0.1379)
        } else {
            0.957 * l * l * l
        };
        let y = if m < 0.2069 {
            0.12842 * (m - 0.1379)
        } else {
            m * m * m
        };
        let z = if n < 0.2069 {
            1.0889 * 0.12842 * (n - 0.1379)
        } else {
            1.0889 * n * n * n
        };
        Some(xyz_to_srgb(x, y, z))
    }

    fn translate_image_line(
        &self,
        dest: &mut [u8],
        src: &[u8],
        pixels: usize,
        _width: i32,
        _height: i32,
        trans_mask: bool,
    ) {
        assert!(!trans_mask && src.len() >= pixels * 3 && dest.len() >= pixels * 3);
        for (lab, bgr) in src
            .chunks_exact(3)
            .zip(dest.chunks_exact_mut(3))
            .take(pixels)
        {
            let values = [
                f32::from(lab[0]) * 100.0 / 255.0,
                f32::from(lab[1]) - 128.0,
                f32::from(lab[2]) - 128.0,
            ];
            let rgb = self.get_rgb(&values).unwrap_or_default();
            bgr[0] = (rgb.blue * 255.0) as i32 as u8;
            bgr[1] = (rgb.green * 255.0) as i32 as u8;
            bgr[2] = (rgb.red * 255.0) as i32 as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

    fn make_space_array(white_point: [f32; 3]) -> CpdfArray {
        let mut array = CpdfArray::new();
        array.append_new_name(b"CalRGB");
        let mut dict = CpdfDictionary::new();
        let mut white = CpdfArray::new();
        for value in white_point {
            white.append_new(CpdfScalarObject::number_from_float(value));
        }
        dict.set_new_for(b"WhitePoint", white);
        array.append_new(dict);
        array
    }

    #[test]
    fn calibrated_white_point_validation_and_defaults() {
        assert!(CpdfCalRgb::load(&make_space_array([0.0, 1.0, 1.0])).is_none());
        assert!(CpdfCalRgb::load(&make_space_array([1.0, 0.9, 1.0])).is_none());
        let array = make_space_array([1.0, 1.0, 1.0]);
        let gray = CpdfCalGray::load(&array).unwrap();
        assert_eq!(gray.gamma(), 1.0);
        assert_eq!(gray.black_point(), [0.0; 3]);
        assert_eq!(
            gray.get_rgb(&[0.25]),
            Some(FxRgbStruct {
                red: 0.25,
                green: 0.25,
                blue: 0.25
            })
        );
        let lab = CpdfLabCs::load(&array).unwrap();
        assert_eq!(lab.get_default_value(1), (0.0, -100.0, 100.0));
        assert_eq!(
            lab.get_rgb(&[0.0, 0.0, 0.0]),
            Some(FxRgbStruct {
                red: 0.0,
                green: 0.0,
                blue: 0.0
            })
        );
        assert_eq!(
            lab.get_rgb(&[100.0, 0.0, 0.0]),
            Some(FxRgbStruct {
                red: 1.0,
                green: 1.0,
                blue: 1.0
            })
        );
    }

    #[test]
    fn calibrated_image_lines_follow_pdfium_byte_order() {
        let array = make_space_array([1.0, 1.0, 1.0]);
        let gray = CpdfCalGray::load(&array).unwrap();
        let rgb = CpdfCalRgb::load(&array).unwrap();
        let src = [255, 0, 0, 0, 255, 0, 0, 0, 255, 128, 128, 128];
        let mut out = [0xbd; 12];
        gray.translate_image_line(&mut out, &src, 4, 4, 1, false);
        assert_eq!(out, [255, 255, 255, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        rgb.translate_image_line(&mut out, &src, 4, 4, 1, false);
        assert_eq!(out, [0, 0, 255, 0, 255, 0, 255, 0, 0, 128, 128, 128]);
    }
}

#[cfg(test)]
#[path = "cpdf_colorspace_unittest.rs"]
mod upstream_tests;
