// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! PWL's device boundary. Shape helpers port the connected DrawFillArea,
//! DrawStrokeRect/Line, DrawShadow and DrawBorder methods in
//! `core/fxge/cfx_renderdevice.cpp`; the backend uses the existing AGG renderer.
use crate::fpdfapi::font::cpdf_font::CpdfFont;
use crate::fpdfapi::render::cpdf_renderoptions::CpdfRenderOptions;
use crate::fpdfapi::render::cpdf_textrenderer::draw_pdf_normal_text_agg;
use crate::fpdfsdk::cpdfsdk_baannot::BorderStyle;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF};
use crate::fxge::agg::cfx_agg_devicedriver::CfxAggDeviceDriver;
use crate::fxge::cfx_color::CfxColor;
use crate::fxge::cfx_fillrenderoptions::CfxFillRenderOptions;
use crate::fxge::cfx_graphstatedata::CfxGraphStateData;
use crate::fxge::cfx_path::{CfxPath, PointType};
use crate::fxge::cfx_renderdevice::draw_path_agg;

fn polygon(points: &[CfxPointF]) -> CfxPath {
    let mut path = CfxPath::new();
    for (index, &point) in points.iter().enumerate() {
        path.append_point(
            point,
            if index == 0 {
                PointType::Move
            } else {
                PointType::Line
            },
        );
    }
    path
}

pub trait PwlDraw {
    #[cfg(feature = "xfa")]
    fn draw_xfa_widget(
        &mut self,
        _widget: &crate::fpdfsdk::fpdfxfa::cpdfxfa_widget::CpdfXfaWidget,
        _matrix: CfxMatrix,
    ) -> bool {
        false
    }
    #[cfg(feature = "xfa")]
    fn draw_xfa_page(
        &mut self,
        _page: &crate::fpdfsdk::fpdfxfa::cpdfxfa_page::CpdfXfaPage,
        _focus: Option<&std::rc::Rc<crate::fpdfsdk::cpdfsdk_annot::CpdfSdkAnnot>>,
        _matrix: CfxMatrix,
        _clip: crate::fxcrt::fx_coordinates::FxRect,
    ) -> bool {
        false
    }

