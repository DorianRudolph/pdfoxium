// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Source stylesheet loading, selector filtering and declaration recovery.
use super::cfx_cssdata::{CfxCssData, Property};
use super::cfx_cssselector::CfxCssSelector;
use super::cfx_cssstylerule::CfxCssStyleRule;
use super::cfx_csssyntaxparser::{CfxCssSyntaxParser, Status};

#[derive(Debug, Default)]
pub struct CfxCssStyleSheet {
    rules: Vec<CfxCssStyleRule>,
}
impl CfxCssStyleSheet {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn count_rules(&self) -> usize {
        self.rules.len()
    }
    pub fn get_rule(&self, index: usize) -> Option<&CfxCssStyleRule> {
        self.rules.get(index)
    }
    pub fn rules(&self) -> &[CfxCssStyleRule] {
        &self.rules
    }
    pub fn load_buffer(&mut self, buffer: &[u32]) -> bool {
        self.rules.clear();
        let mut syntax = CfxCssSyntaxParser::new(buffer);
        loop {
            let status = match syntax.do_syntax_parse() {
                Status::StyleRule => self.load_style_rule(&mut syntax),
                status => status,
            };
            match status {
                Status::EOS => return true,
                Status::Error => return false,
                _ => {}
            }
        }
    }
    fn load_style_rule(&mut self, syntax: &mut CfxCssSyntaxParser<'_>) -> Status {
        let mut selectors = Vec::new();
        let mut rule_index: Option<usize> = None;
        let mut property: Option<&'static Property> = None;
        let mut name = Vec::new();
        loop {
            match syntax.do_syntax_parse() {
                Status::Selector => {
                    if let Some(selector) = CfxCssSelector::from_string(syntax.get_current_string())
                    {
                        selectors.push(selector);
                    }
                }
                Status::PropertyName => {
                    let value = syntax.get_current_string();
                    property = CfxCssData::get_property_by_name(value);
                    name.clear();
                    if property.is_none() {
                        name.extend_from_slice(value);
                    }
                }
                Status::PropertyValue => {
                    let value = syntax.get_current_string();
                    if !value.is_empty() {
                        // The source's never-assigned iValueLen loses every
                        // custom property. Use the actual unknown name.
                        if let Some(index) = rule_index {
                            let declaration = self.rules[index].get_declaration_mut();
                            if let Some(property) = property {
                                declaration.add_property(property, value);
                            } else if !name.is_empty() {
                                declaration.add_custom_property(&name, value);
                            }
                        }
                    }
                }
                Status::DeclOpen => {
                    if rule_index.is_none() && !selectors.is_empty() {
                        let mut rule = CfxCssStyleRule::new();
                        rule.set_selector(&mut selectors);
                        rule_index = Some(self.rules.len());
                        self.rules.push(rule);
                    } else {
                        Self::skip_rule_set(syntax);
                        return Status::None;
                    }
                }
                Status::DeclClose => {
                    if let Some(index) = rule_index {
                        let declaration = self.rules[index].get_declaration();
                        if declaration.empty() && declaration.custom_properties().is_empty() {
                            self.rules.pop();
                        }
                    }
                    return Status::None;
                }
                Status::EOS => return Status::EOS,
                _ => return Status::Error,
            }
        }
    }
    fn skip_rule_set(syntax: &mut CfxCssSyntaxParser<'_>) {
        loop {
            match syntax.do_syntax_parse() {
                Status::Selector
                | Status::DeclOpen
                | Status::PropertyName
                | Status::PropertyValue => {}
                _ => return,
            }
        }
    }
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::super::{
        cfx_css::{CfxCssProperty as P, CfxCssPropertyValue as V},
        cfx_cssdata::hash_code_lowered_wide,
        cfx_cssdeclaration::CfxCssDeclaration,
        cfx_cssnumbervalue::CfxCssNumberUnit as U,
        cfx_cssvalue::{CfxCssPrimitiveType as T, CfxCssValue},
    };
    use super::*;
    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    fn load_with_rule_count(text: &str, count: usize) -> CfxCssStyleSheet {
        let mut sheet = CfxCssStyleSheet::new();
        assert!(sheet.load_buffer(&wide(text)), "{text}");
        assert_eq!(sheet.count_rules(), count, "{text}");
        sheet
    }
    fn load_and_verify_decl(text: &str, selectors: &[&str], count: usize) -> CfxCssStyleSheet {
        let sheet = load_with_rule_count(text, 1);
        let style = sheet.get_rule(0).unwrap();
        assert_eq!(style.count_selector_lists(), selectors.len());
        for (index, selector) in selectors.iter().enumerate() {
            assert_eq!(
                style.get_selector_list(index).unwrap().name_hash(),
                hash_code_lowered_wide(&wide(selector))
            );
        }
        assert_eq!(style.get_declaration().property_count_for_testing(), count);
        sheet
    }
    fn verify_float(declaration: &CfxCssDeclaration, property: P, expected: f32, unit: U) {
        let (value, _) = declaration
            .get_property(property)
            .expect("source property present");
        assert_eq!(value.get_type(), T::Number);
        match value.as_ref() {
            CfxCssValue::Number(value) => {
                assert_eq!(value.unit, unit);
                assert_eq!(value.value, expected);
            }
            _ => panic!("expected number"),
        }
    }
    fn verify_enum(declaration: &CfxCssDeclaration, property: P, expected: V) {
        let (value, _) = declaration.get_property(property).unwrap();
        assert_eq!(value.get_type(), T::Enum);
        assert!(matches!(value.as_ref(), CfxCssValue::Enum(value) if *value == expected));
    }
    fn verify_list(declaration: &CfxCssDeclaration, property: P, expected: &[V]) {
        let (value, _) = declaration.get_property(property).unwrap();
        let CfxCssValue::List(values) = value.as_ref() else {
            panic!("expected list")
        };
        assert_eq!(values.len(), expected.len());
        for (value, expected) in values.iter().zip(expected) {
            assert_eq!(value.get_type(), T::Enum);
            assert!(matches!(value.as_ref(), CfxCssValue::Enum(value) if value == expected));
        }
    }
    fn has_selector(style: &CfxCssStyleRule, selector: &str) -> bool {
        let hash = hash_code_lowered_wide(&wide(selector));
        style
            .selectors
            .iter()
            .any(|selector| selector.name_hash() == hash)
    }
    fn verify_border(declaration: &CfxCssDeclaration, value: f32, unit: U) {
        for property in [
            P::BorderLeftWidth,
            P::BorderRightWidth,
            P::BorderTopWidth,
            P::BorderBottomWidth,
        ] {
            verify_float(declaration, property, value, unit);
        }
    }
    #[test]
    fn ParseEmpty() {
        load_with_rule_count("", 0);
    }
    #[test]
    fn ParseBlankEmpty() {
        load_with_rule_count("  \n\r\t", 0);
    }
    #[test]
    fn ParseStrayClose1() {
        assert!(!CfxCssStyleSheet::new().load_buffer(&wide("}")));
    }
    #[test]
    fn ParseStrayClose2() {
        load_with_rule_count("foo }", 0);
    }
    #[test]
    fn ParseStrayClose3() {
        assert!(!CfxCssStyleSheet::new().load_buffer(&wide("foo {a: b}}")));
    }
    #[test]
    fn ParseEmptySelector() {
        assert!(!CfxCssStyleSheet::new().load_buffer(&wide("{a: b}")));
    }
    #[test]
    fn ParseEmptyBody() {
        load_with_rule_count("foo {}", 0);
    }
    #[test]
    fn ParseMultipleSelectors() {
        let sheet = load_with_rule_count(
            "a { border: 10px; }\nbcdef { text-decoration: underline; }\n* { padding: 0; }\n",
            3,
        );
        let style = sheet.get_rule(0).unwrap();
        assert_eq!(style.count_selector_lists(), 1);
        assert!(has_selector(style, "a"));
        let declaration = style.get_declaration();
        assert_eq!(declaration.property_count_for_testing(), 4);
        verify_border(declaration, 10.0, U::Pixels);

        let style = sheet.get_rule(1).unwrap();
        assert_eq!(style.count_selector_lists(), 1);
        assert!(has_selector(style, "bcdef"));
        assert!(!has_selector(style, "bcde"));
        let declaration = style.get_declaration();
        assert_eq!(declaration.property_count_for_testing(), 1);
        verify_list(declaration, P::TextDecoration, &[V::Underline]);

        let style = sheet.get_rule(2).unwrap();
        assert_eq!(style.count_selector_lists(), 1);
        assert!(has_selector(style, "*"));
        let declaration = style.get_declaration();
        assert_eq!(declaration.property_count_for_testing(), 4);
        for property in [
            P::PaddingLeft,
            P::PaddingRight,
            P::PaddingTop,
            P::PaddingBottom,
        ] {
            verify_float(declaration, property, 0.0, U::Number);
        }
    }
    #[test]
    fn ParseChildSelectors() {
        let sheet = load_with_rule_count("a b c { border: 10px; }", 1);
        let style = sheet.get_rule(0).unwrap();
        assert_eq!(style.count_selector_lists(), 1);
        let mut selector = style.get_selector_list(0);
        for expected in ["c", "b", "a"] {
            let current = selector.expect("source descendant chain");
            assert_eq!(current.name_hash(), hash_code_lowered_wide(&wide(expected)));
            selector = current.next_selector();
        }
        assert!(selector.is_none());
        let declaration = style.get_declaration();
        assert_eq!(declaration.property_count_for_testing(), 4);
        verify_border(declaration, 10.0, U::Pixels);
    }
    #[test]
    fn ParseUnhandledSelectors() {
        let mut sheet = CfxCssStyleSheet::new();
        for text in [
            "a > b { padding: 0; }",
            "a[first] { padding: 0; }",
            "a+b { padding: 0; }",
            "a ^ b { padding: 0; }",
        ] {
            assert!(sheet.load_buffer(&wide(text)));
            assert_eq!(sheet.count_rules(), 0);
        }
    }
    #[test]
    fn ParseMultipleSelectorsCombined() {
        load_and_verify_decl("a, b, c { border: 5px; }", &["a", "b", "c"], 4);
    }
    #[test]
    fn ParseBorder() {
        let sheet = load_and_verify_decl("a { border: 5px; }", &["a"], 4);
        verify_border(sheet.get_rule(0).unwrap().get_declaration(), 5.0, U::Pixels);
    }
    #[test]
    fn ParseBorderFull() {
        let sheet = load_and_verify_decl("a { border: 5px solid red; }", &["a"], 4);
        verify_border(sheet.get_rule(0).unwrap().get_declaration(), 5.0, U::Pixels);
    }
    #[test]
    fn ParseBorderLeft() {
        let sheet = load_and_verify_decl("a { border-left: 2.5pc; }", &["a"], 1);
        verify_float(
            sheet.get_rule(0).unwrap().get_declaration(),
            P::BorderLeftWidth,
            2.5,
            U::Picas,
        );
    }
    #[test]
    fn ParseBorderLeftThick() {
        let sheet = load_and_verify_decl("a { border-left: thick; }", &["a"], 1);
        verify_enum(
            sheet.get_rule(0).unwrap().get_declaration(),
            P::BorderLeftWidth,
            V::Thick,
        );
    }
    #[test]
    fn ParseBorderRight() {
        let sheet = load_and_verify_decl("a { border-right: 2.5pc; }", &["a"], 1);
        verify_float(
            sheet.get_rule(0).unwrap().get_declaration(),
            P::BorderRightWidth,
            2.5,
            U::Picas,
        );
    }
    #[test]
    fn ParseBorderTop() {
        let sheet = load_and_verify_decl("a { border-top: 2.5pc; }", &["a"], 1);
        verify_float(
            sheet.get_rule(0).unwrap().get_declaration(),
            P::BorderTopWidth,
            2.5,
            U::Picas,
        );
    }
    #[test]
    fn ParseBorderBottom() {
        let sheet = load_and_verify_decl("a { border-bottom: 2.5pc; }", &["a"], 1);
        verify_float(
            sheet.get_rule(0).unwrap().get_declaration(),
            P::BorderBottomWidth,
            2.5,
            U::Picas,
        );
    }
    #[test]
    fn ParseWithCommentsInSelector() {
        let sheet = load_and_verify_decl("/**{*/a/**}*/ { border-bottom: 2.5pc; }", &["a"], 1);
        verify_float(
            sheet.get_rule(0).unwrap().get_declaration(),
            P::BorderBottomWidth,
            2.5,
            U::Picas,
        );
    }
    #[test]
    fn ParseWithCommentsInProperty() {
        let sheet = load_and_verify_decl("a { /*}*/border-bottom: 2.5pc; }", &["a"], 1);
        verify_float(
            sheet.get_rule(0).unwrap().get_declaration(),
            P::BorderBottomWidth,
            2.5,
            U::Picas,
        );
    }
    #[test]
    fn ParseWithCommentsInValue() {
        let sheet =
            load_and_verify_decl("a { border-bottom: /*;*/2.5pc;/* color:red;*/ }", &["a"], 1);
        verify_float(
            sheet.get_rule(0).unwrap().get_declaration(),
            P::BorderBottomWidth,
            2.5,
            U::Picas,
        );
    }
    #[test]
    fn ParseWithUnterminatedCommentInSelector() {
        load_with_rule_count("a/* { border-bottom: 2.5pc; }", 0);
    }
    #[test]
    fn ParseWithUnterminatedCommentInProperty() {
        load_with_rule_count("a { /*border-bottom: 2.5pc; }", 1);
    }
    #[test]
    fn ParseWithUnterminatedCommentInValue() {
        load_with_rule_count("a { border-bottom: /*2.5pc; }", 1);
    }
    #[test]
    fn custom_properties_survive_and_reloads_replace_prior_rules() {
        let mut sheet = load_with_rule_count(
            "a { xfa-tab-stops: left 12pt; } b { color:red; xfa-spacerun:yes; }",
            2,
        );
        let first = sheet.get_rule(0).unwrap().get_declaration();
        assert!(first.empty());
        assert_eq!(first.custom_properties().len(), 1);
        assert_eq!(first.custom_properties()[0].name, wide("xfa-tab-stops"));
        assert_eq!(first.custom_properties()[0].value, wide("left 12pt"));
        let second = sheet.get_rule(1).unwrap().get_declaration();
        assert_eq!(second.property_count_for_testing(), 1);
        assert_eq!(second.custom_properties()[0].name, wide("xfa-spacerun"));
        assert_eq!(second.custom_properties()[0].value, wide("yes"));
        let retained = sheet.get_rule(1).unwrap().declaration.clone();
        assert!(!sheet.load_buffer(&wide("}")));
        assert_eq!(sheet.count_rules(), 0);
        assert_eq!(retained.property_count_for_testing(), 1);
        assert!(sheet.load_buffer(&wide("b {margin:1pt 2pt;}")));
        assert_eq!(sheet.count_rules(), 1);
        assert_eq!(
            sheet
                .get_rule(0)
                .unwrap()
                .get_declaration()
                .property_count_for_testing(),
            4
        );
        assert!(sheet.load_buffer(&[]));
        assert!(sheet.rules().is_empty());
        assert!(sheet.get_rule(0).is_none());
    }
}
