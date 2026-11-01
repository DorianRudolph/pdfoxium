// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `fxjs/cjs_color.{h,cpp}`. The static color object retains PDFium's color
//! storage/conversions and operation order behind Boa's ordinary value APIs.
//! Scalar native data is traced by derives; no untraced JS handles or unsafe
//! callback captures are used. Object/cell borrows end before user coercions.
use super::js_resources::{js_format_error_string, js_get_string_from_id, JsMessage};
use crate::fxge::cfx_color::{CfxColor, ColorType};
use boa_engine::{
    js_string,
    object::{builtins::JsArray, FunctionObjectBuilder, JsObject, ObjectInitializer},
    property::Attribute,
    Context, JsData, JsError, JsNativeError, JsResult, JsString, JsValue, NativeFunction,
};
use boa_gc::{Finalize, GcRefCell, Trace};

// The engine's small scalar native-data representation mirrors CFX_Color.
// Deriving Trace for scalars avoids custom unsafe tracing implementations.
#[derive(Clone, Debug, Finalize, Trace)]
struct ColorData {
    space: u8,
    components: [f32; 4],
}
impl From<CfxColor> for ColorData {
    fn from(color: CfxColor) -> Self {
        Self {
            space: color.color_type as u8,
            components: [color.color1, color.color2, color.color3, color.color4],
        }
    }
}
impl ColorData {
    fn color(&self) -> CfxColor {
        let [a, b, c, d] = self.components;
        match self.space {
            1 => CfxColor::make_gray(a),
            2 => CfxColor::make_rgb(a, b, c),
            3 => CfxColor::make_cmyk(a, b, c, d),
            _ => CfxColor::make_transparent(),
        }
    }
}
#[derive(Debug, Finalize, Trace, JsData)]
struct ColorBinding {
    colors: GcRefCell<[ColorData; 12]>,
}
impl Default for ColorBinding {
    fn default() -> Self {
        let colors = [
            CfxColor::make_gray(0.),
            CfxColor::make_rgb(0., 0., 1.),
            CfxColor::make_cmyk(1., 0., 0., 0.),
            CfxColor::make_gray(0.25),
            CfxColor::make_gray(0.5),
            CfxColor::make_rgb(0., 1., 0.),
            CfxColor::make_gray(0.75),
            CfxColor::make_cmyk(0., 1., 0., 0.),
            CfxColor::make_rgb(1., 0., 0.),
            CfxColor::make_transparent(),
            CfxColor::make_gray(1.),
            CfxColor::make_cmyk(0., 0., 1., 0.),
        ];
        Self {
            colors: GcRefCell::new(colors.map(ColorData::from)),
        }
    }
}

pub fn convert_pwl_color_to_array(context: &mut Context, color: CfxColor) -> JsValue {
    let components = [color.color1, color.color2, color.color3, color.color4];
    let (name, count) = match color.color_type {
        ColorType::Transparent => ("T", 0),
        ColorType::Gray => ("G", 1),
        ColorType::Rgb => ("RGB", 3),
        ColorType::Cmyk => ("CMYK", 4),
    };
    let values = std::iter::once(JsValue::from(JsString::from(name))).chain(
        components[..count]
            .iter()
            .map(|&number| JsValue::from(f64::from(number))),
    );
    JsArray::from_iter(values, context).into()
}

pub fn convert_array_to_pwl_color(context: &mut Context, array: &JsObject) -> JsResult<CfxColor> {
    let length = JsArray::from_object(array.clone())?.length(context)?;
    if length == 0 {
        return Ok(CfxColor::default());
    }
    let space = array.get(0u32, context)?.to_string(context)?;
    if space == js_string!("T") {
        return Ok(CfxColor::make_transparent());
    }
    let a = if length > 1 {
        array.get(1u32, context)?.to_number(context)? as f32
    } else {
        0.
    };
    if space == js_string!("G") {
        return Ok(CfxColor::make_gray(a));
    }
    let b = if length > 2 {
        array.get(2u32, context)?.to_number(context)? as f32
    } else {
        0.
    };
    let c = if length > 3 {
        array.get(3u32, context)?.to_number(context)? as f32
    } else {
        0.
    };
    if space == js_string!("RGB") {
        return Ok(CfxColor::make_rgb(a, b, c));
    }
    let d = if length > 4 {
        array.get(4u32, context)?.to_number(context)? as f32
    } else {
        0.
    };
    if space == js_string!("CMYK") {
        return Ok(CfxColor::make_cmyk(a, b, c, d));
    }
    Ok(CfxColor::default())
}