    fn draw_annotation_ap(
        &mut self,
        page: &crate::fpdfsdk::cpdfsdk_pageview::SdkPage,
        dict: &crate::fpdfapi::parser::cpdf_object::ObjectRef,
        mode: crate::fpdfdoc::cpdf_annot::AppearanceMode,
        matrix: CfxMatrix,
    ) -> bool;
    fn save_state(&mut self);
    fn restore_state(&mut self);
    fn set_clip_rect(&mut self, rect: CfxFloatRect, matrix: CfxMatrix);
    fn draw_path(
        &mut self,
        path: &CfxPath,
        matrix: CfxMatrix,
        graph: Option<&CfxGraphStateData>,
        fill: u32,
        stroke: u32,
        options: CfxFillRenderOptions,
    ) -> bool;
    fn draw_text(
        &mut self,
        point: CfxPointF,
        font: &CpdfFont,
        size: f32,
        matrix: CfxMatrix,
        bytes: &[u8],
        fill: u32,
    ) -> bool;
    fn fill_rect(&mut self, matrix: CfxMatrix, rect: CfxFloatRect, argb: u32) {
        let mut path = CfxPath::new();
        path.append_float_rect(rect);
        self.draw_path(
            &path,
            matrix,
            None,
            argb,
            0,
            CfxFillRenderOptions::winding_options(),
        );
    }
    fn fill_color_rect(
        &mut self,
        matrix: CfxMatrix,
        rect: CfxFloatRect,
        color: CfxColor,
        alpha: i32,
    ) {
        self.fill_rect(matrix, rect, color.to_fx_color(alpha));
    }
    fn fill_area(&mut self, matrix: CfxMatrix, points: &[CfxPointF], argb: u32) {
        if points.is_empty() {
            return;
        }
        self.draw_path(
            &polygon(points),
            matrix,
            None,
            argb,
            0,
            CfxFillRenderOptions::even_odd_options(),
        );
    }
    fn stroke_rect(&mut self, matrix: CfxMatrix, rect: CfxFloatRect, argb: u32, width: f32) {
        let mut path = CfxPath::new();
        path.append_float_rect(rect);
        let mut graph = CfxGraphStateData::default();
        graph.set_line_width(width);
        self.draw_path(
            &path,
            matrix,
            Some(&graph),
            0,
            argb,
            CfxFillRenderOptions::even_odd_options(),
        );
    }
    fn stroke_line(
        &mut self,
        matrix: CfxMatrix,
        from: CfxPointF,
        to: CfxPointF,
        argb: u32,
        width: f32,
    ) {
        let path = polygon(&[from, to]);
        let mut graph = CfxGraphStateData::default();
        graph.set_line_width(width);
        self.draw_path(
            &path,
            matrix,
            Some(&graph),
            0,
            argb,
            CfxFillRenderOptions::even_odd_options(),
        );
    }
    fn shadow(&mut self, matrix: CfxMatrix, rect: CfxFloatRect, alpha: i32, start: i32, end: i32) {
        if rect.is_empty() || !rect.height().is_finite() {
            return;
        }
        let step = (end as f32 - start as f32) / rect.height();
        let mut y = rect.bottom + 0.5;
        while y <= rect.top - 0.5 {
            let gray = (start as f32 + (step * (y - rect.bottom)).trunc()) as i32;
            let g = gray.clamp(0, 255) as u32;
            let argb = ((alpha.clamp(0, 255) as u32) << 24) | (g << 16) | (g << 8) | g;
            self.stroke_line(
                matrix,
                CfxPointF::new(rect.left, y),
                CfxPointF::new(rect.right, y),
                argb,
                1.5,
            );
            let next = y + 1.0;
            if next == y {
                break;
            }
            y = next;
        }
    }
    #[allow(clippy::too_many_arguments)]
    fn draw_border(
        &mut self,
        matrix: CfxMatrix,
        rect: CfxFloatRect,
        width: f32,
        color: CfxColor,
        left_top: CfxColor,
        right_bottom: CfxColor,
        style: BorderStyle,
        alpha: i32,
    ) {
        if width <= 0.0 {
            return;
        }
        let (l, r, t, b, h) = (rect.left, rect.right, rect.top, rect.bottom, width / 2.0);
        let argb = color.to_fx_color(alpha);
        match style {
            BorderStyle::Solid => {
                let mut path = CfxPath::new();
                path.append_rect(l, b, r, t);
                path.append_rect(l + width, b + width, r - width, t - width);
                self.draw_path(
                    &path,
                    matrix,
                    None,
                    argb,
                    0,
                    CfxFillRenderOptions::even_odd_options(),
                );
            }
            BorderStyle::Dash => {
                let path = polygon(&[
                    CfxPointF::new(l + h, b + h),
                    CfxPointF::new(l + h, t - h),
                    CfxPointF::new(r - h, t - h),
                    CfxPointF::new(r - h, b + h),
                    CfxPointF::new(l + h, b + h),
                ]);
                let mut graph = CfxGraphStateData::default();
                graph.set_dash_array(vec![3.0, 3.0]);
                graph.set_line_width(width);
                self.draw_path(
                    &path,
                    matrix,
                    Some(&graph),
                    0,
                    argb,
                    CfxFillRenderOptions::winding_options(),
                );
            }
            BorderStyle::Beveled | BorderStyle::Inset => {
                let mut graph = CfxGraphStateData::default();
                graph.set_line_width(h);
                let lt = polygon(&[
                    CfxPointF::new(l + h, b + h),
                    CfxPointF::new(l + h, t - h),
                    CfxPointF::new(r - h, t - h),
                    CfxPointF::new(r - width, t - width),
                    CfxPointF::new(l + width, t - width),
                    CfxPointF::new(l + width, b + width),
                    CfxPointF::new(l + h, b + h),
                ]);
                self.draw_path(
                    &lt,
                    matrix,
                    Some(&graph),
                    left_top.to_fx_color(alpha),
                    0,
                    CfxFillRenderOptions::even_odd_options(),
                );
                let rb = polygon(&[
                    CfxPointF::new(r - h, t - h),
                    CfxPointF::new(r - h, b + h),
                    CfxPointF::new(l + h, b + h),
                    CfxPointF::new(l + width, b + width),
                    CfxPointF::new(r - width, b + width),
                    CfxPointF::new(r - width, t - width),
                    CfxPointF::new(r - h, t - h),
                ]);
                self.draw_path(
                    &rb,
                    matrix,
                    Some(&graph),
                    right_bottom.to_fx_color(alpha),
                    0,
                    CfxFillRenderOptions::even_odd_options(),
                );
                let mut path = CfxPath::new();
                path.append_rect(l, b, r, t);
                path.append_rect(l + h, b + h, r - h, t - h);
                self.draw_path(
                    &path,
                    matrix,
                    Some(&graph),
                    argb,
                    0,
                    CfxFillRenderOptions::even_odd_options(),
                );
            }
            BorderStyle::Underline => {
                self.stroke_line(
                    matrix,
                    CfxPointF::new(l, b + h),
                    CfxPointF::new(r, b + h),
                    argb,
                    width,
                );
            }
        }
    }
}

