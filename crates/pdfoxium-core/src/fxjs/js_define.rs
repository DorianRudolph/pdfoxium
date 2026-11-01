// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Keyword argument helpers from `fxjs/js_define.{h,cpp}`. Typed native data,
//! traced Boa values and safe function builders replace the V8 binding templates
//! at their callers. JS_DateParse is represented by `js_date::date_parse`.
use boa_engine::{Context, JsResult, JsString, JsValue};

/// `None` represents an empty native handle, distinct from explicit undefined.
/// A single object argument supplies keyword properties; arrays are positional.
/// Property getters run in source order and preserve thrown JavaScript values.
pub fn expand_keyword_params(
    context: &mut Context,
    originals: &[JsValue],
    keywords: &[&str],
) -> JsResult<Vec<Option<JsValue>>> {
    assert!(!keywords.is_empty());
    let mut result = vec![None; keywords.len()];
    for (slot, value) in result.iter_mut().zip(originals) {
        *slot = Some(value.clone());
    }
    if originals.len() != 1 {
        return Ok(result);
    }
    let Some(object) = originals[0].as_object().filter(|value| !value.is_array()) else {
        return Ok(result);
    };
    result[0] = None;
    for (slot, name) in result.iter_mut().zip(keywords) {
        let value = object.get(JsString::from(*name), context)?;
        if !value.is_undefined() {
            *slot = Some(value);
        }
    }
    Ok(result)
}

/// The source deliberately excludes symbols and big integers from its accepted
/// argument kinds. Positional undefined is known; an absent keyword is unknown.
pub fn is_expanded_param_known(value: Option<&JsValue>) -> bool {
    value.is_some_and(|value| !value.is_symbol() && !value.is_bigint())
}

#[cfg(test)]
mod tests {
    use super::*;
    use boa_engine::Source;

    #[test]
    fn positional_and_keyword_undefined_arrays_and_value_kinds() {
        let mut context = Context::default();
        let undefined = JsValue::undefined();
        let positional = expand_keyword_params(&mut context, &[undefined], &["a", "b"]).unwrap();
        assert!(is_expanded_param_known(positional[0].as_ref()));
        assert!(positional[1].is_none());
        let object = context
            .eval(Source::from_bytes("({a:undefined,b:null})"))
            .unwrap();
        let expanded = expand_keyword_params(&mut context, &[object], &["a", "b"]).unwrap();
        assert!(expanded[0].is_none());
        assert!(expanded[1].as_ref().unwrap().is_null());
        let array = context.eval(Source::from_bytes("[1,2]")).unwrap();
        let positional = expand_keyword_params(&mut context, &[array], &["a", "b"]).unwrap();
        assert!(positional[0].as_ref().unwrap().is_object());
        for source in ["Symbol('unknown')", "12n"] {
            let value = context.eval(Source::from_bytes(source)).unwrap();
            assert!(!is_expanded_param_known(Some(&value)));
        }
    }

    #[test]
    fn inherited_keyword_getters_run_in_order_and_opaque_errors_escape() {
        let mut context = Context::default();
        let object = context.eval(Source::from_bytes(
            "var seen=''; var thrown={marker:17}; Object.create({get a(){seen+='a'; return 3},get b(){seen+='b'; throw thrown},get c(){seen+='c'; return 4}})"
        )).unwrap();
        let error = expand_keyword_params(&mut context, &[object], &["a", "b", "c"]).unwrap_err();
        let thrown = context.eval(Source::from_bytes("thrown")).unwrap();
        assert_eq!(error.as_opaque(), Some(&thrown));
        assert_eq!(
            context
                .eval(Source::from_bytes("seen"))
                .unwrap()
                .as_string()
                .unwrap()
                .to_vec(),
            "ab".encode_utf16().collect::<Vec<_>>()
        );
    }
}
