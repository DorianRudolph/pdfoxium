// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Actual FWL/AGG/model tests for image and barcode concrete FF owners.
use super::*;
use crate::{
    fxcrt::{
        cfx_read_only_container_stream::CfxReadOnlyContainerStream, fx_stream::SeekableReadStream,
    },
    fxge::{
        agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
        cfx_font::CfxFont,
        dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat},
    },
    xfa::{
        fgas::font::cfgas_gefont::CfgasGeFont,
        fwl::{cfwl_app::CfwlApp, cfwl_widget::FwlWidget, test_support::new_app},
        fxfa::{
            cxfa_eventparam::CxfaEventParam,
            cxfa_ffimage::{ImageResource, XfaImageOwner},
            cxfa_ffimage_widget::CxfaFfImage,
            cxfa_ffimageedit_widget::CxfaFfImageEdit,
            cxfa_ffwidget::{CaptionLayout, FfDocViewHost},
            cxfa_textprovider::TextProviderType,
            fxfa::{AlertButton, AlertIcon, AlertReturn, XfaEventError, XfaWidgetStatus as S},
            fxfa_basic::{XfaAttribute as A, XfaElement as E, XfaPacketType, XfaUnit},
            layout::{cxfa_layoutitem::CxfaLayoutStore, cxfa_viewlayoutitem::CxfaViewLayoutItem},
            parser::{cxfa_document::CxfaDocument, cxfa_measurement::CxfaMeasurement},
        },
    },
};
use std::cell::Cell;
fn wide(value: &str) -> Vec<u32> {
    value.chars().map(u32::from).collect()
}
struct ImageOwner {
    calls: Cell<usize>,
    bytes: RefCell<Vec<u8>>,
}
impl XfaImageOwner for ImageOwner {
    fn get_pdf_named_image(&self, _: &[u32]) -> Option<ImageResource> {
        None
    }
    fn open_linked_file(&self, _: &[u32]) -> Option<Rc<dyn SeekableReadStream>> {
        self.calls.set(self.calls.get() + 1);
        Some(Rc::new(CfxReadOnlyContainerStream::new(
            self.bytes.borrow().clone(),
        )))
    }
}
struct Host {
    app: Rc<CfwlApp>,
    font: Rc<CfgasGeFont>,
    images: Rc<ImageOwner>,
}
impl Host {
    fn new() -> Rc<Self> {
        let (app, _) = new_app();
        let font = CfgasGeFont::from_font(
            CfxFont::from_span(
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf"
                )),
                false,
                0,
            )
            .unwrap(),
        )
        .unwrap();
        let mut bytes = vec![0; 58];
        bytes[..2].copy_from_slice(b"BM");
        bytes[10..14].copy_from_slice(&54u32.to_le_bytes());
        bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
        bytes[18..22].copy_from_slice(&1u32.to_le_bytes());
        bytes[22..26].copy_from_slice(&1u32.to_le_bytes());
        bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
        bytes[28..30].copy_from_slice(&24u16.to_le_bytes());
        bytes[54..58].copy_from_slice(&[0, 0, 255, 0]);
        Rc::new(Self {
            app,
            font,
            images: Rc::new(ImageOwner {
                calls: Cell::new(0),
                bytes: RefCell::new(bytes),
            }),
        })
    }
}
impl FfDocViewHost for Host {
    fn fwl_app(&self) -> Rc<CfwlApp> {
        self.app.clone()
    }
    fn is_interactive(&self) -> bool {
        true
    }
    fn page_rect(&self, _: &CxfaViewLayoutItem) -> CfxRectF {
        CfxRectF::new(0., 0., 300., 300.)
    }
    fn invalidate_rect(&self, _: Option<&CxfaViewLayoutItem>, _: CfxRectF) {}
    fn display_caret(&self, _: &dyn FfWidget, _: bool, _: Option<CfxRectF>) {}
    fn process_event(&self, _: &CxfaNode, _: V, _: &mut CxfaEventParam) -> XfaEventError {
        XfaEventError::kSuccess
    }
    fn set_change_mark(&self) {}
    fn add_validate_node(&self, _: &CxfaNode) {}
    fn highlight_color(&self) -> u32 {
        0x808080ff
    }
    fn message_box(
        &self,
        _: &[u32],
        _: &[u32],
        _: AlertIcon,
        _: AlertButton,
    ) -> Option<AlertReturn> {
        None
    }
    fn load_caption(&self, node: &CxfaNode) -> FfResult<Option<CaptionLayout>> {
        assert!(node.get_caption_if_exists().is_none());
        Ok(None)
    }
    fn caption_layout(&self, _: &CxfaNode) -> Option<CaptionLayout> {
        None
    }
    fn text_layout(&self, _: &CxfaNode) -> Option<CaptionLayout> {
        None
    }
    fn load_text_layout(
        &self,
        _: &CxfaNode,
        _: TextProviderType,
    ) -> FfResult<Option<CaptionLayout>> {
        Ok(None)
    }
    fn get_font(&self, _: &CxfaNode) -> Option<Rc<CfgasGeFont>> {
        Some(self.font.clone())
    }
    fn get_popup_pos(
        &self,
        _: &dyn FfWidget,
        _: f32,
        _: f32,
        _: CfxRectF,
        _: &mut CfxRectF,
    ) -> bool {
        false
    }
    fn goto_url(&self, _: &[u32]) {}
    fn add_calculate_node(&self, _: &CxfaNode) {}
    fn popup_menu(&self, _: &dyn FfWidget, _: CfxPointF) {}
    fn update_ui_display(&self, _: &CxfaNode, _: Option<&dyn FfWidget>) {}
    fn app_title(&self) -> Vec<u32> {
        wide("Image/barcode test")
    }
    fn set_focus_node(&self, _: Option<&CxfaNode>) -> FfResult<()> {
        Ok(())
    }
    fn image_owner(&self) -> Option<Rc<dyn XfaImageOwner>> {
        Some(self.images.clone())
    }
}
fn node(document: &Rc<CxfaDocument>, kind: E) -> CxfaNode {
    document.create_node(XfaPacketType::Form, kind).unwrap()
}
fn attach<W: FfWidget + 'static>(
    widget: &Rc<W>,
    host: &Rc<Host>,
    store: &Rc<CxfaLayoutStore>,
    rect: CfxRectF,
) -> CxfaContentLayoutItem {
    let dynamic: Rc<dyn FfDocViewHost> = host.clone();
    widget.widget().set_doc_view(Some(&dynamic));
    let item = store.create_content(widget.widget().node(), Some(widget.clone()));
    item.set_position(rect.top_left());
    item.set_size(rect.size());
    item.set_status_bits(S::kVisible);
    widget
        .widget()
        .node()
        .set_layout_item(Some((*item).clone()));
    item
}
fn image_value(node: &CxfaNode) -> CxfaNode {
    let image = node
        .get_or_create_property(0, E::Value)
        .unwrap()
        .get_or_create_property(0, E::Image)
        .unwrap();
    image.image_set_href(&wide("test.bmp"));
    image.image_set_content_type(&wide("image/bmp"));
    image.set_enum(A::Aspect, V::None, false);
    image
}
fn bitmap() -> CfxDibitmap {
    let mut bitmap = CfxDibitmap::create(160, 80, FxdibFormat::Bgra, 0).unwrap();
    bitmap.clear(0xffffffff);
    bitmap
}
#[test]
fn complete_barcode_table_and_case_sensitive_source_names() {
    assert_eq!(BARCODES.len(), 62);
    for &(hash, name, kind) in BARCODES {
        let name = wide(name);
        assert_eq!(hash_code_lowered_wide(&name), hash);
        assert_eq!(CxfaFfBarcode::get_barcode_type_by_name(&name), kind);
    }
    for name in ["", "code39", "CODE128", "qrcode", "not-a-barcode"] {
        assert_eq!(
            CxfaFfBarcode::get_barcode_type_by_name(&wide(name)),
            BcType::Unknown
        );
    }
}
#[test]
fn image_widget_loads_caches_renders_margin_and_respects_visibility() {
    let document = CxfaDocument::new();
    let model = node(&document, E::Draw);
    image_value(&model);
    let margin = model.get_or_create_property(0, E::Margin).unwrap();
    for attribute in [A::LeftInset, A::TopInset, A::RightInset, A::BottomInset] {
        margin.set_measure(attribute, CxfaMeasurement::new(2., XfaUnit::Pt), false);
    }
    let widget = CxfaFfImage::new(&model);
    assert!(!widget.is_loaded());
    assert!(widget.load_widget().is_err());
    let host = Host::new();
    let store = CxfaLayoutStore::new();
    let item = attach(&widget, &host, &store, CfxRectF::new(4., 4., 20., 20.));
    assert!(widget.load_widget().unwrap());
    assert!(widget.load_widget().unwrap());
    assert_eq!(host.images.calls.get(), 1);
    assert!(widget.is_loaded());
    let mut output = bitmap();
    {
        let mut device = CfxAggDeviceDriver::new(&mut output, false, None);
        let mut graphics = CfgasGeGraphics::new(&mut device);
        widget
            .render_widget(
                &mut graphics,
                CfxMatrix::default(),
                HighlightOption::NoHighlight,
            )
            .unwrap();
    }
    assert_eq!(
        &output.scanline(10).unwrap()[10 * 4..11 * 4],
        &[0, 0, 255, 255]
    );
    assert_eq!(&output.scanline(4).unwrap()[4 * 4..5 * 4], &[255; 4]);
    item.clear_status_bits(S::kVisible);
    let mut output = bitmap();
    {
        let mut device = CfxAggDeviceDriver::new(&mut output, false, None);
        let mut graphics = CfgasGeGraphics::new(&mut device);
        widget
            .render_widget(
                &mut graphics,
                CfxMatrix::default(),
                HighlightOption::NoHighlight,
            )
            .unwrap();
    }
    assert!(output.buffer().iter().all(|&byte| byte == 255));
    widget.pre_finalize();
    assert!(!widget.is_loaded());
}
#[test]
fn image_edit_real_picturebox_rect_focus_reload_and_teardown() {
    let document = CxfaDocument::new();
    let model = node(&document, E::Field);
    let ui = model
        .get_or_create_property(0, E::Ui)
        .unwrap()
        .get_or_create_property(0, E::ImageEdit)
        .unwrap();
    image_value(&model);
    let margin = ui.get_or_create_property(0, E::Margin).unwrap();
    for (attribute, value) in [
        (A::LeftInset, 1.),
        (A::TopInset, 2.),
        (A::RightInset, 3.),
        (A::BottomInset, 4.),
    ] {
        margin.set_measure(attribute, CxfaMeasurement::new(value, XfaUnit::Pt), false);
    }
    let widget = CxfaFfImageEdit::new(&model);
    let host = Host::new();
    let store = CxfaLayoutStore::new();
    attach(&widget, &host, &store, CfxRectF::new(4., 4., 40., 30.));
    assert!(widget.load_widget().unwrap());
    let picture = widget.field.normal_widget().unwrap();
    assert_eq!(
        picture.get_widget_rect(),
        super::super::cxfa_ffimageedit::image_edit_rect(
            widget.field.ui_rect(),
            model.get_ui_margin()
        )
    );
    assert_eq!(
        widget.get_form_field_type(),
        crate::fpdfdoc::cpdf_formfield::FormFieldType::XfaImageField
    );
    assert!(widget.commit_data());
    let rect = picture.get_widget_rect();
    let point = CfxPointF::new(rect.left + 1., rect.top + 1.);
    assert!(widget.accepts_focus_on_button_down(0, point, MouseCommand::LeftButtonDown));
    model.set_enum(A::Access, V::ReadOnly, false);
    assert!(!widget.accepts_focus_on_button_down(0, point, MouseCommand::LeftButtonDown));
    model.set_enum(A::Access, V::Open, false);
    assert!(widget.on_l_button_down(0, point));
    assert!(widget.widget().is_button_down());
    widget.update_fwl_data();
    assert_eq!(host.images.calls.get(), 2);
    assert!(widget.resource().is_some());
    host.images.bytes.borrow_mut().clear();
    widget.update_fwl_data();
    assert!(widget.resource().is_none());
    widget.pre_finalize();
    let weak = Rc::downgrade(&widget);
    drop(store);
    drop(widget);
    assert!(weak.upgrade().is_none());
}
#[test]
fn barcode_uses_one_edit_identity_real_encoding_and_protected_focus() {
    let document = CxfaDocument::new();
    let model = node(&document, E::Field);
    let barcode = model
        .get_or_create_property(0, E::Ui)
        .unwrap()
        .get_or_create_property(0, E::Barcode)
        .unwrap();
    barcode.set_cdata(A::Type, &wide("code128"));
    barcode.set_enum(A::TextLocation, V::None, false);
    let text = model
        .get_or_create_property(0, E::Value)
        .unwrap()
        .get_or_create_property(0, E::Text)
        .unwrap();
    text.set_content(&wide("ABC123"), &wide("ABC123"), false);
    let widget = CxfaFfBarcode::new(&model, &barcode);
    let host = Host::new();
    let store = CxfaLayoutStore::new();
    attach(&widget, &host, &store, CfxRectF::new(2., 2., 140., 60.));
    assert!(widget.load_widget().unwrap());
    let control = widget.control().unwrap();
    let normal = widget.text_edit.field.normal_widget().unwrap();
    assert_eq!(normal.base().id(), control.base().id());
    assert!(normal.as_edit().is_some());
    assert_eq!(control.get_type(), BcType::Code128);
    assert_eq!(normal.as_edit().unwrap().get_text(), wide("ABC123"));
    assert!(widget.accepts_focus_on_button_down(
        0,
        CfxPointF::new(10., 10.),
        MouseCommand::LeftButtonDown
    ));
    let mut output = bitmap();
    {
        let mut device = CfxAggDeviceDriver::new(&mut output, false, None);
        let mut graphics = CfgasGeGraphics::new(&mut device);
        widget
            .render_widget(
                &mut graphics,
                CfxMatrix::default(),
                HighlightOption::NoHighlight,
            )
            .unwrap();
    }
    assert!(control.encode_succeeded());
    assert!(output
        .buffer()
        .chunks_exact(4)
        .any(|pixel| pixel[..3] == [0, 0, 0]));
    barcode.set_cdata(A::Type, &wide("QRCode"));
    widget.update_widget_property();
    assert_eq!(control.get_type(), BcType::QrCode);
    assert!(!widget.accepts_focus_on_button_down(
        0,
        CfxPointF::new(10., 10.),
        MouseCommand::LeftButtonDown
    ));
    widget.pre_finalize();
    let weak = Rc::downgrade(&widget);
    drop(store);
    drop(widget);
    assert!(weak.upgrade().is_none());
}
#[test]
fn image_fragments_share_node_cache_identity_and_prefinalize_invalidation() {
    let document = CxfaDocument::new();
    let model = node(&document, E::Draw);
    image_value(&model);
    let first = CxfaFfImage::new(&model);
    let second = CxfaFfImage::new(&model);
    let host = Host::new();
    let store = CxfaLayoutStore::new();
    attach(&first, &host, &store, CfxRectF::new(1., 1., 20., 20.));
    attach(&second, &host, &store, CfxRectF::new(30., 1., 20., 20.));
    assert!(first.load_widget().unwrap());
    assert!(second.is_loaded());
    assert!(second.load_widget().unwrap());
    assert_eq!(host.images.calls.get(), 1);
    let bitmap = first.resource().unwrap().bitmap;
    assert!(Rc::ptr_eq(&bitmap, &second.resource().unwrap().bitmap));
    assert!(Rc::ptr_eq(&bitmap, &model.get_layout_image().unwrap()));
    assert_eq!(model.get_layout_image_dpi(), (96, 96));
    assert!(model.set_layout_image(Some(bitmap.clone())));
    assert_eq!(host.images.calls.get(), 1);
    first.pre_finalize();
    assert!(!second.is_loaded());
    assert!(model.get_layout_image().is_none());
    // SetBitmap(nullptr) clears only the bitmap; native physical metadata stays.
    assert_eq!(model.get_layout_image_dpi(), (96, 96));
    assert!(second.load_widget().unwrap());
    assert_eq!(host.images.calls.get(), 2);
    assert!(!Rc::ptr_eq(&bitmap, &second.resource().unwrap().bitmap));
    assert!(first.is_loaded());
    let cloned = model.clone_node(true).unwrap();
    assert!(cloned.get_layout_image().is_none());
    second.pre_finalize();
    assert!(!first.is_loaded());
}
#[test]
fn image_edit_fragments_share_replacement_failure_and_teardown_state() {
    let document = CxfaDocument::new();
    let model = node(&document, E::Field);
    model
        .get_or_create_property(0, E::Ui)
        .unwrap()
        .get_or_create_property(0, E::ImageEdit)
        .unwrap();
    image_value(&model);
    let first = CxfaFfImageEdit::new(&model);
    let second = CxfaFfImageEdit::new(&model);
    let host = Host::new();
    let store = CxfaLayoutStore::new();
    attach(&first, &host, &store, CfxRectF::new(1., 1., 20., 20.));
    attach(&second, &host, &store, CfxRectF::new(30., 1., 20., 20.));
    assert!(first.load_widget().unwrap());
    assert!(second.load_widget().unwrap());
    assert_eq!(host.images.calls.get(), 1);
    let old = first.resource().unwrap().bitmap;
    assert!(Rc::ptr_eq(&old, &second.resource().unwrap().bitmap));
    first.update_fwl_data();
    assert_eq!(host.images.calls.get(), 2);
    let replacement = first.resource().unwrap().bitmap;
    assert!(!Rc::ptr_eq(&old, &replacement));
    assert!(Rc::ptr_eq(&replacement, &second.resource().unwrap().bitmap));
    assert!(model.get_layout_image().is_none());
    assert_eq!(model.get_edit_image_dpi(), (96, 96));
    host.images.bytes.borrow_mut().clear();
    first.update_fwl_data();
    assert!(first.resource().is_none());
    assert!(second.resource().is_none());
    assert!(model.get_edit_image().is_none());
    assert_eq!(model.get_edit_image_dpi(), (96, 96));
    assert!(model.set_edit_image(Some(replacement.clone())));
    assert!(Rc::ptr_eq(&replacement, &second.resource().unwrap().bitmap));
    second.pre_finalize();
    assert!(first.resource().is_none());
}

#[test]
fn get_barcode_type_by_name_native() {
    assert_eq!(
        CxfaFfBarcode::get_barcode_type_by_name(&wide("")),
        BcType::Unknown
    );
    assert_eq!(
        CxfaFfBarcode::get_barcode_type_by_name(&wide("not_found")),
        BcType::Unknown
    );
    assert_eq!(
        CxfaFfBarcode::get_barcode_type_by_name(&wide("ean13")),
        BcType::Ean13
    );
    assert_eq!(
        CxfaFfBarcode::get_barcode_type_by_name(&wide("pdf417")),
        BcType::Pdf417
    );
    assert_eq!(
        CxfaFfBarcode::get_barcode_type_by_name(&wide("code3Of9")),
        BcType::Code39
    );
}
