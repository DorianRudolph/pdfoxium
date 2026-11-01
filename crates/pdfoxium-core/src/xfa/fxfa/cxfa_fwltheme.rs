// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cxfa_fwltheme.{h,cpp}` over actual FF adapters and document font services.
//! Text output ownership is taken out of the cache before callbacks or drawing.
use super::{cxfa_ffapp::CxfaFfApp, cxfa_ffdoc::CxfaFfDoc, cxfa_ffwidget::FfWidget};
use crate::{
    fxcrt::{
        fx_codepage::FxCodePage,
        fx_coordinates::{CfxRectF, CfxSizeF},
    },
    xfa::{
        fde::cfde_textout::CfdeTextOut,
        fgas::{
            font::{cfgas_gefont::CfgasGeFont, cfgas_gemodule},
            graphics::cfgas_gegraphics::CfgasGeGraphics,
        },
        fwl::{
            cfwl_themebackground::CfwlThemeBackground,
            cfwl_themepart::{CfwlThemePart, Part, PartState},
            cfwl_themetext::CfwlThemeText,
            cfwl_widget::FwlType,
            ifwl_themeprovider::{ThemeProvider, ThemeRenderer},
            theme::cfwl_utils::{FWL_THEME_FONT_SIZE, FWL_THEME_TEXT_COLOR},
        },
    },
};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};
pub struct CxfaFwlTheme {
    app: Weak<CxfaFfApp>,
    font: Rc<CfgasGeFont>,
    calendar: RefCell<Option<Rc<CfgasGeFont>>>,
    text_out: RefCell<Option<CfdeTextOut>>,
    renderer: ThemeRenderer,
}
fn widget(part: &CfwlThemePart) -> Option<Rc<dyn FfWidget>> {
    part.widget
        .base()
        .get_outmost()?
        .base()
        .adapter_iface()?
        .xfa_widget()
}
impl CxfaFwlTheme {
    pub fn new(app: Weak<CxfaFfApp>, font: Rc<CfgasGeFont>) -> Rc<Self> {
        Rc::new(Self {
            app,
            font,
            calendar: RefCell::new(None),
            text_out: RefCell::new(None),
            renderer: ThemeRenderer::new(),
        })
    }
    pub fn load_calendar_font(&self, doc: &CxfaFfDoc) -> bool {
        if self.calendar.borrow().is_some() {
            return true;
        }
        let Some(app) = self.app.upgrade() else {
            return false;
        };
        let pdf_fonts = doc.pdf_font_mgr();
        let mut found = None;
        for family in ["Arial", "Courier New", "DejaVu Sans"] {
            let family: Vec<_> = family.chars().map(u32::from).collect();
            found = app.font_mgr().get_font(&pdf_fonts, &family, 0);
            if found.is_some() {
                break;
            }
        }
        if found.is_none() {
            found = cfgas_gemodule::font_manager().get_font_by_code_page(
                FxCodePage::MswinWesternEuropean,
                0,
                None,
            );
        }
        let loaded = found.is_some();
        let previous = self.calendar.replace(found);
        drop(previous);
        drop(app);
        loaded
    }
    pub fn pre_finalize(&self) {
        let output = self.text_out.borrow_mut().take();
        drop(output);
    }
    fn text_font(
        &self,
        params: &CfwlThemeText,
        ff: &Rc<dyn FfWidget>,
    ) -> Option<(Rc<CfgasGeFont>, f32, u32)> {
        if params.widget.get_class_id() == FwlType::MonthCalendar {
            return Some((
                self.calendar.borrow().clone()?,
                FWL_THEME_FONT_SIZE,
                FWL_THEME_TEXT_COLOR,
            ));
        }
        let node = ff.widget().node();
        let host = ff.widget().doc_view()?;
        Some((
            host.get_font(node)?,
            node.get_font_size(),
            node.get_text_color(),
        ))
    }
}
impl ThemeProvider for CxfaFwlTheme {
    fn draw_background(&self, params: &CfwlThemeBackground, graphics: &mut CfgasGeGraphics) {
        self.renderer.draw_background(params, graphics);
    }
    fn draw_text(&self, params: &CfwlThemeText, graphics: &mut CfgasGeGraphics) {
        if params.text.is_empty() {
            return;
        }
        let Some(ff) = widget(&params.base) else {
            return;
        };
        let Some((font, size, mut color)) = self.text_font(params, &ff) else {
            return;
        };
        if params.widget.get_class_id() == FwlType::MonthCalendar {
            if params.part == Part::DatesIn
                && !params.states.intersects(PartState::FLAGGED)
                && params
                    .states
                    .intersects(PartState::HOVERED | PartState::SELECTED)
            {
                color = 0xff888888;
            }
            if params.part == Part::Caption {
                color = 0xff0099ff;
            }
        }
        let cached = self.text_out.borrow_mut().take();
        let mut output = cached.unwrap_or_default();
        output.set_font(font);
        output.set_font_size(size);
        output.set_text_color(color);
        output.set_styles(params.tto_styles);
        output.set_alignment(params.tto_align);
        let mut matrix = params.matrix;
        matrix.concat(graphics.get_matrix());
        output.set_matrix(matrix);
        output.draw_logic_text(
            Some(graphics.get_render_device()),
            &params.text,
            params.part_rect,
        );
        let previous = self.text_out.replace(Some(output));
        drop(previous);
    }
    fn calc_text_rect(&self, params: &CfwlThemeText, rect: &mut CfxRectF) {
        let Some(ff) = widget(&params.base) else {
            return;
        };
        let Some((font, size, color)) = self.text_font(params, &ff) else {
            return;
        };
        let cached = self.text_out.borrow_mut().take();
        let mut output = cached.unwrap_or_default();
        output.set_font(font);
        output.set_font_size(size);
        output.set_text_color(color);
        output.set_styles(params.tto_styles);
        output.set_alignment(params.tto_align);
        output.calc_logic_rect(&params.text, rect);
        let previous = self.text_out.replace(Some(output));
        drop(previous);
    }
    fn get_cx_border_size(&self) -> f32 {
        1.0
    }
    fn get_cy_border_size(&self) -> f32 {
        1.0
    }
    fn get_ui_margin(&self, part: &CfwlThemePart) -> CfxRectF {
        let Some(ff) = widget(part) else {
            return CfxRectF::default();
        };
        let node = ff.widget().node();
        let mut margin = node.get_ui_margin();
        if let Some(para) = node.get_para_if_exists() {
            margin.left += para.para_get_margin_left();
            if node
                .get_ui_child_node()
                .is_some_and(|ui| ui.get_boolean(super::fxfa_basic::XfaAttribute::MultiLine))
            {
                margin.width += para.para_get_margin_right();
            }
        }
        if let Some(item) = ff.widget().layout_item() {
            if item.previous().is_none() {
                if item.next().is_some() {
                    margin.height = 0.0;
                }
            } else if item.next().is_none() {
                margin.top = 0.0;
            } else {
                margin.top = 0.0;
                margin.height = 0.0;
            }
        }
        margin
    }
    fn get_font_size(&self, part: &CfwlThemePart) -> f32 {
        widget(part).map_or(FWL_THEME_FONT_SIZE, |ff| ff.widget().node().get_font_size())
    }
    fn get_font(&self, part: &CfwlThemePart) -> Rc<CfgasGeFont> {
        if let Some(ff) = widget(part) {
            if let Some(host) = ff.widget().doc_view() {
                return host
                    .get_font(ff.widget().node())
                    .expect("actual XFA font service could not resolve a required widget font");
            }
        }
        self.font.clone()
    }
    fn get_fwl_font(&self) -> Rc<CfgasGeFont> {
        self.font.clone()
    }
    fn get_line_height(&self, part: &CfwlThemePart) -> f32 {
        widget(part).map_or(12.0, |ff| ff.widget().node().get_line_height())
    }
    fn get_scroll_bar_width(&self) -> f32 {
        9.0
    }
    fn get_text_color(&self, part: &CfwlThemePart) -> u32 {
        widget(part).map_or(FWL_THEME_TEXT_COLOR, |ff| {
            ff.widget().node().get_text_color()
        })
    }
    fn get_space_above_below(&self, part: &CfwlThemePart) -> CfxSizeF {
        widget(part)
            .and_then(|ff| ff.widget().node().get_para_if_exists())
            .map_or(CfxSizeF::default(), |para| {
                CfxSizeF::new(para.para_get_space_above(), para.para_get_space_below())
            })
    }
}
