// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Node-owned ImageLayoutData/ImageEditData from cxfa_node.cpp. Bitmap ownership
//! and DPI outlive individual layout fragments. No node-storage borrow survives
//! named-image, stream or decoder callbacks.
use super::*;
use crate::{
    fxge::dib::cfx_dibitmap::CfxDibitmap,
    xfa::fxfa::cxfa_ffimage::{load_image_data_with_metadata, ImageResource, XfaImageOwner},
};

pub(super) struct NodeImageData {
    kind: XfaFfWidgetType,
    bitmap: Option<Rc<CfxDibitmap>>,
    dpi: (i32, i32),
    named: bool,
}
impl NodeImageData {
    fn new(kind: XfaFfWidgetType) -> Self {
        Self {
            kind,
            bitmap: None,
            dpi: (0, 0),
            named: false,
        }
    }
}
impl CxfaNode {
    fn init_image_data(&self, kind: XfaFfWidgetType) -> bool {
        if !self.is_alive() {
            return false;
        }
        if let Some(existing) = self
            .0
            .with_value(|node| node.image_data.as_ref().map(|data| data.kind))
        {
            return existing == kind;
        }
        // InitLayoutData selects the real widget type. This call can prepare UI
        // properties, so perform it before borrowing the node storage.
        if self.get_ff_widget_type() != kind || !self.is_alive() {
            return false;
        }
        self.0.with_value_mut(|node| {
            let data = node
                .image_data
                .get_or_insert_with(|| Box::new(NodeImageData::new(kind)));
            data.kind == kind
        })
    }
    fn image_resource(&self, kind: XfaFfWidgetType) -> Option<ImageResource> {
        if !self.is_alive() {
            return None;
        }
        self.0.with_value(|node| {
            let data = node.image_data.as_ref().filter(|data| data.kind == kind)?;
            Some(ImageResource {
                bitmap: data.bitmap.clone()?,
                dpi: data.dpi,
                named: data.named,
            })
        })
    }
    fn image_dpi(&self, kind: XfaFfWidgetType) -> (i32, i32) {
        if !self.is_alive() {
            return (0, 0);
        }
        self.0.with_value(|node| {
            node.image_data
                .as_ref()
                .filter(|data| data.kind == kind)
                .map_or((0, 0), |data| data.dpi)
        })
    }
    fn set_image_bitmap(&self, kind: XfaFfWidgetType, bitmap: Option<Rc<CfxDibitmap>>) -> bool {
        if !self.is_alive() {
            return false;
        }
        // Destruction of an unloaded widget must not create UI/layout state.
        if bitmap.is_none() && self.0.with_value(|node| node.image_data.is_none()) {
            return true;
        }
        if !self.init_image_data(kind) {
            return false;
        }
        let old = self.0.with_value_mut(|node| {
            let data = node.image_data.as_mut().unwrap();
            if data
                .bitmap
                .as_ref()
                .zip(bitmap.as_ref())
                .is_some_and(|(a, b)| Rc::ptr_eq(a, b))
            {
                return None;
            }
            std::mem::replace(&mut data.bitmap, bitmap)
        });
        drop(old);
        true
    }
    fn load_node_image(&self, owner: &dyn XfaImageOwner, kind: XfaFfWidgetType) -> bool {
        if !self.init_image_data(kind) {
            return false;
        }
        if self.image_resource(kind).is_some() {
            return true;
        }
        let Some(image) = self
            .get_form_value_if_exists()
            .and_then(|value| value.value_get_image_if_exists())
        else {
            return false;
        };
        let (mut named, mut dpi) = self.0.with_value(|node| {
            let data = node.image_data.as_ref().unwrap();
            (data.named, data.dpi)
        });
        let resource = load_image_data_with_metadata(owner, &image, &mut named, &mut dpi);
        if !self.is_alive() {
            return false;
        }
        // Native LoadImageData writes its out-parameters and then SetBitmap
        // after callbacks return. A nested load/invalidation therefore finishes
        // before this outer assignment; do not keep a mutable lease across it.
        let bitmap = resource.map(|value| value.bitmap);
        let loaded = bitmap.is_some();
        let old = self.0.with_value_mut(|node| {
            let data = node.image_data.as_mut().unwrap();
            data.named = named;
            data.dpi = dpi;
            if data
                .bitmap
                .as_ref()
                .zip(bitmap.as_ref())
                .is_some_and(|(a, b)| Rc::ptr_eq(a, b))
            {
                return None;
            }
            std::mem::replace(&mut data.bitmap, bitmap)
        });
        drop(old);
        loaded
    }
    pub fn load_layout_image(&self, owner: &dyn XfaImageOwner) -> bool {
        self.load_node_image(owner, XfaFfWidgetType::Image)
    }
    pub fn load_edit_image(&self, owner: &dyn XfaImageOwner) -> bool {
        self.load_node_image(owner, XfaFfWidgetType::ImageEdit)
    }
    pub fn get_layout_image(&self) -> Option<Rc<CfxDibitmap>> {
        self.get_layout_image_resource()
            .map(|resource| resource.bitmap)
    }
    pub fn get_edit_image(&self) -> Option<Rc<CfxDibitmap>> {
        self.get_edit_image_resource()
            .map(|resource| resource.bitmap)
    }
    pub fn get_layout_image_resource(&self) -> Option<ImageResource> {
        self.image_resource(XfaFfWidgetType::Image)
    }
    pub fn get_edit_image_resource(&self) -> Option<ImageResource> {
        self.image_resource(XfaFfWidgetType::ImageEdit)
    }
    pub fn get_layout_image_dpi(&self) -> (i32, i32) {
        self.image_dpi(XfaFfWidgetType::Image)
    }
    pub fn get_edit_image_dpi(&self) -> (i32, i32) {
        self.image_dpi(XfaFfWidgetType::ImageEdit)
    }
    pub fn set_layout_image(&self, bitmap: Option<Rc<CfxDibitmap>>) -> bool {
        self.set_image_bitmap(XfaFfWidgetType::Image, bitmap)
    }
    pub fn set_edit_image(&self, bitmap: Option<Rc<CfxDibitmap>>) -> bool {
        self.set_image_bitmap(XfaFfWidgetType::ImageEdit, bitmap)
    }
}
