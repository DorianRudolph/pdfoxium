// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Stock script messages from `fxjs/js_resources.{h,cpp}`. Strings retain UTF-16
//! code units at the engine boundary, including custom unpaired surrogates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum JsMessage {
    Alert = 1,
    ParamError,
    InvalidInputError,
    ParamTooLongError,
    ParseDateError,
    RangeBetweenError,
    RangeGreaterError,
    RangeLessError,
    NotSupportedError,
    BusyError,
    DuplicateEventError,
    SecondParamNotDateError,
    SecondParamInvalidDateError,
    GlobalNotFoundError,
    ReadOnlyError,
    TypeError,
    ValueError,
    PermissionError,
    BadObjectError,
    ObjectTypeError,
    UnknownProperty,
    InvalidSetError,
    UserGestureRequiredError,
    TooManyOccurrences,
    UnknownMethod,
    WouldBeCyclic,
}

pub fn js_get_string_from_id(message: JsMessage) -> Vec<u16> {
    use JsMessage::*;
    let text = match message {
        Alert => "Alert",
        ParamError => "Incorrect number of parameters passed to function.",
        InvalidInputError => "The input value is invalid.",
        ParamTooLongError => "The input value is too long.",
        ParseDateError => "The input value can't be parsed as a valid date/time (%ls).",
        RangeBetweenError => {
            "The input value must be greater than or equal to %ls and less than or equal to %ls."
        }
        RangeGreaterError => "The input value must be greater than or equal to %ls.",
        RangeLessError => "The input value must be less than or equal to %ls.",
        NotSupportedError => "Operation not supported.",
        BusyError => "System is busy.",
        DuplicateEventError => "Duplicate formfield event found.",
        SecondParamNotDateError => "The second parameter can't be converted to a Date.",
        SecondParamInvalidDateError => "The second parameter is an invalid Date.",
        GlobalNotFoundError => "Global value not found.",
        ReadOnlyError => "Cannot assign to readonly property.",
        TypeError => "Incorrect parameter type.",
        ValueError => "Incorrect parameter value.",
        PermissionError => "Permission denied.",
        BadObjectError => "Object no longer exists.",
        ObjectTypeError => "Object is of the wrong type.",
        UnknownProperty => "Unknown property.",
        InvalidSetError => "Set not possible, invalid or unknown.",
        UserGestureRequiredError => "User gesture required.",
        TooManyOccurrences => "Too many occurrences.",
        UnknownMethod => "Unknown method.",
        WouldBeCyclic => "Operation would create a cycle.",
    };
    text.encode_utf16().collect()
}

pub fn js_format_error_string(class: &str, property: Option<&str>, details: &[u16]) -> Vec<u16> {
    let mut result: Vec<u16> = class.encode_utf16().collect();
    if let Some(property) = property {
        result.push(u16::from(b'.'));
        result.extend(property.encode_utf16());
    }
    result.extend(": ".encode_utf16());
    result.extend_from_slice(details);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn formatted_errors_keep_class_property_and_utf16_details() {
        assert_eq!(
            js_get_string_from_id(JsMessage::BusyError),
            "System is busy.".encode_utf16().collect::<Vec<_>>()
        );
        assert_eq!(
            js_get_string_from_id(JsMessage::DuplicateEventError),
            "Duplicate formfield event found."
                .encode_utf16()
                .collect::<Vec<_>>()
        );
        let details = [0xd800, 0, 0xdc00];
        let mut expected: Vec<_> = "色.値: ".encode_utf16().collect();
        expected.extend(details);
        assert_eq!(js_format_error_string("色", Some("値"), &details), expected);
        assert_eq!(
            js_format_error_string("color", None, &[]),
            "color: ".encode_utf16().collect::<Vec<_>>()
        );
    }
}
