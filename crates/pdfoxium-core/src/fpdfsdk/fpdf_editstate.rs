// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Page-object matrix, color and graphics-state editing from
//! fpdfsdk/fpdf_editpage.cpp. Opaque public handles live in pdfoxium-capi.

use crate::fpdfapi::page::cpdf_colorspace::{get_stock_device_cs, ColorSpaceFamily};
use crate::fpdfapi::page::cpdf_formobject::CpdfFormObject;
use crate::fpdfapi::page::cpdf_imageobject::CpdfImageObject;
use crate::fpdfapi::page::cpdf_pageobject::{PageObject, PageObjectType};
use crate::fpdfapi::page::cpdf_pathobject::CpdfPathObject;
use crate::fpdfapi::page::cpdf_textobject::CpdfTextObject;
use crate::fxcrt::fx_coordinates::{CfxMatrix, CfxPointF};
use crate::fxge::cfx_graphstatedata::{LineCap, LineJoin};
use crate::fxge::dib::fx_dib::{
    fxsys_get_b_value, fxsys_get_g_value, fxsys_get_r_value, fxsys_get_unsigned_alpha, BlendMode,
};

pub fn object_matrix(object: &dyn PageObject) -> Option<CfxMatrix> {
    match object.object_type() {
        PageObjectType::Text => Some(
            object
                .as_any()
                .downcast_ref::<CpdfTextObject>()?
                .text_matrix(),
        ),
        PageObjectType::Path => Some(object.as_any().downcast_ref::<CpdfPathObject>()?.matrix()),
        PageObjectType::Image => Some(object.as_any().downcast_ref::<CpdfImageObject>()?.matrix()),
        PageObjectType::Shading => None,
        PageObjectType::Form => Some(
            object
                .as_any()
                .downcast_ref::<CpdfFormObject>()?
                .form_matrix(),
        ),
    }
}

pub fn has_transparency(object: &dyn PageObject) -> bool {
    let state = object.base().general_state();
    if state.get_blend_type() != BlendMode::Normal
        || state.get_soft_mask().is_some()
        || state.get_fill_alpha() != 1.0
        || (object.object_type() == PageObjectType::Path && state.get_stroke_alpha() != 1.0)
    {
        return true;
    }
    let Some(form) = object.as_any().downcast_ref::<CpdfFormObject>() else {
        return false;
    };
    let transparency = form.form().holder().transparency();
    transparency.is_group() || transparency.is_isolated()
}

pub fn set_blend_mode(object: &mut dyn PageObject, mode: &[u8]) {
    object
        .base_mut()
        .mutable_general_state()
        .set_blend_mode(mode);
    object.base_mut().set_dirty(true);
}

pub fn set_fill_color(object: &mut dyn PageObject, rgba: [u32; 4]) -> bool {
    if rgba.iter().any(|&component| component > 255) {
        return false;
    }
    let [r, g, b, a] = rgba;
    let base = object.base_mut();
    base.mutable_general_state()
        .set_fill_alpha(a as f32 / 255.0);
    base.mutable_color_state().set_fill_color(
        Some(get_stock_device_cs(ColorSpaceFamily::DeviceRgb)),
        vec![r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0],
    );
    base.set_dirty(true);
    true
}

pub fn fill_color(object: &dyn PageObject) -> Option<[u32; 4]> {
    let base = object.base();
    if !base.color_state().has_ref() {
        return None;
    }
    let color = base.color_state().get_fill_color_ref();
    Some([
        u32::from(fxsys_get_r_value(color)),
        u32::from(fxsys_get_g_value(color)),
        u32::from(fxsys_get_b_value(color)),
        fxsys_get_unsigned_alpha(base.general_state().get_fill_alpha()),
    ])
}