pub struct AggPwlDraw<'driver, 'bitmap, 'storage> {
    driver: &'driver mut CfxAggDeviceDriver<'bitmap, 'storage>,
    _options: CpdfRenderOptions,
}
impl<'driver, 'bitmap, 'storage> AggPwlDraw<'driver, 'bitmap, 'storage> {
    pub fn new(driver: &'driver mut CfxAggDeviceDriver<'bitmap, 'storage>) -> Self {
        Self {
            driver,
            _options: CpdfRenderOptions::default(),
        }
    }
}
impl<'driver, 'bitmap, 'storage> AggPwlDraw<'driver, 'bitmap, 'storage> {
    pub fn with_options(
        driver: &'driver mut CfxAggDeviceDriver<'bitmap, 'storage>,
        options: CpdfRenderOptions,
    ) -> Self {
        Self {
            driver,
            _options: options,
        }
    }
}
impl PwlDraw for AggPwlDraw<'_, '_, '_> {
    #[cfg(feature = "xfa")]
    fn draw_xfa_widget(
        &mut self,
        widget: &crate::fpdfsdk::fpdfxfa::cpdfxfa_widget::CpdfXfaWidget,
        matrix: CfxMatrix,
    ) -> bool {
        let mut graphics =
            crate::xfa::fgas::graphics::cfgas_gegraphics::CfgasGeGraphics::new(self.driver);
        widget.on_draw(&mut graphics, matrix).unwrap_or(false)
    }
    #[cfg(feature = "xfa")]
    fn draw_xfa_page(
        &mut self,
        page: &crate::fpdfsdk::fpdfxfa::cpdfxfa_page::CpdfXfaPage,
        focus: Option<&std::rc::Rc<crate::fpdfsdk::cpdfsdk_annot::CpdfSdkAnnot>>,
        matrix: CfxMatrix,
        clip: crate::fxcrt::fx_coordinates::FxRect,
    ) -> bool {
        let mut graphics =
            crate::xfa::fgas::graphics::cfgas_gegraphics::CfgasGeGraphics::new(self.driver);
        page.draw_focus_annot(&mut graphics, focus, matrix, clip)
            .unwrap_or(false)
    }

    fn draw_annotation_ap(
        &mut self,
        page: &crate::fpdfsdk::cpdfsdk_pageview::SdkPage,
        dict: &crate::fpdfapi::parser::cpdf_object::ObjectRef,
        mode: crate::fpdfdoc::cpdf_annot::AppearanceMode,
        matrix: CfxMatrix,
    ) -> bool {
        let Some(geometry) = page.geometry_page() else {
            return false;
        };
        let mut annot = crate::fpdfdoc::cpdf_annot::CpdfAnnot::new(dict.clone(), &page.page_data());
        // The SDK owner already checks whether a popup is open. This temporary
        // render wrapper must retain that decision rather than reset it.
        if annot.subtype() == crate::fpdfdoc::cpdf_annot::Subtype::Popup {
            annot.set_open_state(true);
        }
        annot.draw_appearance_with_options(
            &geometry,
            self.driver,
            matrix,
            mode,
            // SDK appearance painting calls CPDF_Annot::DrawAppearance,
            // whose render context uses default options. Page rendering
            // applies its own options before the interactive overlay.
            None,
        )
    }
    fn save_state(&mut self) {
        self.driver.save_state();
    }
    fn restore_state(&mut self) {
        self.driver.restore_state(false);
    }
    fn set_clip_rect(&mut self, rect: CfxFloatRect, matrix: CfxMatrix) {
        self.driver
            .set_clip_rect(matrix.transform_float_rect(rect).get_outer_rect());
    }
    fn draw_path(
        &mut self,
        path: &CfxPath,
        matrix: CfxMatrix,
        graph: Option<&CfxGraphStateData>,
        fill: u32,
        stroke: u32,
        options: CfxFillRenderOptions,
    ) -> bool {
        draw_path_agg(
            self.driver,
            path,
            Some(matrix),
            graph,
            fill,
            stroke,
            options,
        )
    }
    fn draw_text(
        &mut self,
        point: CfxPointF,
        font: &CpdfFont,
        size: f32,
        matrix: CfxMatrix,
        bytes: &[u8],
        fill: u32,
    ) -> bool {
        // CPDF_TextRenderer::DrawTextString: retain font codes and PDF metrics;
        // no Unicode shaping or replacement text renderer is introduced.
        if matches!(font, CpdfFont::Type3(_)) {
            return false;
        }
        let count = font.count_char(bytes);
        if count == 0 {
            return true;
        }
        let (mut codes, mut positions) = (Vec::with_capacity(count), Vec::with_capacity(count));
        let (mut offset, mut advance) = (0, 0.0);
        for _ in 0..count {
            let code = font.get_next_char(bytes, &mut offset);
            codes.push(code);
            positions.push(advance);
            advance += font.get_char_width(code) as f32 * size / 1000.0;
        }
        let origin = matrix.transform(point);
        let mut transform = matrix;
        transform.e = origin.x;
        transform.f = origin.y;
        draw_pdf_normal_text_agg(
            self.driver,
            &codes,
            &positions,
            font,
            size,
            transform,
            fill,
            // CPWL_EditImpl::DrawTextString creates normal, ClearType
            // render options independently of the page/overlay flags.
            &CpdfRenderOptions::default(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxge::dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat};
    #[test]
    fn widget_border_and_clip_state_use_the_existing_agg_pipeline() {
        let mut bitmap = CfxDibitmap::create(24, 24, FxdibFormat::Bgra, 0).unwrap();
        {
            let mut driver = CfxAggDeviceDriver::new(&mut bitmap, false, None);
            let mut draw = AggPwlDraw::new(&mut driver);
            draw.save_state();
            draw.set_clip_rect(
                CfxFloatRect::new(0.0, 0.0, 12.0, 24.0),
                CfxMatrix::default(),
            );
            draw.fill_rect(
                CfxMatrix::default(),
                CfxFloatRect::new(0.0, 0.0, 24.0, 24.0),
                0xffff0000,
            );
            draw.restore_state();
            draw.draw_border(
                CfxMatrix::default(),
                CfxFloatRect::new(2.0, 2.0, 22.0, 22.0),
                2.0,
                CfxColor::make_gray(0.0),
                CfxColor::make_gray(1.0),
                CfxColor::make_gray(0.5),
                BorderStyle::Solid,
                255,
            );
        }
        assert_eq!(
            &bitmap.scanline(12).unwrap()[6 * 4..6 * 4 + 4],
            &[0, 0, 255, 255]
        );
        assert_eq!(
            &bitmap.scanline(12).unwrap()[16 * 4..16 * 4 + 4],
            &[0, 0, 0, 0]
        );
        assert_eq!(
            &bitmap.scanline(2).unwrap()[16 * 4..16 * 4 + 4],
            &[0, 0, 0, 255]
        );
    }
}
