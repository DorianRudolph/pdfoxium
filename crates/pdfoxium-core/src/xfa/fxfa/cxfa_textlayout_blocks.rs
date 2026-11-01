// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Pagination and saved loader state from `cxfa_textlayout.{h,cpp}`.
//! Private child of cxfa_textlayout; resumes the same real CFGAS pipeline.
use super::*;
const HEIGHT_TOLERANCE: f32 = 0.001;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct BlockData {
    pub index: usize,
    pub length: usize,
}
#[derive(Clone, Copy, Debug)]
pub(super) struct BlockHeight {
    pub index: usize,
    pub height: f32,
}
#[derive(Clone, Debug)]
pub(super) struct LoaderContext {
    pub width: f32,
    pub height: f32,
    pub start_line_offset: f32,
    pub char_index: usize,
    pub total_lines: Option<usize>,
    pub xml_node: Option<XmlNodeRef>,
    pub parent_style: Option<Rc<CfxCssComputedStyle>>,
    pub node: Option<CxfaNode>,
    pub block_heights: Vec<BlockHeight>,
    pub filter_space: bool,
}
impl Default for LoaderContext {
    fn default() -> Self {
        Self {
            width: 0.0,
            height: 0.0,
            start_line_offset: 0.0,
            char_index: 0,
            total_lines: None,
            xml_node: None,
            parent_style: None,
            node: None,
            block_heights: Vec::new(),
            filter_space: false,
        }
    }
}
impl CxfaTextLayout {
    pub fn has_block(&self) -> bool {
        self.has_block
    }
    pub fn clear_blocks(&mut self) {
        self.blocks.clear();
    }
    pub fn reset_has_block(&mut self) {
        self.has_block = false;
    }
    fn count_blocks(&self) -> usize {
        self.blocks.len().max(1)
    }
    fn next_index_from_last_block(&self) -> usize {
        self.blocks
            .last()
            .map_or(0, |block| block.index.saturating_add(block.length))
    }
    pub fn start_layout(&mut self, width: f32) -> Result<f32> {
        if !width.is_finite() {
            return Err(TextLayoutError::InvalidSize);
        }
        let context = self
            .loader_context
            .get_or_insert_with(LoaderContext::default);
        if width < 0.0 || (context.width > -1.0 && (width - context.width).abs() > 0.0) {
            context.start_line_offset = 0.0;
            context.char_index = 0;
            context.xml_node = None;
            context.parent_style = None;
            context.node = None;
            // Old page offsets belong to the former width. Retaining them
            // across reflow in the source skipped lines in the next layout.
            context.block_heights.clear();
            context.filter_space = false;
            self.line_heights.clear();
            self.blocks.clear();
            self.unload();
        }
        self.loader_context.as_mut().unwrap().width = width;
        if width >= 0.0 {
            return Ok(width);
        }
        Ok(self
            .calc_size(CfxSizeF::default(), CfxSizeF::default())?
            .width)
    }
    pub fn get_layout_height(&mut self) -> Result<f32> {
        let Some(context) = self.loader_context.as_ref() else {
            return Ok(0.0);
        };
        if self.line_heights.is_empty() && context.width > 0.0 {
            let size = CfxSizeF {
                width: context.width,
                height: context.height,
            };
            return Ok(self.calc_size(size, size)?.height);
        }
        let height = context.height;
        Ok(if height < 0.1 {
            self.line_heights.iter().sum()
        } else {
            height
        })
    }
    fn update_loader_height(&mut self, height: f32) -> Result<()> {
        let Some(context) = self.loader_context.as_mut() else {
            return Ok(());
        };
        context.height = height;
        if height < 0.0 {
            let height = self.get_layout_height()?;
            self.loader_context.as_mut().unwrap().height = height;
        }
        Ok(())
    }
    pub fn do_layout(&mut self, text_height: f32) -> Result<f32> {
        if !text_height.is_finite() {
            return Err(TextLayoutError::InvalidSize);
        }
        self.update_loader_height(text_height)?;
        Ok(text_height)
    }
    pub fn do_split_layout(
        &mut self,
        block_index: usize,
        calc_height: f32,
        text_height: f32,
    ) -> Result<f32> {
        if !calc_height.is_finite() || !text_height.is_finite() {
            return Err(TextLayoutError::InvalidSize);
        }
        if self.loader_context.is_none() {
            return Ok(calc_height);
        }
        self.update_loader_height(text_height)?;
        if calc_height < 0.0 {
            return Ok(calc_height);
        }
        self.has_block = true;
        if self.blocks.is_empty() && self.loader_context.as_ref().unwrap().height > 0.0 {
            let mut height = text_height - self.get_layout_height()?;
            if height > 0.0 {
                match self.text_parser.get_v_align(&self.provider) {
                    XfaAttributeValue::Middle => height /= 2.0,
                    XfaAttributeValue::Bottom => {}
                    _ => height = 0.0,
                }
                self.loader_context.as_mut().unwrap().start_line_offset = height;
            }
        }
        let context = self.loader_context.as_ref().unwrap();
        let mut position = context.start_line_offset;
        let mut line_index = 0;
        if !self.blocks.is_empty() {
            line_index = self
                .blocks
                .get(block_index)
                .map_or_else(|| self.next_index_from_last_block(), |block| block.index);
            position -= context
                .block_heights
                .iter()
                .take(block_index)
                .map(|block| block.height)
                .sum::<f32>();
        }
        let Some(&first_height) = self.line_heights.get(line_index) else {
            return Ok(calc_height);
        };
        if first_height - calc_height > HEIGHT_TOLERANCE {
            return Ok(0.0);
        }
        for index in line_index..self.line_heights.len() {
            let height = self.line_heights[index];
            if position + height - calc_height <= HEIGHT_TOLERANCE {
                position += height;
                continue;
            }
            let block = BlockData {
                index: line_index,
                length: index - line_index,
            };
            if let Some(current) = self.blocks.get_mut(block_index) {
                *current = block;
            } else {
                self.blocks.push(block);
            }
            if index != line_index {
                return Ok(position);
            }
            if calc_height > position {
                return Ok(calc_height);
            }
            let context = self.loader_context.as_mut().unwrap();
            if let Some(block) = context
                .block_heights
                .get_mut(block_index)
                .filter(|block| block.index == block_index)
            {
                block.height = calc_height;
            } else {
                context.block_heights.push(BlockHeight {
                    index: block_index,
                    height: calc_height,
                });
            }
            return Ok(calc_height);
        }
        Ok(calc_height)
    }
    pub fn item_blocks(&mut self, text_rect: CfxRectF, block_index: usize) -> Result<()> {
        if !text_rect.height.is_finite() {
            return Err(TextLayoutError::InvalidSize);
        }
        let Some(context) = self.loader_context.as_ref() else {
            return Ok(());
        };
        if self.line_heights.is_empty() {
            return Ok(());
        }
        let mut position = context.start_line_offset;
        let mut line_index = 0;
        if block_index > 0 {
            if block_index <= context.block_heights.len() {
                position -= context
                    .block_heights
                    .iter()
                    .take(block_index)
                    .map(|block| block.height)
                    .sum::<f32>();
            } else {
                position = 0.0;
            }
            line_index = self.next_index_from_last_block();
        }
        for index in line_index..self.line_heights.len() {
            let height = self.line_heights[index];
            if position + height - text_rect.height > HEIGHT_TOLERANCE {
                self.blocks.push(BlockData {
                    index: line_index,
                    length: index - line_index,
                });
                return Ok(());
            }
            position += height;
        }
        if self.line_heights.len() > line_index {
            self.blocks.push(BlockData {
                index: line_index,
                length: self.line_heights.len() - line_index,
            });
        }
        Ok(())
    }
    fn is_end(&self, save: bool) -> bool {
        save && self
            .loader_context
            .as_ref()
            .and_then(|context| context.total_lines)
            .is_some_and(|limit| limit > 0 && self.lines >= limit)
    }
    fn layout_internal(&mut self, block_index: usize) -> Result<bool> {
        if block_index >= self.count_blocks() {
            return Ok(false);
        }
        let Some(context) = self.loader_context.as_mut() else {
            return Ok(false);
        };
        if context.width < 1.0 {
            return Ok(false);
        }
        context.total_lines = None;
        self.lines = 0;
        let size = CfxSizeF {
            width: context.width,
            height: context.height,
        };
        let block_height_count = context.block_heights.len();
        if block_index < block_height_count {
            return Ok(true);
        }
        if block_index == block_height_count {
            let mut position = context.start_line_offset
                - context
                    .block_heights
                    .iter()
                    .map(|block| block.height)
                    .sum::<f32>();
            context.char_index = 0;
            context.total_lines = self.blocks.get(block_index).map(|block| block.length);
            self.unload();
            self.breaker = Some(self.create_break(true)?);
            self.loader(size.width, &mut position, true)?;
            if self.blocks.is_empty()
                && self.loader_context.as_ref().unwrap().start_line_offset < 0.1
            {
                self.update_align(size.height, position);
            }
            return Ok(true);
        }
        if self.text_data_node.is_none() {
            return Ok(true);
        }
        if !self.blocks.is_empty() && block_index < self.blocks.len() - 1 {
            self.loader_context.as_mut().unwrap().total_lines =
                Some(self.blocks[block_index].length);
        }
        // A line break may carry an entire word into the current CFGAS
        // buffer. Keep that shaped run across blocks. Native Reset followed
        // by replaying only the triggering character loses its word prefix.
        if self.breaker.is_none() {
            return Err(TextLayoutError::InvalidSize);
        }
        let mut position = 0.0;
        if !self.rich_text {
            if let Some(node) = self.loader_context.as_ref().unwrap().node.clone() {
                self.load_text(&node, size.width, &mut position, true)?;
            }
            return Ok(true);
        }
        let Some(container) = self.get_xml_container_node() else {
            return Ok(true);
        };
        let Some(mut saved) = self.loader_context.as_ref().unwrap().xml_node.clone() else {
            return Ok(true);
        };
        let mut current = Some(saved.clone());
        // Continue siblings, then close ancestors until reaching the original
        // rich-text container. No prior text is reshaped or copied into blocks.
        loop {
            while let Some(node) = current {
                let parent = self.loader_context.as_ref().unwrap().parent_style.clone();
                if !self.load_rich_text(
                    &node,
                    size.width,
                    &mut position,
                    parent,
                    true,
                    None,
                    true,
                    false,
                    0,
                    0,
                )? {
                    return Ok(true);
                }
                current = node.next_sibling();
            }
            let Some(parent) = saved.parent() else {
                break;
            };
            if parent == container {
                break;
            }
            // Each closed ancestor restores its actual parent CSS scope;
            // the saved leaf's style must not leak into later siblings.
            let style = self
                .text_parser
                .get_parse_context_from_map(&parent)
                .and_then(|context| context.parent_style.clone());
            self.loader_context.as_mut().unwrap().parent_style = style.clone();
            if !self.load_rich_text(
                &parent,
                size.width,
                &mut position,
                style,
                true,
                None,
                false,
                false,
                0,
                0,
            )? {
                break;
            }
            saved = parent.clone();
            current = parent.next_sibling();
        }
        Ok(true)
    }
    pub fn draw_block(
        &mut self,
        device: &mut CfxAggDeviceDriver<'_, '_>,
        matrix: CfxMatrix,
        clip: CfxRectF,
        block_index: usize,
    ) -> Result<bool> {
        if self.piece_lines.is_empty() {
            for index in 0..self.count_blocks() {
                self.layout_internal(index)?;
            }
            self.tabstop_context = None;
            self.loader_context = None;
        }
        let (start, length) = if self.blocks.is_empty() {
            (0, self.piece_lines.len())
        } else if let Some(block) = self.blocks.get(block_index) {
            (block.index, block.length)
        } else {
            return Ok(false);
        };
        self.draw_lines(device, matrix, clip, start, length)
    }
}

