// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2020 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `xfa/fwl/ifwl_themeprovider.{h,cpp}`. The host supplies real font and UI
//! metrics; `ThemeRenderer` retains the source's eleven concrete theme choices.
use super::{
    cfwl_themebackground::CfwlThemeBackground,
    cfwl_themepart::CfwlThemePart,
    cfwl_themetext::CfwlThemeText,
    cfwl_widget::FwlType,
    theme::{self, cfwl_utils::FWL_THEME_TEXT_COLOR, cfwl_widgettp::CfwlWidgetTp},
};
use crate::{
    fxcrt::fx_coordinates::{CfxRectF, CfxSizeF},
    xfa::fgas::{font::cfgas_gefont::CfgasGeFont, graphics::cfgas_gegraphics::CfgasGeGraphics},
};
use std::rc::Rc;

pub trait ThemeProvider {
    fn draw_background(&self, params: &CfwlThemeBackground, graphics: &mut CfgasGeGraphics);
    fn draw_text(&self, params: &CfwlThemeText, graphics: &mut CfgasGeGraphics);
    fn calc_text_rect(&self, params: &CfwlThemeText, rect: &mut CfxRectF);
    fn get_cx_border_size(&self) -> f32;
    fn get_cy_border_size(&self) -> f32;
    fn get_ui_margin(&self, part: &CfwlThemePart) -> CfxRectF;
    fn get_font_size(&self, part: &CfwlThemePart) -> f32;
    fn get_font(&self, part: &CfwlThemePart) -> Rc<CfgasGeFont>;
    fn get_fwl_font(&self) -> Rc<CfgasGeFont>;
    fn get_line_height(&self, part: &CfwlThemePart) -> f32;
    fn get_scroll_bar_width(&self) -> f32;
    fn get_text_color(&self, part: &CfwlThemePart) -> u32;
    fn get_space_above_below(&self, part: &CfwlThemePart) -> CfxSizeF;
}

pub struct ThemeRenderer {
    themes: [CfwlWidgetTp; 11],
}
impl Default for ThemeRenderer {
    fn default() -> Self {
        Self::new()
    }
}
impl ThemeRenderer {
    pub fn new() -> Self {
        Self {
            themes: std::array::from_fn(|_| CfwlWidgetTp::new()),
        }
    }
    fn theme(&self, kind: FwlType) -> Option<&CfwlWidgetTp> {
        let index = match kind {
            FwlType::CheckBox => 0,
            FwlType::ListBox => 1,
            FwlType::PictureBox => 2,
            FwlType::ScrollBar => 3,
            FwlType::Edit => 4,
            FwlType::ComboBox => 5,
            FwlType::MonthCalendar => 6,
            FwlType::DateTimePicker => 7,
            FwlType::PushButton => 8,
            FwlType::Caret => 9,
            FwlType::Barcode => 10,
            _ => return None,
        };
        Some(&self.themes[index])
    }
    pub fn draw_background(
        &self,
        params: &CfwlThemeBackground,
        graphics: &mut CfgasGeGraphics,
    ) -> bool {
        match params.widget.get_class_id() {
            FwlType::CheckBox => theme::cfwl_checkboxtp::draw_background(params, graphics),
            FwlType::ListBox => theme::cfwl_listboxtp::draw_background(params, graphics),
            FwlType::PictureBox => theme::cfwl_pictureboxtp::draw_background(params, graphics),
            FwlType::ScrollBar => theme::cfwl_scrollbartp::draw_background(params, graphics),
            FwlType::Edit => theme::cfwl_edittp::draw_background(params, graphics),
            FwlType::ComboBox => theme::cfwl_comboboxtp::draw_background(params, graphics),
            FwlType::MonthCalendar => {
                theme::cfwl_monthcalendartp::draw_background(params, graphics)
            }
            FwlType::DateTimePicker => {
                theme::cfwl_datetimepickertp::draw_background(params, graphics)
            }
            FwlType::PushButton => theme::cfwl_pushbuttontp::draw_background(params, graphics),
            FwlType::Caret => theme::cfwl_carettp::draw_background(params, graphics),
            FwlType::Barcode => theme::cfwl_barcodetp::draw_background(params, graphics),
            _ => return false,
        }
        true
    }
    pub fn draw_text(
        &self,
        provider: &dyn ThemeProvider,
        params: &CfwlThemeText,
        graphics: &mut CfgasGeGraphics,
    ) -> bool {
        let kind = params.widget.get_class_id();
        let Some(theme) = self.theme(kind) else {
            return false;
        };
        let color = match kind {
            FwlType::CheckBox => theme::cfwl_checkboxtp::text_color(params),
            FwlType::MonthCalendar => theme::cfwl_monthcalendartp::text_color(params),
            _ => FWL_THEME_TEXT_COLOR,
        };
        theme.draw_text_with_color(provider, params, graphics, color);
        true
    }
    pub fn calc_text_rect(
        &self,
        provider: &dyn ThemeProvider,
        params: &CfwlThemeText,
        rect: &mut CfxRectF,
    ) -> bool {
        let Some(theme) = self.theme(params.widget.get_class_id()) else {
            return false;
        };
        theme.calc_text_rect(provider, params, rect);
        true
    }
}
