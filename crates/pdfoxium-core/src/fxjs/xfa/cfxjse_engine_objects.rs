// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Non-node Object-derived CJX identity/attribute/script adapter. All wrappers
//! retain weak engine links; these cached bindings are shared with SOM lookups.
use super::super::cfxjse_resolveprocessor::{
    ObjectResolveResult, PseudoModel, PseudoObject, ResolvedObject,
};
use super::*;
use crate::xfa::fxfa::{fxfa_basic::XfaAttribute, parser::xfa_basic_data::attribute_to_name};
use boa_engine::property::PropertyKey;
#[derive(Clone)]
pub(super) struct PseudoBinding {
    pub(super) object: JsObject,
    attributes: HashMap<XfaAttribute, (Option<JsObject>, Option<JsObject>)>,
}
impl CfxJseEngine {
    pub fn current_this_object(&self) -> Option<ResolvedObject> {
        self.frames
            .borrow()
            .last()
            .and_then(|frame| frame.object.clone())
    }
    pub fn resolved_object_from_value(&self, value: &JsValue) -> Option<ResolvedObject> {
        if let Some(node) = self.node_from_value(value) {
            return Some(ResolvedObject::Node(node));
        }
        let object = value.as_object()?;
        self.pseudo_bindings
            .borrow()
            .iter()
            .find(|(_, binding)| binding.object == object)
            .map(|(&kind, _)| ResolvedObject::Pseudo(PseudoObject::new(&self.document, kind)))
    }
    pub fn get_or_create_pseudo_binding(
        &self,
        kind: PseudoModel,
        context: &mut Context,
    ) -> JsResult<JsObject> {
        if !Self::from_context(context).is_some_and(|runtime| std::ptr::eq(runtime.as_ref(), self))
        {
            return Err(JsNativeError::typ()
                .with_message("XFA binding requires its document's JavaScript context")
                .into());
        }
        if let Some(binding) = self.pseudo_bindings.borrow().get(&kind) {
            return Ok(binding.object.clone());
        }
        let runtime = self
            .weak_self
            .upgrade()
            .ok_or_else(|| JsNativeError::reference().with_message("XFA context has expired"))?;
        let mut realm = contexts::RealmScope::new(context, self.main_realm.clone());
        let context = realm.context();
        let object = match kind {
            PseudoModel::DataWindow => {
                super::super::cjx_datawindow::datawindow_object(&runtime, context)?
            }
            PseudoModel::Event => {
                super::super::cjx_eventpseudomodel::event_object(&runtime, context)?
            }
            PseudoModel::Host => super::super::cjx_hostpseudomodel::host_object(&runtime, context)?,
            PseudoModel::Log => super::super::cjx_logpseudomodel::log_object(&runtime, context)?,
            PseudoModel::Signature => {
                super::super::cjx_signaturepseudomodel::signature_object(&runtime, context)?
            }
            PseudoModel::Layout => {
                super::super::cjx_layoutpseudomodel::layout_object(&runtime, context)?
            }
        };
        // Keep the native callbacks independently of later JavaScript property
        // shadowing: ResolveForAttributeRs invokes CJX's source callback.
        let mut attributes = HashMap::new();
        for key in object.own_property_keys(context)? {
            let PropertyKey::String(name) = key else {
                continue;
            };
            let Some(attribute) =
                find_script_attribute_descriptor(kind.element(), &wide_from_utf16(&name.to_vec()))
            else {
                continue;
            };
            let descriptor = self.reflection.get_descriptor(&object, &name, context)?;
            let Some(descriptor) = descriptor.as_object() else {
                continue;
            };
            let getter = descriptor
                .get(JsString::from("get"), context)?
                .as_callable();
            let setter = descriptor
                .get(JsString::from("set"), context)?
                .as_callable();
            attributes.insert(attribute.attribute, (getter, setter));
        }
        self.pseudo_bindings.borrow_mut().insert(
            kind,
            PseudoBinding {
                object: object.clone(),
                attributes,
            },
        );
        Ok(object)
    }
    pub fn get_or_create_object_binding(
        &self,
        object: &ResolvedObject,
        context: &mut Context,
    ) -> JsResult<JsObject> {
        self.resolver
            .check_object_reference(object)
            .map_err(resolve_error)?;
        match object {
            ResolvedObject::Node(node) => self.get_or_create_binding(node, context),
            ResolvedObject::Pseudo(object) => {
                self.get_or_create_pseudo_binding(object.kind(), context)
            }
        }
    }
    pub fn read_resolved_attribute(
        &self,
        object: &ResolvedObject,
        descriptor: ScriptAttributeDescriptor,
        context: &mut Context,
    ) -> JsResult<JsValue> {
        self.resolver
            .check_object_reference(object)
            .map_err(resolve_error)?;
        if let ResolvedObject::Node(node) = object {
            return self.read_script_attribute(node, descriptor, context);
        }
        let ResolvedObject::Pseudo(object) = object else {
            unreachable!()
        };
        let binding = self.get_or_create_pseudo_binding(object.kind(), context)?;
        let getter = self
            .pseudo_bindings
            .borrow()
            .get(&object.kind())
            .and_then(|binding| binding.attributes.get(&descriptor.attribute))
            .and_then(|callbacks| callbacks.0.clone())
            .ok_or_else(|| {
                JsNativeError::typ().with_message(format!(
                    "Missing XFA pseudo attribute {}",
                    attribute_to_name(descriptor.attribute)
                ))
            })?;
        getter.call(&binding.into(), &[], context)
    }
    pub fn write_resolved_attribute(
        &self,
        object: &ResolvedObject,
        descriptor: ScriptAttributeDescriptor,
        value: &JsValue,
        context: &mut Context,
    ) -> JsResult<()> {
        self.resolver
            .check_object_reference(object)
            .map_err(resolve_error)?;
        if let ResolvedObject::Node(node) = object {
            return self.write_script_attribute(node, descriptor, value, context);
        }
        let ResolvedObject::Pseudo(object) = object else {
            unreachable!()
        };
        let binding = self.get_or_create_pseudo_binding(object.kind(), context)?;
        let setter = self
            .pseudo_bindings
            .borrow()
            .get(&object.kind())
            .and_then(|binding| binding.attributes.get(&descriptor.attribute))
            .and_then(|callbacks| callbacks.1.clone())
            .ok_or_else(|| JsNativeError::typ().with_message("Invalid property set operation."))?;
        setter.call(&binding.into(), std::slice::from_ref(value), context)?;
        Ok(())
    }
    pub fn resolve_object_values(
        &self,
        reference: Option<&CxfaNode>,
        expression: &[u32],
        flags: XfaResolveFlags,
    ) -> Result<Option<ObjectResolveResult>, ResolveError> {
        self.resolving.set(self.resolving.get() + 1);
        let _scope = ResolveScope(&self.resolving);
        self.resolver
            .resolve_object_values(reference, expression, flags)
    }
    pub fn resolve_object_values_from(
        &self,
        reference: Option<&ResolvedObject>,
        expression: &[u32],
        flags: XfaResolveFlags,
    ) -> Result<Option<ObjectResolveResult>, ResolveError> {
        self.resolving.set(self.resolving.get() + 1);
        let _scope = ResolveScope(&self.resolving);
        self.resolver
            .resolve_object_values_from(reference, expression, flags)
    }
    pub(super) fn result_object_value(
        &self,
        result: ObjectResolveResult,
        context: &mut Context,
    ) -> JsResult<JsValue> {
        let Some(object) = result.objects.first() else {
            return Ok(JsValue::undefined());
        };
        if result.result_type == ResolveResultType::Attribute {
            let attribute = result
                .script_attribute
                .ok_or_else(|| JsNativeError::typ().with_message("Missing script attribute"))?;
            self.read_resolved_attribute(object, attribute, context)
        } else {
            Ok(self.get_or_create_object_binding(object, context)?.into())
        }
    }
}

