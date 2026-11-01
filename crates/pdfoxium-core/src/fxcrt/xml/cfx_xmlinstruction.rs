// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fxcrt/xml/cfx_xmlinstruction.{h,cpp}`.
use super::cfx_xmldocument::CfxXmlDocument;
use super::cfx_xmlnode::{XmlNodeData, XmlNodeRef};
use super::xml_string::XmlString;
use crate::fxcrt::fx_stream::WriteStream;
use crate::fxcrt::fx_string::utf8_encode;

pub(crate) struct CfxXmlInstruction {
    pub name: XmlString,
    pub target_data: Vec<XmlString>,
}
/// Live nonowning view of the instruction's vector, avoiding either an
/// escaping RefCell borrow or a snapshot that would hide later AppendData.
#[derive(Clone)]
pub struct XmlTargetData(XmlNodeRef);
impl XmlTargetData {
    pub fn len(&self) -> usize {
        self.0.with(|data| match data {
            XmlNodeData::Instruction(inst) => inst.target_data.len(),
            _ => panic!("XML node is not an instruction"),
        })
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn get(&self, index: usize) -> Option<XmlString> {
        self.0.with(|data| match data {
            XmlNodeData::Instruction(inst) => inst.target_data.get(index).cloned(),
            _ => panic!("XML node is not an instruction"),
        })
    }
    pub fn iter(&self) -> impl Iterator<Item = XmlString> + '_ {
        // The native range-for captures its end before the first write.
        (0..self.len()).map(|index| self.get(index).expect("instruction data index"))
    }
}

impl XmlNodeRef {
    pub fn is_original_xfa_version(&self) -> bool {
        self.name()
            .iter()
            .copied()
            .eq("originalXFAVersion".chars().map(u32::from))
    }
    pub fn is_acrobat(&self) -> bool {
        self.name()
            .iter()
            .copied()
            .eq("acrobat".chars().map(u32::from))
    }
    pub fn target_data(&self) -> XmlTargetData {
        XmlTargetData(self.clone())
    }
    pub fn append_data(&self, data: &[u32]) {
        self.with_mut(|node| match node {
            XmlNodeData::Instruction(instruction) => {
                instruction.target_data.push(XmlString::from(data))
            }
            _ => panic!("XML node is not an instruction"),
        });
    }
    pub fn append_data_owned(&self, data: Vec<u32>) {
        self.with_mut(|node| match node {
            XmlNodeData::Instruction(instruction) => {
                instruction.target_data.push(XmlString::from(data))
            }
            _ => panic!("XML node is not an instruction"),
        });
    }
    pub(crate) fn clone_instruction(&self, doc: &mut CfxXmlDocument) -> Self {
        let name = self.name();
        let target_data = self.target_data().iter().collect();
        doc.create_node(XmlNodeData::Instruction(CfxXmlInstruction {
            name,
            target_data,
        }))
    }
    pub(crate) fn save_instruction(&self, stream: &mut dyn WriteStream) {
        let name = self.name();
        if name.len() == 3
            && name
                .iter()
                .zip(b"xml")
                .all(|(&ch, &ascii)| ch <= 127 && (ch as u8).eq_ignore_ascii_case(&ascii))
        {
            stream.write_string(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>");
            return;
        }
        stream.write_string(b"<?");
        stream.write_string(&utf8_encode(&name));
        stream.write_string(b" ");
        let data = self.target_data();
        for target in data.iter() {
            stream.write_string(&utf8_encode(&target));
            stream.write_string(b" ");
        }
        stream.write_string(b"?>");
    }
}
#[cfg(test)]
#[path = "cfx_xmlinstruction_unittest.rs"]
mod upstream_tests;
