// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! RenderTilingPatternToBitmap/GetRenderedPattern from fpdfsdk/fpdf_editpage.cpp.
use crate::fpdfapi::page::cpdf_docpagedata::CpdfDocPageData;
use crate::fpdfapi::page::cpdf_page::CpdfPage;
use crate::fpdfapi::page::cpdf_pageobject::PageObject;
use crate::fpdfapi::page::cpdf_pattern::PatternKind;
use crate::fpdfapi::page::cpdf_tilingpattern::CpdfTilingPattern;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::retain_object;
use crate::fpdfapi::render::cpdf_rendercontext::CpdfRenderContext;
use crate::fpdfapi::render::cpdf_renderoptions::RenderOptionsFlags;
use crate::fxcrt::fx_coordinates::CfxMatrix;
use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
use crate::fxge::dib::fx_dib::{alpha_and_color_ref_to_argb, FxdibFormat};
use std::rc::Rc;

pub fn rendered_pattern(
    data: Rc<CpdfDocPageData>,
    object: &dyn PageObject,
    stroke: bool,
) -> Option<CfxDibitmap> {
    let colors = object.base().color_state();
    let color = if stroke {
        colors.get_stroke_color()
    } else {
        colors.get_fill_color()
    }?;
    if !color.is_pattern() {
        return None;
    }
    let pattern = color.get_pattern()?;
    if !matches!(pattern.kind(), PatternKind::Tiling { .. }) {
        return None;
    }
    let mut tile =
        CpdfTilingPattern::new(pattern.pattern_object().clone(), pattern.parent_matrix())?;
    let form = if tile.colored() {
        tile.load(object, data.clone())?
    } else {
        // The uncolored tile supplies coverage; the final CompositeMask below
        // applies the object's opacity. Inheriting that opacity into the cell
        // as well would square it (50% becomes 25%). Copy only the small
        // general state needed by TilingPattern::Load, with neutral opacity.
        use crate::fpdfapi::page::cpdf_pathobject::CpdfPathObject;
        let mut mask_parent = CpdfPathObject::default();
        *mask_parent.base_mut().mutable_general_state() = object.base().general_state().clone();
        mask_parent
            .base_mut()
            .mutable_general_state()
            .set_fill_alpha(1.0);
        mask_parent
            .base_mut()
            .mutable_general_state()
            .set_stroke_alpha(1.0);
        tile.load(&mask_parent, data.clone())?
    };
    let rect = tile.bbox().get_outer_rect();
    if rect.is_empty() || !rect.valid() {
        return None;
    }

    // The native context takes a document with no page/cache. Supply a private
    // page resource view to the existing safe renderer, retaining only document
    // data. The pattern form supplies its own resources and complete geometry.
    let page = CpdfPage::new_with_page_data(retain_object(CpdfDictionary::new()), data)?;
    let context = CpdfRenderContext::with_page_resources(&page, None);
    let cell = crate::fpdfapi::render::cpdf_rendertiling::draw_pattern_bitmap(
        &context,
        &tile,
        &form,
        CfxMatrix::default(),
        rect.width(),
        rect.height(),
        &RenderOptionsFlags::default(),
    )?;
    if tile.colored() {
        return Some(cell);
    }
    let reference = if stroke {
        colors.get_stroke_color_ref()
    } else {
        colors.get_fill_color_ref()
    };
    if reference == u32::MAX {
        return None;
    }
    let state = object.base().general_state();
    let alpha = if stroke {
        state.get_stroke_alpha()
    } else {
        state.get_fill_alpha()
    };
    let color = alpha_and_color_ref_to_argb((alpha * 255.0) as i32, reference);
    let mut result = CfxDibitmap::create(rect.width(), rect.height(), FxdibFormat::Bgra, 0)?;
    result
        .composite_mask_normal(0, 0, rect.width(), rect.height(), &cell, color, 0, 0, false)
        .then_some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::page::cpdf_colorspace::load_supported_cs;
    use crate::fpdfapi::page::cpdf_pathobject::CpdfPathObject;
    use crate::fpdfapi::page::cpdf_pattern::CpdfPattern;
    use crate::fpdfapi::parser::cpdf_array::CpdfArray;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
    use crate::fxcrt::fx_coordinates::CfxFloatRect;

    fn object(colored: bool) -> CpdfPathObject {
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(
            b"PaintType",
            CpdfScalarObject::number_from_signed(if colored { 1 } else { 2 }),
        );
        dict.set_new_for(b"XStep", CpdfScalarObject::number_from_signed(4));
        dict.set_new_for(b"YStep", CpdfScalarObject::number_from_signed(4));
        dict.set_rect_for(b"BBox", CfxFloatRect::new(0., 0., 4., 4.));
        let stream = retain_object(CpdfStream::from_data_and_dict(
            if colored {
                b"1 0 0 rg 1 1 2 2 re f".to_vec()
            } else {
                b"1 1 2 2 re f".to_vec()
            },
            retain_object(dict),
        ));
        let pattern = Rc::new(CpdfPattern::new_tiling(stream, CfxMatrix::default()).unwrap());
        let mut object = CpdfPathObject::default();
        if !colored {
            let mut cs = CpdfArray::new();
            cs.append_new_name(b"Pattern");
            cs.append_new_name(b"DeviceRGB");
            object
                .base_mut()
                .mutable_color_state()
                .get_mutable_fill_color()
                .set_color_space(load_supported_cs(&retain_object(cs)).unwrap());
        }
        object
            .base_mut()
            .mutable_color_state()
            .set_fill_pattern(pattern, &[0., 0., 1.]);
        object
    }
    #[test]
    fn colored_tiles_keep_pixels_and_uncolored_tiles_apply_operand_color_and_alpha() {
        let data = Rc::new(CpdfDocPageData::new());
        let colored = object(true);
        let bitmap = rendered_pattern(data.clone(), &colored, false).unwrap();
        assert_eq!((bitmap.width(), bitmap.height()), (4, 4));
        assert_eq!(&bitmap.scanline(2).unwrap()[8..12], &[0, 0, 255, 255]);
        assert_eq!(&bitmap.scanline(0).unwrap()[0..4], &[0, 0, 0, 0]);
        let mut uncolored = object(false);
        uncolored
            .base_mut()
            .mutable_general_state()
            .set_fill_alpha(0.5);
        let bitmap = rendered_pattern(data, &uncolored, false).unwrap();
        assert_eq!(&bitmap.scanline(2).unwrap()[8..12], &[255, 0, 0, 127]);
    }
    #[test]
    fn non_pattern_and_missing_stroke_do_not_produce_bitmaps() {
        let data = Rc::new(CpdfDocPageData::new());
        assert!(rendered_pattern(data.clone(), &CpdfPathObject::default(), false).is_none());
        assert!(rendered_pattern(data, &object(true), true).is_none());
    }
}
