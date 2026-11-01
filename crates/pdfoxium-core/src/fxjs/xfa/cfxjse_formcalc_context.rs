// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Connected scalar runtime and translation from `cfxjse_formcalc_context.{h,cpp}`.
//! The existing FormCalc lexer/parser/emitter produces JavaScript executed by
//! Boa through CfxJsEngine's safe context lease. No expression interpreter or
//! alternate VM is introduced. Document/current-node locale selection supplies
//! picture formatting and date/time conversions; bound nodes use the SOM/CJX
//! path and network functions delegate to the real optional embedding owner.
use super::formcalc_values::{
    argument_mismatch, compiler_error, get_simple_value, param_count, value_to_utf8_string,
};
use super::{
    formcalc_calendar, formcalc_iso, formcalc_locale, formcalc_misc, formcalc_nodes,
    formcalc_numeric, formcalc_operators, formcalc_patterns, formcalc_services, formcalc_strings,
    formcalc_times,
};
use crate::fxcrt::fx_string::{utf16_encode, utf8_decode};
use crate::fxjs::cfxjs_engine::CfxJsEngine;
use crate::xfa::fxfa::formcalc::{cxfa_fmlexer::CxfaFmLexer, cxfa_fmparser::CxfaFmParser};
use boa_engine::{
    js_string,
    native_function::NativeFunctionPointer,
    object::{FunctionObjectBuilder, ObjectInitializer},
    property::Attribute,
    Context, JsData, JsNativeError, JsObject, JsResult, JsString, JsSymbol, JsValue,
    NativeFunction, Source,
};
use boa_gc::{Finalize, Trace};
use std::rc::Rc;

#[derive(Trace, Finalize, JsData)]
struct FormcalcBinding {
    // This private host is constructed from built-in XML locales and a numeric
    // timezone cell; it contains no JavaScript values or engine/context owner.
    #[unsafe_ignore_trace]
    locale: Option<formcalc_locale::FormcalcLocaleHost>,
}

pub(super) fn locale_host(
    this: &JsValue,
    context: &Context,
) -> JsResult<formcalc_locale::FormcalcLocaleHost> {
    if let Some(host) = super::formcalc_document::locale_host(context) {
        return Ok(host);
    }
    let host = this
        .as_object()
        .and_then(|object| {
            object
                .downcast_ref::<FormcalcBinding>()
                .and_then(|data| data.locale.clone())
        })
        .ok_or_else(|| JsNativeError::typ().with_message("FormCalc locale host unavailable"))?;
    host.refresh_time_zone(context);
    Ok(host)
}

/// Compile an entire FormCalc program, preserving source spelling and the
/// existing per-translation parser/emitter depth and output-size limits.
pub fn translate(source: &[u32]) -> Option<Vec<u32>> {
    if source.is_empty() {
        return Some(Vec::new());
    }
    let mut parser = CxfaFmParser::new(CxfaFmLexer::new(source));
    let ast = parser.parse()?;
    if parser.has_error() {
        return None;
    }
    ast.to_javascript()
}

/// Register the implemented source methods into the caller's existing realm.
/// XFA document contexts use this same path; no placeholder resolver or invented
/// document is attached to standalone scalar contexts.
pub fn register_formcalc(context: &mut Context) -> JsResult<JsObject> {
    register_with_locale_host(context, None, false)
}

/// Install picture and date/time methods with an explicitly selected built-in locale.
/// Document contexts select their actual current-node locale automatically.
pub fn register_formcalc_with_locale(
    context: &mut Context,
    locale_name: &[u32],
) -> JsResult<JsObject> {
    let host = formcalc_locale::FormcalcLocaleHost::new(context, locale_name);
    register_with_locale_host(context, Some(host), false)
}

