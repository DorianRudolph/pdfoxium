// Copyright 2014 The PDFium Authors
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Complete tables and registration from `fxjs/cjs_{border,display,font,
//! highlight,position,scalehow,scalewhen,style,zoomtype,globalconsts,
//! globalarrays}.{h,cpp}`. `CJS_Object::DefineConsts` installs ordinary mutable
//! properties; only the getter-backed global constants reject assignment.
//! Traced getter captures retain the global arrays, replacing the source's
//! persistent array map without hiding JavaScript roots in native host data.
use boa_engine::{
    object::{builtins::JsArray, FunctionObjectBuilder, ObjectInitializer},
    property::{Attribute, PropertyDescriptor},
    Context, JsObject, JsResult, JsString, JsValue, NativeFunction,
};

const STRING_OBJECTS: &[(&str, &[(&str, &str)])] = &[
    (
        "border",
        &[
            ("s", "solid"),
            ("b", "beveled"),
            ("d", "dashed"),
            ("i", "inset"),
            ("u", "underline"),
        ],
    ),
    (
        "font",
        &[
            ("Times", "Times-Roman"),
            ("TimesB", "Times-Bold"),
            ("TimesI", "Times-Italic"),
            ("TimesBI", "Times-BoldItalic"),
            ("Helv", "Helvetica"),
            ("HelvB", "Helvetica-Bold"),
            ("HelvI", "Helvetica-Oblique"),
            ("HelvBI", "Helvetica-BoldOblique"),
            ("Cour", "Courier"),
            ("CourB", "Courier-Bold"),
            ("CourI", "Courier-Oblique"),
            ("CourBI", "Courier-BoldOblique"),
            ("Symbol", "Symbol"),
            ("ZapfD", "ZapfDingbats"),
        ],
    ),
    (
        "highlight",
        &[
            ("n", "none"),
            ("i", "invert"),
            ("p", "push"),
            ("o", "outline"),
        ],
    ),
    (
        "style",
        &[
            ("ch", "check"),
            ("cr", "cross"),
            ("di", "diamond"),
            ("ci", "circle"),
            ("st", "star"),
            ("sq", "square"),
        ],
    ),
    (
        "zoomtype",
        &[
            ("none", "NoVary"),
            ("fitP", "FitPage"),
            ("fitW", "FitWidth"),
            ("fitH", "FitHeight"),
            ("fitV", "FitVisibleWidth"),
            ("pref", "Preferred"),
            ("refW", "ReflowWidth"),
        ],
    ),
];
const NUMBER_OBJECTS: &[(&str, &[(&str, i32)])] = &[
    (
        "display",
        &[("visible", 0), ("hidden", 1), ("noPrint", 2), ("noView", 3)],
    ),
    (
        "position",
        &[
            ("textOnly", 0),
            ("iconOnly", 1),
            ("iconTextV", 2),
            ("textIconV", 3),
            ("iconTextH", 4),
            ("textIconH", 5),
            ("overlay", 6),
        ],
    ),
    ("scaleHow", &[("proportional", 0), ("anamorphic", 1)]),
    (
        "scaleWhen",
        &[("always", 0), ("never", 1), ("tooBig", 2), ("tooSmall", 3)],
    ),
];
const GLOBAL_STRINGS: &[(&str, &str)] = &[
    ("IDS_GREATER_THAN", "Invalid value: must be greater than or equal to % s."),
    ("IDS_GT_AND_LT", "Invalid value: must be greater than or equal to % s and less than or equal to % s."),
    ("IDS_LESS_THAN", "Invalid value: must be less than or equal to % s."),
    ("IDS_INVALID_MONTH", "**Invalid**"),
    ("IDS_INVALID_DATE", "Invalid date / time: please ensure that the date / time exists.Field"),
    ("IDS_INVALID_VALUE", "The value entered does not match the format of the field"),
    ("IDS_AM", "am"),
    ("IDS_PM", "pm"),
    ("IDS_MONTH_INFO", "January[1] February[2] March[3] April[4] May[5] June[6] July[7] August[8] September[9] October[10] November[11] December[12] Sept[9] Jan[1] Feb[2] Mar[3] Apr[4] Jun[6] Jul[7] Aug[8] Sep[9] Oct[10] Nov[11] Dec[12]"),
    ("IDS_STARTUP_CONSOLE_MSG", "** ^ _ ^ **"),
];
const GLOBAL_ARRAYS: &[(&str, &[&str])] = &[
    ("RE_NUMBER_ENTRY_DOT_SEP", &[r"[+-]?\d*\.?\d*"]),
    (
        "RE_NUMBER_COMMIT_DOT_SEP",
        &[r"[+-]?\d+(\.\d+)?", r"[+-]?\.\d+", r"[+-]?\d+\."],
    ),
    ("RE_NUMBER_ENTRY_COMMA_SEP", &[r"[+-]?\d*,?\d*"]),
    (
        "RE_NUMBER_COMMIT_COMMA_SEP",
        &[r"[+-]?\d+([.,]\d+)?", r"[+-]?[.,]\d+", r"[+-]?\d+[.,]"],
    ),
    ("RE_ZIP_ENTRY", &[r"\d{0,5}"]),
    ("RE_ZIP_COMMIT", &[r"\d{5}"]),
    ("RE_ZIP4_ENTRY", &[r"\d{0,5}(\.|[- ])?\d{0,4}"]),
    ("RE_ZIP4_COMMIT", &[r"\d{5}(\.|[- ])?\d{4}"]),
    (
        "RE_PHONE_ENTRY",
        &[
            r"\d{0,3}(\.|[- ])?\d{0,3}(\.|[- ])?\d{0,4}",
            r"\(\d{0,3}",
            r"\(\d{0,3}\)(\.|[- ])?\d{0,3}(\.|[- ])?\d{0,4}",
            r"\(\d{0,3}(\.|[- ])?\d{0,3}(\.|[- ])?\d{0,4}",
            r"\d{0,3}\)(\.|[- ])?\d{0,3}(\.|[- ])?\d{0,4}",
            r"011(\.|[- \d])*",
        ],
    ),
    (
        "RE_PHONE_COMMIT",
        &[
            r"\d{3}(\.|[- ])?\d{4}",
            r"\d{3}(\.|[- ])?\d{3}(\.|[- ])?\d{4}",
            r"\(\d{3}\)(\.|[- ])?\d{3}(\.|[- ])?\d{4}",
            r"011(\.|[- \d])*",
        ],
    ),
    (
        "RE_SSN_ENTRY",
        &[r"\d{0,3}(\.|[- ])?\d{0,2}(\.|[- ])?\d{0,4}"],
    ),
    ("RE_SSN_COMMIT", &[r"\d{3}(\.|[- ])?\d{2}(\.|[- ])?\d{4}"]),
];

