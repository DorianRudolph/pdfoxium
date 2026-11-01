// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fpdfapi/page/cpdf_meshstream.{h,cpp}`.

use super::cpdf_colorspace::ColorSpaceRef;
use super::cpdf_shadingpattern::{CpdfShadingPattern, ShadingType};
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fpdfapi::parser::cpdf_stream_acc::CpdfStreamAcc;
use crate::fxcrt::cfx_bitstream::CfxBitStream;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF};
use crate::fxge::dib::fx_dib::FxRgbStruct;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CpdfMeshVertex {
    pub position: CfxPointF,
    pub rgb: FxRgbStruct<f32>,
}

pub struct CpdfMeshStream<'a> {
    shading_type: ShadingType,
    has_functions: bool,
    color_space: ColorSpaceRef,
    access: CpdfStreamAcc<'a>,
    bit_pos: usize,
    coord_bits: u32,
    component_bits: u32,
    flag_bits: u32,
    components: u32,
    coord_max: u32,
    component_max: u32,
    xmin: f32,
    xmax: f32,
    ymin: f32,
    ymax: f32,
    color_min: [f32; 8],
    color_max: [f32; 8],
}

impl<'a> CpdfMeshStream<'a> {
    pub fn load(
        shading_type: ShadingType,
        has_functions: bool,
        stream: &'a CpdfStream,
        color_space: ColorSpaceRef,
    ) -> Option<Self> {
        let mut access = CpdfStreamAcc::new(stream);
        access.load_all_data_filtered();
        let dict = stream.get_dict();
        let borrowed = dict.borrow();
        let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
        let coord_bits = dict.get_integer_for(b"BitsPerCoordinate", 0) as u32;
        let component_bits = dict.get_integer_for(b"BitsPerComponent", 0) as u32;
        let check_bpc = matches!(
            shading_type,
            ShadingType::FreeFormGouraudTriangleMesh
                | ShadingType::LatticeFormGouraudTriangleMesh
                | ShadingType::CoonsPatchMesh
                | ShadingType::TensorProductPatchMesh
        );
        if check_bpc
            && (!matches!(coord_bits, 1 | 2 | 4 | 8 | 12 | 16 | 24 | 32)
                || !matches!(component_bits, 1 | 2 | 4 | 8 | 12 | 16))
        {
            return None;
        }
        let flag_bits = dict.get_integer_for(b"BitsPerFlag", 0) as u32;
        if matches!(
            shading_type,
            ShadingType::FreeFormGouraudTriangleMesh
                | ShadingType::CoonsPatchMesh
                | ShadingType::TensorProductPatchMesh
        ) && !matches!(flag_bits, 2 | 4 | 8)
        {
            return None;
        }
        let color_components = color_space.component_count();
        if color_components > 8 {
            return None;
        }
        let components = if has_functions { 1 } else { color_components };
        let decode = dict.get_array_for(b"Decode")?;
        let borrowed_decode = decode.borrow();
        let decode = borrowed_decode.as_any().downcast_ref::<CpdfArray>()?;
        if decode.size() != 4 + components as usize * 2 {
            return None;
        }
        let mut color_min = [0.0; 8];
        let mut color_max = [0.0; 8];
        for i in 0..components as usize {
            color_min[i] = decode.get_float_at(i * 2 + 4);
            color_max[i] = decode.get_float_at(i * 2 + 5);
        }
        Some(Self {
            shading_type,
            has_functions,
            color_space,
            access,
            bit_pos: 0,
            coord_bits,
            component_bits,
            flag_bits,
            components,
            coord_max: if coord_bits == 32 {
                u32::MAX
            } else {
                (1u32 << coord_bits) - 1
            },
            component_max: (1u32 << component_bits) - 1,
            xmin: decode.get_float_at(0),
            xmax: decode.get_float_at(1),
            ymin: decode.get_float_at(2),
            ymax: decode.get_float_at(3),
            color_min,
            color_max,
        })
    }

