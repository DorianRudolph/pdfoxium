// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Built-in XML and node-backed locale selection/caching from `cxfa_localemgr.{h,cpp}`.
use super::cxfa_xmllocale::CxfaXmlLocale;
use super::locale_data::*;
use super::{cxfa_node::CxfaNode, cxfa_nodelocale::CxfaNodeLocale};
use crate::fxcodec::flate::flate_or_lzw_decode;
use crate::xfa::fgas::crt::locale_iface::LocaleIface;
use crate::xfa::fgas::crt::locale_mgr_iface::LocaleMgrIface;
use crate::xfa::fxfa::fxfa_basic::{XfaAttribute, XfaElement};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum LangId {
    ZhHk = 0x0c04,
    ZhCn = 0x0804,
    ZhTw = 0x0404,
    NlNl = 0x0413,
    EnGb = 0x0809,
    EnUs = 0x0409,
    FrFr = 0x040c,
    DeDe = 0x0407,
    ItIt = 0x0410,
    JaJp = 0x0411,
    KoKr = 0x0412,
    PtBr = 0x0416,
    RuRu = 0x0419,
    EsLa = 0x080a,
    EsEs = 0x0c0a,
}

pub fn get_language_id(language: &[u32]) -> LangId {
    // The source lowers the wide name and compares the first language pair
    // and positions3..5. Only these four codepoints affect dispatch.
    let lower = |index: usize| {
        language.get(index).copied().map(|value| {
            char::from_u32(value)
                .and_then(|c| c.to_lowercase().next())
                .map_or(value, u32::from)
        })
    };
    let pair = (lower(0), lower(1));
    let second = if language.len() >= 5 {
        (lower(3), lower(4))
    } else {
        (None, None)
    };
    let is = |pair: (Option<u32>, Option<u32>), value: &[u8; 2]| {
        pair == (Some(u32::from(value[0])), Some(u32::from(value[1])))
    };
    if is(pair, b"zh") {
        if is(second, b"cn") {
            return LangId::ZhCn;
        }
        if is(second, b"tw") {
            return LangId::ZhTw;
        }
        if is(second, b"hk") {
            return LangId::ZhHk;
        }
    } else if is(pair, b"ja") {
        return LangId::JaJp;
    } else if is(pair, b"ko") {
        return LangId::KoKr;
    } else if is(pair, b"en") {
        return if is(second, b"gb") {
            LangId::EnGb
        } else {
            LangId::EnUs
        };
    } else if is(pair, b"de") {
        return LangId::DeDe;
    } else if is(pair, b"fr") {
        return LangId::FrFr;
    } else if is(pair, b"es") {
        return if is(second, b"es") {
            LangId::EsEs
        } else {
            LangId::EsLa
        };
    } else if is(pair, b"it") {
        return LangId::ItIt;
    } else if is(pair, b"pt") {
        return LangId::PtBr;
    } else if is(pair, b"nl") {
        return LangId::NlNl;
    } else if is(pair, b"ru") {
        return LangId::RuRu;
    }
    LangId::EnUs
}

