// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Rich text CSS/tree parsing and text properties from `cxfa_textparser.{h,cpp}`.
use super::{
    cxfa_textprovider::CxfaTextProvider,
    cxfa_texttabstopscontext::CxfaTextTabstopsContext,
    fxfa_basic::{XfaAttribute, XfaAttributeValue, XfaUnit},
    parser::cxfa_measurement::CxfaMeasurement,
};
use crate::{
    fxcrt::{
        css::{
            cfx_css::*, cfx_csscomputedstyle::CfxCssComputedStyle,
            cfx_cssdata::hash_code_lowered_wide, cfx_cssdeclaration::CfxCssDeclaration,
            cfx_cssstyleselector::CfxCssStyleSelector, cfx_cssstylesheet::CfxCssStyleSheet,
        },
        fx_string::string_to_int_wide,
        xml::cfx_xmlnode::{XmlNodeRef, XmlNodeType},
    },
    fxge::fx_font::{FONT_STYLE_FORCE_BOLD, FONT_STYLE_ITALIC},
    xfa::fgas::font::cfgas_gefont::CfgasGeFont,
};
use std::{collections::HashMap, rc::Rc};

fn wide(s: &str) -> Vec<u32> {
    s.chars().map(u32::from).collect()
}
fn ascii_eq(v: &[u32], s: &str) -> bool {
    v.iter().copied().eq(s.bytes().map(u32::from))
}
fn ascii_eq_no_case(v: &[u32], s: &str) -> bool {
    v.iter()
        .copied()
        .map(|c| if (65..=90).contains(&c) { c + 32 } else { c })
        .eq(s.bytes().map(u32::from))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        fxcrt::{
            cfx_read_only_span_stream::CfxReadOnlySpanStream,
            xml::{cfx_xmldocument::CfxXmlDocument, cfx_xmlparser::CfxXmlParser},
        },
        xfa::fxfa::{
            cxfa_textprovider::TextProviderType,
            fxfa_basic::{XfaElement, XfaPacketType},
            parser::{cxfa_document::CxfaDocument, cxfa_document_builder::CxfaDocumentBuilder},
        },
    };
    use std::cell::RefCell;

    fn build(input: &[u8]) -> (Rc<CxfaDocument>, super::super::parser::cxfa_node::CxfaNode) {
        let mut bytes = input.to_vec();
        bytes.push(0);
        let xml = Rc::new(RefCell::new(
            CfxXmlParser::new(Rc::new(CfxReadOnlySpanStream::new(&bytes)))
                .parse()
                .unwrap(),
        ));
        let document = CxfaDocument::new();
        let mut builder = CxfaDocumentBuilder::new(document.clone());
        assert!(builder.build_document(xml, XfaPacketType::Form));
        let node = builder.get_root_node().unwrap();
        assert!(document.set_root(Some(node.clone())));
        (document, node)
    }
    #[test]
    fn tag_validate() {
        // All native CXFATextParserTest.TagValidate inputs and assertions.
        for name in [
            "br", "Br", "BR", "a", "b", "i", "p", "li", "ol", "ul", "sub", "sup", "span", "body",
            "html",
        ] {
            assert!(CxfaTextParser::tag_validate(&wide(name)), "{name}");
        }
        for name in ["", "tml", "xhtml", "htmlx"] {
            assert!(!CxfaTextParser::tag_validate(&wide(name)), "{name}");
        }
    }
    #[test]
    fn model_sources_styles_and_embedded_display_values() {
        let (_document, form) = build(br##"<form xmlns="http://www.xfa.org/schema/xfa-form/2.8/"><subform>
          <field name="target" id="target"><ui><textEdit/></ui><value><text>embedded</text></value></field>
          <field name="source"><ui><checkButton/></ui><font typeface="Courier" size="12pt" baselineShift="2pt" underline="2" lineThrough="1"/>
            <para hAlign="center" vAlign="middle" marginLeft="3pt" marginRight="4pt" textIndent="5pt" lineHeight="18pt"/>
            <value><exData contentType="text/html"><body xmlns="http://www.w3.org/1999/xhtml"><p>rich</p></body></exData></value>
            <caption><font typeface="Caption"/><value><text>caption</text></value></caption>
            <items><text name="rollover">over</text><text name="down">down</text></items>
          </field>
        </subform></form>"##);
        let subform = form.get_child(0, XfaElement::Subform, false).unwrap();
        let target = subform.get_first_child_by_name(&wide("target")).unwrap();
        let source = subform.get_first_child_by_name(&wide("source")).unwrap();
        let provider = CxfaTextProvider::new(source.clone(), TextProviderType::Text);
        let (text, rich) = provider.get_text_node();
        assert!(rich);
        assert_eq!(text.unwrap().get_element_type(), XfaElement::ExData);
        assert!(provider.is_check_button_and_auto_width());
        assert_eq!(provider.get_embedded_obj(&wide("target")), None);
        target.set_widget_ready();
        assert_eq!(
            provider.get_embedded_obj(&wide("target")),
            Some(wide("embedded"))
        );
        let mut parser = CxfaTextParser::new();
        let root = parser.create_root_style(&provider);
        assert_eq!(root.get_font_size(), 12.0);
        assert_eq!(root.get_text_align(), CfxCssTextAlign::Center);
        assert_eq!(root.get_margin_width().unwrap().right.value, 4.0);
        assert_eq!(root.get_text_indent().value, 5.0);
        assert_eq!(parser.get_baseline(&provider, Some(&root)), -2.0);
        assert_eq!(parser.get_baseline(&provider, None), 2.0);
        assert_eq!(parser.get_underline(&provider, Some(&root)), 2);
        assert_eq!(parser.get_linethrough(&provider, Some(&root)), 1);
        assert_eq!(
            parser.get_line_height(&provider, Some(&root), true, 0.5),
            6.0
        );
        assert_eq!(
            parser.get_line_height(&provider, Some(&root), false, 0.5),
            9.0
        );
        assert_eq!(parser.get_v_align(&provider), XfaAttributeValue::Middle);
        let caption = CxfaTextProvider::new(source.clone(), TextProviderType::Caption);
        assert_eq!(
            caption.get_text_node().0.unwrap().get_content(),
            wide("caption")
        );
        assert_eq!(
            caption.get_font_if_exists().unwrap().font_get_typeface(),
            wide("Caption")
        );
        for (kind, content) in [
            (TextProviderType::Rollover, "over"),
            (TextProviderType::Down, "down"),
        ] {
            let p = CxfaTextProvider::new(source.clone(), kind);
            assert_eq!(p.get_text_node().0.unwrap().get_content(), wide(content));
            assert_eq!(p.get_embedded_obj(&wide("target")), None);
        }
        let mut dom = CfxXmlDocument::new();
        let embed = dom.create_element(&wide("span"));
        embed.set_attribute(&wide("xfa:embed"), &wide("#target"));
        assert_eq!(parser.get_embedded_obj(&provider, &embed), None); // default type is som
        embed.set_attribute(&wide("xfa:embedType"), &wide("URI"));
        assert_eq!(
            parser.get_embedded_obj(&provider, &embed),
            Some(wide("embedded"))
        );
        embed.set_attribute(&wide("xfa:embedMode"), &wide("RAW"));
        assert_eq!(
            parser.get_embedded_obj(&provider, &embed),
            Some(wide("embedded"))
        );
        embed.set_attribute(&wide("xfa:embedMode"), &wide("other"));
        assert_eq!(parser.get_embedded_obj(&provider, &embed), None);
    }
    #[test]
    fn rich_css_contexts_inheritance_custom_properties_and_sorted_tabs() {
        let (_document, form) = build(br#"<form xmlns="http://www.xfa.org/schema/xfa-form/2.8/"><subform><field><value><text>x</text></value></field></subform></form>"#);
        let field = form
            .get_child(0, XfaElement::Subform, false)
            .unwrap()
            .get_child(0, XfaElement::Field, false)
            .unwrap();
        let provider = CxfaTextProvider::new(field, TextProviderType::Text);
        let mut xml = CfxXmlDocument::new();
        let body = xml.create_element(&wide("body"));
        let bold = xml.create_element(&wide("b"));
        body.append_last_child(&bold);
        bold.set_attribute(&wide("style"), &wide("font-size:20pt; xfa-font-horizontal-scale:80; xfa-font-vertical-scale:75; xfa-spacerun:YES; xfa-tab-count:2; tab-interval:1in; xfa-tab-stops:right 2in left l('.') 36pt center 36pt"));
        let text = xml.create_text(&wide("a  b"));
        bold.append_last_child(&text);
        let unknown = xml.create_element(&wide("script"));
        body.append_last_child(&unknown);
        let ignored = xml.create_element(&wide("b"));
        unknown.append_last_child(&ignored);
        let cdata = xml.create_chardata(&wide("not a text node"));
        body.append_last_child(&cdata);
        let mut parser = CxfaTextParser::new();
        parser.do_parse(&body, &provider);
        assert!(parser.is_parsed());
        assert_eq!(
            parser.get_parse_context_from_map(&body).unwrap().display,
            CfxCssDisplay::Block
        );
        assert!(parser.get_parse_context_from_map(&ignored).is_none());
        assert!(parser.get_parse_context_from_map(&cdata).is_none());
        let root = parser.create_root_style(&provider);
        let body_style = parser.compute_style(&body, Some(root)).unwrap();
        let bold_style = parser.compute_style(&bold, Some(body_style)).unwrap();
        // Native CSS Bolder maps directly to 900, not relative 700.
        assert_eq!(bold_style.get_font_weight(), 900);
        assert_eq!(bold_style.get_font_size(), 20.0);
        assert_eq!(parser.get_tab_interval(Some(&bold_style)), 72.0);
        assert_eq!(parser.count_tabs(Some(&bold_style)), 2);
        assert!(parser.is_space_run(Some(&bold_style)));
        assert_eq!(
            parser.get_hor_scale(&provider, Some(&bold_style), Some(&bold)),
            80
        );
        assert_eq!(parser.get_ver_scale(&provider, Some(&bold_style)), 75);
        assert!(parser
            .compute_style(&text, Some(bold_style.clone()))
            .is_none());
        let mut inherited = CfxCssComputedStyle::new();
        inherited.set_font_size(20.0);
        assert_eq!(
            parser.get_hor_scale(&provider, Some(&inherited), Some(&text)),
            80
        );
        let mut tabs = CxfaTextTabstopsContext::new();
        assert!(parser.get_tabstops(Some(&bold_style), &mut tabs));
        assert_eq!(
            tabs.tabstops.iter().map(|s| s.position).collect::<Vec<_>>(),
            [36.0, 36.0, 144.0]
        );
        assert_eq!(
            tabs.tabstops[0].align,
            hash_code_lowered_wide(&wide("center"))
        );
        tabs.tab_index = 2;
        tabs.has_tabstops = true;
        tabs.left = 5.0;
        tabs.reset();
        assert_eq!(tabs.tab_index, -1);
        assert_eq!(tabs.left, 0.0);
        assert!(!tabs.has_tabstops);
        assert_eq!(tabs.tabstops.len(), 3);
        tabs.remove_all();
        assert!(tabs.tabstops.is_empty());
        parser.reset();
        assert!(!parser.is_parsed());
        assert!(parser.get_parse_context_from_map(&body).is_none());
    }
}
fn custom<'a>(style: Option<&'a CfxCssComputedStyle>, name: &str) -> Option<&'a [u32]> {
    style?.get_custom_style(&wide(name))
}

