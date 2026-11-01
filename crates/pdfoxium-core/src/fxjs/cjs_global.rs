// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `fxjs/cjs_global.{h,cpp}`. Boa target properties retain actual, traced JS
//! values, while native metadata supplies PDFium's persistence/deletion flags.
//! A Proxy adapts the named-property interceptor without replacing JavaScript
//! property semantics. Explicit outer-script commits fix the source's uncalled
//! commit method; GC finalization never calls script or an embedder delegate.
use super::{
    cfx_globaldata::CfxGlobalData,
    cfx_keyvalue::{CfxKeyValue, CfxValue},
    cfxjs_engine::CfxJsEngine,
    cjs_event_context::wide_from_utf16,
    js_resources::{js_format_error_string, js_get_string_from_id, JsMessage},
};
use crate::fxcrt::fx_string::{utf16_encode, utf8_decode, utf8_encode};
use boa_engine::{
    js_string,
    object::{
        builtins::{JsArray, JsProxy},
        FunctionObjectBuilder, ObjectInitializer,
    },
    property::{Attribute, PropertyKey},
    Context, JsData, JsError, JsNativeError, JsObject, JsResult, JsString, JsValue, NativeFunction,
};
use boa_gc::{Finalize, GcRefCell, Trace};
use std::{
    collections::{BTreeMap, HashSet},
    rc::Rc,
};

#[derive(Clone, Debug, Default, Trace, Finalize)]
struct EntryFlags {
    persistent: bool,
    deleted: bool,
}
#[derive(Debug, Trace, Finalize, JsData)]
struct GlobalBinding {
    entries: GcRefCell<BTreeMap<Vec<u16>, EntryFlags>>,
    reflect_set: JsObject,
    reflect_delete: JsObject,
    reflect_define: JsObject,
}
struct GlobalLink(Rc<CjsGlobal>);
/// The source CJS_Global owns a snapshot per runtime and shares CFX_GlobalData
/// with other runtimes. These ordinary object handles are external GC roots;
/// no Rust Rc points back from the GC native binding to this owner.
pub struct CjsGlobal {
    target: JsObject,
    object: JsObject,
    store: Rc<CfxGlobalData>,
    reflect_descriptor: JsObject,
    reflect_prototype: JsObject,
}
fn error(method: &str, message: JsMessage) -> JsError {
    let message = js_format_error_string("global", Some(method), &js_get_string_from_id(message));
    JsError::from_opaque(JsString::from(message.as_slice()).into())
}

