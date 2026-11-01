// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `xfa/fwl/cfwl_barcode.{h,cpp}`. The editable view and barcode share one
//! widget identity. Cache keys track committed text, geometry and options;
//! host font/metric callbacks execute without a cache/engine borrow.
use super::{
    cfwl_app::CfwlApp,
    cfwl_edit::CfwlEdit,
    cfwl_event::{EventType, FwlEvent},
    cfwl_message::FwlMessage,
    cfwl_themepart::{CfwlThemePart, Part},
    cfwl_widget::{CfwlWidget, FwlType, FwlWidget, Properties, WidgetRef, WidgetState},
    fwl_widgethit::FwlWidgetHit,
};
use crate::{
    fxbarcode::{
        bc_library::{BcTextLoc, BcType},
        bc_writer::BarcodeFont,
        cfx_barcode::CfxBarcode,
    },
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF},
    fxge::cfx_font::CfxFont,
    xfa::fgas::{font::cfgas_gefont::CfgasGeFont, graphics::cfgas_gegraphics::CfgasGeGraphics},
};
use std::{
    cell::{Cell, RefCell},
    ops::Deref,
    rc::Rc,
};

// Retain the complete FGAS owner, including PDF-font ownership and synthetic
// style metadata, instead of copying font bytes or rebuilding its face.
impl BarcodeFont for CfgasGeFont {
    fn dev_font(&self) -> &CfxFont<'static> {
        self.get_dev_font()
    }
}

