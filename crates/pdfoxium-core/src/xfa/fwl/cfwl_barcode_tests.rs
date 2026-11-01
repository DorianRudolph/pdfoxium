// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Connected widget coverage for the source barcode cache, edit/focus switch,
//! option forwarding and callback lifetime. These are Rust widget tests, not a
//! claim to have run a native document/FORM embedder suite.
use super::*;
use crate::{
    fxcrt::{cfx_timer::TimerHandler, fx_coordinates::CfxSizeF},
    fxge::{
        agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
        dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat},
    },
    xfa::fwl::{
        cfwl_app::AppAdapter,
        cfwl_themebackground::CfwlThemeBackground,
        cfwl_themetext::CfwlThemeText,
        cfwl_widget::WidgetStyle,
        cfwl_widgetmgr::WidgetMgrAdapter,
        ifwl_themeprovider::ThemeProvider,
        test_support::{new_app, TestHost},
    },
};
use std::rc::Weak;

fn wide(value: &str) -> Vec<u32> {
    value.chars().map(u32::from).collect()
}
fn barcode(app: &Rc<CfwlApp>) -> Rc<CfwlBarcode> {
    let control = CfwlBarcode::with_properties(
        app,
        Properties {
            styles: WidgetStyle::CHILD | WidgetStyle::BORDER,
            ..Default::default()
        },
        None,
    );
    control
        .base()
        .set_widget_rect(CfxRectF::new(0., 0., 300., 70.));
    control.set_type(BcType::Code39);
    control.set_text_location(BcTextLoc::None);
    control.set_text(&wide("ABC123"));
    control.update();
    control
}
fn render(control: &CfwlBarcode) -> Vec<u8> {
    let mut bitmap = CfxDibitmap::create(340, 100, FxdibFormat::Bgra, 0).unwrap();
    bitmap.clear(0xffffffff);
    {
        let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
        let mut graphics = CfgasGeGraphics::new(&mut device);
        control.draw_widget(&mut graphics, CfxMatrix::default());
    }
    bitmap.buffer().to_vec()
}
fn ink(bytes: &[u8]) -> bool {
    bytes.chunks_exact(4).any(|p| p[..3] != [255, 255, 255])
}

#[test]
fn source_unknown_focus_and_protection_use_the_real_barcode_and_edit_views() {
    let (app, _) = new_app();
    let unknown = CfwlBarcode::new(&app);
    unknown
        .base()
        .set_widget_rect(CfxRectF::new(0., 0., 300., 70.));
    unknown.update();
    assert!(unknown.is_protected_type());
    assert!(!unknown.encode_succeeded());
    assert!(!ink(&render(&unknown)));

    let control = barcode(&app);
    assert_eq!(control.get_class_id(), FwlType::Barcode);
    assert!(control.encode_succeeded());
    assert!(!control.is_protected_type());
    let bars = render(&control);
    assert!(ink(&bars));
    // The source computes the client translation; the port actually applies it.
    assert!(!ink(&bars[..340 * 4]));
    assert!(bars
        .chunks_exact(340 * 4)
        .all(|row| row[..4] == [255, 255, 255, 255]));
    control.set_states(WidgetState::FOCUSED);
    control.update();
    let editing = render(&control);
    assert!(ink(&editing));
    assert_ne!(editing, bars);
    control.clear_states(WidgetState::FOCUSED);
    assert_eq!(render(&control), bars);
}

#[test]
fn committed_inherited_edits_undo_and_redo_refresh_cached_bars() {
    let (app, _) = new_app();
    let control = barcode(&app);
    let before = render(&control);
    let key = control.cache.borrow().as_ref().unwrap().key;
    assert_eq!(render(&control), before);
    assert_eq!(control.cache.borrow().as_ref().unwrap().key, key);
    control.set_cursor_position(control.get_text_length());
    assert!(control.paste(&wide("9")));
    assert!(!control.encode_succeeded());
    let after = render(&control);
    assert!(control.encode_succeeded());
    assert_ne!(after, before);
    assert!(control.undo());
    assert_eq!(control.get_text(), wide("ABC123"));
    assert_eq!(render(&control), before);
    assert!(control.redo());
    assert_eq!(render(&control), after);
}

