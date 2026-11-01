// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `xfa/fde/cfde_textout.{h,cpp}`. The selected render device is the existing
//! borrowed AGG driver. Font views share retained faces rather than copying
//! embedded font data. Piece indices and owner slices replace unstable pointers.
use super::cfde_data::{FdeTextAlignment, FdeTextStyle};
use crate::fxcrt::fx_coordinates::{CfxMatrix, CfxRectF, CfxSizeF};
use crate::fxge::{
    agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
    cfx_font::CfxFont,
    cfx_renderdevice::draw_normal_text_agg,
    cfx_substfont::CfxSubstFont,
    cfx_textrenderoptions::{AliasingType, CfxTextRenderOptions},
    fx_font::{font_style_is_force_bold, font_style_is_italic},
    text_char_pos::TextCharPos,
};
use crate::xfa::fgas::{
    font::cfgas_gefont::CfgasGeFont,
    layout::{
        cfgas_break::LayoutStyle,
        cfgas_char::BreakType,
        cfgas_txtbreak::{CfgasTxtBreak, TxtBreakRun},
    },
};
use std::rc::Rc;

fn none_or_piece(status: BreakType) -> bool {
    matches!(status, BreakType::None | BreakType::Piece)
}

#[derive(Clone, Copy, Debug, Default)]
struct Piece {
    start_char: usize,
    char_count: usize,
    char_styles: u32,
    bounds: CfxRectF,
}

#[derive(Default)]
struct Line {
    new_reload: bool,
    pieces: Vec<Piece>,
}
impl Line {
    fn add_piece(&mut self, index: usize, piece: Piece) -> usize {
        if index >= self.pieces.len() {
            self.pieces.push(piece);
            self.pieces.len()
        } else {
            self.pieces[index] = piece;
            // Always return the next slot. The source returns the current
            // slot only when replacing, repeatedly overwriting it on reload.
            index + 1
        }
    }
}

pub struct CfdeTextOut {
    txt_break: CfgasTxtBreak,
    font: Option<Rc<CfgasGeFont>>,
    font_size: f32,
    line_space: f32,
    line_pos: f32,
    tolerance: f32,
    alignment: FdeTextAlignment,
    styles: FdeTextStyle,
    char_widths: Vec<i32>,
    txt_color: u32,
    txt_bk_styles: LayoutStyle,
    text: Vec<u32>,
    matrix: CfxMatrix,
    tto_lines: Vec<Line>,
    cur_line: usize,
    cur_piece: usize,
    total_lines: i32,
    char_pos: Vec<TextCharPos>,
}
impl Default for CfdeTextOut {
    fn default() -> Self {
        Self::new()
    }
}
impl CfdeTextOut {
    pub fn new() -> Self {
        Self {
            txt_break: CfgasTxtBreak::new(),
            font: None,
            font_size: 12.,
            line_space: 12.,
            line_pos: 0.,
            tolerance: 0.,
            alignment: FdeTextAlignment::TopLeft,
            styles: FdeTextStyle::default(),
            char_widths: Vec::new(),
            txt_color: 0xff000000,
            txt_bk_styles: LayoutStyle::NONE,
            text: Vec::new(),
            matrix: CfxMatrix::default(),
            tto_lines: Vec::new(),
            cur_line: 0,
            cur_piece: 0,
            total_lines: 0,
            char_pos: Vec::new(),
        }
    }