impl CfxJseEngine {
    pub fn run_script_object(
        &self,
        kind: CxfaScriptType,
        source: &[u32],
        this_object: Option<&ResolvedObject>,
    ) -> JsResult<JsValue> {
        if let Some(object) = this_object {
            self.resolver
                .check_object_reference(object)
                .map_err(resolve_error)?;
        }
        if kind == CxfaScriptType::Unknown {
            return Err(JsNativeError::syntax()
                .with_message("Unknown XFA script language")
                .into());
        }
        let translated;
        let source = if kind == CxfaScriptType::Formcalc {
            translated = translate(source)
                .ok_or_else(|| JsNativeError::syntax().with_message("Invalid FormCalc program"))?;
            translated.as_slice()
        } else {
            source
        };
        self.frames.borrow_mut().push(ScriptFrame {
            node: this_object.and_then(ResolvedObject::as_node).cloned(),
            object: this_object.cloned(),
            kind,
        });
        let _scope = FrameScope(self);
        self.javascript.with_context(|context| {
            let mut realm = contexts::RealmScope::new(context, self.main_realm.clone());
            let context = realm.context();
            let sdk = crate::fxjs::cjs_runtime::CjsRuntime::from_context(context);
            let mut execute = || {
                let source = JsString::from(utf16_encode(source).as_slice());
                if let Some(object) = this_object {
                    let this = self.get_or_create_object_binding(object, context)?;
                    // Same eval wrapper as CFXJSE_Context::ExecuteScript; completion
                    // values, declarations and the actual `this` obey Boa semantics.
                    let wrapper = context.eval(Source::from_bytes(
                        "(function () { return eval(arguments[0]); })",
                    ))?;
                    wrapper.as_callable().expect("native eval wrapper").call(
                        &this.into(),
                        &[source.into()],
                        context,
                    )
                } else {
                    context.eval(Source::from_utf16(&source.to_vec()))
                }
            };
            if let Some(sdk) = sdk {
                sdk.with_event_context(execute)
            } else {
                execute()
            }
        })
    }
}
