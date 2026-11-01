// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! ImageEdit's connected image-cache update and image-rectangle behavior.
//! Concrete lifecycle/input is implemented by cxfa_ffimageedit_widget on the
//! shared FFField and actual FWL PictureBox.
use super::{
    cxfa_ffimage::{ImageResource, XfaImageOwner},
    parser::cxfa_node::CxfaNode,
};
use crate::fxcrt::fx_coordinates::CfxRectF;
/// UpdateFWLData clears the old image before asking the document owner to load
/// replacement data, preserving invalidation and callback ordering.
pub fn update_image_edit_data(owner: &dyn XfaImageOwner, node: &CxfaNode) -> bool {
    node.set_edit_image(None);
    node.load_edit_image(owner)
}
pub fn image_edit_rect(ui_rect: CfxRectF, ui_margin: CfxRectF) -> CfxRectF {
    let mut rect = ui_rect;
    rect.deflate(
        ui_margin.left,
        ui_margin.top,
        ui_margin.width,
        ui_margin.height,
    );
    rect
}

/// ImageEdit's bitmap draw after the common field border and caption draw.
pub fn draw_edit_image(
    graphics: &mut crate::xfa::fgas::graphics::cfgas_gegraphics::CfgasGeGraphics<'_, '_, '_>,
    node: &CxfaNode,
    resource: &ImageResource,
    normal_widget_rect: CfxRectF,
    matrix: crate::fxcrt::fx_coordinates::CfxMatrix,
) {
    use super::fxfa_basic::XfaAttributeValue;
    let aspect = node
        .get_form_value_if_exists()
        .and_then(|value| value.value_get_image_if_exists())
        .map_or(XfaAttributeValue::Fit, |image| image.image_get_aspect());
    let (horizontal, vertical) = super::cxfa_ffimage::image_alignment(node);
    super::cxfa_imagerenderer::draw_image(
        graphics,
        normal_widget_rect,
        matrix,
        resource.bitmap.clone(),
        aspect,
        resource.dpi,
        horizontal,
        vertical,
    );
}