    pub fn set_font(&mut self, font: Rc<CfgasGeFont>) {
        self.font = Some(font.clone());
        self.txt_break.set_font(font);
    }
    pub fn set_font_size(&mut self, size: f32) {
        if size.is_finite() && size > 0. {
            self.font_size = size;
            self.txt_break.set_font_size(size);
        }
    }
    pub fn set_text_color(&mut self, color: u32) {
        self.txt_color = color;
    }
    pub fn set_styles(&mut self, styles: FdeTextStyle) {
        self.styles = styles;
        self.txt_bk_styles = if styles.single_line {
            LayoutStyle::SINGLE_LINE
        } else {
            LayoutStyle::NONE
        };
        self.txt_break.set_layout_styles(self.txt_bk_styles);
    }
    pub fn set_alignment(&mut self, alignment: FdeTextAlignment) {
        self.alignment = alignment;
        self.txt_break.set_alignment(match alignment {
            FdeTextAlignment::Center => 1,
            FdeTextAlignment::CenterRight => 2,
            _ => 0,
        });
    }
    pub fn set_line_space(&mut self, space: f32) {
        if space.is_finite() && space > 1. {
            self.line_space = space;
        }
    }
    pub fn set_matrix(&mut self, matrix: CfxMatrix) {
        self.matrix = matrix;
    }
    pub fn set_line_break_tolerance(&mut self, tolerance: f32) {
        if tolerance.is_finite() {
            self.tolerance = tolerance;
            self.txt_break.set_line_break_tolerance(tolerance);
        }
    }
    pub fn get_total_lines(&self) -> i32 {
        self.total_lines
    }

    pub fn calc_logic_size(&mut self, text: &[u32], size: &mut CfxSizeF) {
        let mut rect = CfxRectF::new(0., 0., size.width, size.height);
        self.calc_logic_rect(text, &mut rect);
        *size = rect.size();
    }
    pub fn calc_logic_rect(&mut self, text: &[u32], rect: &mut CfxRectF) {
        self.total_lines = 0;
        if text.is_empty() {
            rect.width = 0.;
            rect.height = 0.;
            return;
        }
        if self.font.is_none() || self.font_size < 1. {
            return;
        }
        if !self.styles.single_line {
            if rect.width < 1. {
                rect.width = self.font_size * 1000.;
            }
            self.txt_break.set_line_width(rect.width);
        }
        let (mut width, mut height, mut start) = (0., 0., rect.right());
        let mut break_char_set = false;
        for &ch in text {
            if !break_char_set && matches!(ch, 10 | 13) {
                break_char_set = true;
                self.txt_break.set_paragraph_break_char(ch);
            }
            let status = self.txt_break.append_char(ch);
            self.retrieve_line_width(status, &mut start, &mut width, &mut height);
        }
        let status = self.txt_break.end_break(BreakType::Paragraph);
        self.retrieve_line_width(status, &mut start, &mut width, &mut height);
        self.txt_break.reset();
        let mut increment = rect.height - height;
        if self.alignment.vertically_centered() {
            increment /= 2.;
        } else if self.alignment == FdeTextAlignment::TopLeft {
            increment = 0.;
        }
        rect.left += start;
        rect.top += increment;
        rect.width = width.min(rect.width);
        rect.height = height;
        if self.styles.last_line_height {
            rect.height -= self.line_space - self.font_size;
        }
    }

    fn retrieve_line_width(
        &mut self,
        status: BreakType,
        start: &mut f32,
        width: &mut f32,
        height: &mut f32,
    ) -> bool {
        if none_or_piece(status) {
            return false;
        }
        let mut line_width = 0.;
        for index in 0..self.txt_break.count_break_pieces() {
            if let Some((piece, _)) = self.txt_break.get_break_piece(index) {
                line_width += piece.width as f32 / 20000.;
                *start = start.min(piece.start_pos as f32 / 20000.);
            }
        }
        self.txt_break.clear_break_pieces();
        if status == BreakType::Paragraph {
            self.txt_break.reset();
        }
        if !self.styles.line_wrap && status == BreakType::Line {
            *width += line_width;
        } else {
            *width = width.max(line_width);
            *height += self.line_space.max(self.font_size);
        }
        self.total_lines = self.total_lines.saturating_add(1);
        true
    }