impl CxfaTextLayout {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn load_rich_text(
        &mut self,
        xml: &XmlNodeRef,
        width: f32,
        position: &mut f32,
        parent: Option<Rc<CfxCssComputedStyle>>,
        save: bool,
        mut link: Option<Rc<CfgasLinkUserData>>,
        end_break: bool,
        mut ordered: bool,
        list_index: i32,
        depth: usize,
    ) -> Result<bool> {
        if depth >= 512 {
            return Err(TextLayoutError::NestingLimit);
        }
        let display = self
            .text_parser
            .get_parse_context_from_map(xml)
            .map(|context| context.display);
        let mut style = None;
        let mut content = false;
        let mut space_below = 0.0;
        if end_break {
            let mut current_ol = false;
            let mut current_li = false;
            if let Some(display) = display {
                content = xml.node_type() == XmlNodeType::Text;
                let name = if xml.node_type() == XmlNodeType::Element {
                    xml.local_tag_name().to_vec()
                } else {
                    Vec::new()
                };
                if eq(&name, "ol") {
                    ordered = true;
                    current_ol = true;
                }
                if !matches!(
                    display,
                    CfxCssDisplay::Block | CfxCssDisplay::Inline | CfxCssDisplay::ListItem
                ) {
                    return Ok(true);
                }
                style = self.text_parser.compute_style(xml, parent.clone());
                if let Some(current) = style.as_mut() {
                    self.init_rich_break(
                        Rc::make_mut(current),
                        display,
                        width,
                        xml,
                        parent.as_deref(),
                    )?;
                } else if let Some(parent) = parent.as_ref() {
                    let mut current = (**parent).clone();
                    self.init_rich_break(&mut current, display, width, xml, Some(parent))?;
                } else {
                    self.init_break(width)?;
                }
                if matches!(display, CfxCssDisplay::Block | CfxCssDisplay::ListItem)
                    && !["body", "html", "ol", "ul"]
                        .iter()
                        .any(|tag| eq(&name, tag))
                {
                    if let Some(margin) = style.as_ref().and_then(|s| s.get_margin_width()) {
                        *position += margin.top.value;
                        space_below = margin.bottom.value;
                    }
                }
                if eq(&name, "a") {
                    let href = xml.get_attribute(&wide("href"));
                    if !href.is_empty() {
                        link = Some(Rc::new(CfgasLinkUserData::new(&href)));
                    }
                }
                let text_style = if content {
                    parent.as_deref()
                } else {
                    style.as_deref()
                };
                let tabs = self.text_parser.count_tabs(text_style);
                let preserve_spaces = self.text_parser.is_space_run(text_style);
                let mut text = if content && tabs == 0 {
                    xml.text().to_vec()
                } else if eq(&name, "br") {
                    vec![10]
                } else if eq(&name, "li") {
                    current_li = true;
                    if ordered {
                        wide(&format!("{list_index}.  "))
                    } else {
                        vec![0xb7, 32]
                    }
                } else if !content {
                    if tabs > 0 {
                        let count =
                            usize::try_from(tabs).map_err(|_| TextLayoutError::InvalidSize)?;
                        let mut text = Vec::new();
                        text.try_reserve(count)
                            .map_err(|_| TextLayoutError::InvalidSize)?;
                        text.resize(count, 9);
                        text
                    } else {
                        self.text_parser
                            .get_embedded_obj(&self.provider, xml)
                            .unwrap_or_default()
                    }
                } else {
                    Vec::new()
                };
                if !text.is_empty() && content && !preserve_spaces {
                    process_text(&mut text);
                }
                if let Some(context) = self.loader_context.as_mut() {
                    if !text.is_empty() && context.filter_space {
                        let start = text.iter().take_while(|&&ch| ch == 32).count();
                        text.drain(..start);
                    }
                    if display == CfxCssDisplay::Block {
                        context.filter_space = true;
                    } else if display == CfxCssDisplay::Inline && context.filter_space {
                        context.filter_space = false;
                    } else if !text.is_empty() {
                        context.filter_space = text.last() == Some(&32);
                    }
                }
                if !text.is_empty() {
                    if self
                        .loader_context
                        .as_ref()
                        .is_none_or(|context| context.char_index == 0)
                    {
                        if let Some(style) = if content {
                            parent.clone()
                        } else {
                            style.clone()
                        } {
                            self.breaker.as_mut().unwrap().set_user_data(Some(Rc::new(
                                CfgasTextUserData {
                                    style,
                                    link_data: link.clone(),
                                },
                            )));
                        }
                    }
                    if self.append_char(&text, position, 0.0, save)? {
                        if let Some(context) = self.loader_context.as_mut() {
                            context.filter_space = false;
                        }
                        if !self.is_end(save) {
                            return Ok(true);
                        }
                        if let Some(context) = self
                            .loader_context
                            .as_mut()
                            .filter(|context| context.total_lines.is_some())
                        {
                            context.xml_node = Some(xml.clone());
                            context.parent_style = parent;
                        }
                        return Ok(false);
                    }
                }
            }
            let mut child = xml.first_child();
            let mut next_index = list_index;
            while let Some(node) = child {
                if current_ol {
                    next_index = next_index.saturating_add(1);
                }
                if !self.load_rich_text(
                    &node,
                    width,
                    position,
                    if display.is_some() {
                        style.clone()
                    } else {
                        parent.clone()
                    },
                    save,
                    link.clone(),
                    true,
                    ordered,
                    next_index,
                    depth + 1,
                )? {
                    return Ok(false);
                }
                child = node.next_sibling();
            }
            if display == Some(CfxCssDisplay::Block) {
                if let Some(context) = self.loader_context.as_mut() {
                    context.filter_space = true;
                }
            }
            if current_li {
                self.end_break(BreakType::Line, position, save)?;
            }
        }
        let Some(display) = display.filter(|_| !content) else {
            return Ok(true);
        };
        self.end_break(
            if display == CfxCssDisplay::Block {
                BreakType::Paragraph
            } else {
                BreakType::Piece
            },
            position,
            save,
        )?;
        if display == CfxCssDisplay::Block {
            *position += space_below;
            if let Some(tabs) = self.tabstop_context.as_mut() {
                tabs.remove_all();
            }
        }
        if !self.is_end(save) {
            return Ok(true);
        }
        if let Some(context) = self
            .loader_context
            .as_mut()
            .filter(|context| context.total_lines.is_some())
        {
            context.xml_node = xml.next_sibling();
            context.parent_style = parent;
        }
        Ok(false)
    }
    pub(super) fn append_char(
        &mut self,
        text: &[u32],
        position: &mut f32,
        space_above: f32,
        save: bool,
    ) -> Result<bool> {
        let first = self
            .loader_context
            .as_ref()
            .map_or(0, |context| context.char_index);
        for (index, &ch) in text.iter().enumerate().skip(first) {
            let status =
                self.breaker
                    .as_mut()
                    .unwrap()
                    .append_char(if ch == 0xa0 { 32 } else { ch });
            if !matches!(status, BreakType::None | BreakType::Piece) {
                self.append_text_line(status, position, save, false)?;
                if self.is_end(save) {
                    if let Some(context) = self.loader_context.as_mut() {
                        context.char_index = index + 1;
                    }
                    return Ok(true);
                }
                if status == BreakType::Paragraph && self.rich_text {
                    *position += space_above;
                }
            }
        }
        if let Some(context) = self.loader_context.as_mut() {
            context.char_index = 0;
        }
        Ok(false)
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
    struct FontHost(Rc<CfgasGeFont>);
    impl XfaTextFontHost for FontHost {
        fn get_font(&self, _: &[u32], _: u32) -> Option<Rc<CfgasGeFont>> {
            Some(self.0.clone())
        }
    }
    fn layout(rich: bool) -> CxfaTextLayout {
        let content="alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon phi chi psi omega";
        let value = if rich {
            format!("<exData contentType=\"text/html\"><body xmlns=\"http://www.w3.org/1999/xhtml\"><p><a href=\"https://example.test/link\" style=\"color:#008000;text-decoration:underline\"><b>{content}</b></a></p><p><span style=\"color:#ff0000\">second paragraph ends here</span></p></body></exData>")
        } else {
            format!("<text>{content}</text>")
        };
        let mut bytes=format!("<form xmlns=\"http://www.xfa.org/schema/xfa-form/2.8/\"><subform><draw><font typeface=\"Roboto\" size=\"12pt\"/><value>{value}</value></draw></subform></form>").into_bytes();
        bytes.push(0);
        let xml = Rc::new(RefCell::new(
            CfxXmlParser::new(Rc::new(CfxReadOnlySpanStream::new(&bytes)))
                .parse()
                .unwrap(),
        ));
        let doc = CxfaDocument::new();
        let mut builder = CxfaDocumentBuilder::new(doc.clone());
        assert!(builder.build_document(xml, XfaPacketType::Form));
        let root = builder.get_root_node().unwrap();
        assert!(doc.set_root(Some(root.clone())));
        let draw = root
            .get_child(0, XfaElement::Subform, false)
            .unwrap()
            .get_child(0, XfaElement::Draw, false)
            .unwrap();
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
        CxfaTextLayout::new(
            Rc::new(CxfaTextProvider::new(draw, TextProviderType::Text)),
            Rc::new(FontHost(font)),
        )
        .unwrap()
    }
    fn text(layout: &CxfaTextLayout) -> Vec<u32> {
        layout
            .piece_lines
            .iter()
            .flat_map(|line| &line.text_pieces)
            .flat_map(|piece| &piece.piece.text)
            .copied()
            .filter(|&ch| ch != 10 && ch != 13)
            .collect()
    }
    fn paginate(layout: &mut CxfaTextLayout, width: f32) -> usize {
        assert_eq!(layout.start_layout(width).unwrap(), width);
        let height = layout.get_layout_height().unwrap();
        assert!(height > 24.0);
        let block_height = layout.line_heights[..2].iter().sum::<f32>();
        assert_eq!(layout.do_split_layout(0, 1.0, height).unwrap(), 0.0);
        assert!(layout.has_block());
        let accepted = layout.do_split_layout(0, block_height, height).unwrap();
        assert!(accepted > 0.0 && accepted <= block_height);
        layout.clear_blocks();
        let mut index = 0;
        while layout.next_index_from_last_block() < layout.line_heights.len() {
            layout
                .item_blocks(CfxRectF::new(0.0, 0.0, width, block_height), index)
                .unwrap();
            assert!(layout.blocks.last().unwrap().length > 0);
            index += 1;
            assert!(index < 100);
        }
        assert!(index > 1);
        let expected_lines = layout.line_heights.len();
        for block in 0..index {
            let previous = layout.piece_lines.len();
            assert!(layout.layout_internal(block).unwrap());
            assert_eq!(
                layout.piece_lines.len() - previous,
                layout.blocks[block].length,
                "block {block} of {index}; char cursor {:?}; blocks {:?}",
                layout
                    .loader_context
                    .as_ref()
                    .map(|context| context.char_index),
                layout.blocks
            );
        }
        assert_eq!(layout.piece_lines.len(), expected_lines);
        index
    }
    #[test]
    fn plain_split_loader_resumes_without_replaying_or_losing_boundary_characters() {
        let mut whole = layout(false);
        whole
            .layout(CfxSizeF {
                width: 70.0,
                height: 1000.0,
            })
            .unwrap();
        let expected = text(&whole);
        let mut paged = layout(false);
        let blocks = paginate(&mut paged, 70.0);
        assert_eq!(text(&paged), expected);
        let mut bitmap = CfxDibitmap::create(100, 100, FxdibFormat::Bgra, 0).unwrap();
        bitmap.clear(0xffffffff);
        {
            let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
            assert!(paged
                .draw_block(
                    &mut device,
                    CfxMatrix::default(),
                    CfxRectF::new(0.0, 0.0, 70.0, 70.0),
                    blocks - 1
                )
                .unwrap());
            assert!(!paged
                .draw_block(
                    &mut device,
                    CfxMatrix::default(),
                    CfxRectF::new(0.0, 0.0, 70.0, 70.0),
                    blocks
                )
                .unwrap());
        }
        assert!(bitmap
            .buffer()
            .chunks_exact(4)
            .any(|p| p[..3] != [255, 255, 255]));
        paged.reset_has_block();
        assert!(!paged.has_block());
        assert_eq!(paged.start_layout(140.0).unwrap(), 140.0);
        assert!(!paged.is_loaded());
        assert!(paged.blocks.is_empty());
        assert!(paged
            .loader_context
            .as_ref()
            .unwrap()
            .block_heights
            .is_empty());
    }
    #[test]
    fn rich_split_loader_restores_nested_styles_links_and_parent_closing_breaks() {
        let mut whole = layout(true);
        whole
            .layout(CfxSizeF {
                width: 75.0,
                height: 1000.0,
            })
            .unwrap();
        let expected = text(&whole);
        let mut paged = layout(true);
        paginate(&mut paged, 75.0);
        assert_eq!(text(&paged), expected);
        let linked: Vec<_> = paged
            .piece_lines
            .iter()
            .flat_map(|line| &line.text_pieces)
            .filter(|piece| piece.link_data.is_some())
            .collect();
        assert!(linked.len() > 3);
        assert!(linked
            .iter()
            .all(|piece| piece.color == 0xff008000 && piece.underline == 1));
        let red: Vec<_> = paged
            .piece_lines
            .iter()
            .flat_map(|line| &line.text_pieces)
            .filter(|piece| piece.color == 0xffff0000)
            .collect();
        assert!(!red.is_empty());
        assert!(red.iter().all(|piece| piece.link_data.is_none()));
        assert!(paged.loader_context.as_ref().unwrap().char_index == 0);
    }
}