    pub fn component_bits(&self) -> u32 {
        self.component_bits
    }
    pub fn components(&self) -> u32 {
        self.components
    }
    pub fn component_min(&self, index: usize) -> f32 {
        self.color_min[index]
    }
    pub fn component_max(&self, index: usize) -> f32 {
        self.color_max[index]
    }
    pub fn shading_type(&self) -> ShadingType {
        self.shading_type
    }
    pub fn is_eof(&self) -> bool {
        self.bit_pos >= self.access.get_span().len() * 8
    }
    pub fn can_read_flag(&self) -> bool {
        self.bits_remaining() >= self.flag_bits as usize
    }
    pub fn can_read_coords(&self) -> bool {
        self.bits_remaining() / 2 >= self.coord_bits as usize
    }
    pub fn can_read_color(&self) -> bool {
        self.component_bits != 0
            && self.bits_remaining() / self.component_bits as usize >= self.components as usize
    }
    fn bits_remaining(&self) -> usize {
        (self.access.get_span().len() * 8).saturating_sub(self.bit_pos)
    }
    pub fn skip_bits(&mut self, count: u32) {
        self.bit_pos = self.bit_pos.wrapping_add(count as usize);
    }
    pub fn byte_align(&mut self) {
        self.bit_pos = self.bit_pos.wrapping_add(7) & !7;
    }
    fn get_bits(&mut self, count: u32) -> u32 {
        let mut bits = CfxBitStream::new(self.access.get_span());
        bits.skip_bits(self.bit_pos);
        let value = bits.get_bits(count);
        self.bit_pos = bits.pos();
        value
    }
    pub fn read_flag(&mut self) -> u32 {
        self.get_bits(self.flag_bits) & 3
    }
    pub fn read_coords(&mut self) -> CfxPointF {
        let x = self.get_bits(self.coord_bits);
        let y = self.get_bits(self.coord_bits);
        if self.coord_bits == 32 {
            let max = self.coord_max as f64;
            CfxPointF::new(
                self.xmin + (x as f64 * (self.xmax - self.xmin) as f64 / max) as f32,
                self.ymin + (y as f64 * (self.ymax - self.ymin) as f64 / max) as f32,
            )
        } else {
            CfxPointF::new(
                self.xmin + x as f32 * (self.xmax - self.xmin) / self.coord_max as f32,
                self.ymin + y as f32 * (self.ymax - self.ymin) / self.coord_max as f32,
            )
        }
    }
    pub fn read_color(&mut self) -> FxRgbStruct<f32> {
        let mut values = [0.0; 8];
        for (i, value) in values.iter_mut().enumerate().take(self.components as usize) {
            *value = self.color_min[i]
                + self.get_bits(self.component_bits) as f32
                    * (self.color_max[i] - self.color_min[i])
                    / self.component_max as f32;
        }
        if self.has_functions {
            FxRgbStruct {
                red: values[0],
                green: 0.0,
                blue: 0.0,
            }
        } else {
            self.color_space.get_rgb_or_zeros_on_error(&values)
        }
    }
    pub fn read_vertex(&mut self, matrix: CfxMatrix) -> Option<(CpdfMeshVertex, u32)> {
        if !self.can_read_flag() {
            return None;
        }
        let flag = self.read_flag();
        if !self.can_read_coords() {
            return None;
        }
        let position = matrix.transform(self.read_coords());
        if !self.can_read_color() {
            return None;
        }
        let rgb = self.read_color();
        self.byte_align();
        Some((CpdfMeshVertex { position, rgb }, flag))
    }
    pub fn read_vertex_row(&mut self, matrix: CfxMatrix, count: i32) -> Vec<CpdfMeshVertex> {
        let mut vertices = Vec::new();
        for _ in 0..count {
            if self.is_eof() || !self.can_read_coords() {
                return Vec::new();
            }
            let position = matrix.transform(self.read_coords());
            if !self.can_read_color() {
                return Vec::new();
            }
            let rgb = self.read_color();
            self.byte_align();
            vertices.push(CpdfMeshVertex { position, rgb });
        }
        vertices
    }
}