    pub fn draw_logic_text(
        &mut self,
        device: Option<&mut CfxAggDeviceDriver<'_, '_>>,
        text: &[u32],
        rect: CfxRectF,
    ) {
        if self.font.is_none()
            || self.font_size < 1.
            || text.is_empty()
            || rect.width < self.font_size
            || rect.height < self.font_size
        {
            return;
        }
        self.txt_break.set_line_width(rect.width);
        self.tto_lines.clear();
        self.text.clear();
        self.cur_line = 0;
        self.cur_piece = 0;
        self.load_text(text, rect);
        self.reload(rect);
        self.do_alignment(rect);
        let Some(device) = device else { return };
        if self.tto_lines.is_empty() {
            return;
        }
        device.save_state();
        // `rect` is the typographic layout/alignment box, not an ink clip.
        // Preserve the caller's viewport clip: descenders and overhangs can
        // legitimately extend beyond a nominal font-size-high line box.
        // Upstream installs no effective additional clip here.
        for line in 0..self.tto_lines.len() {
            for index in 0..self.tto_lines[line].pieces.len() {
                let piece = self.tto_lines[line].pieces[index];
                let count = self.get_display_pos(piece);
                if count != 0 {
                    Self::draw_string(
                        device,
                        self.txt_color,
                        self.font.as_ref().expect("selected font"),
                        &mut self.char_pos[..count],
                        self.font_size,
                        self.matrix,
                    );
                }
            }
        }
        device.restore_state(false);
    }

    fn load_text(&mut self, text: &[u32], rect: CfxRectF) {
        self.text.extend_from_slice(text);
        self.char_widths.resize(text.len(), 0);
        let line_step = self.line_space.max(self.font_size);
        self.line_pos = rect.top;
        let (mut start_char, mut piece_widths) = (0, 0);
        let mut stopped = false;
        for &ch in text {
            let status = self.txt_break.append_char(ch);
            if none_or_piece(status) {
                continue;
            }
            let end = self.retrieve_pieces(status, false, rect, &mut start_char, &mut piece_widths);
            if end
                && (self.styles.line_wrap
                    || matches!(status, BreakType::Paragraph | BreakType::Page))
            {
                piece_widths = 0;
                self.cur_line += 1;
                self.cur_piece = 0;
                self.line_pos += line_step;
            }
            if self.line_pos + line_step > rect.bottom() {
                let index = if end {
                    self.cur_line.saturating_sub(1)
                } else {
                    self.cur_line
                };
                if let Some(line) = self.tto_lines.get_mut(index) {
                    line.new_reload = true;
                }
                stopped = true;
                break;
            }
        }
        let status = self.txt_break.end_break(BreakType::Paragraph);
        if !none_or_piece(status) && !stopped {
            self.retrieve_pieces(status, false, rect, &mut start_char, &mut piece_widths);
        }
        self.txt_break.clear_break_pieces();
        self.txt_break.reset();
    }

    fn retrieve_pieces(
        &mut self,
        status: BreakType,
        reload: bool,
        rect: CfxRectF,
        start_char: &mut usize,
        piece_widths: &mut i32,
    ) -> bool {
        let line_step = self.line_space.max(self.font_size);
        let line_width = (rect.width * 20000.).round() as i32;
        let count = self.txt_break.count_break_pieces();
        let mut need_reload = false;
        for index in 0..count {
            let Some((piece, chars)) = self.txt_break.get_break_piece(index) else {
                continue;
            };
            // start_char is the absolute position in text_, not an offset into
            // each newly returned line. Native skips later lines using it.
            let chars = piece.chars(chars);
            let mut width = 0i32;
            let mut used = 0usize;
            for ch in chars {
                let char_width = ch.char_width.max(0);
                if (self.styles.single_line || !self.styles.line_wrap)
                    && i64::from(line_width) - i64::from(*piece_widths) - i64::from(width)
                        < i64::from(char_width)
                {
                    need_reload = true;
                    break;
                }
                width = width.saturating_add(char_width);
                if let Some(slot) = self.char_widths.get_mut(start_char.saturating_add(used)) {
                    *slot = char_width;
                }
                used += 1;
            }
            let length = chars.len();
            let output = (used > 0).then(|| Piece {
                start_char: *start_char,
                char_count: used,
                char_styles: piece.char_styles | if piece.bidi_level & 1 != 0 { 0x40 } else { 0 },
                bounds: CfxRectF::new(
                    rect.left + piece.start_pos as f32 / 20000.,
                    self.line_pos,
                    width as f32 / 20000.,
                    line_step,
                ),
            });
            if let Some(output) = output {
                self.append_piece(output, need_reload, reload && index == count - 1);
            } else if !reload {
                if let Some(line) = self.tto_lines.get_mut(self.cur_line) {
                    line.new_reload = true;
                }
            }
            *start_char = start_char.saturating_add(length);
            *piece_widths = piece_widths.saturating_add(width);
        }
        self.txt_break.clear_break_pieces();
        self.styles.single_line
            || self.styles.line_wrap
            || need_reload
            || status == BreakType::Paragraph
    }