fn color_error(property: &str, message: JsMessage) -> JsError {
    let details = js_get_string_from_id(message);
    let formatted = js_format_error_string("color", Some(property), &details);
    // fxv8::ThrowExceptionHelper throws the UTF-16 message itself, not a new
    // Error object. Preserve that observable thrown value at this boundary.
    JsError::from_opaque(JsValue::from(JsString::from(formatted.as_slice())))
}
fn array_argument(value: &JsValue, method: &str) -> JsResult<JsObject> {
    value
        .as_object()
        .filter(JsObject::is_array)
        .ok_or_else(|| color_error(method, JsMessage::TypeError))
}
fn color_receiver(this: &JsValue) -> JsResult<JsObject> {
    this.as_object()
        .filter(|object| object.downcast_ref::<ColorBinding>().is_some())
        .ok_or_else(|| {
            JsNativeError::typ()
                .with_message("Illegal invocation")
                .into()
        })
}
fn property_holder(this: &JsValue) -> JsResult<JsObject> {
    let mut object = this.as_object();
    // Accessor inheritance mirrors the native callback's Holder, rather than
    // requiring an inherited property's receiver to contain native data itself.
    while let Some(current) = object {
        if current.downcast_ref::<ColorBinding>().is_some() {
            return Ok(current);
        }
        object = current.prototype();
    }
    Err(JsNativeError::typ()
        .with_message("Illegal invocation")
        .into())
}
fn get_property(this: &JsValue, context: &mut Context, index: usize) -> JsResult<JsValue> {
    let object = property_holder(this)?;
    let color = {
        let data = object
            .downcast_ref::<ColorBinding>()
            .expect("checked binding");
        let color = data.colors.borrow()[index].color();
        color
    };
    Ok(convert_pwl_color_to_array(context, color))
}
fn set_property(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
    index: usize,
    name: &str,
) -> JsResult<JsValue> {
    let object = property_holder(this)?;
    let value = args
        .first()
        .ok_or_else(|| color_error(name, JsMessage::ParamError))?;
    let array = array_argument(value, name)?;
    let color = convert_array_to_pwl_color(context, &array)?;
    object
        .downcast_ref::<ColorBinding>()
        .expect("checked binding")
        .colors
        .borrow_mut()[index] = color.into();
    Ok(JsValue::undefined())
}
macro_rules! color_property {
    ($get:ident, $set:ident, $index:expr, $name:literal) => {
        fn $get(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
            get_property(this, context, $index)
        }
        fn $set(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
            set_property(this, args, context, $index, $name)
        }
    };
}
color_property!(get_black, set_black, 0, "black");
color_property!(get_blue, set_blue, 1, "blue");
color_property!(get_cyan, set_cyan, 2, "cyan");
color_property!(get_dark_gray, set_dark_gray, 3, "dkGray");
color_property!(get_gray, set_gray, 4, "gray");
color_property!(get_green, set_green, 5, "green");
color_property!(get_light_gray, set_light_gray, 6, "ltGray");
color_property!(get_magenta, set_magenta, 7, "magenta");
color_property!(get_red, set_red, 8, "red");
color_property!(get_transparent, set_transparent, 9, "transparent");
color_property!(get_white, set_white, 10, "white");
color_property!(get_yellow, set_yellow, 11, "yellow");

