// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Model-facing text sources from `cxfa_textprovider.{h,cpp}`.
use super::{
    fxfa_basic::{XfaAttribute, XfaElement, XfaHashCode},
    parser::{
        cxfa_localevalue::XfaValuePicture,
        cxfa_node::{CxfaNode, XfaFfWidgetType},
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TextProviderType {
    Text,
    Caption,
    Rollover,
    Down,
}

#[derive(Clone, Debug)]
pub struct CxfaTextProvider {
    node: CxfaNode,
    kind: TextProviderType,
}

impl CxfaTextProvider {
    pub fn new(node: CxfaNode, kind: TextProviderType) -> Self {
        Self { node, kind }
    }
    pub fn node(&self) -> &CxfaNode {
        &self.node
    }
    pub fn kind(&self) -> TextProviderType {
        self.kind
    }
    /// The boolean records the source's `bRichText` out-parameter.
    pub fn get_text_node(&self) -> (Option<CxfaNode>, bool) {
        if matches!(
            self.kind,
            TextProviderType::Text | TextProviderType::Caption
        ) {
            let owner = if self.kind == TextProviderType::Text {
                Some(self.node.clone())
            } else {
                self.node.get_child(0, XfaElement::Caption, false)
            };
            let child = owner
                .and_then(|n| n.get_child(0, XfaElement::Value, false))
                .and_then(|value| value.first_child());
            let rich = child.as_ref().is_some_and(|child| {
                child.get_element_type() == XfaElement::ExData
                    && child
                        .try_attribute(XfaAttribute::ContentType, false)
                        .is_some_and(|v| ascii_eq(&v, "text/html"))
            });
            return (child, rich);
        }
        let name = if self.kind == TextProviderType::Rollover {
            "rollover"
        } else {
            "down"
        };
        let mut child = self
            .node
            .get_child(0, XfaElement::Items, false)
            .and_then(|n| n.first_child());
        while let Some(node) = child {
            if ascii_eq(&node.get_cdata(XfaAttribute::Name), name) {
                return (Some(node), false);
            }
            child = node.next_sibling();
        }
        (None, false)
    }
    pub fn get_para_if_exists(&self) -> Option<CxfaNode> {
        if self.kind == TextProviderType::Text {
            self.node.get_child(0, XfaElement::Para, false)
        } else {
            self.node
                .get_child(0, XfaElement::Caption, false)?
                .get_child(0, XfaElement::Para, false)
        }
    }
    pub fn get_font_if_exists(&self) -> Option<CxfaNode> {
        if self.kind != TextProviderType::Text {
            if let Some(font) = self
                .node
                .get_child(0, XfaElement::Caption, false)
                .and_then(|caption| caption.get_child(0, XfaElement::Font, false))
            {
                return Some(font);
            }
        }
        // Missing caption is handled safely rather than dereferencing null.
        self.node.get_child(0, XfaElement::Font, false)
    }
    pub fn is_check_button_and_auto_width(&self) -> bool {
        self.node.get_ff_widget_type() == XfaFfWidgetType::CheckButton
            && self.node.try_measure(XfaAttribute::W, false).is_none()
    }
    pub fn get_embedded_obj(&self, id: &[u32]) -> Option<Vec<u32>> {
        if self.kind != TextProviderType::Text {
            return None;
        }
        let document = self.node.document()?;
        let target = self
            .node
            .parent()
            .and_then(|parent| document.get_node_by_id(Some(&parent), id))
            .or_else(|| {
                document
                    .get_xfa_node(XfaHashCode::Form as u32)
                    .and_then(|form| document.get_node_by_id(Some(&form), id))
            })?;
        target
            .is_widget_ready()
            .then(|| target.get_value(XfaValuePicture::Display))
    }
}

fn ascii_eq(value: &[u32], text: &str) -> bool {
    value.iter().copied().eq(text.bytes().map(u32::from))
}
