// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Constructor metadata from the pinned CXFA_Node subclasses.
//!
//! This implements generic node construction and default/property lookup. Typed
//! subclass methods, layout, and CJX bindings remain separate porting work.
use super::super::fxfa_basic::{
    XfaAttribute, XfaAttributeType, XfaAttributeValue, XfaElement, XfaPacketType,
};

/// Native CXFA_Object discriminator, preserving its source ordinal values.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(i32)]
pub enum XfaObjectType {
    Object,
    List,
    Node,
    NodeC,
    NodeV,
    ModelNode,
    TextNode,
    TreeList,
    ContainerNode,
    ContentNode,
    ThisProxy,
}

pub const PROPERTY_ONE_OF: u8 = 1;
pub const PROPERTY_DEFAULT_ONE_OF: u8 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PropertyData {
    pub element: XfaElement,
    pub occurrence_count: u8,
    pub flags: u8,
}

/// A null CData default stays distinct from a missing attribute definition.
/// Native CData access converts that null default to an empty string. Numeric
/// null pointers, including an explicit `nullptr` Boolean, mean numeric zero.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttributeDefault {
    None,
    Boolean(bool),
    Integer(i32),
    Enum(XfaAttributeValue),
    Text(&'static str),
    Measure(&'static str),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttributeData {
    pub attribute: XfaAttribute,
    pub attribute_type: XfaAttributeType,
    pub default_value: AttributeDefault,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NodeSchema {
    pub element: XfaElement,
    pub object_type: XfaObjectType,
    pub valid_packets: u32,
    pub properties: &'static [PropertyData],
    pub attributes: &'static [AttributeData],
}
impl NodeSchema {
    pub fn supports_packet(&self, packet: XfaPacketType) -> bool {
        self.valid_packets & packet_mask(packet) != 0
    }
    pub fn property(&self, element: XfaElement) -> Option<&PropertyData> {
        self.properties
            .iter()
            .find(|property| property.element == element)
    }
    pub fn attribute(&self, attribute: XfaAttribute) -> Option<&AttributeData> {
        self.attributes
            .iter()
            .find(|data| data.attribute == attribute)
    }
}
/// XFA_XDPPACKET is a bit mask over the XFA_PacketType discriminants.
pub const fn packet_mask(packet: XfaPacketType) -> u32 {
    1u32 << packet as u8
}

const fn property(element: XfaElement, occurrence_count: u8, flags: u8) -> PropertyData {
    PropertyData {
        element,
        occurrence_count,
        flags,
    }
}
const fn attribute(
    attribute: XfaAttribute,
    attribute_type: XfaAttributeType,
    default_value: AttributeDefault,
) -> AttributeData {
    AttributeData {
        attribute,
        attribute_type,
        default_value,
    }
}

include!("node_schema_data.rs");

#[cfg(test)]
#[path = "node_schema_tests.rs"]
mod tests;
