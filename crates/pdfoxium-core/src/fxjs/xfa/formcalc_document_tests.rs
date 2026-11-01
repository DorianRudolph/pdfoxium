// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::*;
use crate::{
    fxcrt::{cfx_read_only_span_stream::CfxReadOnlySpanStream, xml::cfx_xmlparser::CfxXmlParser},
    xfa::fxfa::{
        fxfa_basic::{XfaElement, XfaHashCode, XfaPacketType},
        parser::{
            cxfa_document::CxfaDocument, cxfa_document_builder::CxfaDocumentBuilder,
            cxfa_model_properties::CxfaScriptType, cxfa_node::CxfaNode,
        },
    },
};
use boa_engine::{context::HostHooks, JsString};
#[derive(Debug, Default)]
struct Hooks(Cell<i32>);
impl HostHooks for Hooks {
    fn local_timezone_offset_seconds(&self, _: i64) -> i32 {
        self.0.get()
    }
}
fn wide(value: &str) -> Vec<u32> {
    value.chars().map(u32::from).collect()
}
fn fixture() -> (Rc<CfxJseEngine>, CxfaNode, CxfaNode, Rc<Hooks>) {
    let mut xml=br#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/2.8/"><subform name="record" locale="en_US"><field name="english"><ui><textEdit/></ui></field><subform name="german" locale="de_DE"><field name="amount"><ui><numericEdit/></ui></field></subform></subform></template><localeSet xmlns="http://www.xfa.org/schema/xfa-locale-set/2.7/"><locale name="custom"><numberSymbols><numberSymbol name="decimal">!</numberSymbol></numberSymbols></locale></localeSet></xdp:xdp>"#.to_vec();
    xml.push(0);
    let parsed = CfxXmlParser::new(Rc::new(CfxReadOnlySpanStream::new(&xml)))
        .parse()
        .unwrap();
    let document = CxfaDocument::new();
    let mut builder = CxfaDocumentBuilder::new(document.clone());
    assert!(builder.build_document(Rc::new(RefCell::new(parsed)), XfaPacketType::Xdp));
    document.set_root(builder.get_root_node());
    document.configure_locales(&wide("en_US"), Rc::new(|| 777));
    let hooks = Rc::new(Hooks::default());
    hooks.0.set(5400);
    let context = Context::builder()
        .host_hooks(hooks.clone())
        .build()
        .unwrap();
    let runtime = CfxJseEngine::with_context(document.clone(), context).unwrap();
    document
        .do_data_merge_with_resolver(runtime.resolver())
        .unwrap();
    let form = document.get_xfa_node(XfaHashCode::Form as u32).unwrap();
    let record = form.get_first_child_by_name(&wide("record")).unwrap();
    let english = record.get_first_child_by_name(&wide("english")).unwrap();
    let german = record
        .get_first_child_by_name(&wide("german"))
        .unwrap()
        .get_first_child_by_name(&wide("amount"))
        .unwrap();
    (runtime, english, german, hooks)
}
fn run(runtime: &CfxJseEngine, node: Option<&CxfaNode>, source: &str) -> boa_engine::JsValue {
    runtime
        .run_script(CxfaScriptType::Formcalc, &wide(source), node)
        .unwrap()
}
#[test]
fn document_current_node_and_explicit_locale_selection() {
    let (runtime, english, german, _) = fixture();
    assert_eq!(
        run(&runtime, Some(&english), "Format(\"num{9.99}\",7.5)")
            .as_string()
            .unwrap(),
        JsString::from("7.50")
    );
    assert_eq!(
        run(&runtime, Some(&german), "Format(\"num{9.99}\",7.5)")
            .as_string()
            .unwrap(),
        JsString::from("7,50")
    );
    assert_eq!(
        run(&runtime, Some(&german), "Parse(\"9.99\",\"7,50\")").as_number(),
        Some(7.5)
    );
    assert_eq!(
        run(&runtime, None, "Format(\"num{9.99}\",7.5)")
            .as_string()
            .unwrap(),
        JsString::from("7.50")
    );
    let explicit = run(&runtime, Some(&german), "DateFmt(2,\"en_US\")");
    assert_eq!(explicit, run(&runtime, Some(&english), "DateFmt(2)"));
    german.set_cdata(
        crate::xfa::fxfa::fxfa_basic::XfaAttribute::Locale,
        &wide("custom"),
    );
    assert_eq!(
        run(&runtime, Some(&german), "Format(\"num{9.99}\",7.5)")
            .as_string()
            .unwrap(),
        JsString::from("7!50")
    );
}
#[test]
fn document_manager_default_is_unchanged_and_boa_timezone_is_live() {
    let (runtime, english, german, hooks) = fixture();
    let manager = runtime.document().get_locale_mgr().unwrap();
    let original = manager.get_def_locale().unwrap();
    run(&runtime, Some(&german), "Format(\"num{9.99}\",7.5)");
    assert!(Rc::ptr_eq(&original, &manager.get_def_locale().unwrap()));
    assert_eq!(original.get_time_zone_in_minutes(), 777);
    assert_eq!(
        run(&runtime, Some(&english), "Num2Time(1001,\"HH:MM:SS\")")
            .as_string()
            .unwrap(),
        JsString::from("01:30:01")
    );
    assert_eq!(
        run(&runtime, Some(&english), "Num2GMTime(1001,\"HH:MM:SS\")")
            .as_string()
            .unwrap(),
        JsString::from("00:00:01")
    );
    hooks.0.set(-12600);
    assert_eq!(
        run(&runtime, Some(&english), "Num2Time(1001,\"HH:MM:SS\")")
            .as_string()
            .unwrap(),
        JsString::from("20:30:01")
    );
    runtime
        .javascript()
        .with_context(|context| {
            let host = locale_host(context).unwrap();
            let before = host.manager.get_def_locale().unwrap();
            host.manager
                .set_def_locale(host.manager.get_locale_by_name(&wide("de_DE")));
            assert_eq!(
                host.manager.get_def_locale().unwrap().get_decimal_symbol(),
                wide(",")
            );
            assert_eq!(
                manager.get_def_locale().unwrap().get_decimal_symbol(),
                wide(".")
            );
            host.manager.set_def_locale(Some(before));
            Ok(())
        })
        .unwrap();
}
#[test]
fn automatic_document_registration_needs_real_locale_service_and_recovers() {
    let document = CxfaDocument::new();
    let runtime = CfxJseEngine::new(document.clone()).unwrap();
    assert!(runtime.eval("pfm_rt.Format('num{9.99}',7.5)").is_err());
    document.configure_locales(&wide("de_DE"), Rc::new(|| 0));
    assert_eq!(
        run(&runtime, None, "Format(\"num{9.99}\",7.5)")
            .as_string()
            .unwrap(),
        JsString::from("7,50")
    );
    assert!(runtime.eval("var marker={};var same=false;try{pfm_rt.Format([1,'p',{get p(){throw marker}}],7.5)}catch(e){same=e===marker}same&&pfm_rt.Format('num{9.99}',7.5)==='7,50'").unwrap().to_boolean());
    assert!(document.get_root().is_none());
    assert!(document
        .create_node(XfaPacketType::Form, XfaElement::Field)
        .is_some());
}
