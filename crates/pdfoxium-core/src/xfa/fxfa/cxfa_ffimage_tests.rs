// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::*;
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxRectF, FxRect},
    fxge::{agg::cfx_agg_devicedriver::CfxAggDeviceDriver, dib::fx_dib::FxdibFormat},
    xfa::{
        fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
        fxfa::{
            cxfa_imagerenderer::{draw_image, image_fit_rect},
            fxfa_basic::{XfaAttribute, XfaPacketType},
            parser::cxfa_document::CxfaDocument,
        },
    },
};
use std::cell::Cell;
fn wide(value: &str) -> Vec<u32> {
    value.chars().map(u32::from).collect()
}
fn bmp() -> Vec<u8> {
    let mut bytes = vec![0; 58];
    bytes[..2].copy_from_slice(b"BM");
    bytes[2..6].copy_from_slice(&58u32.to_le_bytes());
    bytes[10..14].copy_from_slice(&54u32.to_le_bytes());
    bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
    bytes[18..22].copy_from_slice(&1u32.to_le_bytes());
    bytes[22..26].copy_from_slice(&1u32.to_le_bytes());
    bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
    bytes[28..30].copy_from_slice(&24u16.to_le_bytes());
    bytes[38..42].copy_from_slice(&2835u32.to_le_bytes());
    bytes[42..46].copy_from_slice(&5669u32.to_le_bytes());
    bytes[54..58].copy_from_slice(&[30, 20, 10, 0]);
    bytes
}
fn embedded(bytes: &[u8]) -> Vec<u32> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = Vec::new();
    for group in bytes.chunks(3) {
        let a = group[0];
        let b = group.get(1).copied().unwrap_or(0);
        let c = group.get(2).copied().unwrap_or(0);
        output.extend_from_slice(&[
            ALPHABET[(a >> 2) as usize],
            ALPHABET[((a & 3) << 4 | b >> 4) as usize],
            if group.len() > 1 {
                ALPHABET[((b & 15) << 2 | c >> 6) as usize]
            } else {
                b'='
            },
            if group.len() > 2 {
                ALPHABET[(c & 63) as usize]
            } else {
                b'='
            },
        ]);
    }
    output.into_iter().map(u32::from).collect()
}
fn nodes() -> (Rc<CxfaDocument>, CxfaNode, CxfaNode) {
    let document = CxfaDocument::new();
    let draw = document
        .create_node(XfaPacketType::Form, XfaElement::Draw)
        .unwrap();
    let value = document
        .create_node(XfaPacketType::Form, XfaElement::Value)
        .unwrap();
    let image = document
        .create_node(XfaPacketType::Form, XfaElement::Image)
        .unwrap();
    assert!(draw.insert_child_and_notify(-1, &value));
    assert!(value.insert_child_and_notify(-1, &image));
    image.image_set_content_type(&wide("image/bmp"));
    image.image_set_transfer_encoding(XfaAttributeValue::Base64);
    (document, draw, image)
}
struct Owner {
    requests: RefCell<Vec<Vec<u32>>>,
    named: Option<ImageResource>,
    bytes: Vec<u8>,
    cache: Option<CxfaNode>,
    calls: Cell<usize>,
}
impl XfaImageOwner for Owner {
    fn get_pdf_named_image(&self, name: &[u32]) -> Option<ImageResource> {
        self.requests.borrow_mut().push(name.to_vec());
        self.named.clone()
    }
    fn open_linked_file(&self, url: &[u32]) -> Option<Rc<dyn SeekableReadStream>> {
        self.calls.set(self.calls.get() + 1);
        self.requests.borrow_mut().push(url.to_vec());
        if let Some(cache) = &self.cache {
            assert!(cache.get_layout_image().is_none());
            assert!(cache.set_layout_image(None));
        }
        Some(Rc::new(
            crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream::new(
                self.bytes.clone(),
            ),
        ))
    }
}
#[test]
fn buffer_decoding_dpi_conversion_default_and_fallback_type() {
    let bytes = bmp();
    let resource = load_image_from_buffer(
        Rc::new(CfxReadOnlySpanStream::new(&bytes)),
        FxcodecImageType::Png,
    )
    .unwrap();
    assert_eq!(resource.dpi, (72, 143));
    assert!(!resource.named);
    assert_eq!(&resource.bitmap.scanline(0).unwrap()[..3], &[30, 20, 10]);
    let mut bytes = bytes;
    bytes[38..46].fill(0);
    assert_eq!(
        load_image_from_buffer(
            Rc::new(CfxReadOnlySpanStream::new(&bytes)),
            FxcodecImageType::Unknown
        )
        .unwrap()
        .dpi,
        (96, 96)
    );
    assert!(load_image_from_buffer(
        Rc::new(CfxReadOnlySpanStream::new(b"invalid")),
        FxcodecImageType::Unknown
    )
    .is_none());
}
#[test]
fn embedded_content_named_pdf_and_linked_owner_routing() {
    let (_doc, _draw, image) = nodes();
    let bytes = bmp();
    image.set_content(&embedded(&bytes), &embedded(&bytes), false);
    image.image_set_href(&wide("unused"));
    let mut owner = Owner {
        requests: RefCell::new(Vec::new()),
        named: None,
        bytes: bytes.clone(),
        cache: None,
        calls: Cell::new(0),
    };
    assert!(load_image_data(&owner, &image).is_some());
    assert!(owner.requests.borrow().is_empty());
    image.set_content(&[], &[], false);
    image.image_set_href(&wide("pdf-image"));
    owner.named = load_image_from_buffer(
        Rc::new(CfxReadOnlySpanStream::new(&bytes)),
        FxcodecImageType::Bmp,
    );
    assert!(load_image_data(&owner, &image).unwrap().named);
    assert_eq!(owner.calls.get(), 0);
    image.image_set_href(&wide("http://example/image"));
    assert!(!load_image_data(&owner, &image).unwrap().named);
    assert_eq!(owner.calls.get(), 1);
    assert_eq!(owner.requests.borrow().len(), 2);
    owner.named = None;
    image.image_set_href(&wide("local.png"));
    assert!(load_image_data(&owner, &image).is_some());
    assert_eq!(owner.calls.get(), 2);
    assert_eq!(owner.requests.borrow().len(), 4);
}
#[test]
fn base64_whitespace_padding_non_ascii_and_trailing_partial() {
    for (source, expected) in [
        (&b"TQ=="[..], &b"M"[..]),
        (b"TWE=", b"Ma"),
        (b"TWFu", b"Man"),
        (b" T\xffW\rF\nu!", b"Man"),
        (b"TWFuTQ", b"Man"),
        (b"=AAA", b""),
    ] {
        assert_eq!(base64_decode(source), expected);
    }
}
#[test]
fn image_cache_reload_releases_borrows_before_owner_callbacks() {
    let (_doc, draw, image) = nodes();
    image.image_set_href(&wide("http://host/file"));
    let owner = Owner {
        requests: RefCell::new(Vec::new()),
        named: None,
        bytes: bmp(),
        cache: Some(draw.clone()),
        calls: Cell::new(0),
    };
    assert!(load_layout_image(&owner, &draw));
    assert!(load_layout_image(&owner, &draw));
    assert_eq!(owner.calls.get(), 1);
    assert!(draw.set_layout_image(None));
    assert!(load_layout_image(&owner, &draw));
    assert_eq!(owner.calls.get(), 2);
    image.image_set_href(&[]);
    assert!(draw.set_layout_image(None));
    assert!(!load_layout_image(&owner, &draw));
    assert!(draw.get_layout_image().is_none());
}
#[test]
fn physical_aspect_modes_and_bottom_alignment() {
    let bounds = CfxRectF::new(10.0, 20.0, 200.0, 100.0);
    let fit = |aspect, h, v| image_fit_rect(bounds, 100, 100, (100, 100), aspect, h, v).unwrap();
    assert_eq!(
        fit(
            XfaAttributeValue::Fit,
            XfaAttributeValue::Center,
            XfaAttributeValue::Middle
        ),
        CfxRectF::new(60.0, 20.0, 100.0, 100.0)
    );
    assert_eq!(
        fit(
            XfaAttributeValue::Actual,
            XfaAttributeValue::Right,
            XfaAttributeValue::Bottom
        ),
        CfxRectF::new(138.0, 48.0, 72.0, 72.0)
    );
    assert_eq!(
        fit(
            XfaAttributeValue::None,
            XfaAttributeValue::Left,
            XfaAttributeValue::Top
        ),
        bounds
    );
    assert_eq!(
        fit(
            XfaAttributeValue::Height,
            XfaAttributeValue::Left,
            XfaAttributeValue::Top
        )
        .width,
        100.0
    );
    assert_eq!(
        fit(
            XfaAttributeValue::Width,
            XfaAttributeValue::Left,
            XfaAttributeValue::Top
        )
        .height,
        200.0
    );
    assert!(image_fit_rect(
        bounds,
        1,
        1,
        (0, 96),
        XfaAttributeValue::Fit,
        XfaAttributeValue::Left,
        XfaAttributeValue::Top
    )
    .is_none());
}
#[test]
fn agg_image_rendering_clip_restore_and_margins() {
    let mut source = CfxDibitmap::create(1, 1, FxdibFormat::Bgra, 0).unwrap();
    source.clear(0xffff0000);
    let source = Rc::new(source);
    let mut target = CfxDibitmap::create(8, 8, FxdibFormat::Bgra, 0).unwrap();
    target.clear(0xffffffff);
    {
        let mut device = CfxAggDeviceDriver::new(&mut target, false, None);
        device.set_clip_rect(FxRect::new(1, 1, 7, 7));
        let before = device.clip_box();
        {
            let mut graphics = CfgasGeGraphics::new(&mut device);
            draw_image(
                &mut graphics,
                CfxRectF::new(2.0, 2.0, 4.0, 4.0),
                CfxMatrix::default(),
                source,
                XfaAttributeValue::None,
                (96, 96),
                XfaAttributeValue::Left,
                XfaAttributeValue::Top,
            );
        }
        assert_eq!(device.clip_box(), before);
    }
    for y in 0..8 {
        for x in 0..8 {
            let expected = if (2..6).contains(&x) && (2..6).contains(&y) {
                [0, 0, 255, 255]
            } else {
                [255; 4]
            };
            assert_eq!(&target.scanline(y).unwrap()[x * 4..x * 4 + 4], &expected);
        }
    }
    let (_doc, draw, image) = nodes();
    image.set_enum(XfaAttribute::Aspect, XfaAttributeValue::None, false);
    let margin = _doc
        .create_node(XfaPacketType::Form, XfaElement::Margin)
        .unwrap();
    margin.set_attribute_by_enum(XfaAttribute::LeftInset, &wide("1pt"), false);
    margin.set_attribute_by_enum(XfaAttribute::TopInset, &wide("1pt"), false);
    margin.set_attribute_by_enum(XfaAttribute::RightInset, &wide("1pt"), false);
    margin.set_attribute_by_enum(XfaAttribute::BottomInset, &wide("1pt"), false);
    draw.insert_child_and_notify(-1, &margin);
    let resource = load_image_from_buffer(
        Rc::new(CfxReadOnlySpanStream::new(&bmp())),
        FxcodecImageType::Bmp,
    )
    .unwrap();
    let mut device = CfxAggDeviceDriver::new(&mut target, false, None);
    let mut graphics = CfgasGeGraphics::new(&mut device);
    assert!(draw_layout_image(
        &mut graphics,
        &draw,
        &resource,
        CfxRectF::new(0.0, 0.0, 4.0, 4.0),
        CfxMatrix::default()
    ));
}
#[test]
fn node_image_metadata_survives_bitmap_clear_and_failed_reload() {
    let (_document, draw, image) = nodes();
    image.image_set_href(&wide("named"));
    let mut named = load_image_from_buffer(
        Rc::new(CfxReadOnlySpanStream::new(&bmp())),
        FxcodecImageType::Bmp,
    )
    .unwrap();
    named.dpi = (72, 144);
    let owner = Owner {
        requests: RefCell::new(Vec::new()),
        named: Some(named),
        bytes: Vec::new(),
        cache: None,
        calls: Cell::new(0),
    };
    assert!(draw.load_layout_image(&owner));
    let bitmap = draw.get_layout_image().unwrap();
    assert!(draw.get_layout_image_resource().unwrap().named);
    assert_eq!(draw.get_layout_image_dpi(), (72, 144));
    assert!(draw.set_layout_image(None));
    assert_eq!(draw.get_layout_image_dpi(), (72, 144));
    assert!(draw.set_layout_image(Some(bitmap.clone())));
    assert!(draw.get_layout_image_resource().unwrap().named);
    draw.set_layout_image(None);
    image.image_set_href(&wide("http://bad"));
    // A stream was opened, so native clears named and writes fallback96 DPI
    // even though there are no valid image bytes to retain.
    assert!(!draw.load_layout_image(&owner));
    assert_eq!(draw.get_layout_image_dpi(), (96, 96));
    draw.set_layout_image(Some(bitmap));
    assert!(!draw.get_layout_image_resource().unwrap().named);
}