    fn append_piece(&mut self, piece: Piece, need_reload: bool, end: bool) {
        if self.cur_line >= self.tto_lines.len() {
            let mut line = Line {
                new_reload: need_reload,
                pieces: Vec::new(),
            };
            self.cur_piece = line.add_piece(self.cur_piece, piece);
            self.tto_lines.push(line);
            self.cur_line = self.tto_lines.len() - 1;
        } else {
            let line = &mut self.tto_lines[self.cur_line];
            line.new_reload = need_reload;
            self.cur_piece = line.add_piece(self.cur_piece, piece);
            if end {
                line.pieces.truncate(self.cur_piece);
            }
        }
        if !end && need_reload {
            self.cur_piece = 0;
        }
    }

    fn reload(&mut self, rect: CfxRectF) {
        for index in 0..self.tto_lines.len() {
            if self.tto_lines[index].new_reload {
                self.cur_line = index;
                self.cur_piece = 0;
                self.reload_line_piece(index, rect);
            }
        }
    }
    fn reload_line_piece(&mut self, line: usize, rect: CfxRectF) {
        // Move the old pieces out while replacement pieces are assembled in
        // the same line. This avoids borrowing or cloning a complete line.
        let pieces = std::mem::take(&mut self.tto_lines[line].pieces);
        let (mut start_char, mut widths) = (0, 0);
        for (index, piece) in pieces.iter().enumerate() {
            if index == 0 {
                self.line_pos = piece.bounds.top;
            }
            start_char = piece.start_char;
            let end = piece
                .start_char
                .saturating_add(piece.char_count)
                .min(self.text.len());
            for char_index in piece.start_char..end {
                let status = self.txt_break.append_char(self.text[char_index]);
                if !none_or_piece(status) {
                    self.retrieve_pieces(status, true, rect, &mut start_char, &mut widths);
                }
            }
        }
        let status = self.txt_break.end_break(BreakType::Paragraph);
        if !none_or_piece(status) {
            self.retrieve_pieces(status, true, rect, &mut start_char, &mut widths);
        }
        self.txt_break.reset();
    }

    fn do_alignment(&mut self, rect: CfxRectF) {
        let Some(piece) = self.tto_lines.last().and_then(|line| line.pieces.first()) else {
            return;
        };
        let mut increment = rect.bottom() - piece.bounds.bottom();
        if self.alignment.vertically_centered() {
            increment /= 2.;
        } else if self.alignment == FdeTextAlignment::TopLeft {
            increment = 0.;
        }
        if increment < 1. {
            return;
        }
        for line in &mut self.tto_lines {
            for piece in &mut line.pieces {
                piece.bounds.top += increment;
            }
        }
    }

    fn get_display_pos(&mut self, piece: Piece) -> usize {
        self.char_pos
            .resize(piece.char_count, TextCharPos::default());
        let Some(text) = self.text.get(piece.start_char..) else {
            return 0;
        };
        let Some(widths) = self.char_widths.get(piece.start_char..) else {
            return 0;
        };
        let mut run = TxtBreakRun {
            text,
            widths,
            length: piece.char_count.min(i32::MAX as usize) as i32,
            font: self.font.clone(),
            font_size: self.font_size,
            styles: self.txt_bk_styles,
            char_styles: piece.char_styles,
            rect: piece.bounds,
            ..Default::default()
        };
        self.txt_break.get_display_pos(&mut run, &mut self.char_pos)
    }

