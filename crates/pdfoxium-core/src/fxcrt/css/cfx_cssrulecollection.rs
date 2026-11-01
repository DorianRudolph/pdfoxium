// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Tag index from `cfx_cssrulecollection.{h,cpp}`. Rule data holds immutable
//! shared owners instead of source pointers into a replaceable stylesheet.
use super::cfx_cssdata::hash_code_lowered_wide;
use super::cfx_cssdeclaration::CfxCssDeclaration;
use super::cfx_cssselector::CfxCssSelector;
use super::cfx_cssstylerule::CfxCssStyleRule;
use super::cfx_cssstylesheet::CfxCssStyleSheet;
use std::collections::BTreeMap;
use std::rc::Rc;

#[derive(Clone, Debug)]
pub struct Data {
    pub selector: Rc<CfxCssSelector>,
    pub declaration: Rc<CfxCssDeclaration>,
}
#[derive(Default, Debug)]
pub struct CfxCssRuleCollection {
    tag_rules: BTreeMap<u32, Vec<Data>>,
}
impl CfxCssRuleCollection {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn get_tag_rule_data(&self, tag_name: &[u32]) -> Option<&[Data]> {
        self.tag_rules
            .get(&hash_code_lowered_wide(tag_name))
            .map(Vec::as_slice)
    }
    pub fn set_rules_from_sheet(&mut self, sheet: Option<&CfxCssStyleSheet>) {
        self.tag_rules.clear();
        if let Some(sheet) = sheet {
            for rule in sheet.rules() {
                self.add_rule(rule);
            }
        }
    }
    fn add_rule(&mut self, rule: &CfxCssStyleRule) {
        for selector in &rule.selectors {
            self.tag_rules
                .entry(selector.name_hash())
                .or_default()
                .push(Data {
                    selector: Rc::clone(selector),
                    declaration: Rc::clone(&rule.declaration),
                });
        }
    }
}
