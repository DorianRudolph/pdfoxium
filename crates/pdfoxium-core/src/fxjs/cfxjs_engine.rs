// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Execution/global/property/constant-array portions of
//! `fxjs/cfxjs_engine.{h,cpp}` and the connected CFX_IsolateWrapper helpers.
//! Boa supplies the language implementation; PDF host object algorithms remain
//! in the CJS modules. Object-definition/template/XFA branches remain open.
//!
//! A native callback may temporarily lend the actual context back to its
//! engine while calling the PDF core or embedder. Nested public calls can then
//! execute synchronously on the same realm/VM. A pooled inert context occupies
//! the suspended Rust borrow, and a guard restores the actual context before
//! Boa resumes. No context/object graph is cloned and no RefCell borrow spans
//! JavaScript execution or a callback.
use boa_engine::{
    native_function::NativeFunctionPointer,
    object::{builtins::JsArray, FunctionObjectBuilder},
    property::PropertyDescriptor,
    Context, JsNativeError, JsObject, JsResult, JsString, JsValue, NativeFunction, Source,
};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::{Rc, Weak},
};

struct EngineLink(Weak<CfxJsEngine>);

pub struct CfxJsEngine {
    context: RefCell<Option<Context>>,
    parked_contexts: RefCell<Vec<Context>>,
    const_arrays: RefCell<BTreeMap<Vec<u16>, JsArray>>,
}

impl CfxJsEngine {
    /// `context` already contains the document's host data and object bindings.
    pub fn new(mut context: Context) -> Rc<Self> {
        Rc::new_cyclic(|weak| {
            context.insert_data(EngineLink(weak.clone()));
            Self {
                context: RefCell::new(Some(context)),
                parked_contexts: RefCell::new(Vec::new()),
                const_arrays: RefCell::new(BTreeMap::new()),
            }
        })
    }

    pub fn from_context(context: &Context) -> Option<Rc<Self>> {
        context.get_data::<EngineLink>()?.0.upgrade()
    }

    /// `CFXJS_Engine::Execute`: compile and run in the existing document realm.
    /// Keep the original thrown value until the runtime's outer error boundary.
    pub fn eval(&self, source: &[u16]) -> JsResult<JsValue> {
        self.with_context(|context| context.eval(Source::from_utf16(source)))
    }

    pub fn with_context<R>(
        &self,
        operation: impl FnOnce(&mut Context) -> JsResult<R>,
    ) -> JsResult<R> {
        let context = self.context.borrow_mut().take().ok_or_else(|| {
            JsNativeError::error()
                .with_message("JavaScript context is active outside a host callback")
        })?;
        let mut lease = ContextLease {
            engine: self,
            context: Some(context),
        };
        operation(lease.context.as_mut().expect("context lease"))
    }

    /// Run PDF-core or embedder work from a native Boa callback. The callback
    /// does not receive a context: ordinary notifier/C ABI paths may reenter
    /// through `eval` or `with_context`, using the temporarily available VM.
    ///
    /// This requires a context created by `CfxJsEngine::new`. The engine link
    /// is weak so bindings and document/runtime owners cannot form a cycle.
    pub fn with_host_callback<R>(context: &mut Context, operation: impl FnOnce() -> R) -> R {
        let engine = Self::from_context(context).expect("native callback has a live engine");
        let placeholder = engine
            .parked_contexts
            .borrow_mut()
            .pop()
            .unwrap_or_default();
        let active = std::mem::replace(context, placeholder);
        assert!(engine.context.borrow_mut().replace(active).is_none());
        let restoration = CallbackContext {
            engine: &engine,
            destination: context,
        };
        let result = operation();
        drop(restoration);
        result
    }

    /// `GetThisObj` returns the document's persistent global object.
    pub fn get_this_obj(&self) -> JsResult<JsObject> {
        self.with_context(|context| Ok(context.global_object()))
    }