fn invalid_binding() -> JsError {
    JsNativeError::typ()
        .with_message("Illegal invocation")
        .into()
}
fn bytes(value: &JsString) -> Vec<u8> {
    utf8_encode(&wide_from_utf16(&value.to_vec()))
}
fn text(value: &[u8]) -> JsString {
    JsString::from(utf16_encode(&utf8_decode(value)).as_slice())
}
fn valid_value(value: &JsValue) -> bool {
    value.is_number()
        || value.is_boolean()
        || value.is_string()
        || value.is_object()
        || value.is_null()
        || value.is_undefined()
}
fn target(args: &[JsValue]) -> JsResult<JsObject> {
    args.first()
        .and_then(JsValue::as_object)
        .filter(|object| object.is::<GlobalBinding>())
        .ok_or_else(invalid_binding)
}
fn mark(target: &JsObject, key: &JsString, deleted: bool) {
    let data = target
        .downcast_ref::<GlobalBinding>()
        .expect("checked global target");
    data.entries
        .borrow_mut()
        .entry(key.to_vec())
        .or_default()
        .deleted = deleted;
}
fn delete(target: &JsObject, key: &JsValue, context: &mut Context) -> JsResult<JsValue> {
    let name = key.as_string();
    let tracked = name.as_ref().is_some_and(|name| {
        target
            .downcast_ref::<GlobalBinding>()
            .expect("checked global target")
            .entries
            .borrow()
            .contains_key(&name.to_vec())
    });
    let callback = target
        .downcast_ref::<GlobalBinding>()
        .expect("checked global target")
        .reflect_delete
        .clone();
    let result = callback.call(
        &JsValue::undefined(),
        &[target.clone().into(), key.clone()],
        context,
    )?;
    if result.to_boolean() && tracked {
        if let Some(name) = name {
            mark(target, &name, true);
        }
    }
    Ok(result)
}
fn set(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let target = target(args)?;
    let key = args.get(1).cloned().unwrap_or_default();
    let value = args.get(2).cloned().unwrap_or_default();
    if !valid_value(&value) {
        return Err(error("PutProperty", JsMessage::ObjectTypeError));
    }
    // The source static method is readonly even when assigned undefined.
    // Its explicit delete operation remains configurable, like V8 templates.
    if key.as_string() == Some(js_string!("setPersistent"))
        && target.has_own_property(js_string!("setPersistent"), context)?
    {
        let callback = target
            .downcast_ref::<GlobalBinding>()
            .expect("checked global target")
            .reflect_set
            .clone();
        return callback.call(&JsValue::undefined(), args, context);
    }
    if value.is_undefined() {
        return delete(&target, &key, context);
    }
    if key.as_string().is_some_and(|name| name.is_empty()) {
        return Err(error("PutProperty", JsMessage::UnknownProperty));
    }
    let result = if let Some(name) = key.as_string() {
        if !target.has_own_property(name.clone(), context)? {
            // Named source globals become own data properties, including keys
            // such as __proto__; do not invoke an inherited prototype setter.
            let receiver = args
                .get(3)
                .and_then(JsValue::as_object)
                .unwrap_or_else(|| target.clone());
            receiver
                .create_data_property(name, value.clone(), context)?
                .into()
        } else {
            let callback = target
                .downcast_ref::<GlobalBinding>()
                .expect("checked global target")
                .reflect_set
                .clone();
            callback.call(&JsValue::undefined(), args, context)?
        }
    } else {
        let callback = target
            .downcast_ref::<GlobalBinding>()
            .expect("checked global target")
            .reflect_set
            .clone();
        callback.call(&JsValue::undefined(), args, context)?
    };
    if result.to_boolean() {
        if let Some(name) = key.as_string() {
            if target.has_own_property(name.clone(), context)? {
                mark(&target, &name, false);
            }
        }
    }
    Ok(result)
}
fn delete_property(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    delete(
        &target(args)?,
        args.get(1).unwrap_or(&JsValue::undefined()),
        context,
    )
}
fn define_property(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let target = target(args)?;
    let key = args.get(1).cloned().unwrap_or_default();
    let descriptor = args
        .get(2)
        .and_then(JsValue::as_object)
        .ok_or_else(invalid_binding)?;
    if descriptor.has_own_property(js_string!("value"), context)? {
        let value = descriptor.get(js_string!("value"), context)?;
        if !valid_value(&value) {
            return Err(error("PutProperty", JsMessage::ObjectTypeError));
        }
    }
    if key.as_string().is_some_and(|name| name.is_empty()) {
        return Err(error("PutProperty", JsMessage::UnknownProperty));
    }
    let callback = target
        .downcast_ref::<GlobalBinding>()
        .expect("checked global target")
        .reflect_define
        .clone();
    let result = callback.call(&JsValue::undefined(), args, context)?;
    if result.to_boolean() {
        if let Some(name) = key.as_string() {
            if name != js_string!("setPersistent") {
                mark(&target, &name, false);
            }
        }
    }
    Ok(result)
}
fn own_keys(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let target = target(args)?;
    // The native std::map enumerates dynamic string keys in lexical order.
    // Actual target descriptors remain authoritative, including non-enumerable
    // and non-configurable keys, so all Proxy ownKeys invariants are respected.
    let mut strings = Vec::new();
    let mut symbols = Vec::new();
    for key in target.own_property_keys(context)? {
        match key {
            PropertyKey::Symbol(symbol) => symbols.push(JsValue::from(symbol)),
            other => {
                let value = JsValue::from(other);
                strings.push(value.as_string().expect("string key"));
            }
        }
    }
    strings.sort_by_key(JsString::to_vec);
    Ok(JsArray::from_iter(
        strings.into_iter().map(JsValue::from).chain(symbols),
        context,
    )
    .into())
}
fn set_persistent(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let owner = context
        .get_data::<GlobalLink>()
        .map(|link| link.0.clone())
        .ok_or_else(invalid_binding)?;
    if !this
        .as_object()
        .is_some_and(|object| JsObject::equals(&object, &owner.object))
    {
        return Err(invalid_binding());
    }
    if args.len() != 2 {
        return Err(error("setPersistent", JsMessage::ParamError));
    }
    // Conversion may run arbitrary user code; acquire the metadata borrow only
    // after it completes, and then re-check whether the property is deleted.
    let name = args[0].to_string(context)?.to_vec();
    let persistent = args[1].to_boolean();
    let data = owner
        .target
        .downcast_ref::<GlobalBinding>()
        .expect("global binding");
    let mut entries = data.entries.borrow_mut();
    let entry = entries
        .get_mut(&name)
        .filter(|entry| !entry.deleted)
        .ok_or_else(|| error("setPersistent", JsMessage::GlobalNotFoundError))?;
    entry.persistent = persistent;
    Ok(JsValue::undefined())
}
fn intrinsic(reflect: &JsObject, name: &str, context: &mut Context) -> JsResult<JsObject> {
    reflect
        .get(JsString::from(name), context)?
        .as_callable()
        .ok_or_else(invalid_binding)
}

