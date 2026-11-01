// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Source-derived model/Boa checks; readiness and the event host are explicit.
use super::*;
use crate::{
    fxcrt::{cfx_read_only_span_stream::CfxReadOnlySpanStream, xml::cfx_xmlparser::CfxXmlParser},
    xfa::fxfa::{
        fxfa_basic::XfaPacketType,
        parser::{cxfa_document::CxfaDocument, cxfa_document_builder::CxfaDocumentBuilder},
    },
};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};
fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
fn build(input: &[u8], packet: XfaPacketType) -> (Rc<CxfaDocument>, CxfaNode, Rc<CfxJseEngine>) {
    let mut bytes = input.to_vec();
    bytes.push(0);
    let xml = CfxXmlParser::new(Rc::new(CfxReadOnlySpanStream::new(&bytes)))
        .parse()
        .unwrap();
    let document = CxfaDocument::new();
    let mut builder = CxfaDocumentBuilder::new(document.clone());
    assert!(builder.build_document(Rc::new(RefCell::new(xml)), packet));
    let root = builder.get_root_node().unwrap();
    assert!(document.set_root(Some(root.clone())));
    let runtime = CfxJseEngine::new(document.clone()).unwrap();
    (document, root, runtime)
}
fn named(parent: &CxfaNode, name: &str) -> CxfaNode {
    parent.get_first_child_by_name(&wide(name)).unwrap()
}
fn script(runtime: &CfxJseEngine, source: &str) {
    assert_eq!(
        runtime.eval(source).unwrap().as_boolean(),
        Some(true),
        "{source}"
    );
}
#[test]
fn choices_preserve_readiness_mapping_selection_and_item_updates() {
    let (_document,form,runtime)=build(br#"<form xmlns="http://www.xfa.org/schema/xfa-form/3.3/"><subform name="record"><field name="choice"><ui><choiceList open="always"/></ui><items><text>A</text></items></field></subform></form>"#,XfaPacketType::Form);
    let field = named(&named(&form, "record"), "choice");
    script(
        &runtime,
        r#"
        var field=xfa.resolveNode('record.choice');
        var coerced=false;
        field.addItem({toString(){coerced=true;return 'ignored'}});
        var indexCoerced=false;
        var absent=field.getSaveItem({valueOf(){indexCoerced=true;return 0}});
        !coerced && indexCoerced && absent===null && field.length===0 && field.selectedIndex===undefined
    "#,
    );
    field.set_widget_ready();
    script(
        &runtime,
        r#"
        field.addItem('Bee','b');field.addItem('C');
        field.length===3 && field.getDisplayItem(1)==='Bee' && field.getSaveItem(1)==='b' &&
        field.getSaveItem(0)==='A' && field.boundItem('Bee')==='b' && field.getSaveItem(99)===null
    "#,
    );
    script(
        &runtime,
        r#"
        field.selectedIndex=1;
        field.rawValue==='b' && field.selectedIndex===1 && field.getItemState(1) &&
        field.formattedValue==='Bee' && field.editValue==='b' && field.parentSubform===null
    "#,
    );
    script(
        &runtime,
        r#"
        field.setItemState(1,0);
        var cleared=field.selectedIndex===-1;
        var deleted=field.deleteItem(1);
        var readonly=false;try{field.length={valueOf(){throw Error('must not coerce')}}}catch(e){readonly=true}
        field.clearItems('native ignores arguments');
        cleared && deleted && readonly && field.length===0 && field.getDisplayItem(-1)===null
    "#,
    );
    assert_eq!(field.count_choice_list_items(false), 0);
}
#[test]
fn multi_selection_and_exclusion_member_identity() {
    let (_document,form,runtime)=build(br#"<form xmlns="http://www.xfa.org/schema/xfa-form/3.3/"><subform name="record"><field name="multi"><ui><choiceList open="multiSelect"/></ui></field><exclGroup name="group"><field name="one"><ui><checkButton/></ui><items><text>yes</text><text>off</text></items><value><text>off</text></value></field><field name="two"><ui><checkButton/></ui><items><text>no</text><text>off</text></items><value><text>off</text></value></field></exclGroup></subform></form>"#,XfaPacketType::Form);
    let record = named(&form, "record");
    let multi = named(&record, "multi");
    let group = named(&record, "group");
    multi.set_widget_ready();
    group.set_widget_ready();
    script(
        &runtime,
        r#"
        var multi=xfa.resolveNode('record.multi');var group=xfa.resolveNode('record.group');
        multi.addItem('One','1');multi.addItem('Two','2');multi.addItem('Three','3');
        multi.selectedIndex=0;multi.setItemState(2,1);multi.setItemState(1,1);multi.setItemState(0,0);
        multi.rawValue==='3\n2' && multi.getItemState(2) && multi.getItemState(1) && !multi.getItemState(0)
    "#,
    );
    assert_eq!(multi.get_selected_items_value(), [wide("3"), wide("2")]);
    script(
        &runtime,
        r#"
        multi.selectedIndex=-1;
        group.rawValue='yes';
        var selected=group.selectedMember()===group.resolveNode('one');
        var rejected=false;try{group.selectedMember('two')}catch(e){rejected=true}
        var coerced=false;group.transient={toString(){coerced=true;return 'x'}};
        var readonly=false;try{group.errorText='x'}catch(e){readonly=true}
        selected && rejected && !coerced && group.transient===undefined && group.errorText===undefined &&
        readonly && multi.rawValue===null && group.resolveNode('two').rawValue==='off'
    "#,
    );
    assert!(multi.get_selected_items_value().is_empty());
    script(
        &runtime,
        "group.rawValue='missing';group.selectedMember()===null && group.rawValue===null",
    );
}
struct Host {
    runtime: Weak<CfxJseEngine>,
    events: RefCell<Vec<(XfaEventType, bool)>>,
    scheduled: Cell<usize>,
}
impl super::super::cjx_subform::XfaWidgetScriptHost for Host {
    fn exec_deep_first(
        &self,
        _node: &CxfaNode,
        event: XfaEventType,
        form_ready: bool,
        recursive: bool,
    ) -> JsResult<XfaEventError> {
        assert!(!form_ready);
        self.events.borrow_mut().push((event, recursive));
        script(
            &self.runtime.upgrade().unwrap(),
            "field===xfa.form.resolveNode('record.numeric')",
        );
        Ok(if event == XfaEventType::Validate {
            XfaEventError::kError
        } else {
            XfaEventError::kSuccess
        })
    }
    fn add_calc_validate(&self, _node: &CxfaNode) -> JsResult<()> {
        self.scheduled.set(self.scheduled.get() + 1);
        script(
            &self.runtime.upgrade().unwrap(),
            "field.formatMessage==='second'",
        );
        Ok(())
    }
}
#[test]
fn picture_values_messages_and_optional_real_event_host() {
    let (document,root,runtime)=build(br#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform restoreState="auto"/></template><form xmlns="http://www.xfa.org/schema/xfa-form/3.3/"><subform name="record"><field name="numeric"><ui><numericEdit/></ui><format><picture>num{z,zz9.99}</picture></format><value><decimal fracDigits="2">1234.5</decimal></value></field></subform></form></xdp:xdp>"#,XfaPacketType::Xdp);
    document.configure_locales(&wide("en_US"), Rc::new(|| 0));
    let form = root.get_first_child_by_class(XfaElement::Form).unwrap();
    let field = named(&named(&form, "record"), "numeric");
    field.set_widget_ready();
    script(
        &runtime,
        r#"
        var subform=xfa.form.resolveNode('record');var field=subform.resolveNode('numeric');
        field.formatMessage='first';
        field.formattedValue==='1,234.50' && field.editValue==='1234.5' && field.formatMessage==='first' &&
        field.execValidate()===false && field.execInitialize()===undefined && field.execEvent('validate')===true
    "#,
    );
    let host = Rc::new(Host {
        runtime: Rc::downgrade(&runtime),
        events: RefCell::new(Vec::new()),
        scheduled: Cell::new(0),
    });
    runtime.set_widget_script_host(Some(host.clone()));
    script(
        &runtime,
        r#"
        field.formattedValue='2,345.60';
        var formatted=field.formattedValue==='2,345.60';
        var previous=field.rawValue;field.formattedValue='bad';
        field.formatMessage='second';field.execInitialize();subform.execCalculate();
        formatted && field.rawValue===previous && field.execValidate()===false && field.execEvent('validate')===false
    "#,
    );
    assert_eq!(host.scheduled.get(), 1);
    assert_eq!(
        *host.events.borrow(),
        [
            (XfaEventType::Initialize, false),
            (XfaEventType::Calculate, true),
            (XfaEventType::Validate, false),
            (XfaEventType::Validate, true)
        ]
    );
    runtime.set_widget_script_host(None);
    script(&runtime, "subform.execValidate()===false");
}
