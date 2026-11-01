// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Source-derived node FormCalc behavior through the real compiler and Boa,
//! using parsed XDP, real data merge, cached node bindings and XML mappings.
use super::*;
use crate::{
    fxcrt::{cfx_read_only_span_stream::CfxReadOnlySpanStream, xml::cfx_xmlparser::CfxXmlParser},
    fxjs::xfa::cfxjse_formcalc_context::CfxJseFormcalcContext,
    xfa::fxfa::{
        fxfa_basic::{XfaHashCode, XfaPacketType},
        parser::{
            cxfa_document::{CxfaDocument, XfaDocumentNotify},
            cxfa_document_builder::CxfaDocumentBuilder,
            cxfa_model_properties::CxfaScriptType,
            cxfa_node::CxfaNode,
        },
    },
};
use std::{
    cell::{Cell, RefCell},
    rc::Weak,
};
fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
fn named(node: &CxfaNode, name: &str) -> CxfaNode {
    node.get_first_child_by_name(&wide(name)).unwrap()
}
fn fixture() -> (Rc<CxfaDocument>, Rc<CfxJseEngine>, CxfaNode) {
    let input=br#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
      <template xmlns="http://www.xfa.org/schema/xfa-template/2.8/"><subform name="record">
        <field name="amount"><ui><numericEdit/></ui><value><integer/></value><bind match="dataRef" ref="$record.amount"/></field>
        <field name="mirror"><ui><numericEdit/></ui><value><integer/></value><bind match="dataRef" ref="$record.amount"/></field>
        <field name="label"><ui><textEdit/></ui></field>
        <field name="empty"><ui><textEdit/></ui></field>
        <field name="a.b"><ui><textEdit/></ui><value><text>dotted</text></value></field>
        <subform name="row"><occur min="0" max="-1" initial="0"/><field name="item"><ui><numericEdit/></ui><value><integer/></value></field></subform>
      </subform></template>
      <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"><xfa:data><record>
        <amount>12</amount><label>hello</label><row><item>10</item></row><row><item>20</item></row>
      </record></xfa:data></xfa:datasets>
    </xdp:xdp>"#;
    let mut bytes = input.to_vec();
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
    let record = named(&form, "record");
    (document, runtime, record)
}
fn execute(runtime: &CfxJseEngine, node: &CxfaNode, source: &str) -> JsValue {
    runtime
        .run_script(CxfaScriptType::Formcalc, &wide(source), Some(node))
        .unwrap_or_else(|error| panic!("{source}: {error}"))
}
fn check(runtime: &CfxJseEngine, node: &CxfaNode, source: &str, expected: f64) {
    assert_eq!(
        execute(runtime, node, source).as_number(),
        Some(expected),
        "{source}"
    );
}

