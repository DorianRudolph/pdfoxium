// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Packet, element and attribute lookup from `xfa_basic_data.{h,cpp}`.
//!
//! Script callback lookup is represented separately as symbolic metadata. The
//! native callback table dispatches selected CJX implementations through Boa;
//! specialized callbacks without a port remain explicit runtime errors.
#![allow(non_camel_case_types, non_upper_case_globals)]
use crate::xfa::fxfa::fxfa_basic::{
    XfaAttribute, XfaAttributeValue, XfaElement, XfaPacketType, XfaScriptType,
};

#[path = "xfa_basic_data_tables.rs"]
mod tables;
use tables::{ATTRIBUTES, ATTRIBUTE_VALUES, ELEMENTS, ELEMENT_ATTRIBUTES, PACKETS};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum XfaPacketMatch {
    kCompleteMatch = 1,
    kPrefixMatch,
    kNoMatch,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum XfaPacketSupport {
    kSupportOne = 1,
    kSupportMany,
}
impl XfaPacketMatch {
    pub const CompleteMatch: Self = Self::kCompleteMatch;
    pub const PrefixMatch: Self = Self::kPrefixMatch;
    pub const NoMatch: Self = Self::kNoMatch;
}
impl XfaPacketSupport {
    pub const SupportOne: Self = Self::kSupportOne;
    pub const SupportMany: Self = Self::kSupportMany;
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PacketInfo {
    pub packet_type: XfaPacketType,
    pub match_type: XfaPacketMatch,
    pub support: XfaPacketSupport,
    pub name: &'static str,
    pub uri: Option<&'static str>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttributeInfo {
    pub attribute: XfaAttribute,
    pub script_type: XfaScriptType,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScriptAttributeDescriptor {
    pub attribute: XfaAttribute,
    pub script_type: XfaScriptType,
    /// Source CJX method name; selected callbacks dispatch in `fxjs::xfa::cjx_object`.
    pub callback_name: &'static str,
}
#[derive(Clone, Copy, Debug)]
struct ElementRecord {
    hash: u32,
    name: &'static str,
    element: XfaElement,
    parent: XfaElement,
}
#[derive(Clone, Copy, Debug)]
struct AttributeRecord {
    hash: u32,
    name: &'static str,
    attribute: XfaAttribute,
    script_type: XfaScriptType,
}
#[derive(Clone, Copy, Debug)]
struct AttributeValueRecord {
    hash: u32,
    name: &'static str,
    value: XfaAttributeValue,
}
#[derive(Clone, Copy, Debug)]
struct ElementAttributeRecord {
    element: XfaElement,
    attribute: XfaAttribute,
    callback_name: &'static str,
}

/// FX_HashCode_GetW: unsigned wrapping polynomial over wide code units.
pub fn hash_name(name: &[u32]) -> u32 {
    crate::fxcrt::widestring::hash_code(name)
}
fn equals_ascii(wide: &[u32], ascii: &str) -> bool {
    wide.len() == ascii.len()
        && wide
            .iter()
            .zip(ascii.bytes())
            .all(|(&w, a)| w == u32::from(a))
}

pub fn get_packet_by_index(packet: XfaPacketType) -> PacketInfo {
    PACKETS[packet as usize].1
}
pub fn get_packet_by_name(name: &[u32]) -> Option<PacketInfo> {
    let hash = hash_name(name);
    let index = PACKETS.partition_point(|record| record.0 < hash);
    let &(record_hash, info) = PACKETS.get(index)?;
    (record_hash == hash && equals_ascii(name, info.name)).then_some(info)
}
/// Unknown elements have no name. This safely handles the source's invalid
/// negative-index case without manufacturing an element-table entry.
pub fn element_to_name(element: XfaElement) -> &'static str {
    usize::try_from(element as i16)
        .ok()
        .and_then(|index| ELEMENTS.get(index))
        .map_or("", |record| record.name)
}
pub fn get_element_by_name(name: &[u32]) -> XfaElement {
    let hash = hash_name(name);
    let index = ELEMENTS.partition_point(|record| record.hash < hash);
    ELEMENTS
        .get(index)
        .filter(|record| record.hash == hash && equals_ascii(name, record.name))
        .map_or(XfaElement::Unknown, |record| record.element)
}
pub fn element_parent(element: XfaElement) -> XfaElement {
    usize::try_from(element as i16)
        .ok()
        .and_then(|index| ELEMENTS.get(index))
        .map_or(XfaElement::Unknown, |record| record.parent)
}
pub fn attribute_to_name(attribute: XfaAttribute) -> &'static str {
    usize::try_from(attribute as i16)
        .ok()
        .and_then(|index| ATTRIBUTES.get(index))
        .map_or("", |record| record.name)
}
pub fn get_attribute_by_name(name: &[u32]) -> Option<AttributeInfo> {
    let hash = hash_name(name);
    let index = ATTRIBUTES.partition_point(|record| record.hash < hash);
    let record = ATTRIBUTES.get(index)?;
    (record.hash == hash && equals_ascii(name, record.name)).then_some(AttributeInfo {
        attribute: record.attribute,
        script_type: record.script_type,
    })
}
pub fn attribute_value_to_name(value: XfaAttributeValue) -> &'static str {
    ATTRIBUTE_VALUES[value as usize].name
}
pub fn get_attribute_value_by_name(name: &[u32]) -> Option<XfaAttributeValue> {
    let hash = hash_name(name);
    let index = ATTRIBUTE_VALUES.partition_point(|record| record.hash < hash);
    let record = ATTRIBUTE_VALUES.get(index)?;
    (record.hash == hash && equals_ascii(name, record.name)).then_some(record.value)
}
/// Source lookup and parent traversal for callback metadata. The runtime
/// dispatches implemented callbacks and explicitly rejects unported services.
pub fn find_script_attribute_descriptor(
    mut element: XfaElement,
    name: &[u32],
) -> Option<ScriptAttributeDescriptor> {
    let info = get_attribute_by_name(name)?;
    for _ in 0..ELEMENTS.len() {
        if element == XfaElement::Unknown {
            return None;
        }
        let key = (element, info.attribute);
        let index =
            ELEMENT_ATTRIBUTES.partition_point(|record| (record.element, record.attribute) < key);
        if let Some(record) = ELEMENT_ATTRIBUTES
            .get(index)
            .filter(|record| (record.element, record.attribute) == key)
        {
            return Some(ScriptAttributeDescriptor {
                attribute: info.attribute,
                script_type: info.script_type,
                callback_name: record.callback_name,
            });
        }
        element = element_parent(element);
    }
    None
}

#[cfg(test)]
#[path = "xfa_basic_data_unittest.rs"]
mod tests;