    /// `DefineGlobalMethod` installs a read-only callable without a constructor
    /// prototype, retaining the native template's enumerable/configurable bits.
    pub fn define_global_method(
        &self,
        name: &[u16],
        length: usize,
        callback: NativeFunctionPointer,
    ) -> JsResult<()> {
        self.with_context(|context| {
            let name = JsString::from(name);
            let function =
                FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(callback))
                    .name(name.clone())
                    .length(length)
                    .constructor(false)
                    .build();
            context.global_object().define_property_or_throw(
                name,
                PropertyDescriptor::builder()
                    .value(function)
                    .writable(false)
                    .enumerable(true)
                    .configurable(true),
                context,
            )?;
            Ok(())
        })
    }

    /// `DefineGlobalConst` uses a getter, as the source template does.
    pub fn define_global_const(&self, name: &[u16], getter: NativeFunctionPointer) -> JsResult<()> {
        self.with_context(|context| {
            let name = JsString::from(name);
            let function =
                FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(getter))
                    .name(name.clone())
                    .length(0)
                    .constructor(false)
                    .build();
            context.global_object().define_property_or_throw(
                name,
                PropertyDescriptor::builder()
                    .get(function)
                    .enumerable(true)
                    .configurable(true),
                context,
            )?;
            Ok(())
        })
    }

    pub fn get_object_property_reentrant(
        &self,
        object: &JsObject,
        name: &[u16],
    ) -> JsResult<JsValue> {
        self.with_context(|context| object.get(JsString::from(name), context))
    }

    pub fn put_object_property_reentrant(
        &self,
        object: &JsObject,
        name: &[u16],
        value: JsValue,
    ) -> JsResult<bool> {
        self.with_context(|context| object.set(JsString::from(name), value, true, context))
    }

    pub fn get_const_array(&self, name: &[u16]) -> Option<JsArray> {
        self.const_arrays.borrow().get(name).cloned()
    }

    pub fn set_const_array(&self, name: &[u16], array: JsArray) {
        let old = self.const_arrays.borrow_mut().insert(name.to_vec(), array);
        drop(old);
    }
}

