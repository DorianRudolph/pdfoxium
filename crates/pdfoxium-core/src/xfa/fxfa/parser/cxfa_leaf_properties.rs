// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Direct own methods from `cxfa_barcode`, `cxfa_binditems` and `cxfa_submit`.
//! Their constructors use node_schema and the executable CJX_Node binding.
use super::cxfa_node::{CxfaNode, XfaFfWidgetType};
use crate::{
    fxcrt::fx_system::wtoi,
    xfa::fxfa::fxfa_basic::{XfaAttribute, XfaAttributeValue, XfaElement, XfaUnit},
};

impl CxfaNode {
    pub fn barcode_from_node(node: Option<&Self>) -> Option<Self> {
        node.filter(|node| node.get_element_type() == XfaElement::Barcode)
            .cloned()
    }
    pub fn barcode_get_default_ff_widget_type(&self) -> XfaFfWidgetType {
        XfaFfWidgetType::Barcode
    }
    pub fn barcode_get_barcode_type(&self) -> Vec<u32> {
        self.get_cdata(XfaAttribute::Type)
    }
    pub fn barcode_get_checksum(&self) -> Option<bool> {
        match self.try_enum(XfaAttribute::Checksum, true)? {
            XfaAttributeValue::None => Some(false),
            XfaAttributeValue::Auto => Some(true),
            _ => None,
        }
    }
    pub fn barcode_get_data_length(&self) -> Option<i32> {
        self.try_cdata(XfaAttribute::DataLength, true)
            .map(|value| wtoi(&value))
    }
    pub fn barcode_get_start_char(&self) -> Option<u8> {
        self.try_cdata(XfaAttribute::StartChar, true)?
            .first()
            .map(|&value| value as u8)
    }
    pub fn barcode_get_end_char(&self) -> Option<u8> {
        self.try_cdata(XfaAttribute::EndChar, true)?
            .first()
            .map(|&value| value as u8)
    }
    pub fn barcode_get_ec_level(&self) -> Option<i32> {
        self.try_cdata(XfaAttribute::ErrorCorrectionLevel, true)
            .map(|value| wtoi(&value))
    }
    pub fn barcode_get_module_width(&self) -> Option<i32> {
        self.try_measure(XfaAttribute::ModuleWidth, true)
            .map(|value| value.to_unit(XfaUnit::Pt) as i32)
    }
    pub fn barcode_get_module_height(&self) -> Option<i32> {
        self.try_measure(XfaAttribute::ModuleHeight, true)
            .map(|value| value.to_unit(XfaUnit::Pt) as i32)
    }
    pub fn barcode_get_print_checksum(&self) -> Option<bool> {
        self.try_boolean(XfaAttribute::PrintCheckDigit, true)
    }
    pub fn barcode_get_text_location(&self) -> Option<XfaAttributeValue> {
        self.try_enum(XfaAttribute::TextLocation, true)
    }
    pub fn barcode_get_truncate(&self) -> Option<bool> {
        self.try_boolean(XfaAttribute::Truncate, true)
    }
    pub fn barcode_get_wide_narrow_ratio(&self) -> Option<i8> {
        let value = self.try_cdata(XfaAttribute::WideNarrowRatio, true)?;
        let Some(colon) = value.iter().position(|&ch| ch == u32::from(b':')) else {
            return Some(wtoi(&value) as i8);
        };
        let denominator = wtoi(&value[colon + 1..]);
        if denominator == 0 {
            return Some(0);
        }
        let numerator = wtoi(&value[..colon]);
        // Preserve the source's f32 division and truncation. Rust saturates
        // out-of-range float casts instead of invoking C++ undefined behavior.
        Some((numerator as f32 / denominator as f32) as i8)
    }

