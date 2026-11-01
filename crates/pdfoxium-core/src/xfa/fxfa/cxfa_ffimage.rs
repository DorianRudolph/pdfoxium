// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Connected image-loading portion of `cxfa_ffimage`, XFA_LoadImageFromBuffer
//! (`cxfa_ffwidget.cpp`) and XFA_LoadImageData (`parser/cxfa_node.cpp`). FFWidget
//! lifecycle/status/caption/layout integration remains in the form-filler layer.
use super::{
    fxfa_basic::{XfaAttributeValue, XfaElement},
    parser::cxfa_node::CxfaNode,
};
use crate::{
    fxcodec::{
        fx_codec::{CfxDibAttribute, ResolutionUnit},
        fx_codec_def::{FxcodecImageType, FxcodecStatus},
        progressive_decoder::ProgressiveDecoder,
    },
    fxcrt::{
        cfx_read_only_span_stream::CfxReadOnlySpanStream,
        fx_codepage::{wide_char_to_multi_byte, FxCodePage},
        fx_stream::SeekableReadStream,
        fx_string::utf8_encode,
    },
    fxge::dib::cfx_dibitmap::CfxDibitmap,
};
use std::{cell::RefCell, rc::Rc};
#[derive(Clone)]
pub struct ImageResource {
    pub bitmap: Rc<CfxDibitmap>,
    pub dpi: (i32, i32),
    pub named: bool,
}
/// Actual document services used by XFA image loading. The owner chooses how
/// named PDF resources and external files are accessed; no network/file IO is
/// initiated implicitly by the engine.
pub trait XfaImageOwner {
    fn get_pdf_named_image(&self, name: &[u32]) -> Option<ImageResource>;
    fn open_linked_file(&self, url: &[u32]) -> Option<Rc<dyn SeekableReadStream>>;
}
pub fn image_type(content_type: &[u32]) -> FxcodecImageType {
    let name: String = content_type
        .iter()
        .filter_map(|&ch| char::from_u32(ch))
        .collect();
    match name.to_ascii_lowercase().as_str() {
        "image/jpg" | "image/jpeg" => FxcodecImageType::Jpg,
        "image/bmp" => FxcodecImageType::Bmp,
        "image/gif" => FxcodecImageType::Gif,
        "image/png" => FxcodecImageType::Png,
        "image/tif" | "image/tiff" => FxcodecImageType::Tiff,
        _ => FxcodecImageType::Unknown,
    }
}
pub fn load_image_from_buffer(
    file: Rc<dyn SeekableReadStream + '_>,
    image_type: FxcodecImageType,
) -> Option<ImageResource> {
    load_image_from_buffer_with_dpi(file, image_type, &mut (0, 0))
}
fn load_image_from_buffer_with_dpi(
    file: Rc<dyn SeekableReadStream + '_>,
    image_type: FxcodecImageType,
    output_dpi: &mut (i32, i32),
) -> Option<ImageResource> {
    let mut decoder = ProgressiveDecoder::new();
    let mut attribute = CfxDibAttribute::default();
    let status = decoder.load_image_info(file, image_type, &mut attribute, false);
    let dpi = |value: i32| {
        let value = match attribute.dpi_unit {
            ResolutionUnit::Centimeter => (value as f32 * 2.54) as i32,
            ResolutionUnit::Meter => (value as f32 / 100.0 * 2.54) as i32,
            _ => value,
        };
        if value > 1 {
            value
        } else {
            96
        }
    };
    // Native writes physical DPI even when subsequent decoding fails.
    *output_dpi = (dpi(attribute.x_dpi), dpi(attribute.y_dpi));
    if status != FxcodecStatus::FrameReady {
        return None;
    }
    let bitmap = Rc::new(RefCell::new(CfxDibitmap::create(
        decoder.width(),
        decoder.height(),
        decoder.get_bitmap_format(),
        0,
    )?));
    bitmap.borrow_mut().clear(0xffffffff);
    let (status, frames) = decoder.get_frames();
    if status != FxcodecStatus::DecodeReady || frames == 0 {
        return None;
    }
    let mut status = decoder.start_decode(bitmap.clone());
    while status == FxcodecStatus::DecodeToBeContinued {
        status = decoder.continue_decode();
    }
    if status != FxcodecStatus::DecodeFinished {
        return None;
    }
    let bitmap = Rc::new(Rc::try_unwrap(bitmap).ok()?.into_inner());
    Some(ImageResource {
        bitmap,
        dpi: *output_dpi,
        named: false,
    })
}
fn inverse_base64(ch: u8) -> Option<u8> {
    match ch {
        b'A'..=b'Z' => Some(ch - b'A'),
        b'a'..=b'z' => Some(ch - b'a' + 26),
        b'0'..=b'9' => Some(ch - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}
pub fn base64_decode(bytes: &[u8]) -> Vec<u8> {
    let filtered: Vec<u8> = bytes
        .iter()
        .copied()
        .filter(|&ch| inverse_base64(ch).is_some() || ch == b'=')
        .collect();
    let mut result = Vec::with_capacity(filtered.len() / 4 * 3);
    for quartet in filtered.chunks_exact(4) {
        let (Some(a), Some(b)) = (inverse_base64(quartet[0]), inverse_base64(quartet[1])) else {
            break;
        };
        result.push(a << 2 | b >> 4);
        if quartet[2] == b'=' {
            continue;
        }
        let c = inverse_base64(quartet[2]).unwrap();
        result.push(b << 4 | c >> 2);
        if quartet[3] != b'=' {
            result.push(c << 6 | inverse_base64(quartet[3]).unwrap());
        }
    }
    result
}
pub fn load_image_data(owner: &dyn XfaImageOwner, image: &CxfaNode) -> Option<ImageResource> {
    load_image_data_with_metadata(owner, image, &mut false, &mut (0, 0))
}
/// Preserve native named-image/DPI out-parameters independently of the bitmap.
/// Opening a stream resets `named` before decode, and decoded metadata survives
/// a failed bitmap read. An absent URL/content/stream leaves prior metadata.
pub(crate) fn load_image_data_with_metadata(
    owner: &dyn XfaImageOwner,
    image: &CxfaNode,
    named: &mut bool,
    dpi: &mut (i32, i32),
) -> Option<ImageResource> {
    if image.get_element_type() != XfaElement::Image {
        return None;
    }
    let href = image.image_get_href();
    let content = image.image_get_content();
    if href.is_empty() && content.is_empty() {
        return None;
    }
    let kind = image_type(&image.image_get_content_type());
    if !content.is_empty() {
        let bytes = if image.image_get_transfer_encoding() == XfaAttributeValue::Base64 {
            base64_decode(&utf8_encode(&content))
        } else {
            let length = wide_char_to_multi_byte(FxCodePage::DefAnsi, &content, &mut []);
            let mut bytes = vec![0; length];
            wide_char_to_multi_byte(FxCodePage::DefAnsi, &content, &mut bytes);
            bytes
        };
        if bytes.is_empty() {
            return None;
        }
        *named = false;
        return load_image_from_buffer_with_dpi(
            Rc::new(CfxReadOnlySpanStream::new(&bytes)),
            kind,
            dpi,
        );
    }
    let http: Vec<u32> = "http://".chars().map(u32::from).collect();
    let ftp: Vec<u32> = "ftp://".chars().map(u32::from).collect();
    if !href.starts_with(&http) && !href.starts_with(&ftp) {
        if let Some(mut resource) = owner.get_pdf_named_image(&href) {
            resource.named = true;
            *named = true;
            *dpi = resource.dpi;
            return Some(resource);
        }
    }
    let file = owner.open_linked_file(&href)?;
    *named = false;
    load_image_from_buffer_with_dpi(file, kind, dpi)
}
/// Source ImageLayoutData cache behavior. The cache borrow is released before
/// invoking owner callbacks, which may run script or invalidate image data.
pub fn load_layout_image(owner: &dyn XfaImageOwner, node: &CxfaNode) -> bool {
    node.load_layout_image(owner)
}

/// Image portion of FFImage::RenderWidget after the shared widget has drawn
/// its border/background and supplied its unrotated rectangle and rotation.
pub fn draw_layout_image(
    graphics: &mut crate::xfa::fgas::graphics::cfgas_gegraphics::CfgasGeGraphics<'_, '_, '_>,
    node: &CxfaNode,
    resource: &ImageResource,
    mut rect: crate::fxcrt::fx_coordinates::CfxRectF,
    matrix: crate::fxcrt::fx_coordinates::CfxMatrix,
) -> bool {
    let Some(image) = node
        .get_form_value_if_exists()
        .and_then(|value| value.value_get_image_if_exists())
    else {
        return false;
    };
    if let Some(margin) = node.get_first_child_by_class(XfaElement::Margin) {
        rect.deflate(
            margin.margin_get_left_inset(),
            margin.margin_get_top_inset(),
            margin.margin_get_right_inset(),
            margin.margin_get_bottom_inset(),
        );
    }
    let (horizontal, vertical) = image_alignment(node);
    super::cxfa_imagerenderer::draw_image(
        graphics,
        rect,
        matrix,
        resource.bitmap.clone(),
        image.image_get_aspect(),
        resource.dpi,
        horizontal,
        vertical,
    );
    true
}
pub(crate) fn image_alignment(node: &CxfaNode) -> (XfaAttributeValue, XfaAttributeValue) {
    node.get_first_child_by_class(XfaElement::Para).map_or(
        (XfaAttributeValue::Left, XfaAttributeValue::Top),
        |para| {
            (
                para.para_get_horizontal_align(),
                para.para_get_vertical_align(),
            )
        },
    )
}

#[cfg(test)]
#[path = "cxfa_ffimage_tests.rs"]
mod tests;