fn convert(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    color_receiver(this)?;
    if args.len() < 2 {
        return Err(color_error("convert", JsMessage::ParamError));
    }
    let array = array_argument(&args[0], "convert")?;
    // The source converts the destination before reading source components.
    let space = args[1].to_string(context)?;
    let kind = if space == js_string!("G") {
        ColorType::Gray
    } else if space == js_string!("RGB") {
        ColorType::Rgb
    } else if space == js_string!("CMYK") {
        ColorType::Cmyk
    } else {
        ColorType::Transparent
    };
    let color = convert_array_to_pwl_color(context, &array)?;
    Ok(convert_pwl_color_to_array(
        context,
        color.convert_color_type(kind),
    ))
}
fn equal(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    color_receiver(this)?;
    if args.len() < 2 {
        return Err(color_error("equal", JsMessage::ParamError));
    }
    let first = array_argument(&args[0], "equal")?;
    let second = array_argument(&args[1], "equal")?;
    let first = convert_array_to_pwl_color(context, &first)?;
    let second = convert_array_to_pwl_color(context, &second)?;
    let best = first.color_type.max(second.color_type);
    Ok(JsValue::from(
        first.convert_color_type(best) == second.convert_color_type(best),
    ))
}

/// Define the complete static `color` host object in one document context.
pub fn register_color(context: &mut Context) -> JsResult<()> {
    type Callback = fn(&JsValue, &[JsValue], &mut Context) -> JsResult<JsValue>;
    let properties: [(&str, Callback, Callback); 12] = [
        ("black", get_black, set_black),
        ("blue", get_blue, set_blue),
        ("cyan", get_cyan, set_cyan),
        ("dkGray", get_dark_gray, set_dark_gray),
        ("gray", get_gray, set_gray),
        ("green", get_green, set_green),
        ("ltGray", get_light_gray, set_light_gray),
        ("magenta", get_magenta, set_magenta),
        ("red", get_red, set_red),
        ("transparent", get_transparent, set_transparent),
        ("white", get_white, set_white),
        ("yellow", get_yellow, set_yellow),
    ];
    let properties: Vec<_> = properties
        .into_iter()
        .map(|(name, get, set)| {
            (
                JsString::from(name),
                NativeFunction::from_fn_ptr(get).to_js_function(context.realm()),
                NativeFunction::from_fn_ptr(set).to_js_function(context.realm()),
            )
        })
        .collect();
    let convert = FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(convert))
        .name(js_string!("convert"))
        .length(0)
        .constructor(false)
        .build();
    let equal = FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(equal))
        .name(js_string!("equal"))
        .length(0)
        .constructor(false)
        .build();
    let mut object = ObjectInitializer::with_native_data(ColorBinding::default(), context);
    for (name, get, set) in properties {
        object.accessor(name, Some(get), Some(set), Attribute::all());
    }
    // V8's source methods are readonly, enumerable and configurable.
    object
        .property(
            js_string!("convert"),
            convert,
            Attribute::ENUMERABLE | Attribute::CONFIGURABLE,
        )
        .property(
            js_string!("equal"),
            equal,
            Attribute::ENUMERABLE | Attribute::CONFIGURABLE,
        );
    let object = object.build();
    context.register_global_property(js_string!("color"), object, Attribute::all())
}

