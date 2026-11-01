// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Source-derived FF widget/field tests against real layout and FWL controls.
use super::*;
use crate::{
    fxcrt::fx_coordinates::CfxSizeF,
    fxge::{
        agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
        cfx_font::CfxFont,
        dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat},
    },
    xfa::{
        fgas::font::cfgas_gefont::CfgasGeFont,
        fwl::{
            cfwl_edit::CfwlEdit,
            cfwl_widget::{FwlWidget, Properties, WidgetRef},
            fwl_widgetdef::{KEY_FLAG_COMMAND, KEY_FLAG_CTRL, VKEY_C},
            test_support::new_app,
        },
        fxfa::{
            cxfa_textparser::XfaTextFontHost,
            cxfa_textprovider::{CxfaTextProvider, TextProviderType},
            fxfa_basic::{XfaAttribute as A, XfaElement as E, XfaPacketType as P, XfaUnit},
            layout::cxfa_layoutitem::CxfaLayoutStore,
            parser::{cxfa_document::CxfaDocument, cxfa_measurement::CxfaMeasurement},
        },
    },
};
use std::collections::HashMap;
fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
fn make_node(doc: &Rc<CxfaDocument>, kind: E) -> CxfaNode {
    doc.create_node(P::Form, kind).unwrap()
}
fn child(doc: &Rc<CxfaDocument>, parent: &CxfaNode, kind: E) -> CxfaNode {
    let node = make_node(doc, kind);
    assert!(parent.insert_child_and_notify(-1, &node));
    node
}
fn measure(node: &CxfaNode, attribute: A, value: f32) {
    node.set_measure(attribute, CxfaMeasurement::new(value, XfaUnit::Pt), false);
}

