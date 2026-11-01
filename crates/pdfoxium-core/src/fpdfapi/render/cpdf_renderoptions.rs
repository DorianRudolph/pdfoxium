// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fpdfapi/render/cpdf_renderoptions.{h,cpp}`.

use std::rc::Rc;

use crate::fpdfapi::page::cpdf_occontext::CpdfOcContext;
use crate::fpdfapi::page::cpdf_pageobject::{PageObject, PageObjectType};
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fxge::dib::fx_dib::{argb_a, argb_b, argb_encode, argb_g, argb_r, rgb_to_gray, FxArgb};

const CACHE_SIZE_LIMIT_BYTES: u32 = 100 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum RenderColorMode {
    #[default]
    Normal,
    Gray,
    Alpha,
    ForcedColor,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RenderOptionsFlags {
    pub clear_type: bool,
    pub no_native_text: bool,
    pub force_halftone: bool,
    pub rect_aa: bool,
    pub break_for_masks: bool,
    pub no_text_smooth: bool,
    pub no_path_smooth: bool,
    pub no_image_smooth: bool,
    pub limited_image_cache: bool,
    pub convert_fill_to_stroke: bool,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct RenderColorScheme {
    pub path_fill_color: FxArgb,
    pub path_stroke_color: FxArgb,
    pub text_fill_color: FxArgb,
    pub text_stroke_color: FxArgb,
}

#[derive(Clone)]
pub struct CpdfRenderOptions {
    color_mode: RenderColorMode,
    draw_annots: bool,
    printing: bool,
    options: RenderOptionsFlags,
    color_scheme: RenderColorScheme,
    oc_context: Option<Rc<CpdfOcContext>>,
}

impl Default for CpdfRenderOptions {
    fn default() -> Self {
        Self {
            color_mode: RenderColorMode::Normal,
            draw_annots: false,
            printing: false,
            options: RenderOptionsFlags {
                clear_type: true,
                ..Default::default()
            },
            color_scheme: RenderColorScheme::default(),
            oc_context: None,
        }
    }
}

impl CpdfRenderOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn translate_color(&self, argb: FxArgb) -> FxArgb {
        if matches!(
            self.color_mode,
            RenderColorMode::Normal | RenderColorMode::Alpha
        ) {
            return argb;
        }
        let gray = rgb_to_gray(
            argb_r(argb).into(),
            argb_g(argb).into(),
            argb_b(argb).into(),
        );
        argb_encode(argb_a(argb).into(), gray as u32, gray as u32, gray as u32)
    }

    pub fn translate_object_fill_color(&self, argb: FxArgb, kind: PageObjectType) -> FxArgb {
        if self.color_mode != RenderColorMode::ForcedColor {
            return self.translate_color(argb);
        }
        match kind {
            PageObjectType::Path => self.color_scheme.path_fill_color,
            PageObjectType::Text => self.color_scheme.text_fill_color,
            _ => argb,
        }
    }

    pub fn translate_object_stroke_color(&self, argb: FxArgb, kind: PageObjectType) -> FxArgb {
        if self.color_mode != RenderColorMode::ForcedColor {
            return self.translate_color(argb);
        }
        match kind {
            PageObjectType::Path => self.color_scheme.path_stroke_color,
            PageObjectType::Text => self.color_scheme.text_stroke_color,
            _ => argb,
        }
    }

    pub fn set_color_scheme(&mut self, scheme: RenderColorScheme) {
        self.color_scheme = scheme;
    }

    pub fn set_color_mode(&mut self, mode: RenderColorMode) {
        self.color_mode = mode;
    }

    pub fn color_mode_is(&self, mode: RenderColorMode) -> bool {
        self.color_mode == mode
    }

    pub fn options(&self) -> &RenderOptionsFlags {
        &self.options
    }

    pub fn options_mut(&mut self) -> &mut RenderOptionsFlags {
        &mut self.options
    }

    pub fn cache_size_limit(&self) -> u32 {
        CACHE_SIZE_LIMIT_BYTES
    }

    pub fn check_ocg_dict_visible(&self, ocg: Option<&ObjectRef>) -> bool {
        self.oc_context
            .as_ref()
            .is_none_or(|context| context.check_ocg_dict_visible(ocg))
    }

    pub fn check_page_object_visible(&self, object: &dyn PageObject) -> bool {
        self.oc_context
            .as_ref()
            .is_none_or(|context| context.check_page_object_visible(object))
    }

    pub fn set_draw_annots(&mut self, draw: bool) {
        self.draw_annots = draw;
    }

    pub fn draw_annots(&self) -> bool {
        self.draw_annots
    }

    pub fn set_printing(&mut self, printing: bool) {
        self.printing = printing;
    }

    pub fn printing(&self) -> bool {
        self.printing
    }

    pub fn set_oc_context(&mut self, context: Option<Rc<CpdfOcContext>>) {
        self.oc_context = context;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::page::cpdf_pathobject::CpdfPathObject;

    #[test]
    fn modes_preserve_alpha_and_use_forced_object_colors() {
        let mut options = CpdfRenderOptions::new();
        assert!(options.options().clear_type);
        assert_eq!(options.cache_size_limit(), 104_857_600);
        let red = 0x7fff_0000;
        assert_eq!(options.translate_color(red), red);
        options.set_color_mode(RenderColorMode::Gray);
        assert_eq!(options.translate_color(red), 0x7f4c_4c4c);
        options.set_color_mode(RenderColorMode::Alpha);
        assert_eq!(options.translate_color(red), red);
        options.set_color_scheme(RenderColorScheme {
            path_fill_color: 1,
            path_stroke_color: 2,
            text_fill_color: 3,
            text_stroke_color: 4,
        });
        options.set_color_mode(RenderColorMode::ForcedColor);
        assert_eq!(
            options.translate_object_fill_color(red, PageObjectType::Path),
            1
        );
        assert_eq!(
            options.translate_object_stroke_color(red, PageObjectType::Text),
            4
        );
        assert_eq!(
            options.translate_object_fill_color(red, PageObjectType::Image),
            red
        );
        assert!(options.check_ocg_dict_visible(None));
        assert!(options.check_page_object_visible(&CpdfPathObject::new(0)));
    }
}