#[cfg(test)]
mod tests {
    use super::*;
    use boa_engine::Source;
    fn context() -> Context {
        let mut context = Context::default();
        register_color(&mut context).unwrap();
        context
    }
    fn eval(context: &mut Context, script: &str) -> JsValue {
        context.eval(Source::from_bytes(script)).unwrap()
    }
    #[test]
    fn predefined_colors_copy_arrays_and_convert_all_spaces() {
        let mut context = context();
        let result = eval(
            &mut context,
            r#"
            JSON.stringify([color.transparent,color.black,color.white,color.red,color.green,color.blue,
                color.cyan,color.magenta,color.yellow,color.dkGray,color.gray,color.ltGray])
        "#,
        );
        assert_eq!(
            result.as_string().unwrap(),
            js_string!(
                r#"[["T"],["G",0],["G",1],["RGB",1,0,0],["RGB",0,1,0],["RGB",0,0,1],["CMYK",1,0,0,0],["CMYK",0,1,0,0],["CMYK",0,0,1,0],["G",0.25],["G",0.5],["G",0.75]]"#
            )
        );
        assert_eq!(
            eval(
                &mut context,
                "let copied=color.red; copied[1]=0; color.equal(color.red,['RGB',1,0,0])"
            ),
            JsValue::from(true)
        );
        assert_eq!(
            eval(
                &mut context,
                "JSON.stringify(color.convert(['G',0.5], 'RGB'))"
            )
            .as_string()
            .unwrap(),
            js_string!("[\"RGB\",0.5,0.5,0.5]")
        );
        assert_eq!(
            eval(&mut context, "color.equal(['G',0],['CMYK',0,0,0,1])"),
            JsValue::from(true)
        );
        assert_eq!(
            eval(
                &mut context,
                "JSON.stringify(color.convert(['RGB',1,0,0], 'unknown'))"
            )
            .as_string()
            .unwrap(),
            js_string!("[\"T\"]")
        );
        eval(
            &mut context,
            "color.black=['CMYK',0,0,0,1]; color.transparent=['G',0.25]",
        );
        assert_eq!(
            eval(
                &mut context,
                "color.equal(color.black,['G',0]) && color.equal(color.transparent,['G',0.25])"
            ),
            JsValue::from(true)
        );
    }
    #[test]
    fn defaults_missing_components_errors_and_coercion_order_match_source() {
        let mut context = context();
        assert_eq!(
            eval(
                &mut context,
                "JSON.stringify(color.convert(['RGB',1], 'RGB'))"
            )
            .as_string()
            .unwrap(),
            js_string!("[\"RGB\",1,0,0]")
        );
        assert_eq!(
            eval(&mut context, "JSON.stringify(color.convert([], 'RGB'))")
                .as_string()
                .unwrap(),
            js_string!("[\"T\"]")
        );
        assert_eq!(
            eval(&mut context, "try { color.convert([]); } catch(e) { e; }")
                .as_string()
                .unwrap(),
            js_string!("color.convert: Incorrect number of parameters passed to function.")
        );
        assert_eq!(
            eval(&mut context, "try { color.red=3; } catch(e) { e; }")
                .as_string()
                .unwrap(),
            js_string!("color.red: Incorrect parameter type.")
        );
        assert_eq!(
            eval(
                &mut context,
                "try { color.equal([],null); } catch(e) { e; }"
            )
            .as_string()
            .unwrap(),
            js_string!("color.equal: Incorrect parameter type.")
        );
        assert_eq!(
            eval(
                &mut context,
                r#"
            let order=[];
            let input=['RGB',{valueOf(){order.push('first');color.red=['G',0.5];return 1;}},0,0];
            color.convert(input,{toString(){order.push('dest');return 'RGB';}});
            order.join(',')==='dest,first' && color.equal(color.red,['G',0.5]);
        "#
            ),
            JsValue::from(true)
        );
        assert_eq!(eval(&mut context, "let sentinel={}; try{color.convert(['RGB',{valueOf(){throw sentinel;}}],'RGB');}catch(e){e===sentinel;}"), JsValue::from(true));
    }
    #[test]
    fn properties_are_per_object_and_gc_safe() {
        let mut context = context();
        let color = context
            .global_object()
            .get(js_string!("color"), &mut context)
            .unwrap();
        context
            .register_global_property(js_string!("originalColor"), color, Attribute::all())
            .unwrap();
        register_color(&mut context).unwrap();
        eval(&mut context, "originalColor.red=['G',0.25]");
        boa_gc::force_collect();
        assert_eq!(eval(&mut context, "color.equal(color.red,['RGB',1,0,0]) && originalColor.equal(originalColor.red,['G',0.25])"), JsValue::from(true));
        assert_eq!(
            eval(
                &mut context,
                "Object.getOwnPropertyDescriptor(color,'equal').writable"
            ),
            JsValue::from(false)
        );
        assert_eq!(
            eval(
                &mut context,
                "let inherited=Object.create(color); color.equal(inherited.red,color.red)"
            ),
            JsValue::from(true)
        );
    }
}
