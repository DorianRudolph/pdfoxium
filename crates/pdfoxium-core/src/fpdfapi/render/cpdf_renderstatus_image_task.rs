// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! The retained CPDF_ImageRenderer loading/DIB continuation used by
//! ContinueSingleObject. Complex native offscreen/pattern branches stay on their
//! source synchronous path; normal image sampling retains its real engine.
use super::super::*;
use crate::{
    fpdfapi::page::{
        cpdf_image::{CpdfImageBitmap, CpdfImageLoadContext},
        cpdf_imageloader::CpdfImageLoader,
    },
    fxcrt::pauseindicator_iface::PauseIndicatorIface,
    fxge::agg::cfx_agg_imagerenderer::CfxAggImageRenderTask,
};
pub(super) enum ImageProgress {
    Pending,
    Done,
}
enum ImageState {
    Loading,
    Painting(CfxAggImageRenderTask<CpdfImageBitmap>),
    Done,
}
pub(super) struct ProgressiveImage {
    state: ImageState,
    clip: Option<CfxAggClipRgn>,
}
struct Yield(bool);
impl PauseIndicatorIface for Yield {
    fn need_to_pause_now(&mut self) -> bool {
        let pause = self.0;
        self.0 = true;
        pause
    }
}
impl ProgressiveImage {
    pub(super) fn start(
        object: &CpdfImageObject,
        driver: &mut CfxAggDeviceDriver<'_, '_>,
        matrix: CfxMatrix,
        options: &CpdfRenderOptions,
        resources: RenderResources<'_>,
    ) -> Self {
        let mut task = Self {
            state: ImageState::Done,
            clip: driver.clip_region_snapshot(),
        };
        let Some(image) = object.image() else {
            return task;
        };
        if !options.check_ocg_dict_visible(image.oc().as_ref())
            || get_unit_rect(object.matrix() * matrix).is_none()
        {
            return task;
        }
        if resources.image_cache.start_get_cached_bitmap(
            image,
            CpdfImageLoadContext {
                page_data: resources.data,
                form_resources: resources.form_resources,
                page_resources: resources.page_resources,
                std_cs: resources.std_cs,
                group_family: resources.group_family,
                load_mask: resources.load_mask,
                max_size: image_decode_size(object.matrix() * matrix),
            },
        ) {
            task.state = ImageState::Loading;
        } else {
            task.loaded(object, driver, matrix, options, resources);
        }
        task
    }
    fn loaded(
        &mut self,
        object: &CpdfImageObject,
        driver: &mut CfxAggDeviceDriver<'_, '_>,
        object_to_device: CfxMatrix,
        options: &CpdfRenderOptions,
        resources: RenderResources<'_>,
    ) {
        let mut loader = CpdfImageLoader::from_cached_planes(
            resources.image_cache.detach_current_bitmap(),
            resources.image_cache.detach_current_mask(),
            resources.image_cache.current_matte_color(),
        );
        let Some(image) = object.image() else { return };
        let matrix = object.matrix() * object_to_device;
        if options.color_mode_is(RenderColorMode::Alpha) && loader.mask().is_none() {
            if let Some(source) = loader.bitmap_ref() {
                start_bitmap_alpha(
                    driver,
                    source,
                    object.base().general_state().get_fill_alpha(),
                    matrix,
                );
            }
            return;
        }
        if let Some(transfer) = object_transfer_function(object, resources) {
            if !transfer.identity() {
                loader.translate_image(transfer);
            }
        }
        let Some(mut source) = loader.bitmap_ref() else {
            return;
        };
        let is_mask = source.is_mask_format();
        if !is_mask && options.color_mode_is(RenderColorMode::Gray) {
            let Some(mut gray) = source.realize() else {
                return;
            };
            gray.convert_color_scale(true);
            source = Rc::new(CpdfImageBitmap::Bitmap(gray));
        }
        let color = if is_mask {
            object_color(object, true, options, resources)
        } else {
            0
        };
        let mut resample = crate::fxge::dib::fx_dib::FxdibResampleOptions {
            halftone: options.options().force_halftone,
            no_smoothing: options.options().no_image_smooth,
            interpolate_bilinear: !options.options().no_image_smooth && image.is_interpol(),
            ..Default::default()
        };
        let alpha = object.base().general_state().get_fill_alpha();
        if let Some(mask) = loader.mask_ref() {
            draw_masked_image(
                driver,
                source,
                mask,
                matrix,
                resample,
                loader.matte_color(),
                alpha,
            );
            return;
        }
        if is_mask
            && object
                .base()
                .color_state()
                .get_fill_color()
                .is_some_and(|color| color.is_pattern() && color.get_pattern().is_some())
        {
            draw_pattern_image(
                object,
                source,
                driver,
                matrix,
                object_to_device,
                options,
                resources,
                0,
                resample,
                loader.matte_color(),
            );
            return;
        }
        let bpp = crate::fxge::dib::fx_dib::get_bpp_from_format(source.format());
        if bpp > 1 {
            let Some(size) = (bpp as usize / 8)
                .checked_mul(source.width() as usize)
                .and_then(|size| size.checked_mul(source.height() as usize))
            else {
                return;
            };
            if size > 60_000_000 && !resample.halftone {
                resample.interpolate_bilinear = true;
            }
        }
        let blend = image_blend_type(object, resources.data, resources.page_resources);
        // object_color already encoded constant stencil opacity in color.
        let bitmap_alpha = if is_mask { 1.0 } else { alpha };
        self.state = ImageState::Painting(
            driver
                .start_dibits_with_blend(source, bitmap_alpha, color, matrix, resample, blend)
                .into_task(),
        );
    }
    pub(super) fn continue_render(
        &mut self,
        object: &CpdfImageObject,
        driver: &mut CfxAggDeviceDriver<'_, '_>,
        matrix: CfxMatrix,
        options: &CpdfRenderOptions,
        resources: RenderResources<'_>,
    ) -> ImageProgress {
        driver.restore_clip_region(self.clip.clone());
        if matches!(self.state, ImageState::Loading) {
            let mut pause = Yield(false);
            if resources.image_cache.continue_load(Some(&mut pause)) {
                return ImageProgress::Pending;
            }
            self.state = ImageState::Done;
            self.loaded(object, driver, matrix, options, resources);
        }
        if let ImageState::Painting(task) = &mut self.state {
            let mut pause = Yield(false);
            if driver.continue_dibits_task(task, Some(&mut pause)) {
                return ImageProgress::Pending;
            }
        }
        self.state = ImageState::Done;
        ImageProgress::Done
    }
}