struct ContextLease<'a> {
    engine: &'a CfxJsEngine,
    context: Option<Context>,
}
impl Drop for ContextLease<'_> {
    fn drop(&mut self) {
        let context = self.context.take().expect("context lease is restored once");
        assert!(self.engine.context.borrow_mut().replace(context).is_none());
    }
}
struct CallbackContext<'a> {
    engine: &'a CfxJsEngine,
    destination: &'a mut Context,
}
impl Drop for CallbackContext<'_> {
    fn drop(&mut self) {
        let active = self
            .engine
            .context
            .borrow_mut()
            .take()
            .expect("nested execution returned its context");
        let placeholder = std::mem::replace(self.destination, active);
        self.engine.parked_contexts.borrow_mut().push(placeholder);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use boa_engine::js_string;
    use std::cell::Cell;

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().collect()
    }
    fn recurse(_: &JsValue, arguments: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        let depth = arguments[0].to_number(context)? as u32;
        let engine = CfxJsEngine::from_context(context).unwrap();
        CfxJsEngine::with_host_callback(context, || {
            boa_gc::force_collect();
            let result = engine.eval(&wide(&format!("recursive({});", depth - 1)));
            boa_gc::force_collect();
            result
        })
    }
    fn throw_nested(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
        let engine = CfxJsEngine::from_context(context).unwrap();
        CfxJsEngine::with_host_callback(context, || {
            engine.eval(&wide("throw { marker: 'nested', values: [1,2,3] };"))
        })
    }
    fn retain_argument(
        _: &JsValue,
        arguments: &[JsValue],
        context: &mut Context,
    ) -> JsResult<JsValue> {
        let object = arguments[0].as_object().unwrap().clone();
        let engine = CfxJsEngine::from_context(context).unwrap();
        CfxJsEngine::with_host_callback(context, || {
            boa_gc::force_collect();
            engine.eval(&wide("for (let i=0;i<1000;i++) { ({temporary:i}); }"))?;
            boa_gc::force_collect();
            Ok::<_, boa_engine::JsError>(())
        })?;
        object.get(js_string!("n"), context)
    }
    fn collect(_: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
        boa_gc::force_collect();
        Ok(JsValue::undefined())
    }
    fn engine() -> Rc<CfxJsEngine> {
        let engine = CfxJsEngine::new(Context::default());
        for (name, callback) in [
            ("reenter", recurse as NativeFunctionPointer),
            ("throwNested", throw_nested),
            ("retainArgument", retain_argument),
            ("collect", collect),
        ] {
            engine
                .define_global_method(&wide(name), 1, callback)
                .unwrap();
        }
        engine
    }

    #[test]
    fn synchronous_reentry_restores_vm_frames_gc_roots_and_opaque_exceptions() {
        let engine = engine();
        assert_eq!(engine.eval(&wide("function recursive(n) { collect(); if(n===0)return 10; return reenter(n)+1; } recursive(4);")).unwrap().as_number(), Some(14.0));
        assert_eq!(engine.parked_contexts.borrow().len(), 4);
        for _ in 0..3 {
            assert_eq!(
                engine.eval(&wide("recursive(4);")).unwrap().as_number(),
                Some(14.0)
            );
        }
        assert_eq!(engine.parked_contexts.borrow().len(), 4);
        assert_eq!(engine.eval(&wide("function localRoot(){ let root={n:17}; return retainArgument(root)+root.n; } localRoot();")).unwrap().as_number(), Some(34.0));
        assert_eq!(engine.eval(&wide("try{throwNested()}catch(e){collect(); e.marker==='nested' && e.values.join(',')==='1,2,3';}")).unwrap().as_boolean(), Some(true));
        assert!(engine.eval(&wide("throwNested();")).is_err());
        assert_eq!(engine.eval(&wide("40+2;")).unwrap().as_number(), Some(42.0));
    }

    #[test]
    fn utf16_properties_and_read_only_global_callable_keep_language_semantics() {
        let engine = engine();
        let source = [b'\'' as u16, 0xd800, 0, 0xd834, 0xdd1e, b'\'' as u16];
        let string = engine.eval(&source).unwrap().as_string().unwrap().clone();
        assert_eq!(
            string.iter().collect::<Vec<_>>(),
            vec![0xd800, 0, 0xd834, 0xdd1e]
        );
        let global = engine.get_this_obj().unwrap();
        assert!(engine
            .put_object_property_reentrant(&global, &wide("value"), 12.into())
            .unwrap());
        assert_eq!(
            engine
                .get_object_property_reentrant(&global, &wide("value"))
                .unwrap()
                .as_number(),
            Some(12.0)
        );
        assert_eq!(engine.eval(&wide("var descriptor=Object.getOwnPropertyDescriptor(globalThis,'reenter'); !descriptor.writable && descriptor.enumerable && !('prototype' in reenter);")).unwrap().as_boolean(), Some(true));
    }

    #[test]
    fn callback_restoration_runs_during_rust_unwinding_without_stranding_context() {
        let engine = engine();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _: JsResult<()> = engine.with_context(|context| {
                CfxJsEngine::with_host_callback::<JsResult<()>>(context, || {
                    panic!("host callback unwinds")
                })
            });
        }));
        assert!(result.is_err());
        assert_eq!(engine.eval(&wide("6*7;")).unwrap().as_number(), Some(42.0));
    }

    #[test]
    fn callback_can_invalidate_host_while_active_context_finishes() {
        let alive = Rc::new(Cell::new(true));
        let mut context = Context::default();
        context.insert_data(alive.clone());
        fn invalidate(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
            let alive = context.get_data::<Rc<Cell<bool>>>().unwrap().clone();
            let engine = CfxJsEngine::from_context(context).unwrap();
            CfxJsEngine::with_host_callback(context, || {
                let result = engine.eval(&wide("41;"));
                alive.set(false);
                boa_gc::force_collect();
                result
            })
        }
        let engine = CfxJsEngine::new(context);
        engine
            .define_global_method(&wide("invalidate"), 0, invalidate)
            .unwrap();
        assert_eq!(
            engine.eval(&wide("invalidate()+1;")).unwrap().as_number(),
            Some(42.0)
        );
        assert!(!alive.get());
        assert!(engine.context.borrow().is_some());
    }
    #[test]
    fn global_constant_getters_stay_live_readonly_and_propagate_original_exceptions() {
        fn next(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
            let count = context.get_data::<Rc<Cell<i32>>>().unwrap();
            count.set(count.get() + 1);
            Ok(count.get().into())
        }
        fn fail(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
            Err(boa_engine::JsError::from_opaque(
                context.global_object().get(js_string!("marker"), context)?,
            ))
        }
        let count = Rc::new(Cell::new(0));
        let mut context = Context::default();
        context.insert_data(count.clone());
        let engine = CfxJsEngine::new(context);
        engine
            .define_global_const(&wide("liveConstant"), next)
            .unwrap();
        engine
            .define_global_const(&wide("throwingConstant"), fail)
            .unwrap();
        assert_eq!(
            engine
                .eval(&wide("JSON.stringify([liveConstant,liveConstant])"))
                .unwrap()
                .as_string()
                .unwrap(),
            js_string!("[1,2]")
        );
        assert_eq!(engine.eval(&wide("var d=Object.getOwnPropertyDescriptor(globalThis,'liveConstant');d.enumerable&&d.configurable&&d.set===undefined&&d.get.length===0&&!('prototype' in d.get)")).unwrap().as_boolean(), Some(true));
        assert!(engine.eval(&wide("'use strict';liveConstant=9;")).is_err());
        assert_eq!(count.get(), 2);
        count.set(40);
        assert_eq!(
            engine.eval(&wide("liveConstant")).unwrap().as_number(),
            Some(41.0)
        );
        assert_eq!(
            engine
                .eval(&wide(
                    "var marker={value:42};try{throwingConstant}catch(e){e===marker}"
                ))
                .unwrap()
                .as_boolean(),
            Some(true)
        );
        assert_eq!(
            engine.eval(&wide("liveConstant")).unwrap().as_number(),
            Some(42.0)
        );
    }

    #[test]
    fn constant_array_cache_preserves_identity_roots_and_replacement_ownership() {
        let engine = engine();
        let name = [b'A' as u16, 0, 0xd800];
        assert!(engine.get_const_array(&name).is_none());
        let first = engine
            .with_context(|context| {
                Ok(JsArray::from_iter(
                    [JsValue::from(1), JsValue::from(2)],
                    context,
                ))
            })
            .unwrap();
        engine.set_const_array(&name, first.clone());
        let retained = engine.get_const_array(&name).unwrap();
        assert!(JsObject::equals(&first, &retained));
        drop(first);
        boa_gc::force_collect();
        engine
            .with_context(|context| {
                retained.set(0u32, JsValue::from(17), true, context)?;
                assert_eq!(
                    engine
                        .get_const_array(&name)
                        .unwrap()
                        .get(0u32, context)?
                        .as_number(),
                    Some(17.0)
                );
                Ok(())
            })
            .unwrap();
        let second = engine
            .with_context(|context| Ok(JsArray::from_iter([JsValue::from(42)], context)))
            .unwrap();
        engine.set_const_array(&name, second.clone());
        assert!(JsObject::equals(
            &second,
            &engine.get_const_array(&name).unwrap()
        ));
        assert!(!JsObject::equals(&retained, &second));
        drop(second);
        boa_gc::force_collect();
        engine
            .with_context(|context| {
                assert_eq!(retained.get(0u32, context)?.as_number(), Some(17.0));
                let current = engine.get_const_array(&name).unwrap();
                assert_eq!(current.length(context)?, 1);
                assert_eq!(current.get(0u32, context)?.as_number(), Some(42.0));
                Ok(())
            })
            .unwrap();
        assert!(engine.get_const_array(&wide("A")).is_none());
    }

    #[test]
    fn reentry_without_a_host_callback_lease_returns_an_error_and_preserves_the_vm() {
        let engine = engine();
        engine
            .with_context(|context| {
                let error = engine.eval(&wide("throw 'must not run';")).unwrap_err();
                let message = error.into_opaque(context).unwrap().to_string(context)?;
                assert!(message
                    .to_std_string_escaped()
                    .contains("active outside a host callback"));
                assert_eq!(
                    context
                        .eval(Source::from_bytes("var retained=42;retained"))?
                        .as_number(),
                    Some(42.0)
                );
                Ok(())
            })
            .unwrap();
        assert_eq!(
            engine.eval(&wide("retained")).unwrap().as_number(),
            Some(42.0)
        );
    }
}

/// The two native GC tree wrappers' AsMapKey cases contain no collection
/// assertion: one node is keyed to100 and queried again by the same identity.
/// Exercise the selected script/model map key rather than a cppgc placeholder.
#[cfg(all(test, feature = "xfa"))]
fn assert_native_tree_node_map_key() {
    use crate::xfa::fxfa::{
        fxfa_basic::{XfaElement, XfaPacketType},
        parser::cxfa_document::CxfaDocument,
    };
    use std::collections::HashMap;
    let document = CxfaDocument::new();
    let node = document
        .create_node(XfaPacketType::Form, XfaElement::Subform)
        .unwrap();
    let mut score = HashMap::new();
    score.insert(node.clone(), 100);
    assert_eq!(score[&node], 100);
}
#[cfg(all(test, feature = "xfa"))]
#[test]
fn native_gced_tree_node_as_map_key() {
    assert_native_tree_node_map_key();
}
#[cfg(all(test, feature = "xfa"))]
#[test]
fn native_gced_tree_node_mixin_as_map_key() {
    assert_native_tree_node_map_key();
}