pub fn register_global(context: &mut Context) -> JsResult<Rc<CjsGlobal>> {
    register_global_with_store(context, CfxGlobalData::get_retained_instance(None))
}
pub fn register_global_with_store(
    context: &mut Context,
    store: Rc<CfxGlobalData>,
) -> JsResult<Rc<CjsGlobal>> {
    // Cache intrinsic Reflect functions before any PDF scripts can replace
    // their visible properties. Public safe Boa operations handle each trap.
    let reflect = context.intrinsics().objects().reflect();
    let binding = GlobalBinding {
        entries: GcRefCell::new(BTreeMap::new()),
        reflect_set: intrinsic(&reflect, "set", context)?,
        reflect_delete: intrinsic(&reflect, "deleteProperty", context)?,
        reflect_define: intrinsic(&reflect, "defineProperty", context)?,
    };
    let method =
        FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(set_persistent))
            .name(js_string!("setPersistent"))
            .length(0)
            .constructor(false)
            .build();
    let target = ObjectInitializer::with_native_data(binding, context)
        .property(
            js_string!("setPersistent"),
            method,
            Attribute::ENUMERABLE | Attribute::CONFIGURABLE,
        )
        .build();
    let object: JsObject = JsProxy::builder(target.clone())
        .set(set)
        .delete_property(delete_property)
        .define_property(define_property)
        .own_keys(own_keys)
        .build(context)?
        .into();
    let owner = Rc::new(CjsGlobal {
        target,
        object,
        store,
        reflect_descriptor: intrinsic(&reflect, "getOwnPropertyDescriptor", context)?,
        reflect_prototype: intrinsic(&reflect, "getPrototypeOf", context)?,
    });
    // Rebuilding stored plain objects never invokes a PDF getter or setter;
    // create_data_property avoids inherited __proto__/prototype setters.
    for element in owner.store.entries() {
        let element = element.borrow();
        let key = text(&element.data.key);
        let value = restore_value(&element.data.value, context)?;
        owner
            .target
            .create_data_property(key.clone(), value, context)?;
        let data = owner
            .target
            .downcast_ref::<GlobalBinding>()
            .expect("global binding");
        data.entries.borrow_mut().insert(
            key.to_vec(),
            EntryFlags {
                persistent: element.persistent,
                deleted: false,
            },
        );
    }
    context.register_global_property(
        js_string!("global"),
        owner.object.clone(),
        Attribute::all(),
    )?;
    context.insert_data(GlobalLink(owner.clone()));
    Ok(owner)
}
fn restore_value(value: &CfxValue, context: &mut Context) -> JsResult<JsValue> {
    Ok(match value {
        CfxValue::Number(value) => (*value).into(),
        CfxValue::Boolean(value) => (*value).into(),
        CfxValue::String(value) => text(value).into(),
        CfxValue::Null => JsValue::null(),
        CfxValue::Object(properties) => {
            let object = ObjectInitializer::new(context).build();
            for property in properties {
                object.create_data_property(
                    text(&property.key),
                    restore_value(&property.value, context)?,
                    context,
                )?;
            }
            object.into()
        }
    })
}