    pub fn draw_string(
        device: &mut CfxAggDeviceDriver<'_, '_>,
        color: u32,
        font: &Rc<CfgasGeFont>,
        chars: &mut [TextCharPos],
        font_size: f32,
        matrix: CfxMatrix,
    ) -> bool {
        if chars.is_empty() {
            return true;
        }
        if font_style_is_italic(font.get_font_styles()) && !font.get_dev_font().is_italic() {
            for pos in chars.iter_mut() {
                pos.adjust_matrix[2] += 0.267949 * pos.adjust_matrix[0];
                pos.adjust_matrix[3] += 0.267949 * pos.adjust_matrix[1];
            }
        }
        let mut current: Option<Rc<CfgasGeFont>> = None;
        let mut start = 0;
        let mut success = true;
        for index in 0..chars.len() {
            let Some(selected) = font.get_subst_font(chars[index].glyph_index as i32) else {
                return false;
            };
            chars[index].glyph_index &= 0x00ffffff;
            chars[index].is_cid_font = false;
            if !current
                .as_ref()
                .is_some_and(|old| Rc::ptr_eq(old, &selected))
            {
                if let Some(old) = &current {
                    success &= Self::draw_group(
                        device,
                        color,
                        old,
                        &chars[start..index],
                        font_size,
                        matrix,
                        font.get_font_styles(),
                    );
                }
                current = Some(selected);
                start = index;
            }
        }
        if let Some(current) = current {
            success &= Self::draw_group(
                device,
                color,
                &current,
                &chars[start..],
                font_size,
                matrix,
                font.get_font_styles(),
            );
        }
        success
    }

