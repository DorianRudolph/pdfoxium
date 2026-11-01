// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! CXFA_Node::GetLocale/GetLocaleName, connected to the existing locale manager.
use super::cxfa_node::CxfaNode;
use crate::xfa::{
    fgas::crt::{locale_iface::LocaleIface, locale_mgr_iface::LocaleMgrIface},
    fxfa::fxfa_basic::{XfaAttribute, XfaElement, XfaHashCode},
};
use std::rc::Rc;

impl CxfaNode {
    pub fn get_locale(&self) -> Option<Rc<dyn LocaleIface>> {
        let name = self.get_locale_name()?;
        let manager = self.document()?.get_locale_mgr()?;
        if name
            .iter()
            .copied()
            .eq(b"ambient".iter().copied().map(u32::from))
        {
            manager.get_def_locale()
        } else {
            manager.get_locale_by_name(&name)
        }
    }

    pub fn get_locale_name(&self) -> Option<Vec<u32>> {
        let document = self.document()?;
        let form = document.get_xfa_node(XfaHashCode::Form as u32)?;
        let top_subform = form.get_first_child_by_class(XfaElement::Subform)?;
        let mut current = self.clone();
        loop {
            if let Some(name) = current.try_cdata(XfaAttribute::Locale, false) {
                return Some(name);
            }
            let Some(parent) = current.parent() else {
                break;
            };
            if parent == top_subform {
                break;
            }
            current = parent;
        }
        // The host explicitly configures the manager. An absent host service is
        // not replaced with another locale or timezone implementation.
        let manager = document.get_locale_mgr()?;
        let config = document.get_xfa_node(XfaHashCode::Config as u32);
        if let Some(name) = manager.get_config_locale_name(config.as_ref()) {
            return Some(name);
        }
        if let Some(name) = top_subform.try_cdata(XfaAttribute::Locale, false) {
            return Some(name);
        }
        manager.get_def_locale().map(|locale| locale.get_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        fxcrt::{
            cfx_read_only_span_stream::CfxReadOnlySpanStream, xml::cfx_xmlparser::CfxXmlParser,
        },
        xfa::fxfa::{
            fxfa_basic::XfaPacketType,
            parser::{cxfa_document::CxfaDocument, cxfa_document_builder::CxfaDocumentBuilder},
        },
    };
    use std::cell::{Cell, RefCell};
    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    fn parsed(
        config: &str,
        top_locale: &str,
        nested_locale: &str,
    ) -> (Rc<CxfaDocument>, CxfaNode, CxfaNode, CxfaNode) {
        let input=format!("<xdp:xdp xmlns:xdp='http://ns.adobe.com/xdp/'>{config}<localeSet xmlns='http://www.xfa.org/schema/xfa-locale-set/2.7/'><locale name='custom'><numberSymbols><numberSymbol name='decimal'>custom-decimal</numberSymbol></numberSymbols></locale></localeSet><template xmlns='http://www.xfa.org/schema/xfa-template/2.8/'><subform restoreState='auto'/></template><form xmlns='http://www.xfa.org/schema/xfa-form/2.8/'><subform name='top' {top_locale}><subform name='nested' {nested_locale}><field name='field'/></subform></subform></form></xdp:xdp>");
        let mut bytes = input.into_bytes();
        bytes.push(0);
        let stream = Rc::new(CfxReadOnlySpanStream::new(&bytes));
        let xml = CfxXmlParser::new(stream).parse().unwrap();
        let document = CxfaDocument::new();
        let mut builder = CxfaDocumentBuilder::new(document.clone());
        assert!(builder.build_document(Rc::new(RefCell::new(xml)), XfaPacketType::Xdp));
        assert!(document.set_root(builder.get_root_node()));
        let form = document.get_xfa_node(XfaHashCode::Form as u32).unwrap();
        let top = form.get_first_child_by_class(XfaElement::Subform).unwrap();
        let nested = top.get_first_child_by_class(XfaElement::Subform).unwrap();
        let field = nested.get_first_child_by_class(XfaElement::Field).unwrap();
        (document, top, nested, field)
    }
    const CONFIG: &str =
        "<config><acrobat><common><locale>de_DE</locale></common></acrobat></config>";

    #[test]
    fn parsed_hierarchy_config_top_subform_and_default_order() {
        for (config, top_locale, nested_locale, expected) in [
            (CONFIG, "locale='fr_FR'", "locale='en_GB'", "en_GB"),
            (CONFIG, "locale='fr_FR'", "", "de_DE"),
            ("", "locale='fr_FR'", "", "fr_FR"),
            ("", "", "", "en_US"),
        ] {
            let (document, top, _, field) = parsed(config, top_locale, nested_locale);
            document.configure_locales(&wide("en_US"), Rc::new(|| 45));
            assert_eq!(field.get_locale_name(), Some(wide(expected)));
            assert_eq!(field.get_locale().unwrap().get_name(), wide(expected));
            if !top_locale.is_empty() {
                // A query on the top subform itself checks its own locale before config.
                assert_eq!(top.get_locale_name(), Some(wide("fr_FR")));
            }
            field.set_cdata(XfaAttribute::Locale, &wide("custom"));
            assert_eq!(field.get_locale_name(), Some(wide("custom")));
            assert_eq!(
                field.get_locale().unwrap().get_decimal_symbol(),
                wide("custom-decimal")
            );
        }
    }

    #[test]
    fn ambient_empty_and_timezone_callback_reentry() {
        let (document, _, _, field) = parsed(CONFIG, "locale='fr_FR'", "");
        let calls = Rc::new(Cell::new(0));
        let count = calls.clone();
        let weak_document = Rc::downgrade(&document);
        document.configure_locales(
            &wide("en_US"),
            Rc::new(move || {
                count.set(count.get() + 1);
                assert!(weak_document.upgrade().unwrap().get_locale_mgr().is_some());
                -210
            }),
        );
        field.set_cdata(XfaAttribute::Locale, &wide("ambient"));
        assert_eq!(field.get_locale_name(), Some(wide("ambient")));
        let locale = field.get_locale().unwrap();
        assert_eq!(locale.get_name(), wide("en_US"));
        assert_eq!(locale.get_time_zone_in_minutes(), -210);
        assert_eq!(calls.get(), 1);
        field.set_cdata(XfaAttribute::Locale, &[]);
        assert_eq!(field.get_locale_name(), Some(Vec::new()));
        assert!(field.get_locale().is_none());
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn missing_form_top_subform_and_explicit_host_manager() {
        let document = CxfaDocument::new();
        let field = document
            .create_node(XfaPacketType::Form, XfaElement::Field)
            .unwrap();
        field.set_cdata(XfaAttribute::Locale, &wide("custom"));
        document.configure_locales(&wide("en_US"), Rc::new(|| 0));
        assert!(field.get_locale_name().is_none());
        let xdp = document
            .create_node(XfaPacketType::Xdp, XfaElement::Xfa)
            .unwrap();
        let form = document
            .create_node(XfaPacketType::Form, XfaElement::Form)
            .unwrap();
        form.set_cdata(XfaAttribute::Name, &wide("form"));
        assert!(xdp.insert_child_and_notify(-1, &form));
        assert!(document.set_root(Some(xdp)));
        assert!(field.get_locale_name().is_none());
        let (document, _, _, field) = parsed("", "", "");
        assert!(document.get_locale_mgr().is_none());
        assert!(field.get_locale_name().is_none());
        assert!(field.get_locale().is_none());
        field.set_cdata(XfaAttribute::Locale, &wide("custom"));
        assert_eq!(field.get_locale_name(), Some(wide("custom")));
        assert!(field.get_locale().is_none());
    }
}
