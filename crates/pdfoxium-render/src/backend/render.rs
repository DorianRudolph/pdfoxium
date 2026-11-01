// Copyright 2026 The PDFium Authors. BSD-3-Clause; see upstream/pdfium/LICENSE.
use crate::backend::prelude::*;
use pdfoxium_core::{
    fpdfapi::{
        page::cpdf_occontext::{CpdfOcContext, UsageType},
        render::{
            cpdf_pagerendercontext::CpdfPageRenderContext,
            cpdf_progressiverenderer::RenderStep,
            cpdf_renderoptions::{CpdfRenderOptions, RenderColorMode},
        },
    },
    fpdfsdk::{
        cpdfsdk_formfillenvironment::CpdfSdkFormFillEnvironment, cpdfsdk_pageview::SdkPage,
        pwl::pwl_draw::AggPwlDraw,
    },
    fxcrt::fx_coordinates::{CfxFloatRect, FxRect},
    fxge::agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PdfPageOrientation {
    Portrait,
    Landscape,
}
impl PdfPageOrientation {
    pub(crate) fn from_width_and_height(w: PdfPoints, h: PdfPoints) -> Self {
        if w > h {
            Self::Landscape
        } else {
            Self::Portrait
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum PdfPageRenderRotation {
    #[default]
    None,
    Degrees90,
    Degrees180,
    Degrees270,
}
impl PdfPageRenderRotation {
    pub(crate) fn as_core(self) -> i32 {
        match self {
            Self::None => 0,
            Self::Degrees90 => 1,
            Self::Degrees180 => 2,
            Self::Degrees270 => 3,
        }
    }
    pub const fn as_degrees(&self) -> f32 {
        match self {
            Self::None => 0.,
            Self::Degrees90 => 90.,
            Self::Degrees180 => 180.,
            Self::Degrees270 => 270.,
        }
    }
    pub const fn as_radians(&self) -> f32 {
        self.as_degrees().to_radians()
    }
}
impl PdfMatrix {
    pub(crate) fn as_core(&self) -> pdfoxium_core::fxcrt::fx_coordinates::CfxMatrix {
        pdfoxium_core::fxcrt::fx_coordinates::CfxMatrix::new(
            self.a(),
            self.b(),
            self.c(),
            self.d(),
            self.e(),
            self.f(),
        )
    }
}
impl PdfPage {
    pub fn rotation(&self) -> Result<PdfPageRenderRotation, PdfiumError> {
        self.check_live()?;
        Ok(match self.state.core.borrow().rotation().rem_euclid(4) {
            1 => PdfPageRenderRotation::Degrees90,
            2 => PdfPageRenderRotation::Degrees180,
            3 => PdfPageRenderRotation::Degrees270,
            _ => PdfPageRenderRotation::None,
        })
    }
    pub fn set_rotation(&mut self, rotation: PdfPageRenderRotation) -> Result<(), PdfiumError> {
        use pdfoxium_core::fpdfapi::parser::{
            cpdf_dictionary::CpdfDictionary, cpdf_scalar_object::CpdfScalarObject,
        };
        self.check_live()?;
        let mut core = self
            .state
            .core
            .try_borrow_mut()
            .map_err(|_| PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)?;
        let page =
            Rc::get_mut(&mut core).ok_or(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)?;
        let dict = page.dict();
        dict.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .ok_or(PdfiumError::PdfiumLibraryInternalError(
                crate::PdfiumInternalError::FormatError,
            ))?
            .set_new_for(
                b"Rotate",
                CpdfScalarObject::number_from_signed(rotation.as_core() * 90),
            );
        page.update_dimensions();
        self.state.text_cache.borrow_mut().take();
        Ok(())
    }
    pub fn render_into_bitmap_with_config(
        &self,
        bitmap: &mut PdfBitmap<'_>,
        config: &PdfRenderConfig,
    ) -> Result<(), PdfiumError> {
        self.check_live()?;
        let s = config.apply_to_page(self);
        if s.width <= 0 || s.height <= 0 {
            return Err(PdfiumError::ImageSizeOutOfBounds);
        }
        let page = self.state.core.borrow().clone();
        let mut options = CpdfRenderOptions::new();
        let flags = s.render_flags;
        let usage = if flags & 0x800 != 0 {
            UsageType::Print
        } else {
            UsageType::View
        };
        options.set_oc_context(Some(Rc::new(CpdfOcContext::new(
            &self.document.core.borrow(),
            usage,
        ))));
        options.set_draw_annots(flags & 1 != 0);
        options.set_printing(flags & 0x800 != 0);
        options.options_mut().clear_type = flags & 2 != 0;
        options.options_mut().no_native_text = flags & 4 != 0;
        options.options_mut().limited_image_cache = flags & 0x200 != 0;
        options.options_mut().force_halftone = flags & 0x400 != 0;
        options.options_mut().no_text_smooth = flags & 0x1000 != 0;
        options.options_mut().no_image_smooth = flags & 0x2000 != 0;
        options.options_mut().no_path_smooth = flags & 0x4000 != 0;
        if flags & 8 != 0 {
            options.set_color_mode(RenderColorMode::Gray);
        }
        let full = FxRect::new(0, 0, bitmap.width(), bitmap.height());
        let display = FxRect::new(
            s.start_x,
            s.start_y,
            s.start_x
                .checked_add(s.width)
                .ok_or(PdfiumError::ImageSizeOutOfBounds)?,
            s.start_y
                .checked_add(s.height)
                .ok_or(PdfiumError::ImageSizeOutOfBounds)?,
        );
        let matrix = if s.do_render_form_data {
            page.display_matrix_for_rect(display, s.rotate)
        } else {
            let mut matrix = page.display_matrix_for_float_rect(
                CfxFloatRect::new(0., 0., page.width(), page.height()),
                0,
            );
            matrix.concat(s.matrix);
            matrix
        };
        let clip = if s.do_render_form_data {
            full
        } else {
            FxRect::new(
                s.clipping.left as i32,
                s.clipping.top as i32,
                s.clipping.right as i32,
                s.clipping.bottom as i32,
            )
        };
        let mut context = CpdfPageRenderContext::new(
            page.clone(),
            options.clone(),
            matrix,
            clip,
            s.is_reversed_byte_order_flag_set,
        );
        bitmap.set_reversed(s.is_reversed_byte_order_flag_set);
        let mut dib = bitmap.dib_mut()?;
        if s.do_clear_bitmap_before_rendering {
            dib.clear(s.clear_color);
        }
        context.start();
        loop {
            match context.render_step(&mut dib) {
                RenderStep::Continue | RenderStep::CheckPause | RenderStep::MaskBreak => {}
                RenderStep::Done => break,
                RenderStep::Failed => {
                    return Err(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)
                }
            }
        }
        if s.do_render_form_data {
            let sdk = SdkPage::new(&self.document.core, &page);
            let environment = CpdfSdkFormFillEnvironment::new(&self.document.core, None);
            if let Some(highlights) = s.form_field_highlight {
                let form = environment.interactive_form();
                for (kind, (color, alpha)) in highlights {
                    if kind == 0 {
                        form.set_all_highlight_colors(color);
                    } else if let Some(kind) =
                        pdfoxium_core::fpdfdoc::cpdf_formfield::int_to_form_field_type(kind)
                    {
                        form.set_highlight_color(color, kind);
                    }
                    form.set_highlight_alpha(alpha);
                }
            }
            if let Some(view) = environment.get_or_create_page_view(&sdk) {
                let mut driver =
                    CfxAggDeviceDriver::new(&mut dib, s.is_reversed_byte_order_flag_set, None);
                driver.set_clip_rect(full);
                let mut device = AggPwlDraw::with_options(&mut driver, options);
                view.page_view_on_draw(&mut device, matrix, flags & 1 != 0, full);
            }
            environment.remove_page_view(&sdk);
        }
        if !dib.is_borrowed() {
            return Err(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure);
        }
        Ok(())
    }
}