impl CjsGlobal {
    /// Commit only at an outer execution boundary. No live map or shared store
    /// borrow spans property enumeration/getters, which may re-enter scripting.
    pub fn commit(&self, context: &mut Context) -> JsResult<()> {
        let names: Vec<_> = self
            .target
            .downcast_ref::<GlobalBinding>()
            .expect("global binding")
            .entries
            .borrow()
            .keys()
            .cloned()
            .collect();
        let mut updates = Vec::with_capacity(names.len());
        for name in names {
            let key = JsString::from(name.as_slice());
            let flags = self
                .target
                .downcast_ref::<GlobalBinding>()
                .expect("global binding")
                .entries
                .borrow()
                .get(&name)
                .cloned()
                .expect("snapshot key");
            let value = if flags.deleted {
                None
            } else {
                self.snapshot_value(
                    &self.target.get(key.clone(), context)?,
                    &mut HashSet::new(),
                    context,
                )?
            };
            // A getter can delete a variable or change its persistence flag.
            let flags = self
                .target
                .downcast_ref::<GlobalBinding>()
                .expect("global binding")
                .entries
                .borrow()
                .get(&name)
                .cloned()
                .expect("retained metadata");
            updates.push((
                bytes(&key),
                if flags.deleted { None } else { value },
                flags.persistent,
            ));
        }
        // Every getter succeeded before publishing any part of this snapshot.
        for (name, value, persistent) in updates {
            match value {
                Some(CfxValue::Number(value)) => {
                    self.store.set_global_variable_number(&name, value)
                }
                Some(CfxValue::Boolean(value)) => {
                    self.store.set_global_variable_boolean(&name, value)
                }
                Some(CfxValue::String(value)) => {
                    self.store.set_global_variable_string(&name, &value)
                }
                Some(CfxValue::Object(value)) => {
                    self.store.set_global_variable_object(&name, value)
                }
                Some(CfxValue::Null) => self.store.set_global_variable_null(&name),
                None => {
                    self.store.delete_global_variable(&name);
                    continue;
                }
            }
            self.store.set_global_variable_persistent(&name, persistent);
        }
        // No delegate is the source's normal configuration: memory sharing
        // still works. A delegate's buffer callback uses the same host lease as
        // PDF callbacks if this context belongs to a running CFXJS_Engine.
        if CfxJsEngine::from_context(context).is_some() {
            CfxJsEngine::with_host_callback(context, || {
                self.store.commit();
            });
        } else {
            self.store.commit();
        }
        Ok(())
    }
    fn snapshot_value(
        &self,
        value: &JsValue,
        active: &mut HashSet<JsObject>,
        context: &mut Context,
    ) -> JsResult<Option<CfxValue>> {
        if let Some(number) = value.as_number() {
            return Ok(Some(CfxValue::Number(number)));
        }
        if let Some(boolean) = value.as_boolean() {
            return Ok(Some(CfxValue::Boolean(boolean)));
        }
        if let Some(string) = value.as_string() {
            return Ok(Some(CfxValue::String(bytes(&string))));
        }
        if value.is_null() {
            return Ok(Some(CfxValue::Null));
        }
        let Some(object) = value.as_object() else {
            return Ok(None);
        };
        // The source format is a tree: omit cyclic backedges instead of its
        // unbounded recursion, while keeping the actual live graph untouched.
        if active.contains(&object) {
            return Ok(None);
        }
        if active.len() >= 256 {
            return Err(JsNativeError::range()
                .with_message("global object snapshot nesting is too deep")
                .into());
        }
        active.insert(object.clone());
        let result = (|| {
            let mut properties = Vec::new();
            for key in self.enumerable_keys(&object, context)? {
                let value = object.get(key.clone(), context)?;
                if let Some(value) = self.snapshot_value(&value, active, context)? {
                    properties.push(CfxKeyValue {
                        key: bytes(&key),
                        value,
                    });
                }
            }
            Ok(Some(CfxValue::Object(properties)))
        })();
        active.remove(&object);
        result
    }
    fn enumerable_keys(&self, object: &JsObject, context: &mut Context) -> JsResult<Vec<JsString>> {
        // V8 GetPropertyNames includes enumerable inherited string properties.
        // Use cached intrinsic descriptor/prototype operations so proxies and
        // getters retain normal JavaScript semantics and opaque exceptions.
        let mut result = Vec::new();
        let mut names = HashSet::new();
        let mut prototypes = HashSet::new();
        let mut current = Some(object.clone());
        while let Some(object) = current {
            if !prototypes.insert(object.clone()) {
                break;
            }
            for key in object.own_property_keys(context)? {
                if matches!(key, PropertyKey::Symbol(_)) {
                    continue;
                }
                let key = JsValue::from(key).as_string().expect("string property key");
                if !names.insert(key.clone()) {
                    continue;
                }
                let descriptor = self.reflect_descriptor.call(
                    &JsValue::undefined(),
                    &[object.clone().into(), key.clone().into()],
                    context,
                )?;
                if let Some(descriptor) = descriptor.as_object() {
                    if descriptor
                        .get(js_string!("enumerable"), context)?
                        .to_boolean()
                    {
                        result.push(key);
                    }
                }
            }
            current = self
                .reflect_prototype
                .call(&JsValue::undefined(), &[object.into()], context)?
                .as_object();
        }
        Ok(result)
    }
}
pub fn commit_global(context: &mut Context) -> JsResult<()> {
    let owner = context.get_data::<GlobalLink>().map(|link| link.0.clone());
    if let Some(owner) = owner {
        owner.commit(context)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use boa_engine::Source;
    fn context(store: Rc<CfxGlobalData>) -> Context {
        let mut context = Context::default();
        register_global_with_store(&mut context, store).unwrap();
        context
    }
    fn eval(context: &mut Context, source: &str) -> JsValue {
        context.eval(Source::from_bytes(source)).unwrap()
    }
    // Source testing/resources/javascript/globals.in covers these scalar,
    // object, deletion and persistent-name cases; keep actual engine values.
    #[test]
    fn dynamic_globals_preserve_values_identity_gc_and_deletion() {
        let store = CfxGlobalData::get_retained_instance(None);
        let mut context = context(store.clone());
        assert_eq!(
            eval(
                &mut context,
                r#"
            var object={colors:['red','green','blue']}; object.self=object;
            var callable=function(){return 42};
            global.yes=true; global.no=false; global.zero=0; global.number=-3.918;
            global.string='\u4025\ud800\0😀'; global.object=object; global.nil=null;
            global.array=object.colors; global.callable=callable; global.absent=undefined;
            global['\u4025\ud800']=global.string;
            global.__proto__={safe:7};
            global.setPersistent('object',true);
            global.setPersistent('yes',true);
            global.same=object;
            global.object===object && global.same===object && global.object.self===object &&
            global.array===object.colors && global.callable===callable && global.callable()===42 &&
            global.yes===true && global.no===false && global.zero===0 && global.number===-3.918 &&
            global.nil===null && !('absent' in global) && global['\u4025\ud800']===global.string &&
            Object.hasOwn(global,'__proto__') && Object.getPrototypeOf(global)!==global.__proto__
        "#
            ),
            JsValue::from(true)
        );
        boa_gc::force_collect();
        assert_eq!(eval(&mut context, "global.object===global.same && global.object.self===global.object && global.string.charCodeAt(1)===0xd800"), JsValue::from(true));
        assert_eq!(
            eval(
                &mut context,
                r#"
            const names=Object.keys(global);
            if(names.join('|')!==[...names].sort().join('|')) throw 'enumeration order';
            for(const key of ['yes','no','zero','number','string','object','nil','array','callable']) {
                if(!global.hasOwnProperty(key)) throw 'missing own property';
            }
            delete global.object;
            if('object' in global || global.hasOwnProperty('object')) throw 'deleted query';
            try{global.setPersistent('object',true);throw 'missing deletion error'}
            catch(e){if(e!=='global.setPersistent: Global value not found.') throw e}
            global.object=global.same;
            global.yes=undefined;
            !('yes' in global) && global.object===global.same
        "#
            ),
            JsValue::from(true)
        );
        commit_global(&mut context).unwrap();
        assert!(store.get_global_variable(b"yes").is_none());
        assert!(
            store
                .get_global_variable(b"object")
                .unwrap()
                .borrow()
                .persistent
        );
        assert_eq!(
            eval(&mut context, "global.object.self===global.object"),
            JsValue::from(true)
        );
    }
    #[test]
    fn commits_share_source_value_trees_and_keep_live_cycles() {
        let store = CfxGlobalData::get_retained_instance(None);
        let mut first = context(store.clone());
        eval(
            &mut first,
            r#"
            global.flag=true; global.setPersistent('flag',true);
            global.text='é\ud800\0😀';
            var inherited={inherited:9};
            var item=Object.create(inherited); item.number=1.25;item.yes=true;item.no=false;
            item.string=global.text; item.nil=null; item.array=[3,4];item.self=item;
            Object.defineProperty(item,'hidden',{value:'excluded'});
            item.unsupported=Symbol('omitted');
            global.object=item;
        "#,
        );
        commit_global(&mut first).unwrap();
        let mut second = context(store.clone());
        assert_eq!(
            eval(
                &mut second,
                r#"
            global.flag===true && global.text==='é\ud800\0😀' &&
            global.object.number===1.25 && global.object.yes===true && global.object.no===false &&
            global.object.string===global.text && global.object.nil===null &&
            global.object.array[0]===3 && global.object.array[1]===4 && global.object.inherited===9 &&
            !('hidden' in global.object) && !('self' in global.object) && !('unsupported' in global.object)
        "#
            ),
            JsValue::from(true)
        );
        assert_eq!(
            eval(&mut first, "global.object.self===global.object"),
            JsValue::from(true)
        );
        eval(
            &mut second,
            "global.flag=7;global.setPersistent('flag',false);delete global.text;",
        );
        commit_global(&mut second).unwrap();
        let flag = store.get_global_variable(b"flag").unwrap();
        assert_eq!(flag.borrow().data.value, CfxValue::Number(7.0));
        assert!(!flag.borrow().persistent);
        assert!(store.get_global_variable(b"text").is_none());
        // Other already-open document contexts retain their source snapshot.
        assert_eq!(
            eval(
                &mut first,
                "global.flag===true && typeof global.text==='string'"
            ),
            JsValue::from(true)
        );
    }
    #[test]
    fn property_coercions_getters_and_proxy_failures_keep_opaque_errors() {
        let store = CfxGlobalData::get_retained_instance(None);
        let mut context = context(store.clone());
        eval(&mut context, "global.stable=1;global.object={old:2};");
        commit_global(&mut context).unwrap();
        assert_eq!(
            eval(
                &mut context,
                r#"
            var marker={};
            for(const value of [1n, Symbol('value')]) {
                try{global.bad=value;throw 'missing unsupported error'}
                catch(e){if(e!=='global.PutProperty: Object is of the wrong type.') throw e}
            }
            try{global['']=1;throw 'empty key accepted'}
            catch(e){if(e!=='global.PutProperty: Unknown property.') throw e}
            try{global.setPersistent();throw 'missing arity error'}
            catch(e){if(e!=='global.setPersistent: Incorrect number of parameters passed to function.') throw e}
            try{global.setPersistent({toString(){throw marker}},true);throw 'missing coercion error'}
            catch(e){if(e!==marker) throw e}
            global.setPersistent({toString(){global.reentered='live';return 'stable'}},true);
            global.object={get value(){global.reentered='getter';throw marker}};
            true
        "#
            ),
            JsValue::from(true)
        );
        let error = commit_global(&mut context).unwrap_err();
        assert_eq!(
            error.into_opaque(&mut context).unwrap(),
            eval(&mut context, "marker")
        );
        assert_eq!(
            store
                .get_global_variable(b"object")
                .unwrap()
                .borrow()
                .data
                .value,
            CfxValue::Object(vec![CfxKeyValue {
                key: b"old".to_vec(),
                value: CfxValue::Number(2.0)
            }])
        );
        assert!(
            store.get_global_variable(b"reentered").is_none(),
            "failed snapshot must not publish partial updates"
        );
        assert_eq!(
            eval(&mut context, "global.reentered"),
            JsValue::from(js_string!("getter"))
        );
        eval(
            &mut context,
            "global.object=new Proxy({}, {ownKeys(){throw marker}});",
        );
        assert_eq!(
            commit_global(&mut context)
                .unwrap_err()
                .into_opaque(&mut context)
                .unwrap(),
            eval(&mut context, "marker")
        );
        eval(&mut context, "global.object={get value(){global.stable=8;return global.stable}}; Reflect.defineProperty=()=>{throw 'poison'};Reflect.getOwnPropertyDescriptor=()=>{throw 'poison'};");
        commit_global(&mut context).unwrap();
        assert_eq!(
            store
                .get_global_variable(b"object")
                .unwrap()
                .borrow()
                .data
                .value,
            CfxValue::Object(vec![CfxKeyValue {
                key: b"value".to_vec(),
                value: CfxValue::Number(8.0)
            }])
        );
        assert_eq!(
            eval(&mut context, "global.setPersistent=undefined; typeof global.setPersistent==='function' && !Object.getOwnPropertyDescriptor(global,'setPersistent').writable"),
            JsValue::from(true)
        );
    }
}