pub fn set_stroke_color(object: &mut dyn PageObject, rgba: [u32; 4]) -> bool {
    if rgba.iter().any(|&component| component > 255) {
        return false;
    }
    let [r, g, b, a] = rgba;
    let base = object.base_mut();
    base.mutable_general_state()
        .set_stroke_alpha(a as f32 / 255.0);
    base.mutable_color_state().set_stroke_color(
        Some(get_stock_device_cs(ColorSpaceFamily::DeviceRgb)),
        vec![r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0],
    );
    base.set_dirty(true);
    true
}

pub fn stroke_color(object: &dyn PageObject) -> Option<[u32; 4]> {
    let base = object.base();
    if !base.color_state().has_ref() {
        return None;
    }
    let color = base.color_state().get_stroke_color_ref();
    Some([
        u32::from(fxsys_get_r_value(color)),
        u32::from(fxsys_get_g_value(color)),
        u32::from(fxsys_get_b_value(color)),
        fxsys_get_unsigned_alpha(base.general_state().get_stroke_alpha()),
    ])
}

pub fn set_stroke_width(object: &mut dyn PageObject, width: f32) -> bool {
    if width < 0.0 {
        return false;
    }
    object
        .base_mut()
        .mutable_graph_state()
        .set_line_width(width);
    object.base_mut().set_dirty(true);
    true
}

pub fn set_line_join(object: &mut dyn PageObject, line_join: i32) -> bool {
    if !(0..=2).contains(&line_join) {
        return false;
    }
    object
        .base_mut()
        .mutable_graph_state()
        .set_line_join(LineJoin::from_pdf_integer(line_join));
    object.base_mut().set_dirty(true);
    true
}

pub fn set_line_cap(object: &mut dyn PageObject, line_cap: i32) -> bool {
    if !(0..=2).contains(&line_cap) {
        return false;
    }
    object
        .base_mut()
        .mutable_graph_state()
        .set_line_cap(LineCap::from_pdf_integer(line_cap));
    object.base_mut().set_dirty(true);
    true
}

pub fn set_dash_phase(object: &mut dyn PageObject, phase: f32) {
    object
        .base_mut()
        .mutable_graph_state()
        .set_line_dash_phase(phase);
    object.base_mut().set_dirty(true);
}

pub fn dash_array(object: &dyn PageObject) -> &[f32] {
    object
        .base()
        .graph_state()
        .get_object()
        .map_or(&[], |data| data.dash_array())
}

pub fn copy_dash_array(object: &dyn PageObject, result: &mut [f32]) -> bool {
    let dashes = dash_array(object);
    if result.len() < dashes.len() {
        return false;
    }
    result[..dashes.len()].copy_from_slice(dashes);
    true
}

pub fn set_dash_array(object: &mut dyn PageObject, dashes: Vec<f32>, phase: f32) {
    object
        .base_mut()
        .mutable_graph_state()
        .set_line_dash(dashes, phase);
    object.base_mut().set_dirty(true);
}