#[derive(Clone, Copy, Default, Debug, PartialEq)]
struct Options {
    text_location: Option<BcTextLoc>,
    cal_checksum: Option<bool>,
    print_checksum: Option<bool>,
    start_char: Option<u8>,
    end_char: Option<u8>,
    wide_narrow_ratio: Option<i8>,
    module_height: Option<i32>,
    module_width: Option<i32>,
    data_length: Option<i32>,
    ec_level: Option<i32>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CacheKey {
    settings: u64,
    text: u64,
    rect: [u32; 4],
}
struct CachedBarcode {
    key: CacheKey,
    engine: CfxBarcode,
    encoded: bool,
}
struct GenerationGuard<'a>(&'a Cell<bool>);
impl Drop for GenerationGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}
pub struct CfwlBarcode {
    edit: CfwlEdit,
    kind: Cell<BcType>,
    options: Cell<Options>,
    settings_revision: Cell<u64>,
    generating: Cell<bool>,
    cache: RefCell<Option<CachedBarcode>>,
}
impl CfwlBarcode {
    pub fn new(app: &Rc<CfwlApp>) -> Rc<Self> {
        Self::with_properties(app, Properties::default(), None)
    }
    pub fn with_properties(
        app: &Rc<CfwlApp>,
        properties: Properties,
        outer: Option<WidgetRef>,
    ) -> Rc<Self> {
        let barcode = Rc::new(Self {
            edit: CfwlEdit::new_unregistered(app, properties, outer),
            kind: Cell::new(BcType::Unknown),
            options: Cell::new(Options::default()),
            settings_revision: Cell::new(0),
            generating: Cell::new(false),
            cache: RefCell::new(None),
        });
        app.register_widget(barcode.clone());
        barcode
    }
    fn invalidate(&self) {
        self.settings_revision
            .set(self.settings_revision.get().wrapping_add(1));
        let old = self.cache.borrow_mut().take();
        drop(old);
    }
    fn key(&self) -> CacheKey {
        let r = self.edit.client_rect();
        CacheKey {
            settings: self.settings_revision.get(),
            text: self.edit.text_revision(),
            rect: [
                r.left.to_bits(),
                r.top.to_bits(),
                r.width.to_bits(),
                r.height.to_bits(),
            ],
        }
    }
    pub fn set_type(&self, kind: BcType) {
        if self.kind.replace(kind) != kind {
            self.invalidate();
        }
    }
    pub fn get_type(&self) -> BcType {
        self.kind.get()
    }
    pub fn set_text(&self, text: &[u32]) {
        self.invalidate();
        self.edit.set_text(text);
    }
    pub fn set_text_skip_notify(&self, text: &[u32]) {
        self.invalidate();
        self.edit.set_text_skip_notify(text);
    }
    pub fn is_protected_type(&self) -> bool {
        self.cache.borrow().as_ref().is_none_or(|cache| {
            matches!(
                cache.engine.get_type(),
                BcType::QrCode | BcType::Pdf417 | BcType::DataMatrix
            )
        })
    }
    fn set_option(&self, change: impl FnOnce(&mut Options)) {
        let mut options = self.options.get();
        change(&mut options);
        if options != self.options.replace(options) {
            self.invalidate();
        }
    }
    pub fn set_module_height(&self, value: i32) {
        self.set_option(|o| o.module_height = Some(value));
    }
    pub fn set_module_width(&self, value: i32) {
        self.set_option(|o| o.module_width = Some(value));
    }
    pub fn set_data_length(&self, value: i32) {
        self.set_option(|o| o.data_length = Some(value));
        self.edit.set_limit(value);
    }
    pub fn set_cal_checksum(&self, value: bool) {
        self.set_option(|o| o.cal_checksum = Some(value));
    }
    pub fn set_print_checksum(&self, value: bool) {
        self.set_option(|o| o.print_checksum = Some(value));
    }
    pub fn set_text_location(&self, value: BcTextLoc) {
        self.set_option(|o| o.text_location = Some(value));
    }
    pub fn set_wide_narrow_ratio(&self, value: i8) {
        self.set_option(|o| o.wide_narrow_ratio = Some(value));
    }
    pub fn set_start_char(&self, value: u8) {
        self.set_option(|o| o.start_char = Some(value));
    }
    pub fn set_end_char(&self, value: u8) {
        self.set_option(|o| o.end_char = Some(value));
    }
    pub fn set_error_correction_level(&self, value: i32) {
        self.set_option(|o| o.ec_level = Some(value));
    }
    pub fn encode_succeeded(&self) -> bool {
        let key = self.key();
        self.cache
            .borrow()
            .as_ref()
            .is_some_and(|cache| cache.key == key && cache.encoded)
    }
    fn generate_barcode_image_cache(&self) {
        if self.base().is_destroyed() {
            return;
        }
        let key = self.key();
        if self
            .cache
            .borrow()
            .as_ref()
            .is_some_and(|cache| cache.key == key)
        {
            return;
        }
        let kind = self.kind.get();
        if kind == BcType::Unknown {
            return;
        }
        let Some(app) = self.base().app() else {
            return;
        };
        // The source changes NeedUpdate to Normal before querying the theme.
        // Preserve that protection against a font callback drawing recursively.
        if self.generating.replace(true) {
            return;
        }
        let _generation = GenerationGuard(&self.generating);
        let theme = app.theme_provider();
        let part = CfwlThemePart::new(Part::None, self);
        // These host calls may reenter, edit or destroy this control. Only a
        // matching generation may publish the subsequently encoded image.
        let font = theme.get_font(&part);
        let font_size = theme.get_font_size(&part);
        let font_color = theme.get_text_color(&part);
        if self.base().is_destroyed() || self.key() != key || self.kind.get() != kind {
            return;
        }
        let mut engine = CfxBarcode::new(kind);
        let font: Rc<dyn BarcodeFont> = font;
        engine.set_font(font);
        engine.set_font_size(font_size);
        engine.set_font_color(font_color);
        let client = self.edit.client_rect();
        engine.set_height(client.height as i32);
        engine.set_width(client.width as i32);
        let options = self.options.get();
        if let Some(value) = options.module_height {
            engine.set_module_height(value);
        }
        if let Some(value) = options.module_width {
            engine.set_module_width(value);
        }
        if let Some(value) = options.data_length {
            engine.set_data_length(value);
        }
        if let Some(value) = options.cal_checksum {
            engine.set_cal_checksum(value);
        }
        if let Some(value) = options.print_checksum {
            engine.set_print_checksum(value);
        }
        if let Some(value) = options.text_location {
            engine.set_text_location(value);
        }
        if let Some(value) = options.wide_narrow_ratio {
            engine.set_wide_narrow_ratio(value);
        }
        if let Some(value) = options.start_char {
            engine.set_start_char(value);
        }
        if let Some(value) = options.end_char {
            engine.set_end_char(value);
        }
        if let Some(value) = options.ec_level {
            engine.set_error_correction_level(value);
        }
        let encoded = engine.encode(&self.edit.get_text());
        if !self.base().is_destroyed() && self.key() == key && self.kind.get() == kind {
            let old = self.cache.replace(Some(CachedBarcode {
                key,
                engine,
                encoded,
            }));
            drop(old);
        }
    }
}
impl Deref for CfwlBarcode {
    type Target = CfwlEdit;
    fn deref(&self) -> &Self::Target {
        &self.edit
    }
}
impl FwlWidget for CfwlBarcode {
    fn as_edit(&self) -> Option<&super::cfwl_edit::CfwlEdit> {
        Some(&self.edit)
    }

