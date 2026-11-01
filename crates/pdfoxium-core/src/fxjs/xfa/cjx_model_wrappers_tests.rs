// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Native method tables, arity/coercion contracts, real model remerge and
//! optional FF event services through the ordinary Boa node bindings.
use super::*;
use crate::{
    fxcrt::{cfx_read_only_span_stream::CfxReadOnlySpanStream, xml::cfx_xmlparser::CfxXmlParser},
    fxjs::xfa::{cfxjse_engine::CfxJseEngine, cjx_subform::XfaWidgetScriptHost},
    xfa::fxfa::{
        cxfa_eventparam::CxfaEventParam,
        fxfa::XfaEventError,
        fxfa_basic::{XfaElement, XfaPacketType},
        parser::{
            cxfa_document::CxfaDocument, cxfa_document_builder::CxfaDocumentBuilder,
            cxfa_document_datamerge::DataMergeLayoutHost,
        },
    },
};
use boa_engine::{js_string, property::Attribute, JsString, Source};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};
fn wide(value: &str) -> Vec<u32> {
    value.chars().map(u32::from).collect()
}
fn expose(runtime: &Rc<CfxJseEngine>, name: &str, node: &CxfaNode) {
    runtime
        .javascript()
        .with_context(|context| {
            let binding = runtime.get_or_create_binding(node, context)?;
            context.register_global_property(JsString::from(name), binding, Attribute::all())?;
            Ok(())
        })
        .unwrap();
}
fn eval(runtime: &CfxJseEngine, script: &str) -> JsValue {
    match runtime.eval(script) {
        Ok(value) => value,
        Err(error) => {
            let message = runtime
                .javascript()
                .with_context(|context| error.clone().into_opaque(context)?.to_string(context))
                .map(|v| v.to_std_string_escaped())
                .unwrap_or_else(|_| format!("{error:?}"));
            panic!("CJX model script failed: {message}\n{script}")
        }
    }
}
struct Fixture {
    runtime: Rc<CfxJseEngine>,
    template: CxfaNode,
    manifest: CxfaNode,
}
fn fixture() -> Fixture {
    let document = CxfaDocument::new();
    let root = document
        .create_node(XfaPacketType::Xdp, XfaElement::Xfa)
        .unwrap();
    assert!(document.set_root(Some(root.clone())));
    let runtime = CfxJseEngine::new(document.clone()).unwrap();
    expose(&runtime, "xfaRoot", &root);
    let mut template = None;
    let mut manifest = None;
    for (name, packet, element) in [
        ("formObject", XfaPacketType::Form, XfaElement::Form),
        (
            "templateObject",
            XfaPacketType::Template,
            XfaElement::Template,
        ),
        (
            "manifestObject",
            XfaPacketType::Template,
            XfaElement::Manifest,
        ),
        ("sourceObject", XfaPacketType::SourceSet, XfaElement::Source),
        (
            "wsdlObject",
            XfaPacketType::ConnectionSet,
            XfaElement::WsdlConnection,
        ),
        ("deltaObject", XfaPacketType::Form, XfaElement::Delta),
        (
            "encryptObject",
            XfaPacketType::Template,
            XfaElement::Encrypt,
        ),
        ("extrasObject", XfaPacketType::Template, XfaElement::Extras),
        (
            "handlerObject",
            XfaPacketType::Template,
            XfaElement::Handler,
        ),
        ("dataObject", XfaPacketType::Datasets, XfaElement::DataValue),
    ] {
        let node = document.create_node(packet, element).unwrap();
        if element == XfaElement::Template {
            template = Some(node.clone());
        }
        if element == XfaElement::Manifest {
            manifest = Some(node.clone());
        }
        expose(&runtime, name, &node);
    }
    Fixture {
        runtime,
        template: template.unwrap(),
        manifest: manifest.unwrap(),
    }
}
#[test]
fn source_all_seventeen_methods_keep_native_zero_argument_contract() {
    let f = fixture();
    eval(
        &f.runtime,
        r#"
    const methods=['addNew','cancel','cancelBatch','close','delete','first','hasDataChanged','isBOF','isEOF','last','next','open','previous','requery','resync','update','updateBatch'];
    let converted=false;const bad={toString(){converted=true;throw 1},valueOf(){converted=true;throw 2}};
    for(const name of methods){if(sourceObject[name]()!==undefined)throw Error(name+' return');let caught=false;try{sourceObject[name](bad)}catch(e){caught=true}if(!caught)throw Error(name+' count');}
    if(converted)throw Error('source coerced unused arguments');
    let receiver=false;try{sourceObject.next.call({})}catch(e){receiver=true}if(!receiver)throw Error('source receiver');
    "#,
    );
}
#[test]
fn native_empty_properties_ignore_assignment_without_coercing() {
    let f = fixture();
    eval(
        &f.runtime,
        r#"
    let converted=false;const bad={toString(){converted=true;throw 1},valueOf(){converted=true;throw 2}};
    for(const [object,name] of [[encryptObject,'format'],[extrasObject,'type'],[handlerObject,'version'],[sourceObject,'db'],[deltaObject,'currentValue'],[deltaObject,'savedValue'],[deltaObject,'target']]){
        if(object[name]!==undefined)throw Error(name+' original');object[name]=bad;if(object[name]!==undefined)throw Error(name+' rewritten');
    }
    if(converted)throw Error('property coercion');if(deltaObject.restore()!==undefined)throw Error('restore');
    let caught=false;try{deltaObject.restore(bad)}catch(e){caught=true}if(!caught)throw Error('restore count');
    if(typeof deltaObject.resolveNode!=='undefined'||typeof deltaObject.clone!=='undefined')throw Error('Delta must derive Object');
    "#,
    );
}
#[test]
fn pseudo_models_keep_source_noop_methods_and_signature_boundaries() {
    let f = fixture();
    eval(
        &f.runtime,
        r#"
    let converted=false;const bad={toString(){converted=true;throw 1},valueOf(){converted=true;throw 2}};
    const window=xfaRoot.dataWindow,log=xfaRoot.log,signature=xfaRoot.signature;
    if(window!==xfaRoot.dataWindow||log!==xfaRoot.log||signature!==xfaRoot.signature)throw Error('stable pseudo identities');
    if(window.className!=='dataWindow'||log.className!=='logPseudoModel'||signature.className!=='signaturePseudoModel')throw Error('class names');
    for(const name of ['gotoRecord','isRecordGroup','moveCurrentRecord','record'])if(window[name](bad,bad)!==undefined)throw Error(name);
    for(const name of ['message','traceEnabled','traceActivate','traceDeactivate','trace'])if(log[name](bad,bad)!==undefined)throw Error(name);
    for(const name of ['currentRecordNumber','isDefined','recordsAfter','recordsBefore']){window[name]=bad;if(window[name]!==undefined||Object.create(window)[name]!==undefined)throw Error(name);}
    for(let n=0;n<=8;n++)for(const [name,min,max,result] of [['verify',1,4,0],['sign',3,7,false],['enumerate',0,0,undefined],['clear',1,2,false]]){
        let failed=false,value;try{value=signature[name](...Array(n).fill(bad))}catch(e){failed=true}
        if(failed!==(n<min||n>max)||(!failed&&value!==result))throw Error(name+' arity '+n);
    }
    if(converted)throw Error('pseudo conversion');let rejected=0;for(const call of [()=>log.trace.call({}),()=>window.record.call(signature),()=>signature.verify.call(window,bad),()=>{signature.className=bad}]){try{call()}catch(e){rejected++}}if(rejected!==4)throw Error('receiver/readonly');
    "#,
    );
}
#[test]
fn retained_pseudo_objects_validate_weak_runtime_lifetime() {
    let f = fixture();
    let object = f
        .runtime
        .javascript()
        .with_context(|context| {
            super::super::cjx_datawindow::datawindow_object(&f.runtime, context)
        })
        .unwrap();
    let weak = Rc::downgrade(&f.runtime);
    drop(f);
    assert!(weak.upgrade().is_none());
    let mut context = Context::default();
    context
        .register_global_property(js_string!("saved"), object, Attribute::all())
        .unwrap();
    assert_eq!(context.eval(Source::from_bytes("let errors=0;for(const call of [()=>saved.record(),()=>saved.recordsBefore,()=>saved.className])try{call()}catch(e){if(e.name==='ReferenceError'&&e.message.includes('expired'))errors++}errors===3")).unwrap().as_boolean(),Some(true));
}
#[test]
fn template_manifest_and_wsdl_use_exact_readiness_and_fixed_results() {
    let f = fixture();
    eval(
        &f.runtime,
        r#"
    for(const name of ['execCalculate','execInitialize','execValidate'])if(templateObject[name]()!==false)throw Error('template initial');
    if(manifestObject.evaluate()!==false)throw Error('manifest initial');
    const bad={toString(){throw Error('unused')},valueOf(){throw Error('unused')}};
    if(templateObject.formNodes(bad)!==true||templateObject.recalculate(bad)!==true||wsdlObject.execute()!==false||wsdlObject.execute(bad)!==false)throw Error('native fixed values');
    let rejected=0;for(const call of [()=>templateObject.execCalculate(bad),()=>templateObject.execInitialize(bad),()=>templateObject.execValidate(bad),()=>templateObject.formNodes(),()=>templateObject.formNodes(bad,bad),()=>templateObject.recalculate(),()=>templateObject.recalculate(bad,bad),()=>manifestObject.evaluate(bad),()=>wsdlObject.execute(bad,bad)])try{call()}catch(e){rejected++}if(rejected!==9)throw Error('counts');
    "#,
    );
    f.template.set_widget_ready();
    f.manifest.set_widget_ready();
    eval(&f.runtime,"if(!templateObject.execCalculate()||!templateObject.execInitialize()||!templateObject.execValidate()||!manifestObject.evaluate())throw Error('actual readiness')");
}
#[test]
fn form_native_empty_list_checksum_and_absent_host_event_behavior() {
    let f = fixture();
    eval(
        &f.runtime,
        r#"
    let converted=false;const bad={toString(){converted=true;throw 1},valueOf(){converted=true;throw 2}};
    if(formObject.execCalculate()!==undefined||formObject.execInitialize()!==undefined||formObject.execValidate()!==false||formObject.recalculate(bad)!==undefined)throw Error('absent notify');
    if(converted)throw Error('absent notify coercion');
    const a=formObject.formNodes(dataObject),b=formObject.formNodes(dataObject);if(a===b||a.length!==0||b.length!==0||a.className!=='treeList')throw Error('source empty persisted list');
    let rejected=0;for(const call of [()=>formObject.formNodes(),()=>formObject.formNodes(dataObject,bad),()=>formObject.formNodes(bad),()=>formObject.execCalculate(bad),()=>formObject.execInitialize(bad),()=>formObject.execValidate(bad),()=>formObject.recalculate(),()=>formObject.recalculate(0,0)])try{call()}catch(e){rejected++}if(rejected!==8)throw Error('form counts/value');
    if(formObject.checksum!=='')throw Error('checksum default');formObject.checksum={toString(){formObject.checksum='inner';return 'outer\u0000\ud800'}};if(formObject.checksum!=='outer\u0000\ud800')throw Error('checksum coercion');
    const marker={};let caught=false;try{formObject.checksum={toString(){throw marker}}}catch(e){caught=e===marker}if(!caught||formObject.checksum!=='outer\u0000\ud800')throw Error('checksum opaque');
    "#,
    );
    let event = Rc::new(RefCell::new(CxfaEventParam::new(XfaEventType::Calculate)));
    f.runtime.with_event(None, event.clone(), || {
        eval(
            &f.runtime,
            "formObject.recalculate();formObject.recalculate(0,0)",
        );
    });
    event.borrow_mut().event_type = XfaEventType::InitCalculate;
    f.runtime.with_event(None, event, || {
        eval(&f.runtime, "formObject.recalculate()");
    });
}
struct EventHost {
    runtime: Weak<CfxJseEngine>,
    calls: RefCell<Vec<(XfaEventType, bool, bool)>>,
    fail_validation: Cell<bool>,
}
impl XfaWidgetScriptHost for EventHost {
    fn exec_deep_first(
        &self,
        _: &CxfaNode,
        event: XfaEventType,
        ready: bool,
        recursive: bool,
    ) -> JsResult<XfaEventError> {
        self.calls.borrow_mut().push((event, ready, recursive));
        // A real host may reenter the same engine while the native method has
        // temporarily parked its mutable VM lease.
        self.runtime
            .upgrade()
            .unwrap()
            .eval("globalThis.hostReentries=(globalThis.hostReentries||0)+1")?;
        Ok(
            if event == XfaEventType::Validate && self.fail_validation.get() {
                XfaEventError::kError
            } else {
                XfaEventError::kSuccess
            },
        )
    }
    fn add_calc_validate(&self, _: &CxfaNode) -> JsResult<()> {
        Ok(())
    }
}
#[test]
fn form_recalculate_preserves_real_host_event_order_and_reentrant_coercion() {
    let f = fixture();
    let host = Rc::new(EventHost {
        runtime: Rc::downgrade(&f.runtime),
        calls: RefCell::new(Vec::new()),
        fail_validation: Cell::new(false),
    });
    f.runtime.set_widget_script_host(Some(host.clone()));
    eval(&f.runtime,"formObject.recalculate({valueOf(){globalThis.coerced=1;return 0}});if(coerced!==1||hostReentries!==3)throw Error('reentrant host')");
    assert_eq!(
        &*host.calls.borrow(),
        &[
            (XfaEventType::Calculate, false, true),
            (XfaEventType::Validate, false, true),
            (XfaEventType::Ready, true, true)
        ]
    );
    host.calls.borrow_mut().clear();
    eval(&f.runtime, "formObject.recalculate(1)");
    assert!(host.calls.borrow().is_empty());
    eval(&f.runtime,"formObject.execInitialize();formObject.execCalculate();if(formObject.execValidate()!==true)throw Error('valid')");
    assert_eq!(
        &*host.calls.borrow(),
        &[
            (XfaEventType::Initialize, false, true),
            (XfaEventType::Calculate, false, true),
            (XfaEventType::Validate, false, true)
        ]
    );
    host.fail_validation.set(true);
    eval(
        &f.runtime,
        "if(formObject.execValidate()!==false)throw Error('invalid')",
    );
    eval(&f.runtime,"const marker={};let caught=false;try{formObject.recalculate({valueOf(){throw marker}})}catch(e){caught=e===marker}if(!caught)throw Error('opaque number conversion')");
}
struct Layout {
    calls: Cell<usize>,
    runtime: Weak<CfxJseEngine>,
}
impl DataMergeLayoutHost for Layout {
    fn set_force_relayout(&self) {
        self.calls.set(self.calls.get() + 1);
        self.runtime
            .upgrade()
            .unwrap()
            .eval("globalThis.relayoutCalls=(globalThis.relayoutCalls||0)+1")
            .unwrap();
    }
}
#[test]
fn form_and_template_remerge_replace_bound_tree_then_invalidate_real_layout() {
    let input=br#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="record"><field name="amount"><ui><textEdit/></ui><bind match="dataRef" ref="$record.amount"/></field></subform></template><xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"><xfa:data><record><amount>original</amount></record></xfa:data></xfa:datasets></xdp:xdp>"#;
    let mut bytes = input.to_vec();
    bytes.push(0);
    let xml = Rc::new(RefCell::new(
        CfxXmlParser::new(Rc::new(CfxReadOnlySpanStream::new(&bytes)))
            .parse()
            .unwrap(),
    ));
    let document = CxfaDocument::new();
    let mut builder = CxfaDocumentBuilder::new(document.clone());
    assert!(builder.build_document(xml, XfaPacketType::Xdp));
    let root = builder.get_root_node().unwrap();
    assert!(document.set_root(Some(root.clone())));
    let runtime = CfxJseEngine::new(document.clone()).unwrap();
    document
        .do_data_merge_with_resolver(runtime.resolver())
        .unwrap();
    let form = root.get_first_child_by_class(XfaElement::Form).unwrap();
    let template = root.get_first_child_by_class(XfaElement::Template).unwrap();
    expose(&runtime, "formObject", &form);
    expose(&runtime, "templateObject", &template);
    let old = form.first_child().unwrap();
    // Missing the full service must fail before detaching any model children.
    eval(&runtime,"let missing=false;try{formObject.remerge()}catch(e){missing=e.message.includes('layout service')}if(!missing)throw Error('missing service')");
    assert_eq!(form.first_child(), Some(old.clone()));
    let host = Rc::new(Layout {
        calls: Cell::new(0),
        runtime: Rc::downgrade(&runtime),
    });
    runtime.set_data_merge_layout_host(Some(host.clone()));
    eval(
        &runtime,
        "if(formObject.remerge()!==undefined)throw Error('form remerge return')",
    );
    let first = form.first_child().unwrap();
    assert_ne!(first, old);
    assert!(old.parent().is_none());
    assert_eq!(host.calls.get(), 1);
    let amount = first.get_first_child_by_class(XfaElement::Field).unwrap();
    assert_eq!(amount.get_raw_value(), wide("original"));
    assert!(amount.get_bind_data().is_some());
    eval(&runtime,"if(templateObject.remerge()!==undefined||relayoutCalls!==2)throw Error('template remerge')");
    assert_ne!(form.first_child(), Some(first));
    assert_eq!(host.calls.get(), 2);
    eval(&runtime,"let rejected=0;for(const call of [()=>formObject.remerge(1),()=>templateObject.remerge(1)])try{call()}catch(e){rejected++}if(rejected!==2)throw Error('remerge counts')");
    assert_eq!(host.calls.get(), 2);
}
