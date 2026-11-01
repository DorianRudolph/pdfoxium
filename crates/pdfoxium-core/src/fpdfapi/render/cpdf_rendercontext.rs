// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Borrowed layers and static rendering from
//! `core/fpdfapi/render/cpdf_rendercontext.{h,cpp}`.

use super::cpdf_renderoptions::CpdfRenderOptions;
use crate::fpdfapi::page::cpdf_page::CpdfPage;
use crate::fpdfapi::page::cpdf_pageobject::PageObject;
use crate::fpdfapi::page::cpdf_pageobjectholder::CpdfPageObjectHolder;
use crate::fxcrt::fx_coordinates::{CfxMatrix, FxRect};
use crate::fxge::agg::cfx_agg_devicedriver::CfxAggDeviceDriver;

#[derive(Clone, Copy)]
pub struct Layer<'a> {
    pub object_holder: &'a CpdfPageObjectHolder,
    pub matrix: CfxMatrix,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
    use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
    use crate::fxge::dib::fx_dib::FxdibFormat;

    #[test]
    fn background_stops_before_object_and_later_layers_and_concatenates_device_matrix() {
        let objects = CpdfIndirectObjectHolder::new();
        let stream = objects
            .borrow_mut()
            .new_indirect_stream_from_bytes(b"1 0 0 rg 0 0 1 1 re f 0 0 1 rg 2 0 1 1 re f");
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(
            b"Contents",
            CpdfReference::new(&objects, stream.borrow().obj_num()),
        );
        let mut page = CpdfPage::new(retain_object(dict)).unwrap();
        page.parse_path_content();
        let stop = page.holder().iter().nth(1).unwrap().as_ref();
        let mut context = CpdfRenderContext::new(&page);
        context.append_layer(page.holder(), CfxMatrix::default());
        context.append_layer(page.holder(), CfxMatrix::new(1.0, 0.0, 0.0, 1.0, 5.0, 0.0));
        let mut bitmap = CfxDibitmap::create(8, 1, FxdibFormat::Bgra, 0).unwrap();
        {
            let mut driver = CfxAggDeviceDriver::new(&mut bitmap, false, None);
            assert!(context.get_background_to_device(
                &mut driver,
                stop,
                None,
                CfxMatrix::new(1.0, 0.0, 0.0, 1.0, 1.0, 0.0)
            ));
            assert_eq!(driver.clip_box(), FxRect::new(0, 0, 8, 1));
        }
        let row = bitmap.scanline(0).unwrap();
        assert_eq!(&row[4..8], &[0, 0, 255, 255]);
        assert_eq!(&row[12..16], &[255; 4]);
        assert_eq!(&row[24..28], &[255; 4]);
    }
}

pub struct CpdfRenderContext<'a> {
    page: &'a CpdfPage,
    page_resources: Option<&'a crate::fpdfapi::parser::cpdf_object::ObjectRef>,
    layers: Vec<Layer<'a>>,
}

impl<'a> CpdfRenderContext<'a> {
    pub fn new(page: &'a CpdfPage) -> Self {
        Self::with_page_resources(page, page.holder().page_resources())
    }
    pub fn with_page_resources(
        page: &'a CpdfPage,
        page_resources: Option<&'a crate::fpdfapi::parser::cpdf_object::ObjectRef>,
    ) -> Self {
        Self {
            page,
            page_resources,
            layers: Vec::new(),
        }
    }
    pub fn page(&self) -> &CpdfPage {
        self.page
    }
    pub fn page_resources(&self) -> Option<&crate::fpdfapi::parser::cpdf_object::ObjectRef> {
        self.page_resources
    }
    pub fn append_layer(&mut self, holder: &'a CpdfPageObjectHolder, matrix: CfxMatrix) {
        self.layers.push(Layer {
            object_holder: holder,
            matrix,
        });
    }
    pub fn layers(&self) -> &[Layer<'a>] {
        &self.layers
    }
    pub fn render(
        &self,
        driver: &mut CfxAggDeviceDriver<'_, '_>,
        stop_object: Option<&dyn PageObject>,
        options: Option<&CpdfRenderOptions>,
        last_matrix: Option<CfxMatrix>,
    ) -> bool {
        super::cpdf_renderstatus::render_context_layers(
            self,
            driver,
            stop_object,
            options,
            last_matrix,
        )
    }
    pub fn get_background_to_device(
        &self,
        driver: &mut CfxAggDeviceDriver<'_, '_>,
        object: &dyn PageObject,
        options: Option<&CpdfRenderOptions>,
        matrix: CfxMatrix,
    ) -> bool {
        driver.fill_rect(
            FxRect::new(0, 0, driver.pixel_width(), driver.pixel_height()),
            0xffffffff,
        );
        self.render(driver, Some(object), options, Some(matrix))
    }
}
