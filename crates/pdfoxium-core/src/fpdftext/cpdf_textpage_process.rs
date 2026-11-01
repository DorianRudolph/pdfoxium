// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Private construction algorithms from `cpdf_textpage.cpp`.
use super::*;
impl<'a> Builder<'a> {
    pub(super) fn find_orientation(&self) -> Orientation {
        let width = self.page.width() as i32;
        let height = self.page.height() as i32;
        if width <= 0 || height <= 0 {
            return Orientation::Unknown;
        }
        // Use interval unions instead of allocating arrays proportional to
        // untrusted page dimensions. Their covered lengths equal the native
        // pixel masks, preserving the source orientation heuristic.
        let (mut horizontal, mut vertical) = (Vec::new(), Vec::new());
        let mut line_height = 0.0_f32;
        let (mut start_h, mut end_h, mut start_v, mut end_v) = (width, 0, height, 0);
        for object in self.page.holder().iter() {
            if !object.base().is_active() || object.object_type() != PageObjectType::Text {
                continue;
            }
            let rect = object.base().rect();
            let min_h = rect.left.clamp(0.0, width as f32) as i32;
            let max_h = rect.right.clamp(0.0, width as f32) as i32;
            let min_v = rect.bottom.clamp(0.0, height as f32) as i32;
            let max_v = rect.top.clamp(0.0, height as f32) as i32;
            if min_h >= max_h || min_v >= max_v {
                continue;
            }
            horizontal.push((min_h, max_h));
            vertical.push((min_v, max_v));
            start_h = start_h.min(min_h);
            end_h = end_h.max(max_h);
            start_v = start_v.min(min_v);
            end_v = end_v.max(max_v);
            if line_height <= 0.0 {
                line_height = rect.height();
            }
        }
        let double = (2.0 * line_height) as i32;
        if end_v - start_v < double {
            return Orientation::Horizontal;
        }
        if end_h - start_h < double {
            return Orientation::Vertical;
        }
        fn percent(intervals: &mut [(i32, i32)], start: i32, end: i32) -> f32 {
            if start >= end {
                return 0.0;
            }
            intervals.sort_unstable();
            let (mut covered, mut last) = (0_i64, start);
            for &(start, end) in intervals.iter() {
                if end > last {
                    covered += i64::from(end - start.max(last));
                    last = end;
                }
            }
            covered as f32 / (end - start) as f32
        }
        let h = percent(&mut horizontal, start_h, end_h);
        if h > 0.8 {
            return Orientation::Horizontal;
        }
        let v = percent(&mut vertical, start_v, end_v);
        if h > v {
            Orientation::Horizontal
        } else if h < v {
            Orientation::Vertical
        } else {
            Orientation::Unknown
        }
    }
    pub(super) fn process_holder(&mut self, holder: &'a CpdfPageObjectHolder, matrix: CfxMatrix) {
        // Explicit traversal stack also handles deeply nested form XObjects
        // without adding recursive Rust frames.
        let mut stack = vec![(holder, matrix, 0_usize)];
        while let Some((holder, matrix, index)) = stack.pop() {
            if index >= holder.page_object_count() {
                continue;
            }
            stack.push((holder, matrix, index + 1));
            let Some(object) = holder.page_object_by_index(index) else {
                continue;
            };
            if !object.base().is_active() {
                continue;
            }
            if let Some(text) = object.as_any().downcast_ref::<CpdfTextObject>() {
                if text.font().is_none() || text.char_codes().is_empty() {
                    continue;
                }
                let id = holder.page_object_id_by_index(index).unwrap();
                let source = Source {
                    text,
                    id,
                    form_matrix: matrix,
                };
                self.sources.entry(id).or_insert(source);
                self.result
                    .objects
                    .entry(id)
                    .or_insert_with(|| TextObjectInfo {
                        font: text.font().unwrap(),
                        font_size: text.font_size(),
                        text_matrix: text.text_matrix(),
                        position: text.position(),
                        fill_color: object_color(text, false),
                        stroke_color: object_color(text, true),
                    });
                self.process_text(source, holder, index);
            } else if let Some(form) = object.as_any().downcast_ref::<CpdfFormObject>() {
                stack.push((form.form().holder(), form.form_matrix() * matrix, 0));
            }
        }
    }
    fn process_text(&mut self, source: Source<'a>, holder: &CpdfPageObjectHolder, index: usize) {
        let text = source.text;
        if text.base().rect().width().abs() < SIZE_EPSILON {
            return;
        }
        let Some(previous) = self.text_objects.last().copied() else {
            self.text_objects.push(source);
            return;
        };
        let mut checked = 0;
        for previous_index in (0..index).rev() {
            let Some(object) = holder.page_object_by_index(previous_index) else {
                continue;
            };
            let Some(candidate) = object.as_any().downcast_ref::<CpdfTextObject>() else {
                continue;
            };
            if std::ptr::eq(candidate, text) {
                continue;
            }
            if self.same_text(candidate, text) {
                return;
            }
            checked += 1;
            if checked == 5 {
                break;
            }
        }
        if previous.text.char_codes().is_empty() {
            return;
        }
        let previous_item = item(previous.text, previous.text.char_codes().len() - 1);
        let previous_width = (previous.text.text_matrix() * previous.form_matrix)
            .transform_distance(
                (char_width(previous_item.code, &previous.text.font().unwrap()) as f32
                    * previous.text.font_size()
                    / 1000.0)
                    .abs(),
            );
        let current_item = item(text, 0);
        let this_width = (text.text_matrix() * source.form_matrix).transform_distance(
            (char_width(current_item.code, &text.font().unwrap()) as f32 * text.font_size()
                / 1000.0)
                .abs(),
        );
        let threshold = previous_width.max(this_width) / 4.0;
        let previous_pos = self
            .display
            .transform(previous.form_matrix.transform(previous.text.position()));
        let this_pos = self
            .display
            .transform(source.form_matrix.transform(text.position()));
        if (this_pos.y - previous_pos.y).abs() > threshold * 2.0 {
            self.process_transformed();
            self.text_objects.clear();
            self.text_objects.push(source);
            return;
        }
        for index in (0..self.text_objects.len()).rev() {
            let previous = self.text_objects[index];
            let position = self
                .display
                .transform(previous.form_matrix.transform(previous.text.position()));
            if this_pos.x >= position.x {
                self.text_objects.insert(index + 1, source);
                return;
            }
        }
        self.text_objects.insert(0, source);
    }
    fn same_text(&self, current: &CpdfTextObject, previous: &CpdfTextObject) -> bool {
        let mut prev = previous.base().rect();
        let rect = current.base().rect();
        if prev.is_empty() && rect.is_empty() {
            let delta = (prev.left - rect.left).abs();
            if self.result.char_list.len() >= 2
                && delta
                    > self.result.char_list[self.result.char_list.len() - 2]
                        .char_box
                        .width()
            {
                return false;
            }
        }
        if !prev.is_empty() || !rect.is_empty() {
            prev.intersect(rect);
            if prev.is_empty()
                || (prev.width() - rect.width()).abs() > rect.width() / 2.0
                || previous.font_size() != current.font_size()
            {
                return false;
            }
        }
        if previous.char_codes() != current.char_codes() {
            return false;
        }
        let Some(&last) = previous.char_codes().last() else {
            return true;
        };
        let difference = current.position() - previous.position();
        let Some(font) = previous.font() else {
            return false;
        };
        let size = previous.font_size();
        let max_size = prev.height().max(prev.width()).max(size);
        difference.x.abs() <= 0.9 * char_width(last, &font) as f32 * size / 1000.0
            && difference.y.abs() <= max_size / 8.0
    }
    fn pre_marked(&self, text: &CpdfTextObject) -> MarkedContent {
        let marks = text.base().content_marks();
        let count = marks.count_items();
        if count == 0 {
            return MarkedContent::Pass;
        }
        let mut actual = Vec::new();
        let mut exists = false;
        let mut last_dict: Option<ObjectRef> = None;
        for index in 0..count {
            let dict = CpdfContentMarkItem::param_from_ref(&marks.get_item(index));
            last_dict = dict.clone();
            let Some(dict) = dict else {
                continue;
            };
            let value = {
                let dict = dict.borrow();
                dict.as_dictionary()
                    .and_then(|d| d.get_object_for(b"ActualText"))
            };
            if let Some(value) = value
                .and_then(|value| crate::fpdfapi::parser::cpdf_object::get_direct_object(&value))
            {
                let borrowed = value.borrow();
                if borrowed.object_type()
                    == crate::fpdfapi::parser::cpdf_scalar_object::ObjectType::String
                {
                    exists = true;
                    actual = borrowed.get_unicode_text();
                }
            }
        }
        if !exists {
            return MarkedContent::Pass;
        }
        if let Some(previous) = self.previous {
            let prev_marks = previous.text.base().content_marks();
            if prev_marks.count_items() == count {
                let previous_dict =
                    CpdfContentMarkItem::param_from_ref(&prev_marks.get_item(count - 1));
                if match (&previous_dict, &last_dict) {
                    (Some(a), Some(b)) => Rc::ptr_eq(a, b),
                    (None, None) => true,
                    _ => false,
                } {
                    return MarkedContent::Done;
                }
            }
        }
        if actual.is_empty() {
            return MarkedContent::Pass;
        }
        if !actual.iter().any(|&c| {
            (c > 0x80 && char::from_u32(c).is_some() && !(0xfffd..=0xffff).contains(&c))
                || (c <= 0x80 && (32..127).contains(&c))
        }) {
            return MarkedContent::Done;
        }
        MarkedContent::Delay
    }
    fn process_marked(&mut self, source: Source<'a>) -> bool {
        let text = source.text;
        let marks = text.base().content_marks();
        let mut actual = Vec::new();
        for index in 0..marks.count_items() {
            if let Some(dict) = CpdfContentMarkItem::param_from_ref(&marks.get_item(index)) {
                let value = {
                    let borrowed = dict.borrow();
                    borrowed
                        .as_dictionary()
                        .and_then(|dict| dict.get_object_for(b"ActualText"))
                };
                actual = value
                    .and_then(|value| {
                        crate::fpdfapi::parser::cpdf_object::get_direct_object(&value)
                    })
                    .map(|value| value.borrow().get_unicode_text())
                    .unwrap_or_default();
            }
        }
        if actual.is_empty() {
            return false;
        }
        let rtl = is_rtl(text)
            || (self.previous.is_some_and(|source| is_rtl(source.text))
                && CfxBidiString::new(&actual, true).overall_direction() == Direction::Right);
        let matrix = text.text_matrix() * source.form_matrix;
        let mut rect = text.base().rect();
        let step = rect.width() / actual.len() as f32;
        let step = if rtl {
            rect.left = rect.right - step;
            -rect.width()
        } else {
            rect.right = rect.left + step;
            rect.width()
        };
        for (index, &character) in actual.iter().enumerate() {
            let mut character = character;
            if character <= 0x80 && !(32..127).contains(&character) {
                character = 32;
            }
            // ActualText is decoded to Unicode scalars, not UCS-2. Keep
            // supplementary characters while retaining the source policy for
            // replacement/noncharacter code points and rejecting invalid scalars.
            if char::from_u32(character).is_none() || (0xfffd..=0xffff).contains(&character) {
                continue;
            }
            let offset = index as f32 * step;
            let rect = CfxFloatRect::new(
                rect.left + offset,
                rect.bottom,
                rect.right + offset,
                rect.top,
            );
            self.temp_text.push(character);
            let info = self.char_info(
                CharType::ActualText,
                INVALID_CHAR_CODE,
                character,
                text.position(),
                rect,
                matrix,
                Some(source),
            );
            self.temp_chars.push(info);
        }
        rtl
    }
    pub(super) fn process_transformed(&mut self) {
        // Each entry is a small borrowed source reference plus matrix, not a
        // text-object/glyph-buffer clone. Processing cannot modify this queue.
        for index in 0..self.text_objects.len() {
            let source = self.text_objects[index];
            let text = source.text;
            if text.base().rect().width().abs() < SIZE_EPSILON {
                continue;
            }
            let marked = self.pre_marked(text);
            if marked == MarkedContent::Done {
                self.previous = Some(source);
                continue;
            }
            if self.previous.is_some() {
                let generated = self.process_insert(source);
                if generated == Generate::LineBreak {
                    self.current_line = text.base().rect();
                } else {
                    self.current_line.union(text.base().rect());
                }
                if !self.process_generate(generated, source) {
                    continue;
                }
            } else {
                self.current_line = text.base().rect();
            }
            let char_start = self.temp_chars.len();
            let text_start = self.temp_text.len();
            if marked == MarkedContent::Delay {
                if self.process_marked(source) {
                    self.reverse_temp(char_start, text_start);
                }
                self.previous = Some(source);
                continue;
            }
            self.previous = Some(source);
            let matrix = text.text_matrix() * source.form_matrix;
            if self.process_items(source, matrix) {
                self.reverse_temp(char_start, text_start);
            }
        }
    }
    fn reverse_temp(&mut self, char_start: usize, text_start: usize) {
        if char_start < self.temp_chars.len() {
            self.temp_chars[char_start..].reverse();
        }
        if text_start < self.temp_text.len() {
            self.temp_text[text_start..].reverse();
        }
    }
    fn add_char_info(&mut self, mut character: u32, info: &CharInfo, rtl: bool) {
        if !is_normal(info) {
            self.result.char_list.push(info.clone());
            return;
        }
        if rtl {
            character = get_mirror_char(character);
        }
        let normalized = if rtl || (0xfb00..=0xfb06).contains(&character) {
            get_unicode_normalization(character)
        } else {
            Vec::new()
        };
        let mut modified = info.clone();
        if normalized.is_empty() {
            self.result.text_buf.push(character);
            if rtl {
                modified.unicode = character;
            }
            self.result.char_list.push(modified);
            return;
        }
        modified.char_type = CharType::Piece;
        for character in normalized {
            modified.unicode = character;
            self.result.text_buf.push(character);
            self.result.char_list.push(modified.clone());
        }
    }
    pub(super) fn close_line(&mut self) {
        if self.temp_chars.is_empty() {
            return;
        }
        // Compact repeated spaces in place, preserving the corresponding
        // character-info alignment and avoiding native repeated vector erase.
        let mut write = 0;
        let mut previous_space = false;
        for read in 0..self.temp_text.len() {
            let character = self.temp_text[read];
            if character == 32 && previous_space {
                continue;
            }
            previous_space = character == 32;
            self.temp_text[write] = character;
            if write != read {
                self.temp_chars[write] = self.temp_chars[read].clone();
            }
            write += 1;
        }
        self.temp_text.truncate(write);
        self.temp_chars.truncate(write);
        let mut bidi = CfxBidiString::new(&self.temp_text, true);
        if self.rtl {
            bidi.set_overall_direction_right();
        }
        let mut direction = bidi.overall_direction();
        for segment in bidi.segments() {
            let end = segment.start + segment.count;
            let rtl = segment.direction == Direction::Right
                || (segment.direction == Direction::Neutral && direction == Direction::Right);
            if rtl {
                direction = Direction::Right;
                let actual = self
                    .temp_chars
                    .get(segment.start)
                    .is_some_and(|info| info.char_type == CharType::ActualText);
                if actual {
                    for index in segment.start..end {
                        let info = self.temp_chars[index].clone();
                        self.add_char_info(self.temp_text[index], &info, true);
                    }
                } else {
                    for index in (segment.start..end).rev() {
                        let info = self.temp_chars[index].clone();
                        self.add_char_info(self.temp_text[index], &info, true);
                    }
                }
            } else {
                if segment.direction != Direction::LeftWeak {
                    direction = Direction::Left;
                }
                for index in segment.start..end {
                    let info = self.temp_chars[index].clone();
                    self.add_char_info(self.temp_text[index], &info, false);
                }
            }
        }
        self.temp_chars.clear();
        self.temp_text.clear();
    }
    fn writing_mode(&self, text: &CpdfTextObject) -> Orientation {
        let count = text.char_codes().len();
        if count <= 1 {
            return self.textline;
        }
        let matrix = text.text_matrix();
        let first = matrix.transform(item(text, 0).origin);
        let last = matrix.transform(item(text, count - 1).origin);
        let dx = (last.x - first.x).abs();
        let dy = (last.y - first.y).abs();
        if dx <= 0.0001 && dy <= 0.0001 {
            return Orientation::Unknown;
        }
        let length = dx.hypot(dy);
        let under = dx / length <= 0.0872;
        if dy / length <= 0.0872 {
            if under {
                self.textline
            } else {
                Orientation::Horizontal
            }
        } else if under {
            Orientation::Vertical
        } else {
            self.textline
        }
    }
    fn is_hyphen(&self, current: u32) -> bool {
        let text = if self.temp_text.is_empty() {
            &self.result.text_buf
        } else {
            &self.temp_text
        };
        let Some(mut index) = text.len().checked_sub(1) else {
            return false;
        };
        while index > 0 && text[index] == 32 {
            index -= 1;
        }
        if !is_hyphen_code(text[index]) {
            return false;
        }
        if index > 0
            && char::from_u32(text[index - 1]).is_some_and(char::is_alphabetic)
            && char::from_u32(current).is_some_and(char::is_alphanumeric)
        {
            return true;
        }
        self.previous_info().is_some_and(|info| {
            matches!(info.char_type, CharType::Piece | CharType::ActualText)
                && is_hyphen_code(info.unicode)
        })
    }
    fn process_insert(&mut self, source: Source<'a>) -> Generate {
        if let Some(previous) = self
            .previous_info()
            .and_then(|info| info.text_object)
            .and_then(|id| self.sources.get(&id))
            .copied()
        {
            self.previous = Some(previous);
        }
        let Some(previous) = self.previous else {
            return Generate::None;
        };
        let text = source.text;
        let prev = previous.text;
        let mut writing = self.writing_mode(text);
        if writing == Orientation::Unknown {
            writing = self.writing_mode(prev);
        }
        if prev.char_codes().is_empty() {
            return Generate::None;
        }
        let prev_item = item(prev, prev.char_codes().len() - 1);
        let current = item(text, 0);
        let rect = text.base().rect();
        let prev_rect = prev.base().rect();
        let font = text.font().unwrap();
        let prev_font = prev.font().unwrap();
        let unicode = font.unicode_from_char_code(current.code);
        let current_char = unicode.first().copied().unwrap_or(current.code);
        let line_end = match writing {
            Orientation::Horizontal => {
                rect.height() > 4.5
                    && prev_rect.height() > 4.5
                    && rect.bottom.max(prev_rect.bottom) >= rect.top.min(prev_rect.top)
            }
            Orientation::Vertical => {
                rect.width() > text.font_size() * 0.1
                    && prev_rect.width() > prev.font_size() * 0.1
                    && rect.right.min(self.current_line.right)
                        <= rect.left.max(self.current_line.left)
            }
            Orientation::Unknown => false,
        };
        if line_end {
            return if self.is_hyphen(current_char) {
                Generate::Hyphen
            } else {
                Generate::LineBreak
            };
        }
        let last_pos = prev_item.origin.x;
        let last_width_raw = char_width(prev_item.code, &prev_font);
        let last_width = (last_width_raw as f32 * prev.font_size() / 1000.0).abs();
        let this_width_raw = char_width(current.code, &font);
        let this_width = (this_width_raw as f32 * text.font_size() / 1000.0).abs();
        let mut threshold = last_width.max(this_width) / 4.0;
        let prev_matrix = prev.text_matrix() * previous.form_matrix;
        let inverse = prev_matrix.get_inverse();
        let pos = inverse.transform(source.form_matrix.transform(text.position()));
        if last_width < this_width {
            threshold = inverse.transform_distance(threshold);
        }
        let mut newline = false;
        if writing == Orientation::Horizontal {
            let mut rect = prev.base().rect();
            let height = rect.height();
            rect.normalize();
            if (rect.is_empty() && height > 5.0)
                || ((pos.y > threshold * 2.0 || pos.y < -threshold * 3.0)
                    && (pos.y.abs() >= 1.0 || pos.y.abs() > pos.x.abs()))
            {
                newline = true;
                if prev.char_codes().len() > 1 {
                    let first = item(prev, 0);
                    let matrix = prev.text_matrix();
                    if prev_item.origin.x > first.origin.x
                        && self.display.a > 0.9
                        && self.display.b < 0.1
                        && self.display.c < 0.1
                        && self.display.d < -0.9
                        && matrix.b < 0.1
                        && matrix.c < 0.1
                    {
                        if CfxFloatRect::new(0.0, prev_rect.bottom, 1000.0, prev_rect.top)
                            .contains_point(text.position())
                            || CfxFloatRect::new(
                                0.0,
                                text.base().rect().bottom,
                                1000.0,
                                text.base().rect().top,
                            )
                            .contains_point(prev.position())
                        {
                            newline = false;
                        }
                    }
                }
            }
        }
        if newline {
            return if self.is_hyphen(current_char) {
                Generate::Hyphen
            } else {
                Generate::LineBreak
            };
        }
        if text.char_codes().len() == 1
            && is_hyphen_code(current_char)
            && self.is_hyphen(current_char)
        {
            return Generate::Hyphen;
        }
        if current_char == 32
            || prev_font.unicode_from_char_code(prev_item.code).last() == Some(&32)
        {
            return Generate::None;
        }
        let mut threshold2 =
            normalize_threshold(last_width_raw.max(this_width_raw) as f32, 400, 700, 800);
        if last_width_raw >= this_width_raw {
            threshold2 *= prev.font_size().abs();
        } else {
            threshold2 *= text.font_size().abs();
            threshold2 = (text.text_matrix() * source.form_matrix).transform_distance(threshold2);
            threshold2 = inverse.transform_distance(threshold2);
        }
        threshold2 /= 1000.0;
        if (threshold2 > 1.4879 && threshold2 < 1.4881)
            || (threshold2 > 1.38999 && threshold2 < 1.39001)
        {
            threshold2 *= 1.5;
        }
        if (last_pos + last_width - pos.x).abs() <= threshold2 {
            return Generate::None;
        }
        let threshold_pos = threshold2 + last_width;
        let difference = pos.x - last_pos;
        if difference.abs() > threshold_pos
            || (pos.x < 0.0 && -threshold_pos > difference)
            || difference > this_width + last_width
        {
            Generate::Space
        } else {
            Generate::None
        }
    }
    fn generate_info(&self, unicode: u32, matrix: CfxMatrix) -> Option<CharInfo> {
        let previous = self.previous_info()?;
        let source = previous.text_object.and_then(|id| self.sources.get(&id));
        let pre_width = source
            .filter(|_| previous.char_code != INVALID_CHAR_CODE)
            .map_or(0, |source| {
                char_width(previous.char_code, &source.text.font().unwrap())
            });
        let mut size = source.map_or(previous.char_box.height(), |source| source.text.font_size());
        if size == 0.0 {
            size = 1.0;
        }
        let origin = CfxPointF::new(
            previous.origin.x + pre_width as f32 * size / 1000.0,
            previous.origin.y,
        );
        Some(self.char_info(
            CharType::Generated,
            INVALID_CHAR_CODE,
            unicode,
            origin,
            CfxFloatRect::from_point(origin),
            matrix,
            None,
        ))
    }
    fn append_generated(&mut self, unicode: u32, matrix: CfxMatrix, temp: bool) {
        if let Some(info) = self.generate_info(unicode, matrix) {
            if temp {
                self.temp_text.push(unicode);
                self.temp_chars.push(info);
            } else {
                self.result.text_buf.push(unicode);
                self.result.char_list.push(info);
            }
        }
    }
    fn process_generate(&mut self, kind: Generate, source: Source<'a>) -> bool {
        match kind {
            Generate::None => true,
            Generate::Space => {
                self.append_generated(32, source.form_matrix, true);
                true
            }
            Generate::LineBreak => {
                self.close_line();
                if !self.result.text_buf.is_empty() {
                    self.append_generated(13, source.form_matrix, false);
                    self.append_generated(10, source.form_matrix, false);
                }
                true
            }
            Generate::Hyphen => {
                if source.text.char_codes().len() == 1 {
                    let current = item(source.text, 0);
                    let unicode = source
                        .text
                        .font()
                        .unwrap()
                        .unicode_from_char_code(current.code);
                    if is_hyphen_code(unicode.first().copied().unwrap_or(current.code)) {
                        return false;
                    }
                }
                while self.temp_text.last() == Some(&32) {
                    self.temp_text.pop();
                    self.temp_chars.pop();
                }
                // A malformed/empty previous line cannot supply a hyphen to edit.
                if let Some(info) = self.temp_chars.last_mut() {
                    self.temp_text.pop();
                    info.char_type = CharType::Hyphen;
                    info.unicode = 2;
                    self.temp_text.push(0xfffe);
                }
                true
            }
        }
    }
    fn process_items(&mut self, source: Source<'a>, matrix: CfxMatrix) -> bool {
        let text = source.text;
        let font = text.font().unwrap();
        let char_space = text.base().text_state().get_char_space();
        let fontsize_h = text.base().text_state().get_font_size_h();
        let spacing = matrix.transform_distance(char_space);
        let mut base_space = 0.0;
        if char_space != 0.0 && text.char_codes().len() >= 2 {
            base_space = spacing;
            let mut kerning = false;
            for &value in text.char_kernings() {
                if value != 0.0 {
                    base_space = base_space.min(-fontsize_h * value / 1000.0 + spacing);
                    kerning = true;
                }
            }
            if base_space < 0.0 || (text.char_codes().len() == 2 && kerning) {
                base_space = 0.0;
            }
        }
        base_space += if char_space > 0.001 {
            -matrix.transform_distance(char_space)
        } else if char_space < -0.001 {
            matrix.transform_distance(char_space.abs())
        } else {
            0.0
        };
        for index in 0..text.char_codes().len() {
            let current = item(text, index);
            let mut spacing = 0.0;
            if index > 0 && text.char_kernings()[index - 1] != 0.0 {
                let last = self
                    .temp_text
                    .last()
                    .or_else(|| self.result.text_buf.last());
                if last.is_some_and(|&last| last != 32) {
                    spacing = -fontsize_h * text.char_kernings()[index - 1] / 1000.0;
                }
            }
            spacing -= base_space;
            if spacing != 0.0 && index > 0 {
                let space_code = font.char_code_from_unicode(32);
                let mut threshold = if space_code != INVALID_CHAR_CODE {
                    fontsize_h * font.get_char_width(space_code) as f32 / 1000.0
                } else {
                    0.0
                };
                if threshold > fontsize_h / 3.0 {
                    threshold = 0.0;
                } else {
                    threshold /= 2.0;
                }
                if threshold == 0.0 {
                    threshold = fontsize_h
                        * normalize_threshold(
                            char_width(current.code, &font) as f32,
                            300,
                            500,
                            700,
                        )
                        / 1000.0;
                }
                if threshold != 0.0 && spacing >= threshold {
                    let origin = matrix.transform(current.origin);
                    self.temp_text.push(32);
                    let info = self.char_info(
                        CharType::Generated,
                        INVALID_CHAR_CODE,
                        32,
                        origin,
                        CfxFloatRect::from_point(origin),
                        source.form_matrix,
                        Some(source),
                    );
                    self.temp_chars.push(info);
                }
            }
            let mut unicode = font.unicode_from_char_code(current.code);
            let mut kind = CharType::Normal;
            if unicode.is_empty() && current.code != 0 {
                unicode.push(current.code);
                kind = CharType::NotUnicode;
            }
            let bounds = font.get_char_bbox(current.code);
            let size = text.font_size() / 1000.0;
            let mut rect = CfxFloatRect::new(
                bounds.left as f32 * size + current.origin.x,
                bounds.bottom as f32 * size + current.origin.y,
                bounds.right as f32 * size + current.origin.x,
                bounds.top as f32 * size + current.origin.y,
            );
            if (rect.top - rect.bottom).abs() < SIZE_EPSILON {
                rect.top = rect.bottom + size;
            }
            if (rect.right - rect.left).abs() < SIZE_EPSILON {
                rect.right = rect.left + object_char_width(text, current.code);
            }
            rect = matrix.transform_float_rect(rect);
            let mut info = self.char_info(
                kind,
                current.code,
                0,
                matrix.transform(current.origin),
                rect,
                matrix,
                Some(source),
            );
            if unicode.is_empty() {
                self.temp_chars.push(info);
                self.temp_text.push(0xfffe);
                continue;
            }
            let threshold = info.matrix.transform_x_distance(0.07 * text.font_size());
            let duplicate = self.temp_chars.iter().rev().take(7).any(|candidate| {
                if candidate.char_code != info.char_code {
                    return false;
                }
                let Some(previous) = candidate
                    .text_object
                    .and_then(|id| self.result.objects.get(&id))
                else {
                    return false;
                };
                if !font_same(&previous.font, &font) {
                    return false;
                }
                let delta = candidate.origin - info.origin;
                delta.x.abs() < threshold && delta.y.abs() < threshold
            });
            if !duplicate {
                for character in unicode {
                    info.unicode = character;
                    self.temp_text
                        .push(if character != 0 { character } else { 0xfffe });
                    self.temp_chars.push(info.clone());
                }
            } else if index == 0 && self.temp_text.last() == Some(&32) {
                self.temp_text.pop();
                self.temp_chars.pop();
            }
        }
        is_rtl(text) && (matrix.a * matrix.d - matrix.b * matrix.c) < 0.0
    }
}

fn object_color(text: &CpdfTextObject, stroke: bool) -> [u32; 4] {
    use crate::fxge::dib::fx_dib::{
        fxsys_get_b_value, fxsys_get_g_value, fxsys_get_r_value, fxsys_get_unsigned_alpha,
    };
    let base = text.base();
    let state = base.color_state();
    let color = if state.has_ref() {
        if stroke {
            state.get_stroke_color_ref()
        } else {
            state.get_fill_color_ref()
        }
    } else {
        0
    };
    let alpha = if stroke {
        base.general_state().get_stroke_alpha()
    } else {
        base.general_state().get_fill_alpha()
    };
    [
        u32::from(fxsys_get_r_value(color)),
        u32::from(fxsys_get_g_value(color)),
        u32::from(fxsys_get_b_value(color)),
        fxsys_get_unsigned_alpha(alpha),
    ]
}