/// The content parser's mesh bounds scan. It follows PDFium's per-record
/// point and color counts, including the reduced counts after a patch flag.
pub fn get_shading_bbox(pattern: &CpdfShadingPattern, matrix: CfxMatrix) -> CfxFloatRect {
    let Some(shading) = pattern.shading_object() else {
        return CfxFloatRect::default();
    };
    let borrowed = shading.borrow();
    let Some(stream) = borrowed.as_any().downcast_ref::<CpdfStream>() else {
        return CfxFloatRect::default();
    };
    let Some(color_space) = pattern.color_space() else {
        return CfxFloatRect::default();
    };
    let Some(mut mesh) = CpdfMeshStream::load(
        pattern.shading_type(),
        !pattern.functions().is_empty(),
        stream,
        color_space,
    ) else {
        return CfxFloatRect::default();
    };
    let shading_type = pattern.shading_type();
    let gouraud = matches!(
        shading_type,
        ShadingType::FreeFormGouraudTriangleMesh | ShadingType::LatticeFormGouraudTriangleMesh
    );
    let full_point_count = match shading_type {
        ShadingType::TensorProductPatchMesh => 16,
        ShadingType::CoonsPatchMesh => 12,
        _ => 1,
    };
    let mut rect = CfxFloatRect::default();
    let mut has_point = false;
    while !mesh.is_eof() {
        let flag = if shading_type == ShadingType::LatticeFormGouraudTriangleMesh {
            0
        } else {
            if !mesh.can_read_flag() {
                break;
            }
            mesh.read_flag()
        };
        let point_count = if !gouraud && flag != 0 {
            full_point_count - 4
        } else {
            full_point_count
        };
        let color_count: i32 = if gouraud {
            1
        } else if flag != 0 {
            2
        } else {
            4
        };
        for _ in 0..point_count {
            if !mesh.can_read_coords() {
                break;
            }
            let point = mesh.read_coords();
            if has_point {
                rect.update_rect(point);
            } else {
                rect = CfxFloatRect::from_point(point);
                has_point = true;
            }
        }
        let Some(bits) = u32::try_from(color_count).ok().and_then(|count| {
            mesh.components()
                .checked_mul(mesh.component_bits())
                .and_then(|value| value.checked_mul(count))
        }) else {
            break;
        };
        mesh.skip_bits(bits);
        if gouraud {
            mesh.byte_align();
        }
    }
    matrix.transform_float_rect(rect)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::page::cpdf_docpagedata::CpdfDocPageData;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

    fn decode(values: &[f32]) -> CpdfArray {
        let mut array = CpdfArray::new();
        for &value in values {
            array.append_new(CpdfScalarObject::number_from_float(value));
        }
        array
    }

    #[test]
    fn validates_mesh_bits_and_decodes_lattice_bounds() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"ShadingType", CpdfScalarObject::number_from_signed(5));
        dict.set_new_name_for(b"ColorSpace", b"DeviceGray");
        dict.set_new_for(
            b"BitsPerCoordinate",
            CpdfScalarObject::number_from_signed(8),
        );
        dict.set_new_for(b"BitsPerComponent", CpdfScalarObject::number_from_signed(8));
        dict.set_new_for(b"Decode", decode(&[0.0, 100.0, 0.0, 100.0, 0.0, 1.0]));
        let stream = retain_object(CpdfStream::from_data_and_dict(
            vec![51, 102, 255, 204, 153, 0],
            retain_object(dict),
        ));
        let pattern = CpdfShadingPattern::new(stream.clone(), true, CfxMatrix::default()).unwrap();
        assert!(pattern.load(&CpdfDocPageData::new()));
        let bbox = get_shading_bbox(&pattern, CfxMatrix::default());
        assert_eq!(bbox, CfxFloatRect::new(20.0, 40.0, 80.0, 60.0));
        let borrowed = stream.borrow();
        let stream = borrowed.as_any().downcast_ref::<CpdfStream>().unwrap();
        let mut mesh = CpdfMeshStream::load(
            pattern.shading_type(),
            false,
            stream,
            pattern.color_space().unwrap(),
        )
        .unwrap();
        let vertices = mesh.read_vertex_row(CfxMatrix::default(), 2);
        assert_eq!(vertices.len(), 2);
        assert_eq!(vertices[0].rgb.red, 1.0);
        assert_eq!(vertices[1].rgb.red, 0.0);
        assert!(mesh.is_eof());
    }
}