pub struct CxfaLocaleMgr {
    node_locales: Vec<Rc<CxfaNodeLocale>>,
    // None means unqueried; Some(None) means a cached absence.
    config_locale: RefCell<Option<Option<Vec<u32>>>>,
    xml_locales: RefCell<Vec<(LangId, Rc<CxfaXmlLocale>)>>,
    def_locale: RefCell<Option<Rc<dyn LocaleIface>>>,
    deflcid: LangId,
    timezone: Rc<dyn Fn() -> i32>,
}
impl CxfaLocaleMgr {
    pub fn new(default_name: &[u32], timezone: Rc<dyn Fn() -> i32>) -> Self {
        Self::with_locale_set(None, default_name, timezone)
    }
    pub fn with_locale_set(
        locale_set: Option<CxfaNode>,
        default_name: &[u32],
        timezone: Rc<dyn Fn() -> i32>,
    ) -> Self {
        let mut result = Self {
            node_locales: Vec::new(),
            config_locale: RefCell::new(None),
            xml_locales: RefCell::new(Vec::new()),
            def_locale: RefCell::new(None),
            deflcid: get_language_id(default_name),
            timezone,
        };
        // Preserve the constructor's source ordering: resolve the requested
        // built-in default before wrapping the supplied localeSet children.
        *result.def_locale.borrow_mut() = result.get_locale_by_name(default_name);
        let mut child = locale_set.and_then(|node| node.first_child());
        while let Some(node) = child {
            child = node.next_sibling();
            result.node_locales.push(Rc::new(CxfaNodeLocale::new(
                Some(node),
                result.timezone.clone(),
            )));
        }
        result
    }
    pub fn get_config_locale_name(&self, config: Option<&CxfaNode>) -> Option<Vec<u32>> {
        if let Some(cached) = self.config_locale.borrow().as_ref() {
            return cached.clone();
        }
        *self.config_locale.borrow_mut() = Some(None);
        let config = config.filter(|node| node.is_alive())?;
        let child = config
            .get_first_child_by_class(XfaElement::Acrobat)
            .or_else(|| config.get_first_child_by_class(XfaElement::Present))?;
        let common = child.get_first_child_by_class(XfaElement::Common)?;
        let locale = common.get_first_child_by_class(XfaElement::Locale)?;
        let name = locale
            .try_cdata(XfaAttribute::Value, false)
            .filter(|name| !name.is_empty())?;
        *self.config_locale.borrow_mut() = Some(Some(name.clone()));
        Some(name)
    }
    fn get_locale(&self, language: LangId) -> Option<Rc<CxfaXmlLocale>> {
        let data = match language {
            LangId::ZhCn => K_ZHCN_LOCALE,
            LangId::ZhTw => K_ZHTW_LOCALE,
            LangId::ZhHk => K_ZHHK_LOCALE,
            LangId::JaJp => K_JAJP_LOCALE,
            LangId::KoKr => K_KOKR_LOCALE,
            LangId::EnGb => K_ENGB_LOCALE,
            LangId::EsLa => K_ESLA_LOCALE,
            LangId::EsEs => K_ESES_LOCALE,
            LangId::DeDe => K_DEDE_LOACALE,
            LangId::FrFr => K_FRFR_LOCALE,
            LangId::ItIt => K_ITIT_LOCALE,
            LangId::PtBr => K_PTBR_LOCALE,
            LangId::NlNl => K_NLNL_LOCALE,
            LangId::RuRu => K_RURU_LOCALE,
            LangId::EnUs => K_ENUS_LOCALE,
        };
        let decoded = flate_or_lzw_decode(false, data, true, 0, 0, 0, 0, 0);
        if decoded.data.is_empty() {
            return None;
        }
        CxfaXmlLocale::create(&decoded.data, self.timezone.clone()).map(Rc::new)
    }
}
impl LocaleMgrIface for CxfaLocaleMgr {
    fn get_def_locale(&self) -> Option<Rc<dyn LocaleIface>> {
        if let Some(locale) = self.def_locale.borrow().as_ref() {
            return Some(locale.clone());
        }
        if let Some(locale) = self.node_locales.first() {
            return Some(locale.clone());
        }
        if let Some((_, locale)) = self.xml_locales.borrow().first() {
            return Some(locale.clone());
        }
        let locale = self.get_locale(self.deflcid)?;
        self.xml_locales
            .borrow_mut()
            .push((self.deflcid, locale.clone()));
        let locale: Rc<dyn LocaleIface> = locale;
        *self.def_locale.borrow_mut() = Some(locale.clone());
        Some(locale)
    }
    fn get_locale_by_name(&self, name: &[u32]) -> Option<Rc<dyn LocaleIface>> {
        for locale in &self.node_locales {
            if locale.get_name() == name {
                return Some(locale.clone());
            }
        }
        if name.len() < 2 {
            return None;
        }
        let language = get_language_id(name);
        {
            let locales = self.xml_locales.borrow();
            for (_, locale) in locales.iter() {
                if locale.get_name() == name {
                    return Some(locale.clone());
                }
            }
            // Native lookup compares only the stored XML name. Aliases such
            // as `fr`/`fr_CA` can never match `fr_FR`, so a persistent runtime
            // otherwise decompresses and retains another locale on every call.
            // Keep exact-name precedence and first-loaded order, then reuse
            // the same built-in buffer by its resolved language identifier.
            if let Some((_, locale)) = locales.iter().find(|(id, _)| *id == language) {
                return Some(locale.clone());
            }
        }
        let locale = self.get_locale(language)?;
        self.xml_locales
            .borrow_mut()
            .push((language, locale.clone()));
        Some(locale)
    }
    fn set_def_locale(&self, locale: Option<Rc<dyn LocaleIface>>) {
        // Drop the previous trait object only after releasing the cell borrow.
        // A host-supplied locale may perform work in its destructor.
        let previous = self.def_locale.replace(locale);
        drop(previous);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xfa::fgas::crt::locale_iface::{DateTimeSubcategory, NumSubcategory};
    use std::cell::Cell;
    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    #[test]
    fn all_fifteen_source_locale_buffers_decode_and_supply_formatter_symbols() {
        let manager = CxfaLocaleMgr::new(&wide("en_US"), Rc::new(|| 90));
        for name in [
            "zh_CN", "zh_TW", "zh_HK", "nl_NL", "en_GB", "en_US", "fr_FR", "de_DE", "it_IT",
            "ja_JP", "ko_KR", "pt_BR", "ru_RU", "es_ES", "es_LA",
        ] {
            let locale = manager.get_locale_by_name(&wide(name)).unwrap();
            assert_eq!(
                locale.get_name(),
                wide(if name == "es_LA" { "es_MX" } else { name }),
                "{name}"
            );
            assert!(!locale.get_decimal_symbol().is_empty(), "{name}");
            assert!(!locale.get_grouping_symbol().is_empty(), "{name}");
            assert!(!locale.get_percent_symbol().is_empty(), "{name}");
            assert!(!locale.get_month_name(0, false).is_empty(), "{name}");
            assert!(!locale.get_day_name(0, true).is_empty(), "{name}");
            assert!(
                !locale
                    .get_date_pattern(DateTimeSubcategory::Medium)
                    .is_empty(),
                "{name}"
            );
            assert!(
                !locale
                    .get_time_pattern(DateTimeSubcategory::Short)
                    .is_empty(),
                "{name}"
            );
            assert!(
                !locale.get_num_pattern(NumSubcategory::Decimal).is_empty(),
                "{name}"
            );
            assert_eq!(locale.get_time_zone_in_minutes(), 90);
        }
    }
    #[test]
    fn language_aliases_defaults_cache_identity_and_live_host_offsets_are_preserved() {
        let offset = Rc::new(Cell::new(60));
        let host_offset = offset.clone();
        let manager = CxfaLocaleMgr::new(&wide("EN_gb"), Rc::new(move || host_offset.get()));
        let default = manager.get_def_locale().unwrap();
        assert_eq!(default.get_name(), wide("en_GB"));
        let cached = manager.get_locale_by_name(&wide("en_GB")).unwrap();
        assert!(Rc::ptr_eq(&default, &cached));
        assert!(manager.get_locale_by_name(&[]).is_none());
        assert!(manager.get_locale_by_name(&wide("e")).is_none());
        assert_eq!(
            manager
                .get_locale_by_name(&wide("fr_CA"))
                .unwrap()
                .get_name(),
            wide("fr_FR")
        );
        assert_eq!(
            manager
                .get_locale_by_name(&wide("bogus"))
                .unwrap()
                .get_name(),
            wide("en_US")
        );
        let french = manager.get_locale_by_name(&wide("fr_FR")).unwrap();
        manager.set_def_locale(Some(french.clone()));
        assert!(Rc::ptr_eq(&manager.get_def_locale().unwrap(), &french));
        manager.set_def_locale(None);
        assert!(Rc::ptr_eq(&manager.get_def_locale().unwrap(), &default));
        assert_eq!(default.get_time_zone_in_minutes(), 60);
        offset.set(-210);
        assert_eq!(default.get_time_zone_in_minutes(), -210);
        let unnamed = CxfaLocaleMgr::new(&[], Rc::new(|| 0));
        assert_eq!(unnamed.get_def_locale().unwrap().get_name(), wide("en_US"));
    }

    #[test]
    fn repeated_aliases_reuse_bounded_builtin_cache_and_preserve_default_order() {
        let manager = CxfaLocaleMgr::new(&wide("en_US"), Rc::new(|| 0));
        let english = manager.get_def_locale().unwrap();
        let french = manager.get_locale_by_name(&wide("fr_CA")).unwrap();
        let latin_spanish = manager.get_locale_by_name(&wide("es_LA")).unwrap();
        assert_eq!(latin_spanish.get_name(), wide("es_MX"));
        for _ in 0..100 {
            for alias in ["fr", "fr_CA", "FR", "FR_fr", "fr_FR"] {
                let locale = manager.get_locale_by_name(&wide(alias)).unwrap();
                assert!(Rc::ptr_eq(&locale, &french), "{alias}");
            }
            for alias in ["es_LA", "es_MX", "ES_mx", "es_419"] {
                let locale = manager.get_locale_by_name(&wide(alias)).unwrap();
                assert!(Rc::ptr_eq(&locale, &latin_spanish), "{alias}");
            }
            for alias in ["bogus", "UNKNOWN", "zz_ZZ", "EN_us"] {
                let locale = manager.get_locale_by_name(&wide(alias)).unwrap();
                assert!(Rc::ptr_eq(&locale, &english), "{alias}");
            }
        }
        assert_eq!(manager.xml_locales.borrow().len(), 3);
        let spanish = manager.get_locale_by_name(&wide("ES_es")).unwrap();
        assert_eq!(spanish.get_name(), wide("es_ES"));
        assert!(!Rc::ptr_eq(&spanish, &latin_spanish));
        assert_eq!(manager.xml_locales.borrow().len(), 4);
        manager.set_def_locale(Some(french));
        manager.set_def_locale(None);
        assert!(Rc::ptr_eq(&manager.get_def_locale().unwrap(), &english));

        let initially_empty = CxfaLocaleMgr::new(&[], Rc::new(|| 0));
        let first_loaded = initially_empty.get_locale_by_name(&wide("FR_ca")).unwrap();
        assert!(Rc::ptr_eq(
            &initially_empty.get_def_locale().unwrap(),
            &first_loaded
        ));
        assert_eq!(initially_empty.xml_locales.borrow().len(), 1);
    }
}

#[cfg(test)]
mod node_tests {
    use super::*;
    use crate::{
        fxcrt::{
            cfx_read_only_span_stream::CfxReadOnlySpanStream, xml::cfx_xmlparser::CfxXmlParser,
        },
        xfa::{
            fgas::crt::locale_iface::{DateTimeSubcategory, NumSubcategory},
            fxfa::{
                fxfa_basic::XfaPacketType,
                parser::{cxfa_document::CxfaDocument, cxfa_document_builder::CxfaDocumentBuilder},
            },
        },
    };
    use std::cell::Cell;
    fn wide(s: &str) -> Vec<u32> {
        s.chars().map(u32::from).collect()
    }
    fn build(xml: &str, packet: XfaPacketType) -> (Rc<CxfaDocument>, CxfaNode) {
        let stream = Rc::new(CfxReadOnlySpanStream::new(xml.as_bytes()));
        let xml = Rc::new(RefCell::new(CfxXmlParser::new(stream).parse().unwrap()));
        let document = CxfaDocument::new();
        let mut builder = CxfaDocumentBuilder::new(document.clone());
        assert!(builder.build_document(xml, packet));
        (document, builder.get_root_node().unwrap())
    }
    const LOCALES: &str = r#"<localeSet><locale name="custom"><calendarSymbols name="gregorian"><monthNames><month>First Month</month><month>Second Month</month></monthNames><monthNames abbr="1"><month>M1</month><month>M2</month></monthNames><dayNames><day>First Day</day><day>Second Day</day></dayNames><dayNames abbr="1"><day>D1</day><day>D2</day></dayNames><meridiemNames><meridiem>Before Noon</meridiem><meridiem>After Noon</meridiem></meridiemNames><eraNames><era>Before Era</era><era>Current Era</era></eraNames></calendarSymbols><numberSymbols><numberSymbol name="decimal">@</numberSymbol><numberSymbol name="grouping">_</numberSymbol><numberSymbol name="percent">pct</numberSymbol><numberSymbol name="minus">minus</numberSymbol></numberSymbols><currencySymbols><currencySymbol name="symbol">coin</currencySymbol></currencySymbols><dateTimeSymbols>GyMd</dateTimeSymbols><datePatterns><datePattern name="short">date-short</datePattern><datePattern name="med">date-med</datePattern><datePattern name="full">date-full</datePattern><datePattern name="long">date-long</datePattern></datePatterns><timePatterns><timePattern name="short">time-short</timePattern><timePattern name="med">time-med</timePattern><timePattern name="full">time-full</timePattern><timePattern name="long">time-long</timePattern></timePatterns></locale><locale name="a"/></localeSet>"#;
    #[test]
    fn node_locale_queries_source_patterns_and_retains_arena_without_document_cycle() {
        let (document, set) = build(LOCALES, XfaPacketType::LocaleSet);
        let weak_document = Rc::downgrade(&document);
        let offset = Rc::new(Cell::new(90));
        let host_offset = offset.clone();
        let manager =
            CxfaLocaleMgr::with_locale_set(Some(set), &[], Rc::new(move || host_offset.get()));
        let locale = manager.get_def_locale().unwrap();
        assert!(Rc::ptr_eq(
            &locale,
            &manager.get_locale_by_name(&wide("custom")).unwrap()
        ));
        assert_eq!(
            manager.get_locale_by_name(&wide("a")).unwrap().get_name(),
            wide("a")
        );
        assert_eq!(locale.get_name(), wide("custom"));
        for (actual, expected) in [
            (locale.get_decimal_symbol(), "@"),
            (locale.get_grouping_symbol(), "_"),
            (locale.get_percent_symbol(), "pct"),
            (locale.get_minus_symbol(), "minus"),
            (locale.get_currency_symbol(), "coin"),
            (locale.get_date_time_symbols(), "GyMd"),
        ] {
            assert_eq!(actual, wide(expected));
        }
        assert_eq!(locale.get_month_name(0, false), wide("First Month"));
        assert_eq!(locale.get_month_name(1, true), wide("M2"));
        assert_eq!(locale.get_day_name(1, false), wide("Second Day"));
        assert_eq!(locale.get_day_name(0, true), wide("D1"));
        assert_eq!(locale.get_meridiem_name(true), wide("Before Noon"));
        assert_eq!(locale.get_meridiem_name(false), wide("After Noon"));
        assert_eq!(locale.get_era_name(false), wide("Before Era"));
        assert_eq!(locale.get_era_name(true), wide("Current Era"));
        for (category, name) in [
            (DateTimeSubcategory::Short, "short"),
            (DateTimeSubcategory::Medium, "med"),
            (DateTimeSubcategory::Default, "med"),
            (DateTimeSubcategory::Full, "full"),
            (DateTimeSubcategory::Long, "long"),
        ] {
            assert_eq!(
                locale.get_date_pattern(category),
                wide(&format!("date-{name}"))
            );
            assert_eq!(
                locale.get_time_pattern(category),
                wide(&format!("time-{name}"))
            );
        }
        for (category, pattern) in [
            (NumSubcategory::Percent, "z,zzz,zzz,zzz,zzz,zzz%"),
            (NumSubcategory::Currency, "$z,zzz,zzz,zzz,zzz,zz9.99"),
            (NumSubcategory::Decimal, "z,zzz,zzz,zzz,zzz,zz9.zzz"),
            (NumSubcategory::Integer, "z,zzz,zzz,zzz,zzz,zzz"),
        ] {
            assert_eq!(locale.get_num_pattern(category), wide(pattern));
        }
        for index in [-1, 2, i32::MAX] {
            assert!(locale.get_month_name(index, false).is_empty());
            assert!(locale.get_day_name(index, true).is_empty());
        }
        assert_eq!(locale.get_time_zone_in_minutes(), 90);
        offset.set(-210);
        assert_eq!(locale.get_time_zone_in_minutes(), -210);
        drop(document);
        assert!(weak_document.upgrade().is_none());
        assert_eq!(locale.get_name(), wide("custom"));
        assert_eq!(locale.get_month_name(0, false), wide("First Month"));
        drop(manager);
        assert_eq!(locale.get_decimal_symbol(), wide("@"));
    }
    #[test]
    fn builtin_constructor_default_precedes_custom_and_node_lookup_precedes_xml() {
        let (_document, set) = build(
            r#"<localeSet><locale name="en_US"><numberSymbols><numberSymbol name="decimal">custom</numberSymbol></numberSymbols></locale></localeSet>"#,
            XfaPacketType::LocaleSet,
        );
        let manager = CxfaLocaleMgr::with_locale_set(Some(set), &wide("en_US"), Rc::new(|| 0));
        let builtin = manager.get_def_locale().unwrap();
        assert_eq!(builtin.get_decimal_symbol(), wide("."));
        let custom = manager.get_locale_by_name(&wide("en_US")).unwrap();
        assert_eq!(custom.get_decimal_symbol(), wide("custom"));
        assert!(!Rc::ptr_eq(&builtin, &custom));
        manager.set_def_locale(None);
        assert!(Rc::ptr_eq(&manager.get_def_locale().unwrap(), &custom));
        let empty = CxfaNodeLocale::new(None, Rc::new(|| 45));
        assert!(empty.get_name().is_empty());
        assert!(empty.get_date_time_symbols().is_empty());
        assert!(empty.get_month_name(0, false).is_empty());
        assert!(empty
            .get_date_pattern(DateTimeSubcategory::Default)
            .is_empty());
        assert!(!empty.get_num_pattern(NumSubcategory::Integer).is_empty());
        assert_eq!(empty.get_time_zone_in_minutes(), 45);
    }
    #[test]
    fn config_locale_cache_distinguishes_unqueried_absent_and_present() {
        let (_document, config) = build(
            "<config><acrobat><common><locale>fr_FR</locale></common></acrobat></config>",
            XfaPacketType::Config,
        );
        let manager = CxfaLocaleMgr::new(&[], Rc::new(|| 0));
        assert!(manager.config_locale.borrow().is_none());
        assert_eq!(
            manager.get_config_locale_name(Some(&config)),
            Some(wide("fr_FR"))
        );
        let locale = config
            .get_first_child_by_class(XfaElement::Acrobat)
            .unwrap()
            .get_first_child_by_class(XfaElement::Common)
            .unwrap()
            .get_first_child_by_class(XfaElement::Locale)
            .unwrap();
        locale.set_cdata(XfaAttribute::Value, &wide("en_US"));
        assert_eq!(manager.get_config_locale_name(None), Some(wide("fr_FR")));
        assert_eq!(
            manager.get_config_locale_name(Some(&config)),
            Some(wide("fr_FR"))
        );
        let absent = CxfaLocaleMgr::new(&[], Rc::new(|| 0));
        assert_eq!(absent.get_config_locale_name(None), None);
        assert_eq!(*absent.config_locale.borrow(), Some(None));
        assert_eq!(absent.get_config_locale_name(Some(&config)), None);
    }
    #[test]
    fn config_acrobat_precedence_present_fallback_and_empty_boundaries() {
        for (xml,expected) in [
            ("<config><present><common><locale>de_DE</locale></common></present></config>",Some("de_DE")),
            ("<config><acrobat><common><locale>fr_FR</locale></common></acrobat><present><common><locale>de_DE</locale></common></present></config>",Some("fr_FR")),
            ("<config><acrobat/><present><common><locale>de_DE</locale></common></present></config>",None),
            ("<config><acrobat><common><locale/></common></acrobat></config>",None),
            ("<config><acrobat><common/></acrobat></config>",None),
            ("<config/>",None),
        ]{
            let (_document,config)=build(xml,XfaPacketType::Config);let manager=CxfaLocaleMgr::new(&[],Rc::new(||0));assert_eq!(manager.get_config_locale_name(Some(&config)),expected.map(wide),"{xml}");
        }
    }
}
