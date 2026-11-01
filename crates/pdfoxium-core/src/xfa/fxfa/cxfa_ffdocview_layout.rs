// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! CXFA_Node widget sizing and real text/field split decisions from
//! `parser/cxfa_node.cpp` (CalcCaptionSize through StartTextLayout).
use super::*;
use crate::{
    fxcrt::fx_coordinates::CfxSizeF,
    xfa::{
        fde::{cfde_data::FdeTextStyle, cfde_textout::CfdeTextOut},
        fxfa::{cxfa_ffimage::ImageResource, parser::cxfa_localevalue::XfaValuePicture},
    },
};

#[derive(Clone, Copy)]
struct FieldSplit {
    start: f32,
    lines: i32,
    height: f32,
}
pub(super) struct WidgetLayoutData {
    height: f32,
    text_out: Option<CfdeTextOut>,
    splits: Vec<FieldSplit>,
}
impl Default for WidgetLayoutData {
    fn default() -> Self {
        Self {
            height: -1.0,
            text_out: None,
            splits: Vec::new(),
        }
    }
}
pub(super) struct WidgetLayoutSlot(RefCell<Option<WidgetLayoutData>>);
impl Default for WidgetLayoutSlot {
    fn default() -> Self {
        Self(RefCell::new(Some(WidgetLayoutData::default())))
    }
}
impl CxfaFfDocView {
    fn with_widget_layout<R>(
        &self,
        node: &CxfaNode,
        operation: impl FnOnce(&mut WidgetLayoutData) -> FfResult<R>,
    ) -> FfResult<R> {
        let _document = self.doc().ok_or(FfWidgetError::DetachedDocumentView)?;
        if !node.is_alive() {
            return Err(FfWidgetError::DetachedDocumentView);
        }
        let slot = self
            .widget_layouts
            .borrow_mut()
            .entry(node.clone())
            .or_insert_with(|| Rc::new(WidgetLayoutSlot::default()))
            .clone();
        let data = slot
            .0
            .borrow_mut()
            .take()
            .ok_or(FfWidgetError::ReentrantTextLayout)?;
        struct Restore {
            slot: Rc<WidgetLayoutSlot>,
            data: Option<WidgetLayoutData>,
        }
        impl Drop for Restore {
            fn drop(&mut self) {
                self.slot.0.replace(self.data.take());
            }
        }
        let mut restore = Restore {
            slot,
            data: Some(data),
        };
        operation(restore.data.as_mut().unwrap())
    }
    pub fn invalidate_widget_layout(&self, node: &CxfaNode) {
        self.widget_layouts.borrow_mut().remove(node);
        self.text_layouts
            .borrow_mut()
            .retain(|(owner, _), _| owner != node);
    }
    pub fn clear_widget_layout_data(&self) {
        self.widget_layouts.borrow_mut().clear();
        self.captions.borrow_mut().clear();
        self.text_layouts.borrow_mut().clear();
    }
    pub fn start_field_draw_layout(&self, node: &CxfaNode, size: CfxSizeF) -> FfResult<CfxSizeF> {
        node.start_widget_layout(self, size)
    }
    pub fn find_split_pos(
        &self,
        node: &CxfaNode,
        index: usize,
        proposed: f32,
    ) -> FfResult<Option<f32>> {
        node.find_split_pos(self, index, proposed)
    }
}
impl CxfaNode {
    fn optional_dimension(&self, attribute: A) -> Option<f32> {
        self.try_measure_as_float(attribute)
    }
    fn width_without_margin(&self, width: f32) -> f32 {
        self.get_margin_if_exists().map_or(width, |margin| {
            width - margin.margin_get_left_inset() - margin.margin_get_right_inset()
        })
    }
    fn height_without_margin(&self, height: f32) -> f32 {
        self.get_margin_if_exists().map_or(height, |margin| {
            height - margin.margin_get_top_inset() - margin.margin_get_bottom_inset()
        })
    }
    pub fn calculate_widget_auto_width(&self, mut width: f32) -> f32 {
        if let Some(margin) = self.get_margin_if_exists() {
            width += margin.margin_get_left_inset() + margin.margin_get_right_inset();
        }
        if let Some(min) = self.optional_dimension(A::MinW) {
            width = width.max(min);
        }
        if let Some(max) = self
            .optional_dimension(A::MaxW)
            .filter(|&value| value > 0.0)
        {
            width = width.min(max);
        }
        width
    }
    pub fn calculate_widget_auto_height(&self, mut height: f32) -> f32 {
        if let Some(margin) = self.get_margin_if_exists() {
            height += margin.margin_get_top_inset() + margin.margin_get_bottom_inset();
        }
        if let Some(min) = self.optional_dimension(A::MinH) {
            height = height.max(min);
        }
        if let Some(max) = self
            .optional_dimension(A::MaxH)
            .filter(|&value| value > 0.0)
        {
            height = height.min(max);
        }
        height
    }
    pub fn calculate_widget_auto_size(&self, size: &mut CfxSizeF) {
        if let Some(margin) = self.get_margin_if_exists() {
            size.width += margin.margin_get_left_inset() + margin.margin_get_right_inset();
            size.height += margin.margin_get_top_inset() + margin.margin_get_bottom_inset();
        }
        if let Some(para) = self.get_para_if_exists() {
            size.width += para.para_get_margin_left() + para.para_get_text_indent();
        }
        if let Some(width) = self.optional_dimension(A::W) {
            size.width = width;
        } else {
            if let Some(min) = self.optional_dimension(A::MinW) {
                size.width = size.width.max(min);
            }
            if let Some(max) = self
                .optional_dimension(A::MaxW)
                .filter(|&value| value > 0.0)
            {
                size.width = size.width.min(max);
            }
        }
        if let Some(height) = self.optional_dimension(A::H) {
            size.height = height;
        } else {
            if let Some(min) = self.optional_dimension(A::MinH) {
                size.height = size.height.max(min);
            }
            if let Some(max) = self
                .optional_dimension(A::MaxH)
                .filter(|&value| value > 0.0)
            {
                size.height = size.height.min(max);
            }
        }
    }
    pub fn calc_caption_size(&self, view: &CxfaFfDocView, size: &mut CfxSizeF) -> FfResult<()> {
        let Some(caption) = self
            .get_child(0, E::Caption, false)
            .filter(|caption| caption.caption_is_visible())
        else {
            return Ok(());
        };
        let layout = view.load_caption(self)?;
        let reserve = caption.caption_get_reserve();
        let placement = caption.caption_get_placement_type();
        let fixed = reserve > 0.01;
        let vertical = matches!(placement, V::Top | V::Bottom);
        if let Some(layout) = layout {
            if !vertical && self.get_ff_widget_type() != XfaFfWidgetType::Button {
                size.width = reserve;
            }
            *size = layout.with_layout(|layout| layout.calc_size(CfxSizeF::default(), *size))?;
            if fixed {
                if vertical {
                    size.height = reserve;
                } else {
                    size.width = reserve;
                }
            }
        } else {
            let font_size = caption
                .get_font_if_exists()
                .or_else(|| self.get_font_if_exists())
                .map_or(10.0, |font| font.font_get_size());
            if vertical {
                size.height = if reserve > 0.0 { reserve } else { font_size };
            } else {
                size.width = reserve.max(0.0);
                size.height = font_size;
            }
        }
        if let Some(margin) = caption.caption_get_margin_if_exists() {
            let horizontal = margin.margin_get_left_inset() + margin.margin_get_right_inset();
            let vertical_inset = margin.margin_get_top_inset() + margin.margin_get_bottom_inset();
            if fixed {
                if vertical {
                    size.width += horizontal;
                } else {
                    size.height += vertical_inset;
                }
            } else {
                size.width += horizontal;
                size.height += vertical_inset;
            }
        }
        Ok(())
    }
    fn calculate_field_auto_size(&self, view: &CxfaFfDocView, size: &mut CfxSizeF) -> FfResult<()> {
        let mut caption_size = CfxSizeF::default();
        self.calc_caption_size(view, &mut caption_size)?;
        let margin = self.get_ui_margin();
        size.width += margin.left + margin.width;
        size.height += margin.top + margin.height;
        if caption_size.width > 0.0 && caption_size.height > 0.0 {
            let placement = self
                .get_child(0, E::Caption, false)
                .map_or(V::Left, |caption| caption.caption_get_placement_type());
            match placement {
                V::Left | V::Right | V::Inline => {
                    size.width += caption_size.width;
                    size.height = size.height.max(caption_size.height);
                }
                V::Top | V::Bottom => {
                    size.height += caption_size.height;
                    size.width = size.width.max(caption_size.width);
                }
                _ => {}
            }
        }
        self.calculate_widget_auto_size(size);
        Ok(())
    }
    fn calculate_text_content_size(
        &self,
        view: &CxfaFfDocView,
        data: &mut WidgetLayoutData,
        size: &mut CfxSizeF,
    ) -> FfResult<()> {
        let font_size = self.get_font_size();
        let mut text = self.get_value(XfaValuePicture::Display);
        if text.is_empty() {
            size.height += font_size;
            return Ok(());
        }
        if text.last() == Some(&10) {
            text.push(10);
        }
        let font = view.get_font(self).ok_or(FfWidgetError::TextLayout(
            super::super::cxfa_textlayout::TextLayoutError::FontUnavailable,
        ))?;
        let output = data.text_out.get_or_insert_with(CfdeTextOut::new);
        output.set_font(font);
        output.set_font_size(font_size);
        output.set_line_break_tolerance(font_size * 0.2);
        output.set_line_space(self.get_line_height());
        output.set_styles(FdeTextStyle {
            last_line_height: true,
            line_wrap: self.get_ff_widget_type() == XfaFfWidgetType::TextEdit
                && self.is_multi_line(),
            ..Default::default()
        });
        output.calc_logic_size(&text, size);
        Ok(())
    }
    fn calculate_text_edit_auto_size(
        &self,
        view: &CxfaFfDocView,
        data: &mut WidgetLayoutData,
        size: &mut CfxSizeF,
    ) -> FfResult<()> {
        if size.width > 0.0 {
            let original = *size;
            let mut caption = CfxSizeF::default();
            self.calc_caption_size(view, &mut caption)?;
            let has_caption = caption.width > 0.01 && caption.height > 0.01;
            let placement = if has_caption {
                self.get_child(0, E::Caption, false)
                    .map_or(V::Left, |caption| caption.caption_get_placement_type())
            } else {
                V::Unknown
            };
            if matches!(placement, V::Left | V::Right | V::Inline) {
                size.width -= caption.width;
            }
            let margin = self.get_ui_margin();
            size.width -= margin.left + margin.width;
            size.width = self.width_without_margin(size.width);
            self.calculate_text_content_size(view, data, size)?;
            size.height += margin.top + margin.height;
            if has_caption {
                match placement {
                    V::Left | V::Right | V::Inline => size.height = size.height.max(caption.height),
                    V::Top | V::Bottom => size.height += caption.height,
                    _ => {}
                }
            }
            size.width = original.width;
            self.calculate_widget_auto_size(size);
            return Ok(());
        }
        self.calculate_text_content_size(view, data, size)?;
        self.calculate_field_auto_size(view, size)
    }
    fn calculate_image_size(&self, resource: &ImageResource) -> CfxSizeF {
        let bitmap = &resource.bitmap;
        let width = bitmap.width() as f32 * 72.0 / resource.dpi.0.max(1) as f32;
        let height = bitmap.height() as f32 * 72.0 / resource.dpi.1.max(1) as f32;
        // Compute the inner dimensions before the common sizing step
        // reinstates the node's outer margins.
        CfxSizeF::new(
            self.optional_dimension(A::W)
                .map_or(width, |width| self.width_without_margin(width)),
            self.optional_dimension(A::H)
                .map_or(height, |height| self.height_without_margin(height)),
        )
    }
    fn calculate_acc_width_and_height(
        &self,
        view: &CxfaFfDocView,
        data: &mut WidgetLayoutData,
        width: f32,
    ) -> FfResult<CfxSizeF> {
        let mut size = CfxSizeF::new(width, data.height);
        match self.get_ff_widget_type() {
            XfaFfWidgetType::Barcode | XfaFfWidgetType::ChoiceList | XfaFfWidgetType::Signature => {
                self.calculate_field_auto_size(view, &mut size)?
            }
            XfaFfWidgetType::TextEdit
            | XfaFfWidgetType::NumericEdit
            | XfaFfWidgetType::PasswordEdit
            | XfaFfWidgetType::DateTimeEdit => {
                self.calculate_text_edit_auto_size(view, data, &mut size)?
            }
            XfaFfWidgetType::CheckButton => {
                let check_size = self.get_check_button_size();
                size = CfxSizeF::new(check_size, check_size);
                self.calculate_field_auto_size(view, &mut size)?;
            }
            XfaFfWidgetType::Button => {
                self.calc_caption_size(view, &mut size)?;
                self.calculate_widget_auto_size(&mut size);
            }
            kind @ (XfaFfWidgetType::Image | XfaFfWidgetType::ImageEdit) => {
                let edit = kind == XfaFfWidgetType::ImageEdit;
                let cached = if edit {
                    self.get_edit_image_resource()
                } else {
                    self.get_layout_image_resource()
                };
                let image = if cached.is_some() {
                    cached
                } else {
                    if let Some(owner) = view.image_owner() {
                        if edit {
                            self.load_edit_image(owner.as_ref());
                        } else {
                            self.load_layout_image(owner.as_ref());
                        }
                    }
                    if edit {
                        self.get_edit_image_resource()
                    } else {
                        self.get_layout_image_resource()
                    }
                };
                size = image
                    .as_ref()
                    .map_or_else(CfxSizeF::default, |image| self.calculate_image_size(image));
                if kind == XfaFfWidgetType::ImageEdit {
                    self.calculate_field_auto_size(view, &mut size)?;
                } else {
                    self.calculate_widget_auto_size(&mut size);
                }
            }
            XfaFfWidgetType::Arc
            | XfaFfWidgetType::Line
            | XfaFfWidgetType::Rectangle
            | XfaFfWidgetType::Subform
            | XfaFfWidgetType::ExclGroup => self.calculate_widget_auto_size(&mut size),
            XfaFfWidgetType::Text | XfaFfWidgetType::None => {}
        }
        data.height = size.height;
        Ok(size)
    }
    pub fn start_widget_layout(
        &self,
        view: &CxfaFfDocView,
        mut size: CfxSizeF,
    ) -> FfResult<CfxSizeF> {
        if !size.width.is_finite() || !size.height.is_finite() {
            return Err(FfWidgetError::TextLayout(
                super::super::cxfa_textlayout::TextLayoutError::InvalidSize,
            ));
        }
        view.with_widget_layout(self, |data| {
            if self.get_ff_widget_type() == XfaFfWidgetType::Text {
                data.height = self.optional_dimension(A::H).unwrap_or(-1.0);
                self.start_text_layout(view, data, &mut size)?;
                return Ok(size);
            }
            if size.width > 0.0 && size.height > 0.0 {
                data.height = size.height;
                return Ok(size);
            }
            data.height = -1.0;
            if size.width >= 0.0 && size.height < 0.0 {
                if let Some(height) = self.optional_dimension(A::H) {
                    size.height = height;
                } else {
                    size = self.calculate_acc_width_and_height(view, data, size.width)?;
                }
                data.height = size.height;
                return Ok(size);
            }
            if size.width < 0.0 && size.height < 0.0 {
                let width = self.optional_dimension(A::W);
                let height = width.and_then(|_| self.optional_dimension(A::H));
                if let (Some(width), Some(height)) = (width, height) {
                    size = CfxSizeF::new(width, height);
                } else {
                    size = self.calculate_acc_width_and_height(view, data, width.unwrap_or(0.0))?;
                }
            }
            data.height = size.height;
            Ok(size)
        })
    }
    fn start_text_layout(
        &self,
        view: &CxfaFfDocView,
        data: &mut WidgetLayoutData,
        size: &mut CfxSizeF,
    ) -> FfResult<()> {
        let layout = view
            .load_text_layout(self, TextProviderType::Text)?
            .ok_or(FfWidgetError::DetachedDocumentView)?;
        layout.with_layout(|layout| {
            if size.width > 0.0 && size.height > 0.0 {
                layout.start_layout(self.width_without_margin(size.width))?;
                layout.do_layout(self.height_without_margin(size.height))?;
                return Ok(());
            }
            if size.width > 0.0 && size.height < 0.0 {
                layout.start_layout(self.width_without_margin(size.width))?;
            }
            if size.width < 0.0 && size.height < 0.0 {
                if let Some(width) = self.optional_dimension(A::W) {
                    layout.start_layout(self.width_without_margin(width))?;
                    size.width = width;
                } else {
                    let width = self.calculate_widget_auto_width(layout.start_layout(-1.0)?);
                    layout.start_layout(self.width_without_margin(width))?;
                    size.width = width;
                }
            }
            if data.height < 0.0 {
                data.height = self.calculate_widget_auto_height(layout.get_layout_height()?);
            }
            layout.do_layout(self.height_without_margin(data.height))?;
            size.height = data.height;
            Ok(())
        })
    }
    pub fn find_split_pos(
        &self,
        view: &CxfaFfDocView,
        index: usize,
        mut proposed: f32,
    ) -> FfResult<Option<f32>> {
        if !proposed.is_finite() {
            return Err(FfWidgetError::TextLayout(
                super::super::cxfa_textlayout::TextLayoutError::InvalidSize,
            ));
        }
        if !self.has_created_ui_widget() || self.get_ff_widget_type() == XfaFfWidgetType::Subform {
            return Ok(None);
        }
        let kind = self.get_ff_widget_type();
        if !matches!(
            kind,
            XfaFfWidgetType::Text
                | XfaFfWidgetType::TextEdit
                | XfaFfWidgetType::NumericEdit
                | XfaFfWidgetType::PasswordEdit
        ) {
            return Ok(Some(0.0));
        }
        view.with_widget_layout(self, |data| {
            let (mut top, mut bottom) = (0.0, 0.0);
            if index == 0 {
                if let Some(margin) = self.get_margin_if_exists() {
                    top = margin.margin_get_top_inset();
                    bottom = margin.margin_get_bottom_inset();
                }
                let ui = self.get_ui_margin();
                top += ui.top;
                bottom += ui.height;
            }
            if kind == XfaFfWidgetType::Text {
                let original = proposed;
                if index == 0 {
                    proposed = (proposed - top).max(0.0);
                }
                let layout = view
                    .text_layout(self)
                    .ok_or(FfWidgetError::DetachedDocumentView)?;
                proposed = layout.with_layout(|layout| {
                    layout.do_split_layout(index, proposed, data.height - top)
                })?;
                if proposed != 0.0 {
                    if index == 0 {
                        proposed += top;
                    }
                    if (original - proposed).abs() < 0.001 {
                        return Ok(None);
                    }
                }
                return Ok(Some(proposed));
            }
            let (mut placement, mut reserve) = (V::Unknown, 0.0);
            if index == 0 {
                if let Some(caption) = self
                    .get_child(0, E::Caption, false)
                    .filter(|caption| !caption.caption_is_hidden())
                {
                    placement = caption.caption_get_placement_type();
                    reserve = caption.caption_get_reserve();
                }
                if placement == V::Top && proposed < reserve + top {
                    return Ok(Some(0.0));
                }
                if placement == V::Bottom && data.height - reserve - bottom != 0.0 {
                    return Ok(Some(0.0));
                }
                if placement != V::Top {
                    reserve = 0.0;
                }
            }
            let mut height = data.height;
            let mut lines = if self.get_value(XfaValuePicture::Display).is_empty() {
                1
            } else {
                if data.text_out.is_none() {
                    height = self
                        .calculate_acc_width_and_height(
                            view,
                            data,
                            self.optional_dimension(A::W).unwrap_or(0.0),
                        )?
                        .height;
                }
                data.text_out
                    .as_ref()
                    .map_or(1, |output| output.get_total_lines())
            };
            if data.splits.len() < index {
                return Ok(None);
            }
            for split in data.splits.iter().take(index) {
                lines -= split.lines;
                height -= split.height;
            }
            if lines <= 0 {
                return Ok(None);
            }
            let line_height = self.get_line_height();
            let font_size = self.get_font_size();
            let text_height = lines as f32 * line_height - line_height + font_size;
            let (mut above, mut start) = (0.0, 0.0);
            if height > 0.1 && index == 0 {
                start = top;
                height -= top + bottom;
                if let Some(para) = self.get_para_if_exists() {
                    above = para.para_get_space_above();
                    height -= above + para.para_get_space_below();
                    start += match para.para_get_vertical_align() {
                        V::Middle => (height - text_height) / 2.0 + above,
                        V::Bottom => height - text_height + above,
                        _ => above,
                    };
                }
                if start < 0.1 {
                    start = 0.0;
                }
            }
            if index > 0 {
                let previous = data.splits[index - 1];
                start = (previous.start - previous.height).max(0.0);
                if start < 0.1 {
                    start = 0.0;
                }
            }
            let version = view
                .doc()
                .map_or(303, |doc| doc.model_document().get_cur_version_mode());
            let mode = self
                .parent()
                .and_then(|node| node.try_enum(A::Layout, true))
                .unwrap_or(V::Position);
            let mut no_content =
                matches!(mode, V::Position | V::Tb | V::Row | V::Table) && version > 208;
            if matches!(mode, V::Tb | V::Row | V::Table) && version <= 208 {
                if start >= proposed {
                    return Ok(Some(0.0));
                }
                no_content = true;
            }
            if !no_content || proposed - top - above < line_height {
                return Ok(Some(0.0));
            }
            let save = |data: &mut WidgetLayoutData, lines, height| {
                let split = FieldSplit {
                    start,
                    lines,
                    height,
                };
                if let Some(current) = data.splits.get_mut(index) {
                    *current = split;
                } else {
                    data.splits.push(split);
                }
            };
            if start + 0.001 >= proposed {
                save(data, 0, proposed);
                return Ok(None);
            }
            if proposed - start < line_height {
                save(data, 0, start);
                return Ok(Some(start));
            }
            let available = proposed + 0.001 - reserve - start;
            let mut fitting = ((available + line_height - font_size) / line_height) as i32;
            if fitting >= lines {
                if proposed - start - text_height >= font_size {
                    save(data, lines, proposed);
                    return Ok(None);
                }
                if height - start - text_height < font_size {
                    fitting -= 1;
                    if fitting == 0 {
                        return Ok(Some(0.0));
                    }
                } else {
                    fitting = (available / line_height) as i32;
                }
            }
            if fitting <= 0 {
                return Ok(Some(0.0));
            }
            let split_height = fitting as f32 * line_height + reserve + start;
            save(data, fitting, split_height);
            Ok(if (split_height - proposed).abs() < 0.001 {
                None
            } else {
                Some(split_height)
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xfa::fxfa::test_support::new_document;
    fn fixture() -> (Rc<CxfaFfDoc>, Rc<CxfaFfDocView>, CxfaNode) {
        let (doc,_host)=new_document(br##"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.0/">
          <subform name="record" layout="tb">
            <field name="edit" w="80pt"><font typeface="Helvetica" size="10pt"/><ui><textEdit multiLine="1"><margin leftInset="1pt" rightInset="2pt" topInset="3pt" bottomInset="4pt"/></textEdit></ui>
              <margin leftInset="2pt" rightInset="3pt" topInset="4pt" bottomInset="5pt"/>
              <caption placement="top" reserve="6pt"><value><text>Caption</text></value></caption>
              <value><text>one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen</text></value>
            </field>
            <draw name="text" w="80pt"><font typeface="Helvetica" size="10pt"/><value><text>one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen</text></value></draw>
            <field name="check"><ui><checkButton size="11pt"/></ui><value><text>1</text></value></field>
          </subform></template></xdp:xdp>"##);
        let view = doc.create_doc_view();
        doc.model_document()
            .do_data_merge_with_resolver(doc.script_context().resolver())
            .unwrap();
        let root = view.root_subform().unwrap();
        (doc, view, root)
    }
    #[test]
    fn actual_field_metrics_caption_insets_and_source_split_state() {
        let (_doc, view, root) = fixture();
        let field = root.get_first_child_by_name(&wide("edit")).unwrap();
        let size = view
            .start_field_draw_layout(&field, CfxSizeF::new(80.0, -1.0))
            .unwrap();
        assert_eq!(size.width, 80.0);
        assert!(size.height > 40.0);
        let line_count = view
            .with_widget_layout(&field, |data| {
                Ok(data.text_out.as_ref().unwrap().get_total_lines())
            })
            .unwrap();
        assert!(line_count > 2);
        assert_eq!(view.find_split_pos(&field, 0, 10.0).unwrap(), Some(0.0));
        let split = view.find_split_pos(&field, 0, 40.0).unwrap().unwrap();
        assert!(split > 10.0 && split <= 40.0);
        let first = view
            .with_widget_layout(&field, |data| Ok(data.splits[0]))
            .unwrap();
        assert!(first.lines > 0);
        assert_eq!(first.height, split);
        let _ = view.find_split_pos(&field, 1, 40.0).unwrap();
        let check = root.get_first_child_by_name(&wide("check")).unwrap();
        let check_size = view
            .start_field_draw_layout(&check, CfxSizeF::new(-1.0, -1.0))
            .unwrap();
        assert!(check_size.width >= 11.0 && check_size.height >= 11.0);
        assert_eq!(view.find_split_pos(&check, 0, 100.0).unwrap(), Some(0.0));
        assert_eq!(view.find_split_pos(&root, 0, 100.0).unwrap(), None);
        view.invalidate_widget_layout(&field);
        assert!(view.widget_layouts.borrow().get(&field).is_none());
    }
    #[test]
    fn actual_draw_text_layout_drives_split_and_slot_reentry_restores_metrics() {
        let (_doc, view, root) = fixture();
        let text = root.get_first_child_by_name(&wide("text")).unwrap();
        let size = view
            .start_field_draw_layout(&text, CfxSizeF::new(80.0, -1.0))
            .unwrap();
        assert!(size.height > 25.0);
        let split = view.find_split_pos(&text, 0, 25.0).unwrap().unwrap();
        assert!(split > 0.0 && split < 25.0);
        view.with_widget_layout(&text, |_| {
            assert_eq!(
                view.start_field_draw_layout(&text, CfxSizeF::new(80.0, -1.0)),
                Err(FfWidgetError::ReentrantTextLayout)
            );
            Ok(())
        })
        .unwrap();
        assert!(view
            .start_field_draw_layout(&text, CfxSizeF::new(80.0, -1.0))
            .is_ok());
        view.clear_widget_layout_data();
        assert!(view.widget_layouts.borrow().is_empty());
        assert!(view.text_layout(&text).is_none());
    }
}