fn define_global_const(
    context: &mut Context,
    name: &str,
    value: JsValue,
    requires_global_receiver: bool,
) -> JsResult<()> {
    let receiver = requires_global_receiver.then(|| context.global_object());
    let getter = NativeFunction::from_copy_closure_with_captures(
        |this, _, (receiver, value), _| {
            // The source array getter finds the document binding through This;
            // borrowing its getter with an unrelated receiver returns undefined.
            if let Some(receiver) = receiver {
                if !this
                    .as_object()
                    .is_some_and(|this| JsObject::equals(&this, receiver))
                {
                    return Ok(JsValue::undefined());
                }
            }
            Ok(value.clone())
        },
        (receiver, value),
    );
    let getter = FunctionObjectBuilder::new(context.realm(), getter)
        .name(JsString::from(format!("get {name}")))
        .length(0)
        .constructor(false)
        .build();
    context.global_object().define_property_or_throw(
        JsString::from(name),
        PropertyDescriptor::builder()
            .get(getter)
            .enumerable(true)
            .configurable(true),
        context,
    )?;
    Ok(())
}

pub fn register_constants(context: &mut Context) -> JsResult<()> {
    // Retain CJS_Runtime::DefineJSObjects' installation order as well as each
    // object's ConstSpecs order.
    for name in [
        "border",
        "display",
        "font",
        "highlight",
        "position",
        "scaleHow",
        "scaleWhen",
        "style",
        "zoomtype",
    ] {
        let mut object = ObjectInitializer::new(context);
        if let Some((_, members)) = STRING_OBJECTS.iter().find(|(key, _)| *key == name) {
            for &(key, value) in *members {
                object.property(JsString::from(key), JsString::from(value), Attribute::all());
            }
        } else {
            let (_, members) = NUMBER_OBJECTS
                .iter()
                .find(|(key, _)| *key == name)
                .expect("constant object table");
            for &(key, value) in *members {
                object.property(JsString::from(key), value, Attribute::all());
            }
        }
        let object = object.build();
        context.register_global_property(JsString::from(name), object, Attribute::all())?;
    }
    for &(name, value) in GLOBAL_STRINGS {
        define_global_const(context, name, JsString::from(value).into(), false)?;
    }
    for &(name, values) in GLOBAL_ARRAYS {
        let array = JsArray::from_iter(
            values.iter().map(|&value| JsString::from(value).into()),
            context,
        );
        define_global_const(context, name, array.into(), true)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use boa_engine::Source;

    fn assert_script(context: &mut Context, source: &str) {
        assert_eq!(
            context
                .eval(Source::from_bytes(source))
                .unwrap()
                .as_boolean(),
            Some(true)
        );
    }

    #[test]
    fn complete_object_tables_and_source_property_attributes() {
        let mut context = Context::default();
        register_constants(&mut context).unwrap();
        assert_script(
            &mut context,
            r#"
            JSON.stringify([border,display,font,highlight,position,scaleHow,scaleWhen,style,zoomtype]) ===
            JSON.stringify([
              {s:'solid',b:'beveled',d:'dashed',i:'inset',u:'underline'},
              {visible:0,hidden:1,noPrint:2,noView:3},
              {Times:'Times-Roman',TimesB:'Times-Bold',TimesI:'Times-Italic',TimesBI:'Times-BoldItalic',
               Helv:'Helvetica',HelvB:'Helvetica-Bold',HelvI:'Helvetica-Oblique',HelvBI:'Helvetica-BoldOblique',
               Cour:'Courier',CourB:'Courier-Bold',CourI:'Courier-Oblique',CourBI:'Courier-BoldOblique',Symbol:'Symbol',ZapfD:'ZapfDingbats'},
              {n:'none',i:'invert',p:'push',o:'outline'},
              {textOnly:0,iconOnly:1,iconTextV:2,textIconV:3,iconTextH:4,textIconH:5,overlay:6},
              {proportional:0,anamorphic:1}, {always:0,never:1,tooBig:2,tooSmall:3},
              {ch:'check',cr:'cross',di:'diamond',ci:'circle',st:'star',sq:'square'},
              {none:'NoVary',fitP:'FitPage',fitW:'FitWidth',fitH:'FitHeight',fitV:'FitVisibleWidth',pref:'Preferred',refW:'ReflowWidth'}
            ]);
        "#,
        );
        assert_script(
            &mut context,
            r#"
            (() => {
              for (let name of ['border','display','font','highlight','position','scaleHow','scaleWhen','style','zoomtype']) {
                let globalDescriptor=Object.getOwnPropertyDescriptor(globalThis,name);
                if(!globalDescriptor.writable || !globalDescriptor.enumerable || !globalDescriptor.configurable) return false;
                for (let key of Object.keys(globalThis[name])) {
                  let descriptor=Object.getOwnPropertyDescriptor(globalThis[name],key);
                  if(!descriptor.writable || !descriptor.enumerable || !descriptor.configurable) return false;
                }
              }
              font.Helv='custom'; delete border.b; display={visible:7};
              return font.Helv==='custom' && !('b' in border) && display.visible===7;
            })();
        "#,
        );
    }

    #[test]
    fn global_strings_and_mutable_identity_preserving_arrays() {
        let mut context = Context::default();
        register_constants(&mut context).unwrap();
        assert_script(
            &mut context,
            r#"
            JSON.stringify([IDS_GREATER_THAN,IDS_GT_AND_LT,IDS_LESS_THAN,IDS_INVALID_MONTH,
              IDS_INVALID_DATE,IDS_INVALID_VALUE,IDS_AM,IDS_PM,IDS_MONTH_INFO,IDS_STARTUP_CONSOLE_MSG]) === JSON.stringify([
              'Invalid value: must be greater than or equal to % s.',
              'Invalid value: must be greater than or equal to % s and less than or equal to % s.',
              'Invalid value: must be less than or equal to % s.','**Invalid**',
              'Invalid date / time: please ensure that the date / time exists.Field',
              'The value entered does not match the format of the field','am','pm',
              'January[1] February[2] March[3] April[4] May[5] June[6] July[7] August[8] September[9] October[10] November[11] December[12] Sept[9] Jan[1] Feb[2] Mar[3] Apr[4] Jun[6] Jul[7] Aug[8] Sep[9] Oct[10] Nov[11] Dec[12]',
              '** ^ _ ^ **']);
        "#,
        );
        assert_script(
            &mut context,
            r#"
            (() => {
              for (let name of Object.keys(globalThis).filter(name=>name.startsWith('RE_') || name.startsWith('IDS_'))) {
                let descriptor=Object.getOwnPropertyDescriptor(globalThis,name);
                if(!descriptor.get || descriptor.set!==undefined || !descriptor.enumerable || !descriptor.configurable) return false;
              }
              let array=RE_ZIP_COMMIT;
              RE_ZIP_COMMIT=['replacement']; IDS_AM='replacement';
              if(array!==RE_ZIP_COMMIT || IDS_AM!=='am') return false;
              array.push('added'); array[0]='changed';
              if(RE_ZIP_COMMIT.join(',')!=='changed,added') return false;
              let getter=Object.getOwnPropertyDescriptor(globalThis,'RE_ZIP_COMMIT').get;
              if(getter.call({})!==undefined || getter.call(globalThis)!==array) return false;
              try{ (function(){'use strict'; RE_ZIP_COMMIT=[]; })(); return false; }catch(e){ if(!(e instanceof TypeError)) return false; }
              delete globalThis.RE_ZIP_COMMIT;
              return !('RE_ZIP_COMMIT' in globalThis) && array.length===2;
            })();
        "#,
        );
        let mut other = Context::default();
        register_constants(&mut other).unwrap();
        boa_gc::force_collect();
        assert_script(
            &mut other,
            r"RE_ZIP_COMMIT.length===1 && RE_ZIP_COMMIT[0]==='\\d{5}'",
        );
    }

    #[test]
    fn complete_regular_expression_tables_execute_in_javascript() {
        let mut context = Context::default();
        register_constants(&mut context).unwrap();
        assert_script(
            &mut context,
            r#"
            JSON.stringify([RE_NUMBER_ENTRY_DOT_SEP,RE_NUMBER_COMMIT_DOT_SEP,RE_NUMBER_ENTRY_COMMA_SEP,
              RE_NUMBER_COMMIT_COMMA_SEP,RE_ZIP_ENTRY,RE_ZIP_COMMIT,RE_ZIP4_ENTRY,RE_ZIP4_COMMIT,
              RE_PHONE_ENTRY,RE_PHONE_COMMIT,RE_SSN_ENTRY,RE_SSN_COMMIT]) === JSON.stringify([
              ['[+-]?\\d*\\.?\\d*'], ['[+-]?\\d+(\\.\\d+)?','[+-]?\\.\\d+','[+-]?\\d+\\.'],
              ['[+-]?\\d*,?\\d*'], ['[+-]?\\d+([.,]\\d+)?','[+-]?[.,]\\d+','[+-]?\\d+[.,]'],
              ['\\d{0,5}'],['\\d{5}'],['\\d{0,5}(\\.|[- ])?\\d{0,4}'],['\\d{5}(\\.|[- ])?\\d{4}'],
              ['\\d{0,3}(\\.|[- ])?\\d{0,3}(\\.|[- ])?\\d{0,4}','\\(\\d{0,3}',
               '\\(\\d{0,3}\\)(\\.|[- ])?\\d{0,3}(\\.|[- ])?\\d{0,4}',
               '\\(\\d{0,3}(\\.|[- ])?\\d{0,3}(\\.|[- ])?\\d{0,4}',
               '\\d{0,3}\\)(\\.|[- ])?\\d{0,3}(\\.|[- ])?\\d{0,4}','011(\\.|[- \\d])*'],
              ['\\d{3}(\\.|[- ])?\\d{4}','\\d{3}(\\.|[- ])?\\d{3}(\\.|[- ])?\\d{4}',
               '\\(\\d{3}\\)(\\.|[- ])?\\d{3}(\\.|[- ])?\\d{4}','011(\\.|[- \\d])*'],
              ['\\d{0,3}(\\.|[- ])?\\d{0,2}(\\.|[- ])?\\d{0,4}'],['\\d{3}(\\.|[- ])?\\d{2}(\\.|[- ])?\\d{4}']
            ]);
        "#,
        );
        assert_script(
            &mut context,
            r#"
            (() => {
              let matches=(patterns,value)=>patterns.some(pattern=>new RegExp('^(?:'+pattern+')$').test(value));
              return matches(RE_NUMBER_ENTRY_DOT_SEP,'-.') && !matches(RE_NUMBER_COMMIT_DOT_SEP,'-.') &&
                ['-1.0','-.1','-1.'].every(value=>matches(RE_NUMBER_COMMIT_DOT_SEP,value)) &&
                ['-1,0','-,1','-1,'].every(value=>matches(RE_NUMBER_COMMIT_COMMA_SEP,value)) &&
                matches(RE_ZIP_ENTRY,'12') && !matches(RE_ZIP_COMMIT,'12') && matches(RE_ZIP_COMMIT,'12345') &&
                matches(RE_ZIP4_COMMIT,'12345-1234') && matches(RE_SSN_COMMIT,'123-45-6789') &&
                ['555-1234','408 555-1234','(408) 555-1234','011 44 123'].every(value=>matches(RE_PHONE_COMMIT,value)) &&
                ['(408','(408 555-1234','408) 555-1234'].every(value=>matches(RE_PHONE_ENTRY,value));
            })();
        "#,
        );
    }
}