#[test]
fn source_options_limit_text_and_changed_options_and_geometry_invalidate_cache() {
    let (app, _) = new_app();
    let control = barcode(&app);
    let key = control.key();
    control.set_wide_narrow_ratio(3);
    assert_ne!(control.key(), key);
    assert!(control.cache.borrow().is_none());
    let changed = control.key();
    control.set_wide_narrow_ratio(3);
    assert_eq!(control.key(), changed);
    control.set_module_height(2);
    control.set_module_width(2);
    control.set_cal_checksum(true);
    control.set_print_checksum(true);
    control.set_start_char(b'A');
    control.set_end_char(b'B');
    // Unsupported setters retain the source's ignored-return behavior: an
    // error-correction option does not disable a linear code.
    control.set_error_correction_level(2);
    // FDE clamps a new character limit to the current text length. Install
    // this shorter limit on an empty edit, as an initial barcode setup does.
    control.clear_text();
    control.set_data_length(3);
    control.set_text(&wide("ABCDEF"));
    assert_eq!(control.get_limit(), 3);
    assert_eq!(control.get_text(), wide("ABC"));
    control.update();
    assert!(control.encode_succeeded());
    let key = control.cache.borrow().as_ref().unwrap().key;
    control
        .base()
        .set_widget_rect(CfxRectF::new(0., 0., 240., 60.));
    control.update();
    assert!(control.encode_succeeded());
    assert_ne!(control.cache.borrow().as_ref().unwrap().key.rect, key.rect);
    assert!(ink(&render(&control)));
}

#[test]
fn source_two_dimensional_protection_and_type_changes_follow_real_engines() {
    let (app, _) = new_app();
    let control = barcode(&app);
    for kind in [BcType::QrCode, BcType::Pdf417, BcType::DataMatrix] {
        control.set_type(kind);
        assert!(control.is_protected_type());
        control.update();
        assert!(control.encode_succeeded(), "{kind:?}");
        assert!(control.is_protected_type());
        assert!(ink(&render(&control)), "{kind:?}");
    }
    control.set_type(BcType::Code39);
    assert!(control.is_protected_type());
    control.update();
    assert!(!control.is_protected_type());
    assert!(control.encode_succeeded());
}

#[test]
fn rejected_content_stays_failed_until_changed_and_does_not_requery_the_host() {
    let host = ReentrantHost::new();
    let app = CfwlApp::new(host.clone());
    let control = barcode(&app);
    control.set_text(&wide("@")); // Outside the source Code39 alphabet.
    control.update();
    assert!(!control.encode_succeeded());
    assert!(!control.is_protected_type()); // The linear engine still exists.
    assert!(!control.cache.borrow().as_ref().unwrap().encoded);
    *host.font_hook.borrow_mut() = Some(Box::new(|| panic!("failed cache was regenerated")));
    assert!(!ink(&render(&control)));
    assert!(!ink(&render(&control)));
    host.font_hook.borrow_mut().take();
    control.set_text_skip_notify(&wide("VALID"));
    assert!(ink(&render(&control)));
    assert!(control.encode_succeeded());
}