#[test]
fn compiler_node_defaults_arithmetic_aggregates_and_references() {
    let (_document, runtime, record) = fixture();
    for (source, expected) in [
        ("this.amount", 12.0),
        ("this.amount + this.mirror", 24.0),
        ("Sum(this.row[*].item)", 30.0),
        ("Count(this.row[*].item)", 2.0),
        ("Avg(this.row[*].item)", 15.0),
        ("this.row[1].item", 20.0),
        ("this..item", 10.0),
        ("this.amount eq this.mirror", 1.0),
    ] {
        check(&runtime, &record, source, expected);
    }
    assert_eq!(
        execute(&runtime, &record, "this.label")
            .as_string()
            .unwrap(),
        JsString::from("hello")
    );
    assert!(execute(&runtime, &record, "this.empty").is_null());
    assert_eq!(
        execute(&runtime, &record, "Choose(2, this.row[*].item)")
            .as_string()
            .unwrap(),
        JsString::from("20")
    );
    assert_eq!(
        execute(&runtime, &record, "Ref(this.amount) eq Ref(this.amount)").as_number(),
        Some(1.0)
    );
    let amount = named(&record, "amount");
    check(&runtime, &amount, "this", 12.0);
}
#[test]
fn compiler_assignments_update_shared_fields_dataset_and_xml() {
    let (document, runtime, record) = fixture();
    check(&runtime, &record, "this.amount = 41 this.mirror + 1", 42.0);
    let amount = named(&record, "amount");
    let mirror = named(&record, "mirror");
    let data = amount.get_bind_data().unwrap();
    assert_eq!(amount.get_content(), wide("41"));
    assert_eq!(mirror.get_content(), wide("41"));
    assert_eq!(data.get_content(), wide("41"));
    assert_eq!(data.get_xml_mapping_node().unwrap().text_data(), wide("41"));
    check(
        &runtime,
        &record,
        "this.row[*].item = 7 Sum(this.row[*].item)",
        14.0,
    );
    assert_eq!(
        document
            .get_xfa_node(XfaHashCode::Record as u32)
            .unwrap()
            .get_first_child_by_name(&wide("amount")),
        Some(data)
    );
    assert!(execute(&runtime, &record, "this.amount = null this.mirror").is_null());
    assert!(amount.is_null());
    assert!(mirror.get_content().is_empty());
}
#[test]
fn indexed_current_node_object_attributes_and_lazy_scalar_attributes() {
    let (_document, runtime, record) = fixture();
    let row = named(&record, "row");
    let second = row.get_next_same_name_sibling(row.get_name_hash()).unwrap();
    // Native ResolveProcessor recognizes `this` before applying conditions.
    // Named sibling access and an indexed object expansion use relative indexes.
    check(&runtime, &second, "this[-1].item", 20.0);
    check(&runtime, &row, "this[+1].item", 10.0);
    check(&runtime, &second, "row[-1].item", 10.0);
    check(&runtime, &row, "row[+1].item", 20.0);
    check(&runtime, &second, "Ref(this)[-1].item", 10.0);
    check(&runtime, &row, "Ref(this)[+1].item", 20.0);
    check(&runtime, &named(&row, "item"), "this.parent.item", 10.0);
    assert_eq!(
        execute(&runtime, &record, "this.amount.name")
            .as_string()
            .unwrap(),
        JsString::from("amount")
    );
    assert_eq!(
        execute(
            &runtime,
            &record,
            "this.amount.name = \"renamed\" this.renamed.name"
        )
        .as_string()
        .unwrap(),
        JsString::from("renamed")
    );
    let dotted = named(&record, "a.b");
    assert_eq!(
        execute(&runtime, &dotted, "Ref(this)[0]")
            .as_string()
            .unwrap(),
        JsString::from("dotted")
    );
    let runtime_clone = runtime.clone();
    let identity = runtime
        .javascript()
        .with_context(|context| {
            let binding = runtime_clone.get_or_create_binding(&row, context)?;
            let expanded = dot_accessor(
                &JsValue::undefined(),
                &[
                    binding.clone().into(),
                    JsString::from("this").into(),
                    JsString::from("parent").into(),
                    0.into(),
                    0.into(),
                ],
                context,
            )?;
            let array = JsArray::from_object(expanded.as_object().unwrap())?;
            Ok(array.get(1, context)?.is_null()
                && array.get(2, context)?
                    == runtime_clone
                        .get_or_create_binding(&record, context)?
                        .into())
        })
        .unwrap();
    assert!(identity);
}
#[test]
fn raw_accessor_array_shapes_named_fallback_and_expansion_order() {
    let (_document, runtime, record) = fixture();
    // Run JavaScript with the same explicit this node to inspect the expansion
    // protocol emitted by the actual FormCalc compiler.
    let source="var a=pfm_rt.dot_acc(this,'this','row',1); var b=pfm_rt.dot_acc(a,'','item',0,0); a[0]===1&&a[1]===null&&a.length===4&&b.length===4&&pfm_rt.Sum(b)===30&&pfm_rt.get_val(pfm_rt.dot_acc(0,'row','item',0,0))===10;";
    assert_eq!(
        runtime
            .run_script(CxfaScriptType::Javascript, &wide(source), Some(&record))
            .unwrap()
            .as_boolean(),
        Some(true)
    );
    let source="var a=pfm_rt.dot_acc(this,'this','row',1); var n=pfm_rt.dot_acc(a,'','name',0,0); n[1]==='name'&&n.length===4&&pfm_rt.Choose(2,n)==='row';";
    assert_eq!(
        runtime
            .run_script(CxfaScriptType::Javascript, &wide(source), Some(&record))
            .unwrap()
            .as_boolean(),
        Some(true)
    );
}
#[test]
fn opaque_getter_and_default_write_exceptions_recover_without_stringifying() {
    let (_document, runtime, record) = fixture();
    let source="var marker={id:73}; var field=this.amount; var thrown={toString(){throw marker}}; var a=false,b=false,c=false; try{pfm_rt.dot_acc(this,'this',thrown,0,0)}catch(e){a=e===marker} try{pfm_rt.asgn_val_op(field,[1,'v',{v:thrown}])}catch(e){b=e===marker} try{pfm_rt.get_val([1,'v',{get v(){throw marker}}])}catch(e){c=e===marker} a&&b&&c&&field.rawValue===12;";
    assert_eq!(
        runtime
            .run_script(CxfaScriptType::Javascript, &wide(source), Some(&record))
            .unwrap()
            .as_boolean(),
        Some(true)
    );
    check(&runtime, &record, "Sum(this.row[*].item)", 30.0);
}
#[test]
fn access_errors_arity_and_standalone_ordinary_objects_keep_their_contract() {
    let (_document, runtime, record) = fixture();
    for source in ["this.missing.child", "this.row[9].item"] {
        assert!(
            runtime
                .run_script(CxfaScriptType::Formcalc, &wide(source), Some(&record))
                .is_err(),
            "{source}"
        );
    }
    let source="[()=>pfm_rt.dot_acc(),()=>pfm_rt.dotdot_acc(this,'this','item'),()=>pfm_rt.dot_acc([], '', 'item',0,0),()=>pfm_rt.dot_acc({}, '', 'amount',0,0)].every(f=>{try{f();return false}catch(e){return e instanceof Error}});";
    assert_eq!(
        runtime
            .run_script(CxfaScriptType::Javascript, &wide(source), Some(&record))
            .unwrap()
            .as_boolean(),
        Some(true)
    );
    let scalar = CfxJseFormcalcContext::new().unwrap();
    assert!(scalar
        .engine()
        .eval(
            &"pfm_rt.get_val({valueOf(){throw 1}})===null"
                .encode_utf16()
                .collect::<Vec<_>>()
        )
        .unwrap()
        .as_boolean()
        .unwrap());
    assert!(scalar
        .engine()
        .eval(
            &"pfm_rt.dot_acc(null,'','field',0,0)"
                .encode_utf16()
                .collect::<Vec<_>>()
        )
        .is_err());
    check(&runtime, &record, "this.amount + 1", 13.0);
}
struct Reenter {
    runtime: Weak<CfxJseEngine>,
    record: CxfaNode,
    entered: Cell<bool>,
    values: RefCell<Vec<f64>>,
}
impl XfaDocumentNotify for Reenter {
    fn on_child_added(&self, _: &CxfaNode, _: &CxfaNode) {}
    fn on_child_removed(&self, _: &CxfaNode, _: &CxfaNode) {}
    fn on_node_ready(&self, _: &CxfaNode) {}
    fn on_attribute_changed(&self, node: &CxfaNode, attribute: XfaAttribute) {
        if attribute != XfaAttribute::Value
            || node.get_packet_type() != XfaPacketType::Datasets
            || self.entered.replace(true)
        {
            return;
        }
        let runtime = self.runtime.upgrade().unwrap();
        let value = runtime
            .run_script(
                CxfaScriptType::Formcalc,
                &wide("Sum(this.row[*].item)"),
                Some(&self.record),
            )
            .unwrap();
        self.values.borrow_mut().push(value.as_number().unwrap());
    }
}
#[test]
fn node_assignment_notification_reenters_formcalc_on_the_same_context() {
    let (document, runtime, record) = fixture();
    let notify = Rc::new(Reenter {
        runtime: Rc::downgrade(&runtime),
        record: record.clone(),
        entered: Cell::new(false),
        values: RefCell::new(Vec::new()),
    });
    document.set_notify(Some(notify.clone()));
    check(&runtime, &record, "this.amount = 9 this.mirror + 1", 10.0);
    assert_eq!(*notify.values.borrow(), [30.0]);
    assert_eq!(runtime.current_this_node(), None);
    check(&runtime, &record, "this.amount", 9.0);
}
