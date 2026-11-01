// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Native CalcData transfer and executable loadXML dependency continuity.
use super::*;
use crate::xfa::fxfa::{fxfa::XfaEventError, test_support::new_document};
fn node(document: &Rc<CxfaDocument>, element: XfaElement) -> CxfaNode {
    document.create_node(XfaPacketType::Form, element).unwrap()
}
fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
#[test]
fn calc_data_moves_without_cloning_and_absent_source_replaces_existing_destination_data() {
    let document = CxfaDocument::new();
    let source = node(&document, XfaElement::Field);
    let target = node(&document, XfaElement::Field);
    let dependent = node(&document, XfaElement::Field);
    source.add_calculation_dependent(&dependent);
    source.add_calculation_dependent(&dependent);
    assert_eq!(
        source.calculation_dependents(),
        Some(vec![dependent.clone()])
    );
    let clone = source.clone_node(false).unwrap();
    assert!(clone.calculation_dependents().is_none());
    target.set_calc_recursion_count(3);
    assert!(target.take_calculation_data_from(&source));
    assert_eq!(target.calculation_dependents(), Some(vec![dependent]));
    assert!(source.calculation_dependents().is_none());
    assert_eq!(target.get_calc_recursion_count(), 3);
    assert!(target.take_calculation_data_from(&target));
    assert!(target.calculation_dependents().is_some());
    assert!(target.take_calculation_data_from(&source));
    assert!(target.calculation_dependents().is_none());
}
#[test]
fn paired_buffer_move_transfers_matching_classes_and_does_not_copy_attribute_maps() {
    let document = CxfaDocument::new();
    let source = node(&document, XfaElement::Subform);
    let target = node(&document, XfaElement::Subform);
    let source_child = node(&document, XfaElement::Subform);
    let target_child = node(&document, XfaElement::Subform);
    source.insert_child_and_notify(-1, &source_child);
    target.insert_child_and_notify(-1, &target_child);
    source.set_cdata(XfaAttribute::Name, &wide("source"));
    target.set_cdata(XfaAttribute::Name, &wide("target"));
    source.add_calculation_dependent(&source_child);
    source_child.add_calculation_dependent(&target);
    assert!(source.sync_buffer_map_data(&target));
    assert_eq!(
        target.calculation_dependents(),
        Some(vec![source_child.clone()])
    );
    assert_eq!(
        target_child.calculation_dependents(),
        Some(vec![target.clone()])
    );
    assert!(source.calculation_dependents().is_none());
    assert!(source_child.calculation_dependents().is_none());
    assert_eq!(target.get_cdata(XfaAttribute::Name), wide("target"));
    let mismatch = node(&document, XfaElement::Area);
    target_child.add_calculation_dependent(&target);
    assert!(target_child.sync_buffer_map_data(&mismatch));
    assert!(mismatch.calculation_dependents().is_none());
    assert!(target_child.calculation_dependents().is_some());
}
#[test]
fn real_load_xml_replacement_transfers_field_dependencies_and_next_edit_recalculates() {
    let (doc, _) = new_document(
        r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="root"><subform name="replace"><field name="source"><ui><textEdit/></ui><value><text>3</text></value></field></subform><field name="calculated"><ui><textEdit/></ui><calculate><script contentType="application/x-javascript">Number(xfa.form.root.replace.source.rawValue)*2</script></calculate></field></subform></template></xdp:xdp>"#,
    );
    let view = doc.create_doc_view();
    let runtime = doc.script_context();
    doc.model_document()
        .do_data_merge_with_resolver(runtime.resolver())
        .unwrap();
    let root = view.root_subform().unwrap();
    let replace = root.get_first_child_by_name(&wide("replace")).unwrap();
    let original = replace.get_first_child_by_name(&wide("source")).unwrap();
    let calculated = root.get_first_child_by_name(&wide("calculated")).unwrap();
    assert_eq!(calculated.process_calculate(&view), XfaEventError::kSuccess);
    assert_eq!(calculated.get_raw_value(), wide("6"));
    assert_eq!(
        original.calculation_dependents(),
        Some(vec![calculated.clone()])
    );
    runtime.eval(r#"xfa.form.root.replace.loadXML('<subform><field name="source"><ui><textEdit/></ui><value><text>7</text></value></field></subform>',true,true)"#).unwrap();
    let replacement = replace.get_first_child_by_name(&wide("source")).unwrap();
    assert_ne!(replacement, original);
    let raw = replacement
        .get_form_value_if_exists()
        .unwrap()
        .first_child()
        .unwrap()
        .first_child()
        .unwrap();
    assert!(replacement.is_widget_ready());
    assert!(
        raw.is_initialized(),
        "loaded raw content must notify subsequent edits"
    );
    assert_eq!(
        replacement.calculation_dependents(),
        Some(vec![calculated.clone()])
    );
    assert!(original.calculation_dependents().is_none());
    runtime
        .eval("xfa.form.root.replace.source.rawValue='11'")
        .unwrap();
    assert_eq!(view.run_calculate_widgets(), XfaEventError::kSuccess);
    assert_eq!(calculated.get_raw_value(), wide("22"));
    assert_eq!(calculated.get_calc_recursion_count(), 0);
}
#[test]
fn native_todo_node_methods_preserve_noop_arity_without_coercing_arguments() {
    let (doc, _) = new_document(
        r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="root"/></template></xdp:xdp>"#,
    );
    doc.model_document()
        .do_data_merge_with_resolver(doc.script_context().resolver())
        .unwrap();
    let result=doc.script_context().eval(r#"(()=>{
        const node=xfa.form.root;
        let conversions=0;
        const argument={toString(){conversions++;throw Error('coerced')},valueOf(){conversions++;throw Error('coerced')}};
        if(node.applyXSL(argument)!==undefined || node.assignNode(argument,argument,argument)!==undefined || node.setElement(argument,argument)!==undefined || node.saveFilteredXML(argument,argument,argument,argument)!==undefined) return false;
        const invalid=[()=>node.applyXSL(),()=>node.applyXSL(1,2),()=>node.assignNode(),()=>node.assignNode(1,2,3,4),()=>node.setElement(),()=>node.setElement(1,2,3)];
        let errors=0;
        for(const call of invalid){try{call()}catch(e){errors++}}
        return conversions===0 && errors===invalid.length && node.name==='root';
    })()"#).unwrap();
    assert_eq!(result.as_boolean(), Some(true));
}
#[test]
fn actual_ff_readiness_gates_raw_values_before_coercion_and_real_notify_enables_them() {
    use boa_engine::{js_string, property::Attribute};
    let (doc, _) = new_document(
        r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="root"/></template></xdp:xdp>"#,
    );
    doc.create_doc_view();
    let runtime = doc.script_context();
    let field = node(&doc.model_document(), XfaElement::Field);
    let group = node(&doc.model_document(), XfaElement::ExclGroup);
    runtime
        .with_script_context(|context| {
            let field_binding = runtime.get_or_create_binding(&field, context)?;
            let group_binding = runtime.get_or_create_binding(&group, context)?;
            context.register_global_property(
                js_string!("unreadyField"),
                field_binding,
                Attribute::all(),
            )?;
            context.register_global_property(
                js_string!("unreadyGroup"),
                group_binding,
                Attribute::all(),
            )?;
            Ok(())
        })
        .unwrap();
    assert!(!field.is_widget_ready() && !group.is_widget_ready());
    assert_eq!(runtime.eval(r#"(()=>{let calls=0;const value={toString(){calls++;throw Error('coerced')}};unreadyField.rawValue=value;unreadyGroup.rawValue=value;return calls===0&&unreadyField.rawValue===undefined&&unreadyGroup.rawValue===undefined})()"#).unwrap().as_boolean(),Some(true));
    assert!(field.get_form_value_if_exists().is_none());
    field.set_initialized_flag_and_notify();
    assert!(field.is_widget_ready());
    assert_eq!(
        runtime
            .eval("unreadyField.rawValue='ready';unreadyField.rawValue==='ready'")
            .unwrap()
            .as_boolean(),
        Some(true)
    );
}
