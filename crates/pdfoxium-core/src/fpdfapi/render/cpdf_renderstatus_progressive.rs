// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Owned part of CPDF_RenderStatus retained across progressive calls. Device
//! borrows are confined to one object; shared clip masks retain their identity.
use super::*;
#[path = "cpdf_renderstatus_image_task.rs"]
mod image;
use image::{ImageProgress, ProgressiveImage};
#[derive(Default)]
pub(crate) struct ProgressiveRenderStatus {
    clip: ClipPathCache,
    image: Option<ProgressiveImage>,
}
impl ProgressiveRenderStatus {
    pub(crate) fn is_pending(&self) -> bool {
        self.image.is_some()
    }
    pub(crate) fn render_object(
        &mut self,
        context: &CpdfRenderContext<'_>,
        driver: &mut CfxAggDeviceDriver<'_, '_>,
        options: &CpdfRenderOptions,
        layer_index: usize,
        object: &dyn PageObject,
    ) -> bool {
        let Some(layer) = context.layers().get(layer_index) else {
            return false;
        };
        let stopped = Cell::new(false);
        let mut resources = RenderResources::new(context, &stopped);
        resources.progressive_root = true;
        resources.transparency = layer.object_holder.transparency();
        driver.save_state();
        let success = if let Some(image_object) = object.as_any().downcast_ref::<CpdfImageObject>()
        {
            if self.image.is_none() {
                if !options.check_page_object_visible(object) {
                    driver.restore_state(false);
                    return true;
                }
                let clip = object.base().clip_path();
                if !self.clip.restore(clip, driver) {
                    self.clip.clear();
                    let clipped = process_clip_path(
                        clip,
                        driver,
                        layer.matrix,
                        options,
                        resources,
                        resources.page_resources,
                        0,
                        &[],
                    );
                    self.clip.record(clip, driver, clipped);
                }
                if let Some(success) = process_transparency(
                    object,
                    driver,
                    layer.matrix,
                    options,
                    resources,
                    0,
                    None,
                    &[],
                ) {
                    driver.restore_state(false);
                    return success;
                }
                self.image = Some(ProgressiveImage::start(
                    image_object,
                    driver,
                    layer.matrix,
                    options,
                    resources,
                ));
            }
            if matches!(
                self.image.as_mut().unwrap().continue_render(
                    image_object,
                    driver,
                    layer.matrix,
                    options,
                    resources
                ),
                ImageProgress::Done
            ) {
                self.image = None;
            }
            true
        } else {
            process_single_object(
                object,
                driver,
                layer.matrix,
                options,
                resources,
                0,
                None,
                &[],
                Some(&mut self.clip),
            )
        };
        driver.restore_state(false);
        if self.image.is_none()
            && object.as_any().is::<CpdfImageObject>()
            && options.options().limited_image_cache
        {
            resources
                .image_cache
                .cache_optimization(options.cache_size_limit() as i32);
        }
        success
    }
}