fn register_with_locale_host(
    context: &mut Context,
    locale: Option<formcalc_locale::FormcalcLocaleHost>,
    document: bool,
) -> JsResult<JsObject> {
    let has_locale = locale.is_some() || document;
    let mut functions = Vec::new();
    for &(name, callback) in formcalc_operators::METHODS
        .iter()
        .chain(formcalc_nodes::METHODS)
        .chain(formcalc_numeric::METHODS)
        .chain(formcalc_misc::METHODS)
        .chain(formcalc_strings::METHODS)
        .chain(formcalc_iso::METHODS)
        .chain(formcalc_services::METHODS)
        .chain(super::formcalc_network::METHODS)
        .chain(formcalc_locale::METHODS.iter().filter(|_| has_locale))
        .chain(formcalc_calendar::METHODS.iter().filter(|_| has_locale))
        .chain(formcalc_patterns::METHODS.iter().filter(|_| has_locale))
        .chain(formcalc_times::METHODS.iter().filter(|_| has_locale))
        .chain(&[
            ("Translate", translate_callback as NativeFunctionPointer),
            ("Eval", eval_callback as NativeFunctionPointer),
        ])
    {
        // The source class owns read-only/nondeletable methods without a
        // constructor prototype. Capture only the scalar function pointer.
        let native = NativeFunction::from_copy_closure(move |this, args, context| {
            if !this
                .as_object()
                .is_some_and(|object| object.is::<FormcalcBinding>())
            {
                return Err(JsNativeError::typ()
                    .with_message("Illegal invocation")
                    .into());
            }
            callback(this, args, context)
        });
        let function = FunctionObjectBuilder::new(context.realm(), native)
            .name(JsString::from(name))
            .length(0)
            .constructor(false)
            .build();
        functions.push((name, function));
    }
    let mut object = ObjectInitializer::with_native_data(FormcalcBinding { locale }, context);
    for (name, function) in functions {
        object.property(JsString::from(name), function, Attribute::ENUMERABLE);
    }
    object.property(
        JsSymbol::to_string_tag(),
        js_string!("XFA_FormCalcClass"),
        Attribute::CONFIGURABLE,
    );
    let object = object.build();
    context.register_global_property(js_string!("pfm_rt"), object.clone(), Attribute::all())?;
    Ok(object)
}

pub fn execute_in_context(context: &mut Context, source: &[u32]) -> JsResult<JsValue> {
    let javascript = translate(source).ok_or_else(compiler_error)?;
    context.eval(Source::from_utf16(&utf16_encode(&javascript)))
}
fn translate_callback(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.len() != 1 {
        return Err(param_count("Eval"));
    }
    let value = get_simple_value(args, 0, context)?;
    let source = value_to_utf8_string(&value, context)?;
    if source.is_empty() {
        return Err(argument_mismatch());
    }
    let javascript = translate(&utf8_decode(&source)).ok_or_else(compiler_error)?;
    Ok(JsString::from(utf16_encode(&javascript).as_slice()).into())
}
fn eval_callback(_: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    if args.len() != 1 {
        return Err(param_count("Eval"));
    }
    let value = get_simple_value(args, 0, context)?;
    let source = value_to_utf8_string(&value, context)?;
    if source.is_empty() {
        return Ok(JsValue::null());
    }
    // The emitter normally rewrites Eval to Translate + JavaScript eval.
    // This direct host call uses the current realm,
    // correcting the source's fresh context without a registered pfm_rt.
    execute_in_context(context, &utf8_decode(&source))
}