    fn draw_group(
        device: &mut CfxAggDeviceDriver<'_, '_>,
        color: u32,
        font: &CfgasGeFont,
        chars: &[TextCharPos],
        font_size: f32,
        matrix: CfxMatrix,
        styles: u32,
    ) -> bool {
        let dev_font = font.get_dev_font();
        let Some(face) = dev_font.face() else {
            return false;
        };
        let mut subst = CfxSubstFont::new();
        let weight = if font_style_is_force_bold(styles) {
            700
        } else {
            400
        };
        subst.set_weight(weight);
        subst.set_weight_cjk(weight);
        subst.set_italic_angle(if font_style_is_italic(styles) { -12 } else { 0 });
        subst.set_italic_cjk(font_style_is_italic(styles));
        let view = CfxFont::from_face_with_substitute(face, subst, false, dev_font.object_tag());
        draw_normal_text_agg(
            device,
            chars,
            &view,
            -font_size,
            matrix,
            color,
            CfxTextRenderOptions::new(AliasingType::Lcd),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxcrt::fx_codepage::FxCodePage;
    use crate::fxge::dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat};

    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    fn font(name: &str) -> Rc<CfgasGeFont> {
        if name == "Arimo" {
            return CfgasGeFont::from_font(
                crate::fxge::cfx_font::CfxFont::from_span(
                    include_bytes!(concat!(
                        env!("CARGO_MANIFEST_DIR"),
                        "/../../tests/capi/resources/test_fonts/Arimo-Regular.ttf"
                    )),
                    false,
                    0,
                )
                .unwrap(),
            )
            .unwrap();
        }
        if name == "Arimo Bold" {
            // Native's host-installed fixture is absent on many systems.
            // Bitmap checks concern layout/ink/clipping, not a font checksum.
            return CfgasGeFont::from_font(
                crate::fxge::cfx_font::CfxFont::from_span(
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
        }
        CfgasGeFont::load_font(Some(&wide(name)), 0, FxCodePage::DefAnsi)
            .unwrap_or_else(|| panic!("source fixture font {name}"))
    }

    fn output(name: &str) -> CfdeTextOut {
        let mut output = CfdeTextOut::new();
        output.set_font(font(name));
        output.set_font_size(12.);
        output
    }
    fn bitmap(width: i32, height: i32) -> CfxDibitmap {
        let bitmap = CfxDibitmap::create(width, height, FxdibFormat::Bgra, 0).unwrap();
        // Native fixture MD5s of the empty storage are represented directly.
        assert!(bitmap.buffer().iter().all(|&byte| byte == 0));
        bitmap
    }
    fn ink(bitmap: &CfxDibitmap) -> usize {
        bitmap
            .buffer()
            .chunks_exact(4)
            .filter(|pixel| pixel[3] != 0)
            .count()
    }

    // Source DrawLogicTextBasic. The external Arimo Bold fixture is replaced
    // by bundled Roboto; native raster checksums are intentionally adapted to
    // actual ink, bounds and deterministic drawing with the selected renderer.
    #[test]
    fn draw_logic_text_basic() {
        let mut output = output("Arimo Bold");
        let mut bitmap = bitmap(200, 100);
        {
            let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
            output.draw_logic_text(
                Some(&mut device),
                &wide("foo"),
                CfxRectF::new(0., 0., 2100., 100.),
            );
        }
        assert!(ink(&bitmap) > 20);
        assert!(bitmap.scanline(30).unwrap().iter().all(|&byte| byte == 0));
        let mut repeated = CfxDibitmap::create(200, 100, FxdibFormat::Bgra, 0).unwrap();
        {
            let mut device = CfxAggDeviceDriver::new(&mut repeated, false, None);
            output.draw_logic_text(
                Some(&mut device),
                &wide("foo"),
                CfxRectF::new(0., 0., 2100., 100.),
            );
        }
        assert_eq!(bitmap.buffer(), repeated.buffer());
    }

    // Source DrawLogicTextEmptyRect, including a fully initialized font/device.
    #[test]
    fn draw_logic_text_empty_rect() {
        let mut output = output("Arimo Bold");
        let mut bitmap = bitmap(200, 100);
        let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
        output.draw_logic_text(Some(&mut device), &wide("foo"), CfxRectF::default());
        drop(device);
        assert_eq!(ink(&bitmap), 0);
        // Native compares the empty bitmap checksum: every byte, including
        // RGB channels of transparent pixels, must remain unchanged.
        assert!(bitmap.buffer().iter().all(|&byte| byte == 0));
    }

    // Source strings are kept completely, including their two Arabic final
    // forms and the five-versus-ten trailing i characters in the two bugs.
    fn large_text(extra_i: bool) -> Vec<u32> {
        let mut result = wide(concat!(
            "SSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSS",
            "SSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSSssssssssss",
            "sssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssss",
            "sssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssss",
            "sssssssssssssssssssssssssssssssssssssssssssssssssnnnnnnnnnnn",
            "\u{feba}Sssssssssssssssssss\u{feba}",
        ));
        result.extend(wide(if extra_i {
            "iiiiiiiiiisssss"
        } else {
            "iiiiisssss"
        }));
        result
    }
    fn large_bitmap_case(extra_i: bool) {
        let mut output = output("DejaVu Sans");
        output.set_styles(FdeTextStyle {
            single_line: true,
            ..Default::default()
        });
        output.set_alignment(FdeTextAlignment::CenterLeft);
        output.set_font_size(10.);
        let mut bitmap = bitmap(2100, 20);
        {
            let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
            output.draw_logic_text(
                Some(&mut device),
                &large_text(extra_i),
                CfxRectF::new(3., 3., 2048., 10.),
            );
        }
        assert!(ink(&bitmap) > 200);
        // The native cases compare complete rendered bitmap checksums. With
        // this raster backend, retain the original inputs and require repeat
        // drawing to reproduce every byte, without inventing a layout-box clip.
        let mut repeated = CfxDibitmap::create(2100, 20, FxdibFormat::Bgra, 0).unwrap();
        {
            let mut device = CfxAggDeviceDriver::new(&mut repeated, false, None);
            let clip = device.clip_box();
            output.draw_logic_text(
                Some(&mut device),
                &large_text(extra_i),
                CfxRectF::new(3., 3., 2048., 10.),
            );
            assert_eq!(device.clip_box(), clip);
        }
        assert_eq!(repeated.buffer(), bitmap.buffer());
    }
    #[test]
    fn draw_logic_text_bug953881() {
        large_bitmap_case(false);
    }
    #[test]
    fn draw_logic_text_bug1342078() {
        large_bitmap_case(true);
    }

    #[test]
    fn layout_box_preserves_descenders_and_respects_caller_clip() {
        use crate::fxcrt::fx_coordinates::FxRect;
        let mut output = output("Arimo");
        output.set_styles(FdeTextStyle {
            single_line: true,
            ..Default::default()
        });
        output.set_alignment(FdeTextAlignment::TopLeft);
        output.set_matrix(CfxMatrix::new(8., 0., 0., 8., 0., 0.));
        let layout = CfxRectF::new(2., 2., 40., 12.);
        let mut full = bitmap(400, 160);
        {
            let mut device = CfxAggDeviceDriver::new(&mut full, false, None);
            let clip = device.clip_box();
            output.draw_logic_text(Some(&mut device), &wide("y"), layout);
            assert_eq!(device.clip_box(), clip);
        }
        // The nominal 12pt line box ends at device row 112. Real Arimo ink
        // extends slightly below it; layout bounds must not shave the y.
        assert!((112..160).any(|y| full
            .scanline(y)
            .unwrap()
            .chunks_exact(4)
            .any(|pixel| pixel[3] != 0)));
        let mut clipped = bitmap(400, 160);
        {
            let mut device = CfxAggDeviceDriver::new(&mut clipped, false, None);
            device.set_clip_rect(FxRect::new(0, 0, 400, 112));
            let clip = device.clip_box();
            output.draw_logic_text(Some(&mut device), &wide("y"), layout);
            assert_eq!(device.clip_box(), clip);
        }
        for y in 0..160 {
            let row = clipped.scanline(y).unwrap();
            if y < 112 {
                assert_eq!(row, full.scanline(y).unwrap());
            } else {
                assert!(row.iter().all(|&byte| byte == 0));
            }
        }
    }

    #[test]
    fn multiline_indices_measurement_clipping_state_and_repeated_layout() {
        let mut output = output("DejaVu Sans");
        output.set_styles(FdeTextStyle {
            line_wrap: true,
            ..Default::default()
        });
        output.set_line_space(16.);
        let mut measured = CfxRectF::new(0., 0., 120., 100.);
        output.calc_logic_rect(&wide("one\ntwo\nthree"), &mut measured);
        assert_eq!(output.get_total_lines(), 3);
        assert_eq!(measured.height, 48.);
        assert!(measured.width > 0. && measured.width < 120.);
        let mut bitmap = bitmap(150, 80);
        {
            let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
            let original = device.clip_box();
            output.set_text_color(0xff00ff00);
            output.draw_logic_text(
                Some(&mut device),
                &wide("one\ntwo\nthree"),
                CfxRectF::new(10., 10., 120., 60.),
            );
            assert_eq!(device.clip_box(), original);
        }
        for y in [(10, 26), (26, 42), (42, 58)] {
            assert!((y.0..y.1).any(|line| bitmap
                .scanline(line)
                .unwrap()
                .chunks_exact(4)
                .any(|pixel| pixel[3] != 0)));
        }
        assert!(bitmap
            .buffer()
            .chunks_exact(4)
            .all(|pixel| pixel[3] == 0 || (pixel[0] == 0 && pixel[1] > 0 && pixel[2] == 0)));
        let mut empty = CfxRectF::new(1., 2., 10., 20.);
        output.calc_logic_rect(&[], &mut empty);
        assert_eq!(empty, CfxRectF::new(1., 2., 0., 0.));
        assert_eq!(output.get_total_lines(), 0);
    }
}