    pub fn bind_items_get_label_ref(&self) -> Vec<u32> {
        self.get_cdata(XfaAttribute::LabelRef)
    }
    pub fn bind_items_get_value_ref(&self) -> Vec<u32> {
        self.get_cdata(XfaAttribute::ValueRef)
    }
    pub fn bind_items_get_ref(&self) -> Vec<u32> {
        self.get_cdata(XfaAttribute::Ref)
    }
    pub fn submit_is_submit_embed_pdf(&self) -> bool {
        self.get_boolean(XfaAttribute::EmbedPDF)
    }
    pub fn submit_get_submit_format(&self) -> XfaAttributeValue {
        self.get_enum(XfaAttribute::Format)
    }
    pub fn submit_get_submit_target(&self) -> Vec<u32> {
        self.get_cdata(XfaAttribute::Target)
    }
    pub fn submit_get_submit_xdp_content(&self) -> Vec<u32> {
        self.get_cdata(XfaAttribute::XdpContent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xfa::fxfa::{
        fxfa_basic::XfaPacketType,
        parser::{cxfa_document::CxfaDocument, cxfa_measurement::CxfaMeasurement},
    };

    fn wide(value: &str) -> Vec<u32> {
        value.chars().map(u32::from).collect()
    }

    #[test]
    fn barcode_defaults_template_values_and_checked_node_identity() {
        let document = CxfaDocument::new();
        let barcode = document
            .create_node(XfaPacketType::Form, XfaElement::Barcode)
            .unwrap();
        assert_eq!(CxfaNode::barcode_from_node(None), None);
        assert_eq!(
            CxfaNode::barcode_from_node(Some(&barcode)),
            Some(barcode.clone())
        );
        let other = document
            .create_node(XfaPacketType::Form, XfaElement::Text)
            .unwrap();
        assert_eq!(CxfaNode::barcode_from_node(Some(&other)), None);
        assert_eq!(
            barcode.barcode_get_default_ff_widget_type(),
            XfaFfWidgetType::Barcode
        );
        assert!(barcode.barcode_get_barcode_type().is_empty());
        assert_eq!(barcode.barcode_get_checksum(), Some(false));
        assert_eq!(barcode.barcode_get_data_length(), Some(0));
        assert_eq!(barcode.barcode_get_ec_level(), Some(0));
        assert_eq!(barcode.barcode_get_start_char(), None);
        assert_eq!(barcode.barcode_get_end_char(), None);
        assert_eq!(barcode.barcode_get_module_width(), Some(0));
        assert_eq!(barcode.barcode_get_module_height(), Some(14));
        assert_eq!(barcode.barcode_get_print_checksum(), Some(false));
        assert_eq!(
            barcode.barcode_get_text_location(),
            Some(XfaAttributeValue::Below)
        );
        assert_eq!(barcode.barcode_get_truncate(), Some(false));
        assert_eq!(barcode.barcode_get_wide_narrow_ratio(), Some(3));

        let template = document
            .create_node(XfaPacketType::Template, XfaElement::Barcode)
            .unwrap();
        template.set_cdata(XfaAttribute::Type, &wide("code39"));
        template.set_cdata(XfaAttribute::DataLength, &wide("+12suffix"));
        template.set_cdata(XfaAttribute::StartChar, &wide("AB"));
        template.set_cdata(XfaAttribute::EndChar, &[0x142]);
        template.set_cdata(XfaAttribute::ErrorCorrectionLevel, &wide("-4"));
        template.set_measure(
            XfaAttribute::ModuleWidth,
            CxfaMeasurement::new(0.1, XfaUnit::In),
            false,
        );
        template.set_measure(
            XfaAttribute::ModuleHeight,
            CxfaMeasurement::new(-2.9, XfaUnit::Pt),
            false,
        );
        template.set_enum(XfaAttribute::Checksum, XfaAttributeValue::Auto, false);
        template.set_enum(XfaAttribute::TextLocation, XfaAttributeValue::Above, false);
        template.set_boolean(XfaAttribute::PrintCheckDigit, true, false);
        template.set_boolean(XfaAttribute::Truncate, true, false);
        barcode.set_template_node(Some(&template));
        assert_eq!(barcode.barcode_get_barcode_type(), wide("code39"));
        assert_eq!(barcode.barcode_get_data_length(), Some(12));
        assert_eq!(barcode.barcode_get_ec_level(), Some(-4));
        assert_eq!(barcode.barcode_get_start_char(), Some(b'A'));
        assert_eq!(barcode.barcode_get_end_char(), Some(b'B'));
        assert_eq!(barcode.barcode_get_module_width(), Some(7));
        assert_eq!(barcode.barcode_get_module_height(), Some(-2));
        assert_eq!(barcode.barcode_get_checksum(), Some(true));
        assert_eq!(
            barcode.barcode_get_text_location(),
            Some(XfaAttributeValue::Above)
        );
        assert_eq!(barcode.barcode_get_print_checksum(), Some(true));
        assert_eq!(barcode.barcode_get_truncate(), Some(true));
        barcode.set_cdata(XfaAttribute::Type, &wide("ean13"));
        assert_eq!(barcode.barcode_get_barcode_type(), wide("ean13"));
        for value in [
            XfaAttributeValue::Checksum_1mod10,
            XfaAttributeValue::Checksum_1mod10_1mod11,
            XfaAttributeValue::Checksum_2mod10,
            XfaAttributeValue::Unknown,
        ] {
            barcode.set_enum(XfaAttribute::Checksum, value, false);
            assert_eq!(barcode.barcode_get_checksum(), None);
        }
    }

    #[test]
    fn barcode_ratio_and_numeric_prefix_rules() {
        let document = CxfaDocument::new();
        let barcode = document
            .create_node(XfaPacketType::Form, XfaElement::Barcode)
            .unwrap();
        for (input, expected) in [
            ("", 0),
            ("4", 4),
            ("130", -126),
            ("5:2", 2),
            ("-5:2", -2),
            ("6:-2", -3),
            ("2:0", 0),
            ("3:", 0),
            (":2", 0),
            ("6:2:1", 3),
            (" 6:2", 0),
            ("999999:1", i8::MAX),
            ("-999999:1", i8::MIN),
        ] {
            barcode.set_cdata(XfaAttribute::WideNarrowRatio, &wide(input));
            assert_eq!(
                barcode.barcode_get_wide_narrow_ratio(),
                Some(expected),
                "{input}"
            );
        }
        for (input, expected) in [
            ("", 0),
            ("bad", 0),
            ("42trailing", 42),
            (" 42", 0),
            ("999999999999999", i32::MAX),
            ("-999999999999999", i32::MIN),
        ] {
            barcode.set_cdata(XfaAttribute::DataLength, &wide(input));
            assert_eq!(barcode.barcode_get_data_length(), Some(expected), "{input}");
        }
    }

    #[test]
    fn parsed_bind_items_and_submit_use_real_attribute_storage() {
        use super::super::cxfa_document_builder::CxfaDocumentBuilder;
        use crate::fxcrt::{
            cfx_read_only_span_stream::CfxReadOnlySpanStream, xml::cfx_xmlparser::CfxXmlParser,
        };
        use std::{cell::RefCell, rc::Rc};
        let input = b"<template xmlns=\"http://www.xfa.org/schema/xfa-template/2.8/\"><subform><field><bindItems ref=\"$record.item[*]\" labelRef=\"label\" valueRef=\"id\"/></field><event><submit format=\"xml\" embedPDF=\"1\" target=\"https://example.test/submit\" xdpContent=\"datasets form\"/></event></subform></template>\0";
        let stream = Rc::new(CfxReadOnlySpanStream::new(input));
        let xml = Rc::new(RefCell::new(CfxXmlParser::new(stream).parse().unwrap()));
        let document = CxfaDocument::new();
        let mut builder = CxfaDocumentBuilder::new(document.clone());
        assert!(builder.build_document(xml, XfaPacketType::Template));
        let subform = builder.get_root_node().unwrap().first_child().unwrap();
        let bind = subform
            .first_child()
            .unwrap()
            .get_child(0, XfaElement::BindItems, false)
            .unwrap();
        assert_eq!(bind.bind_items_get_ref(), wide("$record.item[*]"));
        assert_eq!(bind.bind_items_get_label_ref(), wide("label"));
        assert_eq!(bind.bind_items_get_value_ref(), wide("id"));
        let submit = subform
            .get_child(0, XfaElement::Event, false)
            .unwrap()
            .first_child()
            .unwrap();
        assert!(submit.submit_is_submit_embed_pdf());
        assert_eq!(submit.submit_get_submit_format(), XfaAttributeValue::Xml);
        assert_eq!(
            submit.submit_get_submit_target(),
            wide("https://example.test/submit")
        );
        assert_eq!(
            submit.submit_get_submit_xdp_content(),
            wide("datasets form")
        );
        let defaults = document
            .create_node(XfaPacketType::Form, XfaElement::Submit)
            .unwrap();
        assert!(!defaults.submit_is_submit_embed_pdf());
        assert_eq!(defaults.submit_get_submit_format(), XfaAttributeValue::Xdp);
        assert!(defaults.submit_get_submit_target().is_empty());
        assert!(defaults.submit_get_submit_xdp_content().is_empty());
        let bind_defaults = document
            .create_node(XfaPacketType::Form, XfaElement::BindItems)
            .unwrap();
        assert!(bind_defaults.bind_items_get_ref().is_empty());
        assert!(bind_defaults.bind_items_get_label_ref().is_empty());
        assert!(bind_defaults.bind_items_get_value_ref().is_empty());
    }
}