/// The font lookup performed by CXFA_FFDoc's font manager. A real owner
/// supplies it; parsing does not substitute an unrelated stock font.
pub trait XfaTextFontHost {
    fn get_font(&self, family: &[u32], styles: u32) -> Option<Rc<CfgasGeFont>>;
}

#[derive(Clone, Debug)]
pub struct TextParseContext {
    pub parent_style: Option<Rc<CfxCssComputedStyle>>,
    pub display: CfxCssDisplay,
    pub declarations: Vec<Rc<CfxCssDeclaration>>,
}

#[derive(Debug, Default)]
pub struct CxfaTextParser {
    parsed: bool,
    selector: Option<CfxCssStyleSelector>,
    contexts: HashMap<XmlNodeRef, TextParseContext>,
}

impl CxfaTextParser {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn reset(&mut self) {
        self.contexts.clear();
        self.parsed = false;
    }
    pub fn is_parsed(&self) -> bool {
        self.parsed
    }
    fn init_css_data(&mut self, provider: &CxfaTextProvider) {
        if self.selector.is_some() {
            return;
        }
        let mut selector = CfxCssStyleSelector::new();
        let font_size = self.get_font_size(provider, None);
        // Malformed model font sizes must not panic in the CSS selector.
        if font_size.is_finite() && font_size > 0.0 {
            selector.set_default_font_size(font_size);
        }
        let mut sheet = CfxCssStyleSheet::new();
        let css = concat!(
            "html,body,ol,p,ul{display:block}",
            "li{display:list-item}",
            "ol,ul{padding-left:33px;margin:1.12em 0}",
            "ol{list-style-type:decimal}",
            "a{color:#0000ff;text-decoration:underline}",
            "b{font-weight:bolder}",
            "i{font-style:italic}",
            "sup{vertical-align:+15em;font-size:.66em}",
            "sub{vertical-align:-15em;font-size:.66em}"
        );
        if sheet.load_buffer(&wide(css)) {
            selector.set_ua_style_sheet(sheet);
        }
        selector.update_style_index();
        self.selector = Some(selector);
    }
    pub fn create_root_style(&mut self, provider: &CxfaTextProvider) -> Rc<CfxCssComputedStyle> {
        self.init_css_data(provider);
        let mut style = self.selector.as_ref().unwrap().create_computed_style(None);
        let mut line_height = 0.0;
        if let Some(para) = provider.get_para_if_exists() {
            let measure = |attr| para.get_measure_in_unit(attr, XfaUnit::Pt);
            line_height = measure(XfaAttribute::LineHeight);
            style.set_text_indent(&CfxCssLength::new(
                CfxCssLengthUnit::Point,
                measure(XfaAttribute::TextIndent),
            ));
            style.set_text_align(match para.get_enum(XfaAttribute::HAlign) {
                XfaAttributeValue::Center => CfxCssTextAlign::Center,
                XfaAttributeValue::Right => CfxCssTextAlign::Right,
                XfaAttributeValue::Justify => CfxCssTextAlign::Justify,
                XfaAttributeValue::JustifyAll => CfxCssTextAlign::JustifyAll,
                _ => CfxCssTextAlign::Left,
            });
            let length = |attr| CfxCssLength::new(CfxCssLengthUnit::Point, measure(attr));
            style.set_margin_width(&CfxCssRect {
                left: length(XfaAttribute::MarginLeft),
                top: length(XfaAttribute::SpaceAbove),
                right: length(XfaAttribute::MarginRight),
                bottom: length(XfaAttribute::SpaceBelow),
            });
        }
        let mut font_size = 10.0;
        if let Some(font) = provider.get_font_if_exists() {
            style.set_color(font.font_get_color());
            style.set_font_style(if font.font_is_italic() {
                CfxCssFontStyle::Italic
            } else {
                CfxCssFontStyle::Normal
            });
            style.set_font_weight(if font.font_is_bold() { 700 } else { 400 });
            style.set_number_vertical_align(-font.font_get_baseline_shift());
            font_size = font.font_get_size();
            style.set_letter_spacing(&CfxCssLength::new(
                CfxCssLengthUnit::Point,
                font.font_get_letter_spacing(),
            ));
            let mut decoration = 0;
            if font.font_get_line_through() > 0 {
                decoration |= CfxCssTextDecoration::LineThrough as u8;
            }
            if font.font_get_underline() > 1 {
                decoration |= CfxCssTextDecoration::Double as u8;
            } else if font.font_get_underline() > 0 {
                decoration |= CfxCssTextDecoration::Underline as u8;
            }
            style.set_text_decoration(decoration);
        }
        style.set_font_size(font_size);
        style.set_line_height(line_height);
        Rc::new(style)
    }
    fn create_style(&self, parent: Option<&CfxCssComputedStyle>) -> CfxCssComputedStyle {
        let mut style = self
            .selector
            .as_ref()
            .unwrap()
            .create_computed_style(parent);
        if let Some(parent) = parent {
            style.set_text_decoration(parent.get_text_decoration());
            style.set_number_vertical_align(
                if parent.get_vertical_align() == CfxCssVerticalAlign::Number {
                    parent.get_number_vertical_align()
                } else {
                    0.0
                },
            );
            if let Some(margin) = parent.get_margin_width() {
                style.set_margin_width(margin);
            }
        }
        style
    }
    pub fn compute_style(
        &mut self,
        xml: &XmlNodeRef,
        parent: Option<Rc<CfxCssComputedStyle>>,
    ) -> Option<Rc<CfxCssComputedStyle>> {
        self.contexts.get_mut(xml)?.parent_style = parent.clone();
        if xml.node_type() == XmlNodeType::Text {
            return None;
        }
        let context = self.contexts.get(xml)?;
        let mut style = self.create_style(parent.as_deref());
        self.selector.as_ref()?.compute_style(
            &context.declarations,
            &xml.get_attribute(&wide("style")),
            &[],
            &mut style,
        );
        Some(Rc::new(style))
    }
    pub fn do_parse(&mut self, container: &XmlNodeRef, provider: &CxfaTextProvider) {
        if self.parsed || !container.is_alive() {
            return;
        }
        self.parsed = true;
        let root_style = self.create_root_style(provider);
        let mut pending = vec![(container.clone(), Some(root_style))];
        // Explicit sibling traversal preserves source order without recursive
        // stack growth on nested rich text.
        while let Some((xml, parent)) = pending.pop() {
            let content = xml.node_type() == XmlNodeType::Text;
            if !content
                && (xml.node_type() != XmlNodeType::Element
                    || !Self::tag_validate(&xml.local_tag_name()))
            {
                continue;
            }
            let mut declarations = Vec::new();
            let mut next_style = None;
            let mut display = CfxCssDisplay::Inline;
            if !content {
                let selector = self.selector.as_ref().unwrap();
                declarations = selector.match_declarations(&xml.local_tag_name());
                let mut style = self.create_style(parent.as_deref());
                selector.compute_style(
                    &declarations,
                    &xml.get_attribute(&wide("style")),
                    &[],
                    &mut style,
                );
                display = style.get_display();
                next_style = Some(Rc::new(style));
            }
            // The source records parent style when ComputeStyle is requested,
            // rather than during this initial declaration walk.
            self.contexts.insert(
                xml.clone(),
                TextParseContext {
                    parent_style: None,
                    display,
                    declarations,
                },
            );
            let mut child = xml.last_child();
            while let Some(node) = child {
                child = node.prev_sibling();
                pending.push((node, next_style.clone()));
            }
        }
    }
    pub fn tag_validate(name: &[u32]) -> bool {
        const TAGS: [u32; 13] = [
            0x61, 0x62, 0x69, 0x70, 0x1f714, 0x22a55, 0x239bb, 0x25881, 0x0bd37faa, 0x0bd37fb8,
            0xa73e3af2, 0xb182eaae, 0xdb8ac455,
        ];
        TAGS.binary_search(&hash_code_lowered_wide(name)).is_ok()
    }
    pub fn get_parse_context_from_map(&self, node: &XmlNodeRef) -> Option<&TextParseContext> {
        self.contexts.get(node)
    }
    pub fn get_v_align(&self, provider: &CxfaTextProvider) -> XfaAttributeValue {
        provider
            .get_para_if_exists()
            .map_or(XfaAttributeValue::Top, |p| p.get_enum(XfaAttribute::VAlign))
    }
    pub fn get_tab_interval(&self, style: Option<&CfxCssComputedStyle>) -> f32 {
        custom(style, "tab-interval").map_or(36.0, |value| {
            CxfaMeasurement::from_string(value).to_unit(XfaUnit::Pt)
        })
    }
    pub fn count_tabs(&self, style: Option<&CfxCssComputedStyle>) -> i32 {
        custom(style, "xfa-tab-count").map_or(0, string_to_int_wide)
    }
    pub fn is_space_run(&self, style: Option<&CfxCssComputedStyle>) -> bool {
        custom(style, "xfa-spacerun").is_some_and(|v| ascii_eq_no_case(v, "yes"))
    }
    pub fn get_font(
        &self,
        host: &dyn XfaTextFontHost,
        provider: &CxfaTextProvider,
        style: Option<&CfxCssComputedStyle>,
    ) -> Option<Rc<CfgasGeFont>> {
        let mut family = wide("Courier");
        let mut flags = 0;
        if let Some(font) = provider.get_font_if_exists() {
            family = font.font_get_typeface();
            if font.font_is_bold() {
                flags |= FONT_STYLE_FORCE_BOLD;
            }
            if font.font_is_italic() {
                flags |= FONT_STYLE_ITALIC;
            }
        }
        if let Some(style) = style {
            if let Some(last) = style.get_last_font_family() {
                family = last;
            }
            flags = 0;
            if style.get_font_weight() > 400 {
                flags |= FONT_STYLE_FORCE_BOLD;
            }
            if style.get_font_style() == CfxCssFontStyle::Italic {
                flags |= FONT_STYLE_ITALIC;
            }
        }
        host.get_font(&family, flags)
    }
    pub fn get_font_size(
        &self,
        provider: &CxfaTextProvider,
        style: Option<&CfxCssComputedStyle>,
    ) -> f32 {
        style.map_or_else(
            || {
                provider
                    .get_font_if_exists()
                    .map_or(10.0, |f| f.font_get_size())
            },
            |s| s.get_font_size(),
        )
    }
    pub fn get_hor_scale(
        &self,
        provider: &CxfaTextProvider,
        style: Option<&CfxCssComputedStyle>,
        xml: Option<&XmlNodeRef>,
    ) -> i32 {
        if style.is_some() {
            if let Some(value) = custom(style, "xfa-font-horizontal-scale") {
                return string_to_int_wide(value);
            }
            let mut node = xml.cloned();
            while let Some(current) = node {
                if let Some(value) = self
                    .contexts
                    .get(&current)
                    .and_then(|c| custom(c.parent_style.as_deref(), "xfa-font-horizontal-scale"))
                {
                    return string_to_int_wide(value);
                }
                node = current.parent();
            }
        }
        provider
            .get_font_if_exists()
            .map_or(100, |f| f.font_get_horizontal_scale() as i32)
    }
    pub fn get_ver_scale(
        &self,
        provider: &CxfaTextProvider,
        style: Option<&CfxCssComputedStyle>,
    ) -> i32 {
        custom(style, "xfa-font-vertical-scale").map_or_else(
            || {
                provider
                    .get_font_if_exists()
                    .map_or(100, |f| f.font_get_vertical_scale() as i32)
            },
            string_to_int_wide,
        )
    }
    pub fn get_underline(
        &self,
        provider: &CxfaTextProvider,
        style: Option<&CfxCssComputedStyle>,
    ) -> i32 {
        if let Some(style) = style {
            let bits = style.get_text_decoration();
            if bits & CfxCssTextDecoration::Double as u8 != 0 {
                2
            } else {
                i32::from(bits & CfxCssTextDecoration::Underline as u8 != 0)
            }
        } else {
            provider
                .get_font_if_exists()
                .map_or(0, |f| f.font_get_underline())
        }
    }
    pub fn get_underline_period(
        &self,
        provider: &CxfaTextProvider,
        style: Option<&CfxCssComputedStyle>,
    ) -> XfaAttributeValue {
        if let Some(v) = custom(style, "underlinePeriod") {
            if ascii_eq(v, "word") {
                XfaAttributeValue::Word
            } else {
                XfaAttributeValue::All
            }
        } else {
            provider
                .get_font_if_exists()
                .map_or(XfaAttributeValue::All, |f| f.font_get_underline_period())
        }
    }
    pub fn get_linethrough(
        &self,
        provider: &CxfaTextProvider,
        style: Option<&CfxCssComputedStyle>,
    ) -> i32 {
        style.map_or_else(
            || {
                provider
                    .get_font_if_exists()
                    .map_or(0, |f| f.font_get_line_through())
            },
            |s| i32::from(s.get_text_decoration() & CfxCssTextDecoration::LineThrough as u8 != 0),
        )
    }
    pub fn get_color(
        &self,
        provider: &CxfaTextProvider,
        style: Option<&CfxCssComputedStyle>,
    ) -> u32 {
        style.map_or_else(
            || {
                provider
                    .get_font_if_exists()
                    .map_or(0xff000000, |f| f.font_get_color())
            },
            |s| s.get_color(),
        )
    }
    pub fn get_baseline(
        &self,
        provider: &CxfaTextProvider,
        style: Option<&CfxCssComputedStyle>,
    ) -> f32 {
        if let Some(style) = style {
            if style.get_vertical_align() == CfxCssVerticalAlign::Number {
                style.get_number_vertical_align()
            } else {
                0.0
            }
        } else {
            provider
                .get_font_if_exists()
                .map_or(0.0, |f| f.font_get_baseline_shift())
        }
    }
    pub fn get_line_height(
        &self,
        provider: &CxfaTextProvider,
        style: Option<&CfxCssComputedStyle>,
        first: bool,
        vertical_scale: f32,
    ) -> f32 {
        let mut height = style.map_or_else(
            || {
                provider.get_para_if_exists().map_or(0.0, |p| {
                    p.get_measure_in_unit(XfaAttribute::LineHeight, XfaUnit::Pt)
                })
            },
            |s| s.get_line_height(),
        );
        if first {
            let size = self.get_font_size(provider, style);
            height = if height < 0.1 { size } else { height.min(size) };
        } else if height < 0.1 {
            height = self.get_font_size(provider, style) * 1.2;
        }
        height * vertical_scale
    }
    pub fn get_embedded_obj(
        &self,
        provider: &CxfaTextProvider,
        xml: &XmlNodeRef,
    ) -> Option<Vec<u32>> {
        if xml.node_type() != XmlNodeType::Element {
            return None;
        }
        let id = xml.get_attribute(&wide("xfa:embed"));
        if id.is_empty() {
            return None;
        }
        let kind = xml.get_attribute(&wide("xfa:embedType"));
        if !ascii_eq_no_case(&kind, "uri") {
            return None;
        }
        let mode = xml.get_attribute(&wide("xfa:embedMode"));
        if !mode.is_empty()
            && !ascii_eq_no_case(&mode, "raw")
            && !ascii_eq_no_case(&mode, "formatted")
        {
            return None;
        }
        provider.get_embedded_obj(if id.first() == Some(&35) {
            &id[1..]
        } else {
            &id
        })
    }
    pub fn get_tabstops(
        &self,
        style: Option<&CfxCssComputedStyle>,
        context: &mut CxfaTextTabstopsContext,
    ) -> bool {
        let Some(value) = custom(style, "xfa-tab-stops").or_else(|| custom(style, "tab-stops"))
        else {
            return false;
        };
        enum State {
            None,
            Alignment,
            StartLeader,
            Location,
        }
        let mut state = State::None;
        let (mut current, mut last) = (0, 0);
        let mut align: &[u32] = &[];
        while current < value.len() {
            let ch = value[current];
            match state {
                State::None => {
                    if ch <= 32 {
                        current += 1;
                    } else {
                        state = State::Alignment;
                        last = current;
                    }
                }
                State::Alignment => {
                    if ch != 32 {
                        current += 1;
                    } else {
                        align = &value[last..current];
                        state = State::StartLeader;
                        current += 1;
                        while current < value.len() && value[current] <= 32 {
                            current += 1;
                        }
                        last = current;
                    }
                }
                State::StartLeader => {
                    if ch != u32::from(b'l') {
                        state = State::Location;
                    } else {
                        let mut depth = 0i32;
                        while current < value.len() {
                            let c = value[current];
                            current += 1;
                            if c == 40 {
                                depth = depth.saturating_add(1);
                            } else if c == 41 {
                                depth = depth.saturating_sub(1);
                                if depth == 0 {
                                    break;
                                }
                            }
                        }
                        while current < value.len() && value[current] <= 32 {
                            current += 1;
                        }
                        last = current;
                        state = State::Location;
                    }
                }
                State::Location => {
                    if ch == 32 {
                        context.append(
                            hash_code_lowered_wide(align),
                            CxfaMeasurement::from_string(&value[last..current])
                                .to_unit(XfaUnit::Pt),
                        );
                        align = &[];
                        state = State::None;
                    }
                    current += 1;
                }
            }
        }
        if !align.is_empty() {
            context.append(
                hash_code_lowered_wide(align),
                CxfaMeasurement::from_string(&value[last..current]).to_unit(XfaUnit::Pt),
            );
        }
        true
    }
}