pub struct CfxJseFormcalcContext {
    engine: Rc<CfxJsEngine>,
}
impl CfxJseFormcalcContext {
    pub fn new() -> JsResult<Self> {
        let mut context = crate::fxjs::boa_adapters::new_context()?;
        register_formcalc(&mut context)?;
        Ok(Self {
            engine: CfxJsEngine::new(context),
        })
    }
    pub fn new_with_locale(locale_name: &[u32]) -> JsResult<Self> {
        let mut context = crate::fxjs::boa_adapters::new_context()?;
        register_formcalc_with_locale(&mut context, locale_name)?;
        Ok(Self {
            engine: CfxJsEngine::new(context),
        })
    }
    pub fn engine(&self) -> &Rc<CfxJsEngine> {
        &self.engine
    }
    pub fn execute(&self, source: &[u32]) -> JsResult<JsValue> {
        self.engine
            .with_context(|context| execute_in_context(context, source))
    }
    /// The source CFXJSE_Context explicitly calls a small eval wrapper with the
    /// supplied object as `this`; execute on the existing realm/lease likewise.
    pub fn execute_with_this(&self, source: &[u32], this: &JsObject) -> JsResult<JsValue> {
        let javascript = translate(source).ok_or_else(compiler_error)?;
        self.engine.with_context(|context| {
            let wrapper = context.eval(Source::from_bytes(
                "(function () { return eval(arguments[0]); })",
            ))?;
            wrapper.as_callable().expect("compiled eval wrapper").call(
                &this.clone().into(),
                &[JsString::from(utf16_encode(&javascript).as_slice()).into()],
                context,
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    fn execute(runtime: &CfxJseFormcalcContext, text: &str) -> JsValue {
        runtime
            .execute(&wide(text))
            .unwrap_or_else(|error| panic!("{text}: {error}"))
    }
    // Enabled TranslateEmpty/TranslateNumber/Numeric/Strings/Booleans/Eval
    // vectors in cfxjse_formcalc_context_embeddertest.cpp. DOM setup is not
    // claimed: these vectors exercise the actual compiler and scalar runtime.
    #[test]
    fn source_translation_numeric_strings_and_boolean_programs_execute() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        assert!(execute(&runtime, "").is_undefined());
        assert!(execute(&runtime, "; only a comment").is_undefined());
        for (program, expected) in [
            ("123", 123),
            ("123 + 456", 579),
            ("2 - 3 * 10 / 2 + 7", -6),
            ("10 * 3 + 5 * 4", 50),
            ("(5 - \"abc\") * 3", 15),
            ("\"100\" / 10e1", 1),
            ("5 + null + 3", 8),
            ("-(17)", -17),
            ("-(-17)", 17),
            ("+(17)", 17),
            ("+(-17)", -17),
            ("if (1 < 2) then\n1\nendif", 1),
            ("if (\"abc\" > \"def\") then\n1 and 0\nelse\n0\nendif", 0),
            ("Eval(\"10*3+5*4\")", 50),
        ] {
            assert_eq!(
                execute(&runtime, program).as_number(),
                Some(f64::from(expected)),
                "{program}"
            );
        }
        for (value, expected) in [(0, 2), (1, 3), (2, 4), (3, 5)] {
            let program = format!(
                "if({value} eq 0) then\n2\nelseif({value} eq 1) then\n3\nelseif({value} eq 2) then\n4\nelse\n5\nendif"
            );
            assert_eq!(
                execute(&runtime, &program).as_number(),
                Some(f64::from(expected)),
                "{program}"
            );
        }
        for (program, expected) in [
            ("0 and 1 or 2 > 1", true),
            ("2 < 3 not 1 == 1", false),
            ("\"abc\" | 2", true),
            ("1 or 0", true),
            ("0 | 0", false),
            ("0 or 1 | 0 or 0", true),
            ("1 and 0", false),
            ("0 and 1 & 0 and 0", false),
            ("not(\"true\")", true),
            ("not(1)", false),
            ("3 == 3", true),
            ("3 <> 4", true),
            ("\"abc\" eq \"def\"", false),
            ("\"def\" ne \"abc\"", true),
            ("5 + 5 == 10", true),
            ("5 + 5 <> \"10\"", false),
            ("3 < 3", false),
            ("3 > 4", false),
            ("\"abc\" <= \"def\"", true),
            ("\"def\" > \"abc\"", true),
            ("12 >= 12", true),
            ("\"true\" < \"false\"", false),
        ] {
            let result = execute(&runtime, program);
            assert!(result.is_boolean() || result.is_number());
            assert_eq!(result.to_boolean(), expected, "{program}");
        }
        for (program, expected) in [
            ("\"abc\"", "abc"),
            (
                "concat(\"The total is \",2,\" dollars and \",57,\" cents.\")",
                "The total is 2 dollars and 57 cents.",
            ),
        ] {
            assert_eq!(
                execute(&runtime, program).as_string().unwrap(),
                JsString::from(expected)
            );
        }
    }
    #[test]
    fn actual_compiler_control_flow_assignment_and_eval_execute_in_boa() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        for (program, expected) in [
            ("var a = 2\na = a + 3\na * 4", 20),
            (
                "var n=0\nvar total=0\nwhile(n < 5) do\nn=n+1\ntotal=total+n\nendwhile\ntotal",
                15,
            ),
            (
                "var total=0\nfor n=1 upto 5 do\ntotal=total+n\nendfor\ntotal",
                15,
            ),
            (
                "var total=0\nfor n=5 downto 1 step 2 do\ntotal=total+n\nendfor\ntotal",
                9,
            ),
            (
                "var total=0\nforeach n in (2,3,4) do\ntotal=total+n\nendfor\ntotal",
                9,
            ),
            (
                "var a=1\nif(a eq 0) then\n2\nelseif(a eq 1) then\n3\nelse\n4\nendif",
                3,
            ),
            ("Eval(\"Eval(\"\"2+3\"\")\")", 5),
        ] {
            assert_eq!(
                execute(&runtime, program).as_number(),
                Some(f64::from(expected)),
                "{program}"
            );
        }
    }
    #[test]
    fn formcalc_native_descriptors_errors_and_context_recovery_are_real() {
        let runtime = CfxJseFormcalcContext::new().unwrap();
        assert_eq!(
            runtime
                .engine()
                .eval(
                    &"Object.prototype.toString.call(pfm_rt)"
                        .encode_utf16()
                        .collect::<Vec<_>>()
                )
                .unwrap()
                .as_string()
                .unwrap(),
            js_string!("[object XFA_FormCalcClass]")
        );
        assert_eq!(runtime.engine().eval(&r#"(()=>{const d=Object.getOwnPropertyDescriptor(pfm_rt,'Abs');return !d.writable&&!d.configurable&&d.enumerable&&!('prototype' in pfm_rt.Abs)})()"#.encode_utf16().collect::<Vec<_>>()).unwrap(),true.into());
        for text in [
            "BogusFunction()",
            "if(1) then",
            "3/0",
            "Abs()",
            "Eval(\"\")",
            "DateFmt()",
        ] {
            assert!(runtime.execute(&wide(text)).is_err(), "{text}");
        }
        assert_eq!(
            runtime
                .engine()
                .eval(&"pfm_rt.Eval('2+3')".encode_utf16().collect::<Vec<_>>())
                .unwrap(),
            5.into()
        );
        assert_eq!(execute(&runtime, "Abs(-7)"), 7.into());
    }
    #[test]
    fn explicit_this_and_getter_reentry_use_the_existing_context_lease() {
        fn reenter(_: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
            let engine = CfxJsEngine::from_context(context).unwrap();
            CfxJsEngine::with_host_callback(context, || {
                engine.eval(&"pfm_rt.Abs(-5)".encode_utf16().collect::<Vec<_>>())
            })
        }
        let runtime = CfxJseFormcalcContext::new().unwrap();
        runtime
            .engine()
            .define_global_method(&"__reenter".encode_utf16().collect::<Vec<_>>(), 0, reenter)
            .unwrap();
        let bound = runtime
            .engine()
            .with_context(|context| {
                let bound = ObjectInitializer::new(context).build();
                context.register_global_property(
                    js_string!("bound"),
                    bound.clone(),
                    Attribute::all(),
                )?;
                Ok(bound)
            })
            .unwrap();
        assert_eq!(
            runtime
                .execute_with_this(&wide("Ref($) eq Ref(bound)"), &bound)
                .unwrap(),
            1.into()
        );
        runtime
            .engine()
            .eval(
                &"globalThis.input=[0,'value',{get value(){return __reenter()}}]"
                    .encode_utf16()
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        assert_eq!(execute(&runtime, "Sum(input, 2)"), 7.into());
        let marker=runtime.engine().eval(&"globalThis.marker={};globalThis.input=[0,'value',{get value(){throw marker}}];marker".encode_utf16().collect::<Vec<_>>()).unwrap();
        let error = runtime.execute(&wide("Sum(input)")).unwrap_err();
        let thrown = runtime
            .engine()
            .with_context(|context| error.into_opaque(context))
            .unwrap();
        assert_eq!(thrown, marker);
        assert_eq!(execute(&runtime, "2+3"), 5.into());
    }
}

pub(super) fn register_formcalc_for_document(context: &mut Context) -> JsResult<JsObject> {
    register_with_locale_host(context, None, true)
}
