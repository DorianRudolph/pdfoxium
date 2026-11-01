// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Text measurement, rich/plain loading and AGG drawing from
//! `cxfa_textlayout.{h,cpp}`. Font selection is supplied by the real owner.
//! Pagination resumes at actual break boundaries using saved model/XML
//! positions; no previous text is reshaped to synthesize individual blocks.
use super::{
    cxfa_textparser::{CxfaTextParser, XfaTextFontHost},
    cxfa_textprovider::CxfaTextProvider,
    cxfa_texttabstopscontext::CxfaTextTabstopsContext,
    fxfa_basic::{XfaAttribute, XfaAttributeValue, XfaUnit},
    parser::{cxfa_document::CxfaDocument, cxfa_node::CxfaNode},
};
use crate::{
    fxcrt::{
        css::{
            cfx_css::*, cfx_csscomputedstyle::CfxCssComputedStyle,
            cfx_cssdata::hash_code_lowered_wide,
        },
        fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF, CfxSizeF},
        xml::cfx_xmlnode::{XmlNodeRef, XmlNodeType},
    },
    fxge::{
        agg::cfx_agg_devicedriver::CfxAggDeviceDriver, cfx_fillrenderoptions::CfxFillRenderOptions,
        cfx_graphstatedata::CfxGraphStateData, cfx_path::CfxPath, text_char_pos::TextCharPos,
    },
    xfa::{
        fde::cfde_textout::CfdeTextOut,
        fgas::{
            font::cfgas_gefont::CfgasGeFont,
            layout::{
                cfgas_break::LayoutStyle,
                cfgas_char::BreakType,
                cfgas_linkuserdata::CfgasLinkUserData,
                cfgas_rtfbreak::{CfgasRtfBreak, LineAlignment},
                cfgas_textpiece::CfgasTextPiece,
                cfgas_textuserdata::CfgasTextUserData,
            },
        },
    },
};
use std::rc::Rc;

#[path = "cxfa_textlayout_blocks.rs"]
mod blocks;
use blocks::{BlockData, LoaderContext};

fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
fn eq(value: &[u32], text: &str) -> bool {
    value.iter().copied().eq(text.bytes().map(u32::from))
}
fn process_text(text: &mut Vec<u32>) {
    let mut length = 0;
    let mut previous = 0;
    for index in 0..text.len() {
        let ch = text[index].max(32);
        if ch == 32 && previous == 32 {
            continue;
        }
        previous = ch;
        text[length] = ch;
        length += 1;
    }
    text.truncate(length);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextLayoutError {
    DetachedModel,
    FontUnavailable,
    InvalidSize,
    RenderingFailed,
    NestingLimit,
}
impl std::fmt::Display for TextLayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for TextLayoutError {}
type Result<T> = std::result::Result<T, TextLayoutError>;

#[derive(Clone)]
pub struct TextPiece {
    pub piece: CfgasTextPiece,
    pub underline: i32,
    pub line_through: i32,
    pub period: XfaAttributeValue,
    pub color: u32,
    pub link_data: Option<Rc<CfgasLinkUserData>>,
}
#[derive(Default)]
pub struct PieceLine {
    pub text_pieces: Vec<TextPiece>,
}

pub struct CxfaTextLayout {
    // Native FFDoc keeps both model/XML owners alive while pieces are loaded.
    _document: Rc<CxfaDocument>,
    provider: Rc<CxfaTextProvider>,
    font_host: Rc<dyn XfaTextFontHost>,
    text_parser: CxfaTextParser,
    text_data_node: Option<CxfaNode>,
    rich_text: bool,
    breaker: Option<CfgasRtfBreak>,
    piece_lines: Vec<PieceLine>,
    tabstop_context: Option<CxfaTextTabstopsContext>,
    lines: usize,
    max_width: f32,
    line_heights: Vec<f32>,
    save_line_height: bool,
    last_pos: f32,
    loader_context: Option<LoaderContext>,
    blocks: Vec<BlockData>,
    has_block: bool,
}
impl CxfaTextLayout {
    pub fn new(provider: Rc<CxfaTextProvider>, font_host: Rc<dyn XfaTextFontHost>) -> Result<Self> {
        let document = provider
            .node()
            .document()
            .ok_or(TextLayoutError::DetachedModel)?;
        Ok(Self {
            _document: document,
            provider,
            font_host,
            text_parser: CxfaTextParser::new(),
            text_data_node: None,
            rich_text: false,
            breaker: None,
            piece_lines: Vec::new(),
            tabstop_context: None,
            lines: 0,
            max_width: 0.0,
            line_heights: Vec::new(),
            save_line_height: false,
            last_pos: 0.0,
            loader_context: None,
            blocks: Vec::new(),
            has_block: false,
        })
    }
    pub fn is_loaded(&self) -> bool {
        !self.piece_lines.is_empty()
    }
    pub fn piece_lines(&self) -> &[PieceLine] {
        &self.piece_lines
    }
    pub fn line_heights(&self) -> &[f32] {
        &self.line_heights
    }
    pub fn unload(&mut self) {
        self.piece_lines.clear();
        self.breaker = None;
    }
    pub fn get_link_url_at_point(&self, point: CfxPointF) -> Vec<u32> {
        self.piece_lines
            .iter()
            .flat_map(|line| &line.text_pieces)
            .find_map(|piece| {
                piece
                    .link_data
                    .as_ref()
                    .filter(|_| piece.piece.rect.contains_point(point))
                    .map(|link| link.get_link_url().to_vec())
            })
            .unwrap_or_default()
    }
    fn selected_font(&self, style: Option<&CfxCssComputedStyle>) -> Result<Rc<CfgasGeFont>> {
        self.text_parser
            .get_font(self.font_host.as_ref(), &self.provider, style)
            .ok_or(TextLayoutError::FontUnavailable)
    }
    fn create_break(&self, default: bool) -> Result<CfgasRtfBreak> {
        let flags = if default {
            LayoutStyle::EXPAND_TAB
        } else {
            LayoutStyle::EXPAND_TAB | LayoutStyle::PAGINATION
        };
        let mut breaker = CfgasRtfBreak::new(flags);
        breaker.set_line_break_tolerance(1.0);
        breaker.set_font(self.selected_font(None)?);
        breaker.set_font_size(self.text_parser.get_font_size(&self.provider, None));
        Ok(breaker)
    }
    fn get_text_data_node(&mut self) {
        let (node, rich) = self.provider.get_text_node();
        self.rich_text = rich;
        if node.is_some() && rich {
            self.text_parser.reset();
        }
        self.text_data_node = node;
    }
    fn get_xml_container_node(&self) -> Option<XmlNodeRef> {
        if !self.rich_text {
            return None;
        }
        let root = self.text_data_node.as_ref()?.get_xml_mapping_node()?;
        let mut child = root.first_child();
        while let Some(node) = child {
            if node.node_type() == XmlNodeType::Element {
                let tag = node.local_tag_name();
                if eq(&tag, "body") || eq(&tag, "html") {
                    return Some(node);
                }
            }
            child = node.next_sibling();
        }
        None
    }
    fn init_break(&mut self, mut width: f32) -> Result<()> {
        let mut start = 0.0;
        let mut start_pos = 0.0;
        let font = self.selected_font(None)?;
        let breaker = self.breaker.as_mut().unwrap();
        if let Some(para) = self.provider.get_para_if_exists() {
            let alignment = match para.get_enum(XfaAttribute::HAlign) {
                XfaAttributeValue::Center => LineAlignment::Center,
                XfaAttributeValue::Right => LineAlignment::Right,
                XfaAttributeValue::Justify => LineAlignment::Justified,
                XfaAttributeValue::JustifyAll => LineAlignment::Distributed,
                _ => LineAlignment::Left,
            };
            breaker.set_alignment(alignment);
            start = para.get_measure_in_unit(XfaAttribute::MarginLeft, XfaUnit::Pt);
            if !self.provider.is_check_button_and_auto_width() || alignment != LineAlignment::Left {
                width -= para.get_measure_in_unit(XfaAttribute::MarginRight, XfaUnit::Pt);
            }
            if width < 0.0 {
                width = start;
            }
            start_pos = start
                + para
                    .get_measure_in_unit(XfaAttribute::TextIndent, XfaUnit::Pt)
                    .max(0.0);
        }
        breaker.set_line_boundary(start, width);
        breaker.set_line_start_pos(start_pos);
        if let Some(font) = self.provider.get_font_if_exists() {
            breaker.set_horizontal_scale(font.font_get_horizontal_scale() as i32);
            breaker.set_vertical_scale(font.font_get_vertical_scale() as i32);
            breaker.set_char_space(font.font_get_letter_spacing());
        }
        let size = self.text_parser.get_font_size(&self.provider, None);
        breaker.set_font_size(size);
        breaker.set_font(font);
        breaker.set_line_break_tolerance(size * 0.2);
        Ok(())
    }
    fn init_rich_break(
        &mut self,
        style: &mut CfxCssComputedStyle,
        display: CfxCssDisplay,
        mut width: f32,
        xml: &XmlNodeRef,
        parent: Option<&CfxCssComputedStyle>,
    ) -> Result<()> {
        let font = self.selected_font(Some(style))?;
        let breaker = self.breaker.as_mut().unwrap();
        if matches!(display, CfxCssDisplay::Block | CfxCssDisplay::ListItem) {
            breaker.set_alignment(match style.get_text_align() {
                CfxCssTextAlign::Right => LineAlignment::Right,
                CfxCssTextAlign::Center => LineAlignment::Center,
                CfxCssTextAlign::Justify => LineAlignment::Justified,
                CfxCssTextAlign::JustifyAll => LineAlignment::Distributed,
                _ => LineAlignment::Left,
            });
            let mut start = 0.0;
            if let Some(rect) = style.get_margin_width().copied() {
                start = rect.left.value;
                width -= rect.right.value;
                if let Some(padding) = style.get_padding_width() {
                    start += padding.left.value;
                    width -= padding.right.value;
                }
                if display == CfxCssDisplay::ListItem {
                    if let Some(parent) = parent {
                        if let Some(margin) = parent.get_margin_width() {
                            start += margin.left.value;
                            width -= margin.right.value;
                            if let Some(padding) = parent.get_padding_width() {
                                start += padding.left.value;
                                width -= padding.right.value;
                            }
                        }
                    }
                    style.set_margin_width(&CfxCssRect {
                        left: CfxCssLength::new(CfxCssLengthUnit::Point, start),
                        ..rect
                    });
                }
            }
            breaker.set_line_boundary(start, width);
            start += style.get_text_indent().value.max(0.0);
            breaker.set_line_start_pos(start);
            breaker.set_tab_width(self.text_parser.get_tab_interval(Some(style)));
            let context = self
                .tabstop_context
                .get_or_insert_with(CxfaTextTabstopsContext::new);
            self.text_parser.get_tabstops(Some(style), context);
            for stop in &context.tabstops {
                breaker.add_positioned_tab(stop.position);
            }
        }
        let size = self.text_parser.get_font_size(&self.provider, Some(style));
        breaker.set_font_size(size);
        breaker.set_line_break_tolerance(size * 0.2);
        breaker.set_font(font);
        breaker.set_horizontal_scale(self.text_parser.get_hor_scale(
            &self.provider,
            Some(style),
            Some(xml),
        ));
        breaker.set_vertical_scale(self.text_parser.get_ver_scale(&self.provider, Some(style)));
        breaker.set_char_space(style.get_letter_spacing().value);
        Ok(())
    }
    pub fn calc_size(&mut self, _minimum: CfxSizeF, maximum: CfxSizeF) -> Result<CfxSizeF> {
        if !maximum.width.is_finite() || !maximum.height.is_finite() {
            return Err(TextLayoutError::InvalidSize);
        }
        let width = if maximum.width < 1.0 {
            65535.0
        } else {
            maximum.width
        };
        self.breaker = Some(self.create_break(false)?);
        self.lines = 0;
        self.max_width = 0.0;
        self.last_pos = 0.0;
        self.line_heights.clear();
        self.save_line_height = true;
        let mut position = 0.0;
        let result = self.loader(width, &mut position, false);
        self.save_line_height = false;
        self.tabstop_context = None;
        result?;
        if position < 0.1 {
            position = self.text_parser.get_font_size(&self.provider, None);
        }
        Ok(CfxSizeF {
            width: self.max_width,
            height: position,
        })
    }
    pub fn layout(&mut self, size: CfxSizeF) -> Result<f32> {
        if !size.width.is_finite() || !size.height.is_finite() {
            return Err(TextLayoutError::InvalidSize);
        }
        if size.width < 1.0 {
            return Ok(0.0);
        }
        self.unload();
        self.breaker = Some(self.create_break(true)?);
        if let Some(context) = self.loader_context.as_mut() {
            context.total_lines = None;
            context.char_index = 0;
        }
        self.lines = 0;
        let mut position = 0.0;
        let result = self.loader(size.width, &mut position, true);
        self.tabstop_context = None;
        if let Err(error) = result {
            self.unload();
            return Err(error);
        }
        self.update_align(size.height, position);
        Ok(position)
    }
    fn update_align(&mut self, height: f32, bottom: f32) {
        let mut offset = height - bottom;
        if offset < 0.1 {
            return;
        }
        match self.text_parser.get_v_align(&self.provider) {
            XfaAttributeValue::Middle => offset /= 2.0,
            XfaAttributeValue::Bottom => {}
            _ => return,
        }
        for piece in self
            .piece_lines
            .iter_mut()
            .flat_map(|line| &mut line.text_pieces)
        {
            piece.piece.rect.top += offset;
        }
    }
    fn loader(&mut self, width: f32, position: &mut f32, save: bool) -> Result<()> {
        self.get_text_data_node();
        let Some(node) = self.text_data_node.clone() else {
            return Ok(());
        };
        if !self.rich_text {
            return self.load_text(&node, width, position, save);
        }
        let Some(xml) = self.get_xml_container_node() else {
            return Ok(());
        };
        if !self.text_parser.is_parsed() {
            self.text_parser.do_parse(&xml, &self.provider);
        }
        let root = self.text_parser.create_root_style(&self.provider);
        self.load_rich_text(
            &xml,
            width,
            position,
            Some(root),
            save,
            None,
            true,
            false,
            0,
            0,
        )
        .map(|_| ())
    }
    fn load_text(
        &mut self,
        node: &CxfaNode,
        width: f32,
        position: &mut f32,
        save: bool,
    ) -> Result<()> {
        self.init_break(width)?;
        let space = self.provider.get_para_if_exists().map_or(0.0, |p| {
            p.get_measure_in_unit(XfaAttribute::SpaceAbove, XfaUnit::Pt)
        });
        if space >= 0.1 {
            *position += space;
        }
        let mut text = node.get_content();
        while text.last() == Some(&32) {
            text.pop();
        }
        if self.append_char(&text, position, space.max(0.0), save)? {
            if let Some(context) = self.loader_context.as_mut() {
                context.node = Some(node.clone());
            }
            Ok(())
        } else {
            self.end_break(BreakType::Paragraph, position, save)?;
            if let Some(context) = self.loader_context.as_mut() {
                context.node = None;
            }
            Ok(())
        }
    }
    fn end_break(&mut self, status: BreakType, position: &mut f32, save: bool) -> Result<()> {
        let status = self.breaker.as_mut().unwrap().end_break(status);
        if !matches!(status, BreakType::None | BreakType::Piece) {
            self.append_text_line(status, position, save, true)?;
        }
        Ok(())
    }
    fn do_tabstops(
        parser: &CxfaTextParser,
        context: &mut CxfaTextTabstopsContext,
        style: &CfxCssComputedStyle,
        line: &mut PieceLine,
    ) {
        if context.tabstops.is_empty() || line.text_pieces.is_empty() {
            return;
        }
        let count = parser.count_tabs(Some(style));
        let last = line.text_pieces.len() - 1;
        if count > 0 {
            // Source checked -1 before incrementing its initial tab index,
            // preventing every first positioned tab from taking effect.
            let next = context.tab_index.saturating_add(1);
            if usize::try_from(next)
                .ok()
                .is_none_or(|i| i >= context.tabstops.len())
            {
                return;
            }
            context.tab_index = next;
            context.has_tabstops = true;
            let previous = last
                .checked_sub(1)
                .map_or(0.0, |i| line.text_pieces[i].piece.rect.right());
            context.tab_width = line.text_pieces[last].piece.rect.right() - previous;
        } else if let Ok(index) = usize::try_from(context.tab_index) {
            let Some(stop) = context.tabstops.get(index) else {
                return;
            };
            let piece = &mut line.text_pieces[last].piece;
            if context.has_tabstops {
                let hash = |name| hash_code_lowered_wide(&wide(name));
                let left = if stop.align == hash("center") {
                    piece.rect.width / 2.0
                } else if stop.align == hash("right") || stop.align == hash("before") {
                    piece.rect.width
                } else if stop.align == hash("decimal") {
                    piece
                        .text
                        .iter()
                        .zip(&piece.widths)
                        .take_while(|(ch, _)| **ch != 46)
                        .map(|(_, width)| *width as f32 / 20000.0)
                        .sum()
                } else {
                    0.0
                };
                context.left = left.min(context.tab_width);
                context.has_tabstops = false;
                context.tab_width = 0.0;
            }
            piece.rect.left -= context.left;
        }
    }
    fn append_text_line(
        &mut self,
        status: BreakType,
        position: &mut f32,
        save: bool,
        end_break: bool,
    ) -> Result<()> {
        let breaker = self.breaker.as_ref().unwrap();
        let count = breaker.count_break_pieces();
        if count < 1 {
            return Ok(());
        }
        let mut line = PieceLine::default();
        if let Some(tabs) = self.tabstop_context.as_mut() {
            tabs.reset();
        }
        let mut style: Option<Rc<CfxCssComputedStyle>> = None;
        let mut line_step = 0.0f32;
        let mut line_width = 0.0;
        for index in 0..count {
            let (piece, chars) = breaker.get_break_piece(index).unwrap();
            if let Some(data) = piece.user_data.as_ref() {
                style = Some(data.style.clone());
            }
            let vertical = piece.vertical_scale as f32 / 100.0;
            let baseline = self
                .text_parser
                .get_baseline(&self.provider, style.as_deref());
            let height = piece.font_size as f32 * vertical / 20.0;
            let mut line_height = self.text_parser.get_line_height(
                &self.provider,
                style.as_deref(),
                self.lines == 0,
                vertical,
            );
            if baseline > 0.0 {
                line_height = line_height.max(baseline + height);
            }
            line_step = line_step.max(line_height);
            line_width += piece.width as f32 / 20000.0;
            if save {
                let font = self.selected_font(style.as_deref())?;
                let text = TextPiece {
                    piece: CfgasTextPiece {
                        text: piece.get_string(chars),
                        widths: piece.get_widths(chars),
                        chars: piece.char_count,
                        horizontal_scale: piece.horizontal_scale,
                        vertical_scale: piece.vertical_scale,
                        bidi_level: piece.bidi_level,
                        font_size: self
                            .text_parser
                            .get_font_size(&self.provider, style.as_deref()),
                        rect: CfxRectF::new(
                            piece.start_pos as f32 / 20000.0,
                            baseline,
                            piece.width as f32 / 20000.0,
                            height,
                        ),
                        font: Some(font),
                    },
                    underline: self
                        .text_parser
                        .get_underline(&self.provider, style.as_deref()),
                    line_through: self
                        .text_parser
                        .get_linethrough(&self.provider, style.as_deref()),
                    period: self
                        .text_parser
                        .get_underline_period(&self.provider, style.as_deref()),
                    color: self.text_parser.get_color(&self.provider, style.as_deref()),
                    link_data: piece
                        .user_data
                        .as_ref()
                        .and_then(|data| data.link_data.clone()),
                };
                line.text_pieces.push(text);
                if let (Some(style), Some(tabs)) = (style.as_ref(), self.tabstop_context.as_mut()) {
                    Self::do_tabstops(&self.text_parser, tabs, style, &mut line);
                }
            }
        }
        if save {
            for piece in &mut line.text_pieces {
                let rect = &mut piece.piece.rect;
                rect.top = (*position + line_step - rect.height - rect.top).max(0.0);
            }
            self.piece_lines.push(line);
        }
        *position += line_step;
        if !save {
            self.max_width = self.max_width.max(line_width);
            if self.save_line_height {
                self.line_heights.push(*position - self.last_pos);
                self.last_pos = *position;
            }
        }
        let breaker = self.breaker.as_mut().unwrap();
        breaker.clear_break_pieces();
        if status == BreakType::Paragraph {
            breaker.reset();
            if style.is_none() && end_break {
                if let Some(para) = self.provider.get_para_if_exists() {
                    let start = para.get_measure_in_unit(XfaAttribute::MarginLeft, XfaUnit::Pt)
                        + para
                            .get_measure_in_unit(XfaAttribute::TextIndent, XfaUnit::Pt)
                            .max(0.0);
                    breaker.set_line_start_pos(start);
                    let below = para.get_measure_in_unit(XfaAttribute::SpaceBelow, XfaUnit::Pt);
                    if below >= 0.1 {
                        *position += below;
                    }
                }
            }
        }
        if let Some(style) = style {
            let mut start = style
                .get_margin_width()
                .map_or(0.0, |margin| margin.left.value);
            let indent = style.get_text_indent().value;
            if indent < 0.0 {
                start -= indent;
            }
            breaker.set_line_start_pos(start);
        }
        self.lines += 1;
        Ok(())
    }
    pub fn draw_string(
        &mut self,
        device: &mut CfxAggDeviceDriver<'_, '_>,
        matrix: CfxMatrix,
        clip: CfxRectF,
    ) -> Result<bool> {
        self.draw_block(device, matrix, clip, 0)
    }
    fn draw_lines(
        &self,
        device: &mut CfxAggDeviceDriver<'_, '_>,
        matrix: CfxMatrix,
        clip: CfxRectF,
        start: usize,
        length: usize,
    ) -> Result<bool> {
        let Some(breaker) = self.breaker.as_ref() else {
            return Ok(false);
        };
        device.save_state();
        device.set_clip_rect(clip.get_outer_rect());
        let result = (|| {
            let mut positions = Vec::new();
            for line in self.piece_lines.iter().skip(start).take(length) {
                let mut counts = Vec::with_capacity(line.text_pieces.len());
                for piece in &line.text_pieces {
                    positions.resize(piece.piece.text.len(), TextCharPos::default());
                    let count = breaker.get_display_pos(&piece.piece, &mut positions);
                    counts.push(count);
                    if count > 0 {
                        let font = piece
                            .piece
                            .font
                            .as_ref()
                            .ok_or(TextLayoutError::FontUnavailable)?;
                        if !CfdeTextOut::draw_string(
                            device,
                            piece.color,
                            font,
                            &mut positions[..count],
                            piece.piece.font_size,
                            matrix,
                        ) {
                            return Err(TextLayoutError::RenderingFailed);
                        }
                    }
                }
                for (index, piece) in line.text_pieces.iter().enumerate() {
                    Self::render_path(
                        breaker,
                        device,
                        line,
                        &counts,
                        index,
                        piece,
                        &mut positions,
                        matrix,
                    )?;
                }
            }
            Ok(length > 0 && start < self.piece_lines.len())
        })();
        device.restore_state(false);
        result
    }
    #[allow(clippy::too_many_arguments)]
    fn render_path(
        breaker: &CfgasRtfBreak,
        device: &mut CfxAggDeviceDriver<'_, '_>,
        line: &PieceLine,
        counts: &[usize],
        index: usize,
        piece: &TextPiece,
        positions: &mut Vec<TextCharPos>,
        matrix: CfxMatrix,
    ) -> Result<()> {
        let no_underline = !(1..=2).contains(&piece.underline);
        let no_line_through = !(1..=2).contains(&piece.line_through);
        if no_underline && no_line_through {
            return Ok(());
        }
        let mut path = CfxPath::new();
        positions.resize(piece.piece.text.len(), TextCharPos::default());
        let count = breaker.get_display_pos(&piece.piece, positions);
        let append = |path: &mut CfxPath, left: f32, right: f32, y: f32| {
            path.append_line(CfxPointF::new(left, y), CfxPointF::new(right, y))
        };
        if count > 0 {
            let positions = &positions[..count];
            let mut y = positions[0].origin.y + 1.05;
            let end = |position: &TextCharPos| {
                position.origin.x + position.font_char_width as f32 * piece.piece.font_size / 1000.0
            };
            for _ in 0..piece.underline {
                if piece.period == XfaAttributeValue::Word {
                    for position in positions {
                        append(&mut path, position.origin.x, end(position), y);
                    }
                } else {
                    append(
                        &mut path,
                        positions[0].origin.x,
                        end(&positions[count - 1]),
                        y,
                    );
                }
                y += 2.0;
            }
            y = positions[0].origin.y - piece.piece.rect.height * 0.25;
            for _ in 0..piece.line_through {
                append(
                    &mut path,
                    positions[0].origin.x,
                    end(&positions[count - 1]),
                    y,
                );
                y += 2.0;
            }
        } else {
            if no_line_through && (no_underline || piece.period != XfaAttributeValue::All) {
                return Ok(());
            }
            let Some(previous) = (0..index).rev().find(|&i| counts[i] > 0) else {
                return Ok(());
            };
            let Some(next) = (index + 1..line.text_pieces.len()).find(|&i| counts[i] > 0) else {
                return Ok(());
            };
            let previous = &line.text_pieces[previous];
            positions.resize(previous.piece.text.len(), TextCharPos::default());
            let count = breaker.get_display_pos(&previous.piece, positions);
            if count == 0 {
                return Ok(());
            }
            let last = &positions[count - 1];
            let left =
                last.origin.x + last.font_char_width as f32 * previous.piece.font_size / 1000.0;
            let next = &line.text_pieces[next];
            positions.resize(next.piece.text.len(), TextCharPos::default());
            if breaker.get_display_pos(&next.piece, positions) == 0 {
                return Ok(());
            }
            let right = positions[0].origin.x;
            let mut y = positions[0].origin.y + 1.05;
            for _ in 0..next.underline {
                append(&mut path, left, right, y);
                y += 2.0;
            }
            y = positions[0].origin.y - next.piece.rect.height * 0.25;
            for _ in 0..next.line_through {
                append(&mut path, left, right, y);
                y += 2.0;
            }
        }
        if !device.draw_path(
            &path,
            Some(matrix),
            Some(&CfxGraphStateData::default()),
            0,
            piece.color,
            false,
            CfxFillRenderOptions::default(),
        ) {
            return Err(TextLayoutError::RenderingFailed);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        fxcrt::{
            cfx_read_only_span_stream::CfxReadOnlySpanStream, xml::cfx_xmlparser::CfxXmlParser,
        },
        fxge::{
            cfx_font::CfxFont,
            dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat},
        },
        xfa::fxfa::{
            cxfa_textprovider::TextProviderType,
            fxfa_basic::{XfaElement, XfaPacketType},
            parser::cxfa_document_builder::CxfaDocumentBuilder,
        },
    };
    use std::cell::RefCell;
    struct FontHost {
        font: Rc<CfgasGeFont>,
        requests: RefCell<Vec<(Vec<u32>, u32)>>,
    }
    impl XfaTextFontHost for FontHost {
        fn get_font(&self, family: &[u32], style: u32) -> Option<Rc<CfgasGeFont>> {
            self.requests.borrow_mut().push((family.to_vec(), style));
            Some(self.font.clone())
        }
    }
    fn host() -> Rc<FontHost> {
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
        Rc::new(FontHost {
            font,
            requests: RefCell::new(Vec::new()),
        })
    }
    fn document() -> (Rc<CxfaDocument>, CxfaNode) {
        let mut bytes=br##"<form xmlns="http://www.xfa.org/schema/xfa-form/2.8/"><subform>
          <draw name="plain"><font typeface="Roboto" size="12pt"/><para vAlign="middle"/><value><text>one two three four five six</text></value></draw>
          <draw name="rich"><font typeface="Roboto" size="12pt"/><value><exData contentType="text/html"><body xmlns="http://www.w3.org/1999/xhtml"><p><a href="https://example.test/" style="color:#ff0000;text-decoration:underline">linked</a><span style="font-family:First,Roboto;font-weight:bold;font-style:italic"> text</span><br/>second line</p><ol><li>first</li><li>second</li></ol></body></exData></value></draw>
        </subform></form>"##.to_vec();
        bytes.push(0);
        let xml = Rc::new(RefCell::new(
            CfxXmlParser::new(Rc::new(CfxReadOnlySpanStream::new(&bytes)))
                .parse()
                .unwrap(),
        ));
        let doc = CxfaDocument::new();
        let mut builder = CxfaDocumentBuilder::new(doc.clone());
        assert!(builder.build_document(xml, XfaPacketType::Form));
        let form = builder.get_root_node().unwrap();
        assert!(doc.set_root(Some(form.clone())));
        let subform = form.get_child(0, XfaElement::Subform, false).unwrap();
        (doc, subform)
    }
    #[test]
    fn measured_plain_and_rich_text_produce_owned_pieces_links_and_real_rendering() {
        let (document, subform) = document();
        let host = host();
        let plain = subform.get_first_child_by_name(&wide("plain")).unwrap();
        let rich = subform.get_first_child_by_name(&wide("rich")).unwrap();
        let mut plain = CxfaTextLayout::new(
            Rc::new(CxfaTextProvider::new(plain, TextProviderType::Text)),
            host.clone(),
        )
        .unwrap();
        let wide_size = plain
            .calc_size(
                CfxSizeF::default(),
                CfxSizeF {
                    width: 400.0,
                    height: 0.0,
                },
            )
            .unwrap();
        let narrow = plain
            .calc_size(
                CfxSizeF::default(),
                CfxSizeF {
                    width: 45.0,
                    height: 0.0,
                },
            )
            .unwrap();
        assert!(wide_size.width > 45.0);
        assert!(narrow.height > wide_size.height);
        assert!(plain.line_heights().len() > 1);
        let height = plain
            .layout(CfxSizeF {
                width: 400.0,
                height: 80.0,
            })
            .unwrap();
        assert_eq!(height, 12.0);
        assert!(plain.piece_lines()[0].text_pieces[0].piece.rect.top >= 34.0);
        let mut layout = CxfaTextLayout::new(
            Rc::new(CxfaTextProvider::new(rich, TextProviderType::Text)),
            host.clone(),
        )
        .unwrap();
        // Layout retains the document and XML owner after caller teardown.
        drop(document);
        drop(subform);
        let measured = layout
            .calc_size(
                CfxSizeF::default(),
                CfxSizeF {
                    width: 180.0,
                    height: 0.0,
                },
            )
            .unwrap();
        let height = layout
            .layout(CfxSizeF {
                width: 180.0,
                height: measured.height,
            })
            .unwrap();
        assert!(height > 24.0);
        assert!(layout.is_loaded());
        let pieces: Vec<_> = layout
            .piece_lines()
            .iter()
            .flat_map(|line| &line.text_pieces)
            .collect();
        let linked = pieces
            .iter()
            .find(|piece| piece.link_data.is_some())
            .unwrap();
        let point = CfxPointF::new(linked.piece.rect.left + 1.0, linked.piece.rect.top + 1.0);
        assert_eq!(
            layout.get_link_url_at_point(point),
            wide("https://example.test/")
        );
        assert!(layout
            .get_link_url_at_point(CfxPointF::new(-1.0, -1.0))
            .is_empty());
        assert_eq!(linked.color, 0xffff0000);
        assert_eq!(linked.underline, 1);
        let all: Vec<_> = pieces
            .iter()
            .flat_map(|piece| piece.piece.text.iter().copied())
            .collect();
        assert!(all.windows(3).any(|text| text == wide("1. ")));
        assert!(all.windows(3).any(|text| text == wide("2. ")));
        assert!(host
            .requests
            .borrow()
            .iter()
            .any(|(family, style)| *family == wide("Roboto")
                && *style
                    == crate::fxge::fx_font::FONT_STYLE_FORCE_BOLD
                        | crate::fxge::fx_font::FONT_STYLE_ITALIC));
        let mut bitmap = CfxDibitmap::create(200, 200, FxdibFormat::Bgra, 0).unwrap();
        bitmap.clear(0xffffffff);
        {
            let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
            assert!(layout
                .draw_string(
                    &mut device,
                    CfxMatrix::default(),
                    CfxRectF::new(0.0, 0.0, 180.0, 180.0)
                )
                .unwrap());
            // A second draw must not accumulate stale per-piece char counts.
            assert!(layout
                .draw_string(
                    &mut device,
                    CfxMatrix::default(),
                    CfxRectF::new(0.0, 0.0, 180.0, 180.0)
                )
                .unwrap());
        }
        assert!(bitmap
            .buffer()
            .chunks_exact(4)
            .any(|pixel| pixel[0] < 255 || pixel[1] < 255 || pixel[2] < 255));
        let right = bitmap.scanline(5).unwrap();
        assert_eq!(&right[195 * 4..196 * 4], &[255, 255, 255, 255]);
        layout.unload();
        assert!(!layout.is_loaded());
    }
    #[test]
    fn unavailable_font_and_malformed_sizes_are_reported_without_fabricated_output() {
        struct NoFont;
        impl XfaTextFontHost for NoFont {
            fn get_font(&self, _: &[u32], _: u32) -> Option<Rc<CfgasGeFont>> {
                None
            }
        }
        let (_doc, subform) = document();
        let provider = Rc::new(CxfaTextProvider::new(
            subform.get_first_child_by_name(&wide("plain")).unwrap(),
            TextProviderType::Text,
        ));
        let mut layout = CxfaTextLayout::new(provider, Rc::new(NoFont)).unwrap();
        assert_eq!(
            layout.layout(CfxSizeF {
                width: 100.0,
                height: 100.0
            }),
            Err(TextLayoutError::FontUnavailable)
        );
        assert!(!layout.is_loaded());
        assert_eq!(
            layout.calc_size(
                CfxSizeF::default(),
                CfxSizeF {
                    width: f32::NAN,
                    height: 100.0
                }
            ),
            Err(TextLayoutError::InvalidSize)
        );
    }
}