// Exercise a real host font query during barcode generation. Taking the hook
// before calling it permits recursive font queries and edit/layout callbacks.
struct ReentrantHost {
    myself: Weak<Self>,
    host: Rc<TestHost>,
    font_hook: RefCell<Option<Box<dyn FnOnce()>>>,
}
impl ReentrantHost {
    fn new() -> Rc<Self> {
        Rc::new_cyclic(|myself| Self {
            myself: myself.clone(),
            host: TestHost::new(),
            font_hook: RefCell::new(None),
        })
    }
}
impl AppAdapter for ReentrantHost {
    fn widget_mgr_adapter(&self) -> Rc<dyn WidgetMgrAdapter> {
        self.host.clone()
    }
    fn timer_handler(&self) -> Option<Rc<dyn TimerHandler>> {
        Some(self.host.clone())
    }
    fn theme_provider(&self) -> Rc<dyn ThemeProvider> {
        self.myself.upgrade().unwrap()
    }
}
impl ThemeProvider for ReentrantHost {
    fn draw_background(&self, p: &CfwlThemeBackground, g: &mut CfgasGeGraphics) {
        self.host.draw_background(p, g);
    }
    fn draw_text(&self, p: &CfwlThemeText, g: &mut CfgasGeGraphics) {
        self.host.draw_text(p, g);
    }
    fn calc_text_rect(&self, p: &CfwlThemeText, r: &mut CfxRectF) {
        self.host.calc_text_rect(p, r);
    }
    fn get_cx_border_size(&self) -> f32 {
        self.host.get_cx_border_size()
    }
    fn get_cy_border_size(&self) -> f32 {
        self.host.get_cy_border_size()
    }
    fn get_ui_margin(&self, p: &CfwlThemePart) -> CfxRectF {
        self.host.get_ui_margin(p)
    }
    fn get_font_size(&self, p: &CfwlThemePart) -> f32 {
        self.host.get_font_size(p)
    }
    fn get_font(&self, p: &CfwlThemePart) -> Rc<CfgasGeFont> {
        let hook = self.font_hook.borrow_mut().take();
        if let Some(hook) = hook {
            hook();
        }
        self.host.get_font(p)
    }
    fn get_fwl_font(&self) -> Rc<CfgasGeFont> {
        self.host.get_fwl_font()
    }
    fn get_line_height(&self, p: &CfwlThemePart) -> f32 {
        self.host.get_line_height(p)
    }
    fn get_scroll_bar_width(&self) -> f32 {
        self.host.get_scroll_bar_width()
    }
    fn get_text_color(&self, p: &CfwlThemePart) -> u32 {
        self.host.get_text_color(p)
    }
    fn get_space_above_below(&self, p: &CfwlThemePart) -> CfxSizeF {
        self.host.get_space_above_below(p)
    }
}

#[test]
fn reentrant_font_query_discards_stale_generation_then_encodes_committed_text() {
    let host = ReentrantHost::new();
    let app = CfwlApp::new(host.clone());
    let control = barcode(&app);
    control.set_module_width(2);
    let weak = Rc::downgrade(&control);
    *host.font_hook.borrow_mut() = Some(Box::new(move || {
        let control = weak.upgrade().unwrap();
        assert!(control.cache.borrow().is_none());
        // Recursive drawing while the host supplies the font must return
        // without querying the theme again or publishing an incomplete engine.
        assert!(!ink(&render(&control)));
        control.set_text_skip_notify(&wide("NEW123"));
    }));
    assert!(!ink(&render(&control)));
    assert_eq!(control.get_text(), wide("NEW123"));
    assert!(control.cache.borrow().is_none());
    assert!(ink(&render(&control)));
    assert!(control.encode_succeeded());
    assert_eq!(control.cache.borrow().as_ref().unwrap().key, control.key());
}

#[test]
fn removal_from_font_callback_does_not_publish_or_draw_a_dead_widget_cache() {
    let host = ReentrantHost::new();
    let app = CfwlApp::new(host.clone());
    let control = barcode(&app);
    control.set_module_width(2);
    let weak = Rc::downgrade(&control);
    let weak_app = Rc::downgrade(&app);
    *host.font_hook.borrow_mut() = Some(Box::new(move || {
        weak_app
            .upgrade()
            .unwrap()
            .remove_widget(weak.upgrade().unwrap().base().id());
    }));
    assert!(!ink(&render(&control)));
    assert!(control.base().is_destroyed());
    assert!(control.cache.borrow().is_none());
    assert!(!control.encode_succeeded());
}