pub fn rotated_bounds(object: &dyn PageObject) -> Option<[CfxPointF; 4]> {
    let matrix = match object.object_type() {
        PageObjectType::Text | PageObjectType::Image => object_matrix(object)?,
        // These are the types supported by the pinned public API.
        _ => return None,
    };
    let bbox = object.base().original_rect();
    // PDF 32000-1:2008, figure 64 specifies the QuadPoints ordering.
    Some([
        matrix.transform(CfxPointF::new(bbox.left, bbox.bottom)),
        matrix.transform(CfxPointF::new(bbox.right, bbox.bottom)),
        matrix.transform(CfxPointF::new(bbox.right, bbox.top)),
        matrix.transform(CfxPointF::new(bbox.left, bbox.top)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxcrt::fx_coordinates::CfxFloatRect;

    #[test]
    fn colors_round_trip_every_channel_value_and_reject_invalid_input() {
        // fpdf_editpath_embeddertest.cpp VerifyCorrectColoursReturned, editing
        // and channel conversion portion; save/reload is covered separately.
        let mut path = CpdfPathObject::default();
        path.base_mut().set_default_states();
        for value in 0..256 {
            let rgba = [value; 4];
            assert!(set_fill_color(&mut path, rgba));
            assert!(set_stroke_color(&mut path, rgba));
            assert_eq!(fill_color(&path), Some(rgba));
            assert_eq!(stroke_color(&path), Some(rgba));
        }
        assert!(!set_stroke_color(&mut path, [100, 100, 100, 300]));
        assert!(!set_fill_color(&mut path, [200, 256, 200, 0]));
        assert_eq!(fill_color(&path), Some([255; 4]));
        assert_eq!(stroke_color(&path), Some([255; 4]));
    }

    #[test]
    fn dash_arrays_preserve_short_outputs_and_unused_tail() {
        // fpdf_editpage_embeddertest.cpp DashingArrayAndPhase buffer contract.
        let mut path = CpdfPathObject::default();
        let mut result = [-1.0; 4];
        assert!(copy_dash_array(&path, &mut result));
        assert_eq!(result, [-1.0; 4]);
        set_dash_array(&mut path, vec![1.0, 2.0, 3.0], 5.0);
        assert_eq!(path.base().graph_state().get_line_dash_phase(), 5.0);
        assert!(!copy_dash_array(&path, &mut result[..2]));
        assert_eq!(result, [-1.0; 4]);
        assert!(copy_dash_array(&path, &mut result));
        assert_eq!(result, [1.0, 2.0, 3.0, -1.0]);
        set_dash_phase(&mut path, 1.0);
        assert_eq!(dash_array(&path), &[1.0, 2.0, 3.0]);
        assert_eq!(path.base().graph_state().get_line_dash_phase(), 1.0);
        set_dash_array(&mut path, Vec::new(), 4.0);
        assert!(dash_array(&path).is_empty());
        assert_eq!(path.base().graph_state().get_line_dash_phase(), 4.0);
    }

    #[test]
    fn transparency_uses_object_type_and_graphics_state() {
        let mut path = CpdfPathObject::default();
        assert!(!has_transparency(&path));
        set_stroke_color(&mut path, [63, 63, 0, 127]);
        assert!(has_transparency(&path));
        set_stroke_color(&mut path, [63, 63, 0, 255]);
        assert!(!has_transparency(&path));
        set_blend_mode(&mut path, b"Multiply");
        assert!(has_transparency(&path));
        set_blend_mode(&mut path, b"Normal");
        assert!(!has_transparency(&path));
        let mut image = CpdfImageObject::default();
        assert_eq!(fill_color(&image), None);
        assert_eq!(stroke_color(&image), None);
        set_stroke_color(&mut image, [0, 0, 0, 127]);
        assert!(!has_transparency(&image));
        set_fill_color(&mut image, [255, 0, 0, 127]);
        assert!(has_transparency(&image));
    }

    #[test]
    fn transformed_image_quad_retains_rotated_corners() {
        // fpdf_editpage_embeddertest.cpp GetBoundsForRotatedImage geometry.
        let mut image = CpdfImageObject::default();
        let matrix = CfxMatrix::new(30.0, -30.0, 40.0, 40.0, 100.0, 100.0);
        image.set_image_matrix(matrix);
        assert_eq!(object_matrix(&image), Some(matrix));
        assert_eq!(
            image.base().rect(),
            CfxFloatRect::new(100.0, 70.0, 170.0, 140.0)
        );
        assert_eq!(
            rotated_bounds(&image),
            Some([
                CfxPointF::new(100.0, 100.0),
                CfxPointF::new(130.0, 70.0),
                CfxPointF::new(170.0, 110.0),
                CfxPointF::new(140.0, 140.0),
            ])
        );
        image.transform(CfxMatrix::new(1.0, 0.0, 0.0, 1.0, 2.0, 3.0));
        assert_eq!(
            rotated_bounds(&image).unwrap()[0],
            CfxPointF::new(102.0, 103.0)
        );
        assert!(image.base().is_dirty());
        assert!(rotated_bounds(&CpdfPathObject::default()).is_none());
    }
}
