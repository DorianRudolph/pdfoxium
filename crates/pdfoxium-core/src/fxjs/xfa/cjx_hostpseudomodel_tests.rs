// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::*;
use crate::{
    fxcrt::{cfx_read_only_span_stream::CfxReadOnlySpanStream, xml::cfx_xmlparser::CfxXmlParser},
    xfa::fxfa::{
        fxfa_basic::{XfaAttributeValue, XfaHashCode, XfaPacketType},
        parser::{
            cxfa_document::CxfaDocument, cxfa_document_builder::CxfaDocumentBuilder,
            cxfa_model_properties::CxfaScriptType,
        },
    },
};
use boa_engine::{js_string, property::Attribute};
use std::cell::{Cell, RefCell};
fn wide(value: &str) -> Vec<u32> {
    value.chars().map(u32::from).collect()
}
#[derive(Debug, PartialEq)]
enum Effect {
    Page(i32),
    Title(Vec<u32>),
    Goto(Vec<u32>),
    Open(CxfaNode),
    Focus(Option<CxfaNode>),
    Reset(Option<CxfaNode>),
    Beep(u32),
    Message(Vec<u32>, Vec<u32>, u32, u32),
    Response(Vec<u32>, Vec<u32>, Vec<u32>, bool),
    Print(i32, i32, XfaPrintOpt),
    Export(Vec<u32>, bool),
}
struct Host {
    runtime: Weak<CfxJseEngine>,
    calculations: Cell<bool>,
    validations: Cell<bool>,
    page: Cell<i32>,
    pages: Cell<i32>,
    title: RefCell<Vec<u32>>,
    focus: RefCell<Option<CxfaNode>>,
    effects: RefCell<Vec<Effect>>,
    throw_response: Cell<bool>,
}
impl Host {
    fn new(runtime: &Rc<CfxJseEngine>) -> Self {
        Self {
            runtime: Rc::downgrade(runtime),
            calculations: Cell::new(true),
            validations: Cell::new(true),
            page: Cell::new(0),
            pages: Cell::new(5),
            title: RefCell::new(wide("Original")),
            focus: RefCell::new(None),
            effects: RefCell::new(Vec::new()),
            throw_response: Cell::new(false),
        }
    }
    fn record(&self, effect: Effect) {
        self.effects.borrow_mut().push(effect);
    }
}
impl XfaHost for Host {
    fn calculations_enabled(&self) -> JsResult<bool> {
        Ok(self.calculations.get())
    }
    fn set_calculations_enabled(&self, value: bool) -> JsResult<()> {
        self.calculations.set(value);
        Ok(())
    }
    fn validations_enabled(&self) -> JsResult<bool> {
        Ok(self.validations.get())
    }
    fn set_validations_enabled(&self, value: bool) -> JsResult<()> {
        self.validations.set(value);
        Ok(())
    }
    fn current_page(&self) -> JsResult<i32> {
        Ok(self.page.get())
    }
    fn set_current_page(&self, page: i32) -> JsResult<()> {
        self.page.set(page);
        self.record(Effect::Page(page));
        Ok(())
    }
    fn num_pages(&self) -> JsResult<i32> {
        Ok(self.pages.get())
    }
    fn language(&self) -> JsResult<Vec<u32>> {
        Ok(wide("en-US"))
    }
    fn platform(&self) -> JsResult<Vec<u32>> {
        Ok(wide("test-platform"))
    }
    fn app_name(&self) -> JsResult<Vec<u32>> {
        Ok(wide("test-app"))
    }
    fn title(&self) -> JsResult<Vec<u32>> {
        Ok(self.title.borrow().clone())
    }
    fn set_title(&self, title: &[u32]) -> JsResult<()> {
        *self.title.borrow_mut() = title.to_vec();
        self.record(Effect::Title(title.to_vec()));
        Ok(())
    }
    fn goto_url(&self, url: &[u32]) -> JsResult<()> {
        self.record(Effect::Goto(url.to_vec()));
        Ok(())
    }
    fn open_list(&self, node: &CxfaNode) -> JsResult<()> {
        self.record(Effect::Open(node.clone()));
        Ok(())
    }
    fn get_focus(&self) -> JsResult<Option<CxfaNode>> {
        Ok(self.focus.borrow().clone())
    }
    fn set_focus(&self, node: Option<&CxfaNode>) -> JsResult<()> {
        *self.focus.borrow_mut() = node.cloned();
        self.record(Effect::Focus(node.cloned()));
        Ok(())
    }
    fn reset_data(&self, node: Option<&CxfaNode>) -> JsResult<()> {
        self.record(Effect::Reset(node.cloned()));
        Ok(())
    }
    fn beep(&self, kind: u32) -> JsResult<()> {
        self.record(Effect::Beep(kind));
        Ok(())
    }
    fn message_box(
        &self,
        message: &[u32],
        title: &[u32],
        icon: u32,
        buttons: u32,
    ) -> JsResult<i32> {
        self.record(Effect::Message(
            message.to_vec(),
            title.to_vec(),
            icon,
            buttons,
        ));
        Ok(4)
    }
    fn response(
        &self,
        question: &[u32],
        title: &[u32],
        default: &[u32],
        masked: bool,
    ) -> JsResult<Vec<u32>> {
        self.record(Effect::Response(
            question.to_vec(),
            title.to_vec(),
            default.to_vec(),
            masked,
        ));
        if self.throw_response.get() {
            self.runtime
                .upgrade()
                .unwrap()
                .eval("subject.currentPage=9;throw marker")?;
        }
        Ok(wide("answer"))
    }
    fn print(&self, start: i32, end: i32, options: XfaPrintOpt) -> JsResult<()> {
        self.record(Effect::Print(start, end, options));
        Ok(())
    }
    fn export_data(&self, path: &[u32], xdp: bool) -> JsResult<()> {
        self.record(Effect::Export(path.to_vec(), xdp));
        Ok(())
    }
}
fn fixture() -> (Rc<CfxJseEngine>, Rc<Host>, CxfaNode) {
    let mut bytes=br#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/2.8/"><subform name="record"><field name="first"><ui><textEdit/></ui></field><field name="second"><ui><choiceList/></ui></field></subform></template></xdp:xdp>"#.to_vec();
    bytes.push(0);
    let xml = CfxXmlParser::new(Rc::new(CfxReadOnlySpanStream::new(&bytes)))
        .parse()
        .unwrap();
    let document = CxfaDocument::new();
    let mut builder = CxfaDocumentBuilder::new(document.clone());
    assert!(builder.build_document(Rc::new(RefCell::new(xml)), XfaPacketType::Xdp));
    document.set_root(builder.get_root_node());
    let runtime = CfxJseEngine::new(document.clone()).unwrap();
    document
        .do_data_merge_with_resolver(runtime.resolver())
        .unwrap();
    let form = document.get_xfa_node(XfaHashCode::Form as u32).unwrap();
    let record = form.get_first_child_by_name(&wide("record")).unwrap();
    let host = Rc::new(Host::new(&runtime));
    runtime.set_host(Some(host.clone()));
    runtime
        .javascript()
        .with_context(|context| {
            let object = host_object(&runtime, context)?;
            context.register_global_property(js_string!("subject"), object, Attribute::all())?;
            Ok(())
        })
        .unwrap();
    (runtime, host, record)
}
fn yes(runtime: &CfxJseEngine, source: &str) {
    assert!(runtime.eval(source).unwrap().to_boolean(), "{source}");
}
#[test]
fn all_properties_writable_flags_readonly_errors_and_identity() {
    let (runtime, host, _) = fixture();
    yes(&runtime,"subject.appType==='Exchange'&&subject.language==='en-US'&&subject.platform==='test-platform'&&subject.name==='test-app'&&subject.version==='11'&&subject.variation==='Full'&&subject.numPages===5&&subject.className==='hostPseudoModel'");
    yes(&runtime,"subject.calculationsEnabled=0;subject.validationsEnabled='';subject.currentPage={valueOf(){return 3}};subject.title={toString(){return 'Changed'}};!subject.calculationsEnabled&&!subject.validationsEnabled&&subject.currentPage===3&&subject.title==='Changed'");
    assert!(!host.calculations.get());
    assert!(!host.validations.get());
    assert_eq!(host.title.borrow().as_slice(), wide("Changed"));
    yes(&runtime,"var caught=0;for(const name of ['appType','language','numPages','platform','name','variation','version','className']){try{subject[name]='bad'}catch(e){caught++}}caught===8");
}
#[test]
fn absent_notify_preserves_no_coercion_and_native_constant_noops() {
    let (runtime, _, _) = fixture();
    runtime.set_host(None);
    yes(&runtime,"var hit=0;const unused={toString(){hit++;throw 1},valueOf(){hit++;throw 2}};subject.title=unused;subject.currentPage=unused;subject.language=unused;subject.name=unused;subject.gotoURL(unused);subject.exportData(unused);subject.messageBox(unused);subject.response(unused);subject.resetData(unused);subject.print(unused,unused,unused,unused,unused,unused,unused,unused);subject.importData(unused);subject.documentInBatch(unused)===0&&subject.documentCountInBatch(unused)===0&&subject.version==='11'&&subject.variation==='Full'&&subject.appType===undefined&&subject.getFocus()===undefined&&hit===0");
    yes(&runtime,"var caught=0;for(const f of [()=>subject.gotoURL(),()=>subject.beep(1,2),()=>subject.exportData(),()=>subject.importData(),()=>subject.messageBox(),()=>subject.response(),()=>subject.resetData(1,2),()=>subject.print()]){try{f()}catch(e){caught++}}caught===8");
}
#[test]
fn dialog_response_beep_export_print_conversion_and_effect_order() {
    let (runtime, host, _) = fixture();
    yes(&runtime,"subject.beep();subject.beep(-1);subject.gotoURL('url');subject.exportData('file');subject.exportData('data',0);subject.messageBox('m','t',-1,99)===4&&subject.response('q','caption','default',2)==='answer'");
    yes(&runtime,"subject.print(true,{valueOf(){return 2}},{valueOf(){return 4}},true,true,true,true,true);subject.importData({toString(){throw 1}})===undefined");
    assert_eq!(
        *host.effects.borrow(),
        vec![
            Effect::Beep(4),
            Effect::Beep(u32::MAX),
            Effect::Goto(wide("url")),
            Effect::Export(wide("file"), true),
            Effect::Export(wide("data"), false),
            Effect::Message(wide("m"), wide("t"), 0, 0),
            Effect::Response(wide("q"), wide("caption"), wide("default"), true),
            Effect::Print(2, 4, XfaPrintOpt(63))
        ]
    );
    yes(&runtime,"var order=[];subject.messageBox({toString(){order.push(1);return 'm'}},{toString(){order.push(2);return 't'}},{valueOf(){order.push(3);return 3}},{valueOf(){order.push(4);return 2}});order.join(',')==='1,2,3,4'");
}
#[test]
fn client_gate_precedes_arity_and_host_page_navigation_keeps_source_limits() {
    let (runtime, host, _) = fixture();
    runtime.set_run_at_type(XfaAttributeValue::Server);
    yes(&runtime,"subject.gotoURL();subject.beep(1,2);subject.messageBox();subject.print();subject.setFocus();subject.openList();subject.title={toString(){throw 1}};subject.variation='ignored';subject.title===undefined&&subject.variation===undefined&&subject.version==='11'&&subject.language==='en-US'");
    assert!(host.effects.borrow().is_empty());
    host.page.set(1);
    runtime.eval("subject.pageUp();subject.pageDown()").unwrap();
    assert_eq!(host.page.get(), 2);
    runtime.eval("subject.pageUp()").unwrap();
    assert_eq!(host.page.get(), 1);
    host.page.set(5);
    runtime.eval("subject.pageDown()").unwrap();
    assert_eq!(host.page.get(), 5);
    host.page.set(9);
    runtime.eval("subject.pageDown()").unwrap();
    assert_eq!(host.page.get(), 4);
    host.pages.set(0);
    runtime.eval("subject.pageDown()").unwrap();
    assert_eq!(host.page.get(), 4);
    runtime.set_run_at_type(XfaAttributeValue::Client);
    assert!(runtime.eval("subject.gotoURL()").is_err());
}
#[test]
fn focus_open_list_and_comma_reset_resolve_real_nodes_and_widget_readiness() {
    let (runtime, host, record) = fixture();
    let first = record.get_first_child_by_name(&wide("first")).unwrap();
    let second = record.get_first_child_by_name(&wide("second")).unwrap();
    first.set_widget_ready();
    let source = wide("subject.setFocus('first');subject.getFocus()===this.first");
    assert!(runtime
        .run_script(CxfaScriptType::Javascript, &source, Some(&record))
        .unwrap()
        .to_boolean());
    runtime.run_script(CxfaScriptType::Javascript,&wide("subject.openList('second');subject.resetData(' first , absent, second ');subject.setFocus(null);subject.resetData('absent');subject.resetData('')"),Some(&record)).unwrap();
    assert_eq!(
        *host.effects.borrow(),
        vec![
            Effect::Focus(Some(first.clone())),
            Effect::Open(second),
            Effect::Reset(Some(first)),
            Effect::Reset(None),
            Effect::Focus(None),
            Effect::Reset(None),
            Effect::Reset(None)
        ]
    );
    let before = host.effects.borrow().len();
    runtime
        .eval("subject.openList('first');subject.setFocus('first');subject.resetData('first')")
        .unwrap();
    assert_eq!(host.effects.borrow().len(), before);
}
#[test]
fn callback_reentry_and_opaque_coercion_errors_keep_identity_and_recover() {
    let (runtime, host, _) = fixture();
    yes(&runtime,"var marker={};var caught=false;try{subject.currentPage={valueOf(){throw marker}}}catch(e){caught=e===marker}caught&&subject.currentPage===0");
    host.throw_response.set(true);
    yes(&runtime,"var same=false;try{subject.response('question')}catch(e){same=e===marker}same&&subject.currentPage===9");
    host.throw_response.set(false);
    yes(
        &runtime,
        "subject.response('again')==='answer'&&subject.messageBox('ok')===4",
    );
}
#[test]
fn weak_runtime_receiver_and_inherited_property_contract() {
    let (runtime, _, _) = fixture();
    yes(&runtime, "Object.create(subject).version==='11'");
    assert!(runtime.eval("subject.beep.call({})").is_err());
    assert!(runtime
        .eval("subject.beep.call(Object.create(subject))")
        .is_err());
    let javascript = runtime.javascript().clone();
    drop(runtime);
    assert!(javascript
        .eval(&"subject.version".encode_utf16().collect::<Vec<_>>())
        .is_err());
}