    fn base(&self) -> &CfwlWidget {
        self.edit.base()
    }
    fn get_class_id(&self) -> FwlType {
        FwlType::Barcode
    }
    fn update(&self) {
        if self.base().is_locked() {
            return;
        }
        self.edit.update();
        self.generate_barcode_image_cache();
    }
    fn draw_widget(&self, graphics: &mut CfgasGeGraphics<'_, '_, '_>, matrix: CfxMatrix) {
        if self.base().get_states().contains(WidgetState::FOCUSED) {
            self.edit.draw_widget(graphics, matrix);
            return;
        }
        self.generate_barcode_image_cache();
        let cached = self.cache.borrow_mut().take();
        let Some(mut cached) = cached else {
            return;
        };
        if cached.encoded && cached.key == self.key() {
            // The source computes this client translation then accidentally
            // discards it. Apply it so a bordered widget draws inside its client.
            let client = self.edit.client_rect();
            let mut transform = CfxMatrix::new(1., 0., 0., 1., client.left, client.top);
            transform.concat(matrix);
            cached
                .engine
                .render_device(graphics.get_render_device(), transform);
        }
        if !self.base().is_destroyed() && self.cache.borrow().is_none() && cached.key == self.key()
        {
            *self.cache.borrow_mut() = Some(cached);
        }
    }
    fn on_process_event(&self, event: &mut FwlEvent) {
        if event.get_type() == EventType::TextWillChange {
            self.invalidate();
        }
        self.edit.on_process_event(event);
    }
    fn on_process_message(&self, message: &mut FwlMessage) {
        self.edit.on_process_message(message);
    }
    fn get_widget_rect(&self) -> CfxRectF {
        self.edit.get_widget_rect()
    }
    fn get_client_rect(&self) -> CfxRectF {
        self.edit.get_client_rect()
    }
    fn get_autosized_widget_rect(&self) -> CfxRectF {
        self.edit.get_autosized_widget_rect()
    }
    fn hit_test(&self, point: CfxPointF) -> FwlWidgetHit {
        self.edit.hit_test(point)
    }
    fn set_states(&self, states: WidgetState) {
        self.edit.set_states(states);
    }
    fn clear_states(&self, states: WidgetState) {
        self.edit.clear_states(states);
    }
    fn modify_style_exts(&self, added: u32, removed: u32) {
        self.edit.modify_style_exts(added, removed);
    }
    fn pre_finalize(&self) {
        self.invalidate();
        self.edit.pre_finalize();
    }
}

#[cfg(test)]
#[path = "cfwl_barcode_tests.rs"]
mod tests;