#[test]
fn comb_theme_keeps_default_border_and_honors_an_explicit_ui_edge() {
    let doc = CxfaDocument::new();
    let node = make_node(&doc, E::Field);
    let ui = child(&doc, &node, E::Ui);
    let edit = child(&doc, &ui, E::TextEdit);
    let widget = CxfaFfWidget::new(&node);
    assert_eq!(widget.get_border_color_and_thickness(), (0xff000000, 1.));
    let border = child(&doc, &edit, E::Border);
    // GetEdgeIfExists(0) creates the schema's half-point first edge.
    assert_eq!(widget.get_border_color_and_thickness(), (0xff000000, 0.5));
    let edge = border.box_get_edge(0).unwrap();
    measure(&edge, A::Thickness, 2.5);
    let color = child(&doc, &edge, E::Color);
    color.set_cdata(A::Value, &wide("12,34,56"));
    assert_eq!(widget.get_border_color_and_thickness(), (0xff0c2238, 2.5));
}
struct Font(Rc<CfgasGeFont>);
impl XfaTextFontHost for Font {
    fn get_font(&self, _: &[u32], _: u32) -> Option<Rc<CfgasGeFont>> {
        Some(self.0.clone())
    }
}
struct Host {
    app: Rc<CfwlApp>,
    font: Rc<Font>,
    interactive: Cell<bool>,
    events: RefCell<Vec<(CxfaNode, V)>>,
    invalidations: RefCell<Vec<CfxRectF>>,
    message_response: Cell<Option<AlertReturn>>,
    messages: RefCell<Vec<Vec<u32>>>,
    captions: RefCell<HashMap<CxfaNode, CaptionLayout>>,
    changed: Cell<usize>,
    validate: RefCell<Vec<CxfaNode>>,
    event_hook: RefCell<Option<Rc<dyn Fn(&CxfaNode, V, &mut CxfaEventParam)>>>,
}
impl Host {
    fn new() -> Rc<Self> {
        let (app, _) = new_app();
        let font = CfgasGeFont::from_font(
            CfxFont::from_span(
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/../../upstream/pdfium/testing/resources/fonts/roboto.ttf"
                )),
                false,
                0,
            )
            .unwrap(),
        )
        .unwrap();
        Rc::new(Self {
            app,
            font: Rc::new(Font(font)),
            interactive: Cell::new(true),
            events: RefCell::new(Vec::new()),
            invalidations: RefCell::new(Vec::new()),
            message_response: Cell::new(Some(AlertReturn::kYes)),
            messages: RefCell::new(Vec::new()),
            captions: RefCell::new(HashMap::new()),
            changed: Cell::new(0),
            validate: RefCell::new(Vec::new()),
            event_hook: RefCell::new(None),
        })
    }
}
impl FfDocViewHost for Host {
    fn fwl_app(&self) -> Rc<CfwlApp> {
        self.app.clone()
    }
    fn is_interactive(&self) -> bool {
        self.interactive.get()
    }
    fn page_rect(&self, _: &CxfaViewLayoutItem) -> CfxRectF {
        CfxRectF::new(0., 0., 200., 300.)
    }
    fn invalidate_rect(&self, _: Option<&CxfaViewLayoutItem>, rect: CfxRectF) {
        self.invalidations.borrow_mut().push(rect);
    }
    fn display_caret(&self, _: &dyn FfWidget, _: bool, _: Option<CfxRectF>) {}
    fn process_event(
        &self,
        node: &CxfaNode,
        activity: V,
        parameter: &mut CxfaEventParam,
    ) -> XfaEventError {
        self.events.borrow_mut().push((node.clone(), activity));
        let hook = self.event_hook.borrow().clone();
        if let Some(hook) = hook {
            hook(node, activity, parameter);
        }
        XfaEventError::kSuccess
    }
    fn set_change_mark(&self) {
        self.changed.set(self.changed.get() + 1);
    }
    fn add_validate_node(&self, node: &CxfaNode) {
        self.validate.borrow_mut().push(node.clone());
    }
    fn highlight_color(&self) -> u32 {
        0x80ffff00
    }
    fn message_box(
        &self,
        message: &[u32],
        _: &[u32],
        _: AlertIcon,
        _: AlertButton,
    ) -> Option<AlertReturn> {
        self.messages.borrow_mut().push(message.to_vec());
        self.message_response.get()
    }
    fn load_caption(&self, node: &CxfaNode) -> FfResult<Option<CaptionLayout>> {
        if node.get_caption_if_exists().is_none() {
            return Ok(None);
        }
        if let Some(cached) = self.caption_layout(node) {
            return Ok(Some(cached));
        }
        let layout = FfTextLayout::new(CxfaTextLayout::new(
            Rc::new(CxfaTextProvider::new(
                node.clone(),
                TextProviderType::Caption,
            )),
            self.font.clone(),
        )?);
        self.captions
            .borrow_mut()
            .insert(node.clone(), layout.clone());
        Ok(Some(layout))
    }
    fn caption_layout(&self, node: &CxfaNode) -> Option<CaptionLayout> {
        self.captions.borrow().get(node).cloned()
    }
    fn text_layout(&self, _: &CxfaNode) -> Option<CaptionLayout> {
        None
    }
    fn load_text_layout(
        &self,
        node: &CxfaNode,
        kind: TextProviderType,
    ) -> FfResult<Option<CaptionLayout>> {
        Ok(Some(FfTextLayout::new(CxfaTextLayout::new(
            Rc::new(CxfaTextProvider::new(node.clone(), kind)),
            self.font.clone(),
        )?)))
    }
    fn get_font(&self, _: &CxfaNode) -> Option<Rc<CfgasGeFont>> {
        Some(self.font.0.clone())
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
        wide("FF test")
    }
    fn set_focus_node(&self, _: Option<&CxfaNode>) -> FfResult<()> {
        Ok(())
    }
    fn image_owner(&self) -> Option<Rc<dyn super::super::cxfa_ffimage::XfaImageOwner>> {
        None
    }
}
fn attach(
    widget: &dyn FfWidget,
    host: &Rc<Host>,
    store: &Rc<CxfaLayoutStore>,
    owner: Rc<dyn LayoutItemWidget>,
    position: CfxPointF,
    size: CfxSizeF,
) -> CxfaContentLayoutItem {
    let host_trait: Rc<dyn FfDocViewHost> = host.clone();
    widget.widget().set_doc_view(Some(&host_trait));
    let item = store.create_content(widget.widget().node(), Some(owner));
    item.set_position(position);
    item.set_size(size);
    item.set_status_bits(S::kVisible);
    widget
        .widget()
        .node()
        .set_layout_item(Some((*item).clone()));
    item
}
#[test]
fn base_geometry_cache_rotation_and_nested_focus_use_real_item_identity() {
    let doc = CxfaDocument::new();
    let host = Host::new();
    let store = CxfaLayoutStore::new();
    let parent_node = make_node(&doc, E::Subform);
    let field_node = child(&doc, &parent_node, E::Field);
    let parent = CxfaFfWidget::new(&parent_node);
    let field = CxfaFfWidget::new(&field_node);
    let parent_item = attach(
        parent.as_ref(),
        &host,
        &store,
        parent.clone(),
        CfxPointF::new(2., 3.),
        CfxSizeF::new(100., 50.),
    );
    let field_item = attach(
        field.as_ref(),
        &host,
        &store,
        field.clone(),
        CfxPointF::new(10., 20.),
        CfxSizeF::new(30., 40.),
    );
    parent_item.append_last_child(&field_item);
    assert_eq!(field.get_widget_rect(), CfxRectF::new(12., 23., 30., 40.));
    field_item.set_position(CfxPointF::new(11., 22.));
    assert_eq!(field.get_widget_rect(), CfxRectF::new(12., 23., 30., 40.));
    field_item.clear_status_bits(S::kRectCached);
    assert_eq!(field.get_widget_rect(), CfxRectF::new(13., 25., 30., 40.));
    for rotation in [0, 90, 180, 270] {
        field_node.set_integer(A::Rotate, rotation, false);
        let normal = field.get_rect_without_rotate();
        assert_eq!(
            field.get_rotate_matrix().transform_rect(normal),
            field.get_widget_rect()
        );
        let point = CfxPointF::new(normal.left + 1., normal.top + 2.);
        assert_eq!(
            field.rotate_to_normal(field.get_rotate_matrix().transform(point)),
            point
        );
    }
    assert!(field.on_set_focus(None).unwrap());
    assert_eq!(
        host.events
            .borrow()
            .iter()
            .map(|(_, activity)| *activity)
            .collect::<Vec<_>>(),
        [V::Enter, V::Enter]
    );
    assert!(parent.is_ancestor_of(Some(field.as_ref())));
    assert!(field.is_focused());
    field.modify_status(S::kAccess, S::kNone);
    assert!(field.on_kill_focus(Some(parent.as_ref())).unwrap());
    assert_eq!(host.events.borrow().len(), 2);
    assert!(!field.is_focused());
    assert!(field.on_kill_focus(None).unwrap());
    assert_eq!(host.events.borrow().last().unwrap().1, V::Exit);
    let weak = Rc::downgrade(&field);
    drop(field);
    assert!(weak.upgrade().is_some());
    drop(store);
    assert!(weak.upgrade().is_none());
    assert!(!field_item.is_alive());
}
#[test]
fn field_caption_placement_input_and_weak_fwl_adapters_are_connected() {
    let doc = CxfaDocument::new();
    let host = Host::new();
    let store = CxfaLayoutStore::new();
    let node = make_node(&doc, E::Field);
    let margin = child(&doc, &node, E::Margin);
    for (attr, value) in [
        (A::LeftInset, 2.),
        (A::TopInset, 3.),
        (A::RightInset, 4.),
        (A::BottomInset, 5.),
    ] {
        measure(&margin, attr, value);
    }
    let caption = child(&doc, &node, E::Caption);
    caption.set_enum(A::Placement, V::Left, false);
    measure(&caption, A::Reserve, 20.);
    let field = crate::xfa::fxfa::cxfa_fftextedit::CxfaFfTextEdit::new(&node);
    attach(
        field.as_ref(),
        &host,
        &store,
        field.clone(),
        CfxPointF::new(10., 20.),
        CfxSizeF::new(100., 50.),
    );
    let page = store.create_view(&make_node(&doc, E::PageArea), None);
    field.base.set_page_view(Some(page));
    let edit = CfwlEdit::new(&host.app, Properties::default(), None);
    let owner: Rc<dyn FfWidget> = field.clone();
    field.attach_edit_widget(&owner, edit.clone() as WidgetRef);
    drop(owner);
    field.perform_layout().unwrap();
    assert_eq!(field.caption_rect(), CfxRectF::new(12., 23., 20., 42.));
    assert_eq!(field.ui_rect(), CfxRectF::new(32., 23., 74., 42.));
    assert!(field.accepts_focus_on_button_down(
        0,
        CfxPointF::new(40., 30.),
        MouseCommand::LeftButtonDown
    ));
    assert!(!field.accepts_focus_on_button_down(
        0,
        CfxPointF::new(20., 30.),
        MouseCommand::LeftButtonDown
    ));
    assert!(field.on_set_focus(None).unwrap());
    assert!(field.on_char(u32::from('x'), 0));
    assert_eq!(edit.get_text(), wide("x"));
    let shortcut_modifier = if cfg!(target_vendor = "apple") {
        KEY_FLAG_COMMAND
    } else {
        KEY_FLAG_CTRL
    };
    assert!(!field.on_key_down(VKEY_C, shortcut_modifier));
    assert!(field.on_char(9, 0));
    assert_eq!(edit.get_text(), wide("x"));
    host.interactive.set(false);
    assert!(!field.on_char(u32::from('y'), 0));
    host.interactive.set(true);
    node.set_enum(A::Access, V::ReadOnly, false);
    assert!(!field.on_char(u32::from('z'), 0));
    assert_eq!(edit.get_text(), wide("x"));
    // Text edits permit left-button focus/selection for read-only content;
    // their override rejects the right-button popup path when access is closed.
    assert!(field.accepts_focus_on_button_down(
        0,
        CfxPointF::new(40., 30.),
        MouseCommand::LeftButtonDown
    ));
    assert!(!field.accepts_focus_on_button_down(
        0,
        CfxPointF::new(40., 30.),
        MouseCommand::RightButtonDown
    ));
    assert_eq!(
        host.invalidations.borrow().last(),
        Some(&CfxRectF::new(-2., -2., 204., 304.))
    );
    assert!(edit.base().adapter_iface().unwrap().xfa_widget().is_some());
    let weak = Rc::downgrade(&field);
    drop(field);
    drop(store);
    assert!(weak.upgrade().is_none());
    assert!(edit.base().adapter_iface().unwrap().xfa_widget().is_none());
}
#[test]
fn calculate_override_preserves_messages_group_state_and_source_commit_order() {
    let doc = CxfaDocument::new();
    doc.recognize_xfa_version_number(&wide("http://www.xfa.org/schema/xfa-template/2.8/"));
    let host = Host::new();
    let node = make_node(&doc, E::Field);
    let calc = child(&doc, &node, E::Calculate);
    calc.set_enum(A::Override, V::Warning, false);
    let field = CxfaFfField::new(&node);
    let host_trait: Rc<dyn FfDocViewHost> = host.clone();
    field.base.set_doc_view(Some(&host_trait));
    assert_eq!(field.calculate_override().unwrap(), 1);
    assert!(node.is_user_interactive());
    assert_eq!(
        host.messages.borrow().as_slice(),
        [wide("Are you sure you want to modify this field?")]
    );
    assert_eq!(field.calculate_override().unwrap(), 1);
    assert_eq!(host.messages.borrow().len(), 1);
    calc.set_enum(A::Override, V::Ignore, false);
    assert_eq!(field.calculate_override().unwrap(), 0);
    calc.set_enum(A::Override, V::Error, false);
    assert_eq!(field.calculate_override().unwrap(), 0);
    assert_eq!(
        host.messages.borrow().last(),
        Some(&wide("You are not allowed to modify this field."))
    );
    assert!(!field.process_committed_data(field.as_ref()).unwrap());
    assert_eq!(host.changed.get(), 0);
    assert!(host.validate.borrow().is_empty());
}
#[test]
fn actual_caption_layout_renders_and_reentry_lease_restores_without_cloning() {
    let doc = CxfaDocument::new();
    let host = Host::new();
    let store = CxfaLayoutStore::new();
    let node = make_node(&doc, E::Field);
    let caption = child(&doc, &node, E::Caption);
    caption.set_enum(A::Placement, V::Top, false);
    measure(&caption, A::Reserve, 20.);
    let value = child(&doc, &caption, E::Value);
    let text = child(&doc, &value, E::Text);
    text.set_content(&wide("Caption"), &wide("Caption"), false);
    let field = CxfaFfField::new(&node);
    attach(
        field.as_ref(),
        &host,
        &store,
        field.clone(),
        CfxPointF::new(5., 5.),
        CfxSizeF::new(100., 50.),
    );
    assert!(field.load_widget().unwrap());
    let layout = host.caption_layout(&node).unwrap();
    layout
        .with_layout(|_| {
            assert_eq!(
                layout.with_layout(|_| Ok(())),
                Err(FfWidgetError::ReentrantTextLayout)
            );
            Ok(())
        })
        .unwrap();
    assert!(layout.with_layout(|layout| Ok(layout.is_loaded())).unwrap());
    let mut bitmap = CfxDibitmap::create(120, 80, FxdibFormat::Bgra, 0).unwrap();
    bitmap.clear(0xffffffff);
    {
        let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
        let mut graphics = CfgasGeGraphics::new(&mut device);
        assert!(field
            .render_caption(&mut graphics, CfxMatrix::default())
            .unwrap());
    }
    assert!(bitmap
        .buffer()
        .chunks_exact(4)
        .any(|pixel| pixel[..3] != [255, 255, 255]));
}
#[test]
fn text_numeric_password_controls_preserve_change_cancellation_commit_and_validation() {
    use crate::xfa::fwl::cfwl_edit::{FWL_STYLEEXT_EDT_NUMBER, FWL_STYLEEXT_EDT_PASSWORD};
    use crate::xfa::fxfa::{
        cxfa_ffnumericedit::CxfaFfNumericEdit, cxfa_ffpasswordedit::CxfaFfPasswordEdit,
        cxfa_fftextedit::CxfaFfTextEdit,
    };
    let doc = CxfaDocument::new();
    let host = Host::new();
    let store = CxfaLayoutStore::new();
    let text_node = make_node(&doc, E::Field);
    let ui = child(&doc, &text_node, E::Ui);
    child(&doc, &ui, E::TextEdit);
    let text = CxfaFfTextEdit::new(&text_node);
    attach(
        text.as_ref(),
        &host,
        &store,
        text.clone(),
        CfxPointF::new(0., 0.),
        CfxSizeF::new(100., 30.),
    );
    assert!(text.load_widget().unwrap());
    assert!(text.on_set_focus(None).unwrap());
    let weak = Rc::downgrade(&text);
    *host.event_hook.borrow_mut() = Some(Rc::new(move |_, activity, event| {
        if activity == V::Change {
            if event.change == wide("x") {
                event.change = wide("X");
            } else if event.change == wide("!") {
                event.cancel_action = true;
            }
            assert!(weak.upgrade().unwrap().base.is_focused());
        }
    }));
    assert!(text.on_char(u32::from('x'), 0));
    assert_eq!(text.get_text(), wide("X"));
    assert!(text.on_char(u32::from('!'), 0));
    assert_eq!(text.get_text(), wide("X"));
    assert!(text.is_data_changed());
    assert!(text.on_kill_focus(None).unwrap());
    assert_eq!(text_node.get_content(), wide("X"));
    assert!(!text.is_data_changed());
    assert_eq!(host.changed.get(), 1);
    assert_eq!(host.validate.borrow().as_slice(), [text_node]);
    *host.event_hook.borrow_mut() = None;
    let numeric_node = make_node(&doc, E::Field);
    let ui = child(&doc, &numeric_node, E::Ui);
    child(&doc, &ui, E::NumericEdit);
    let value = child(&doc, &numeric_node, E::Value);
    let decimal = child(&doc, &value, E::Decimal);
    decimal.set_integer(A::LeadDigits, 3, false);
    decimal.set_integer(A::FracDigits, 2, false);
    let numeric = CxfaFfNumericEdit::new(&numeric_node);
    attach(
        numeric.as_ref(),
        &host,
        &store,
        numeric.clone(),
        CfxPointF::default(),
        CfxSizeF::new(100., 30.),
    );
    numeric.load_widget().unwrap();
    assert!(numeric.on_validate(&wide("123.45")));
    assert!(numeric.on_validate(&wide("-12.3")));
    assert!(!numeric.on_validate(&wide("1234.5")));
    assert!(!numeric.on_validate(&wide("12.345")));
    assert_ne!(
        numeric
            .text_edit
            .field
            .normal_widget()
            .unwrap()
            .base()
            .get_style_exts()
            & FWL_STYLEEXT_EDT_NUMBER,
        0
    );
    let password_node = make_node(&doc, E::Field);
    let ui = child(&doc, &password_node, E::Ui);
    let password_model = child(&doc, &ui, E::PasswordEdit);
    password_model.set_cdata(A::PasswordChar, &wide("*"));
    let password = CxfaFfPasswordEdit::new(&password_node, &password_model);
    attach(
        password.as_ref(),
        &host,
        &store,
        password.clone(),
        CfxPointF::default(),
        CfxSizeF::new(100., 30.),
    );
    password.load_widget().unwrap();
    assert_ne!(
        password
            .text_edit
            .field
            .normal_widget()
            .unwrap()
            .base()
            .get_style_exts()
            & FWL_STYLEEXT_EDT_PASSWORD,
        0
    );
    assert!(password.on_set_focus(None).unwrap());
    assert!(password.on_char(u32::from('s'), 0));
    assert_eq!(password.get_text(), wide("s"));
}
