// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Direct `xfa/fxfa/cxfa_eventparam.{h,cpp}`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum XfaEventType {
    Click,
    Change,
    DocClose,
    DocReady,
    Enter,
    Exit,
    Full,
    IndexChange,
    Initialize,
    MouseDown,
    MouseEnter,
    MouseExit,
    MouseUp,
    PostExecute,
    PostOpen,
    PostPrint,
    PostSave,
    PostSign,
    PostSubmit,
    PreExecute,
    PreOpen,
    PrePrint,
    PreSave,
    PreSign,
    PreSubmit,
    Ready,
    InitCalculate,
    InitVariables,
    Calculate,
    Validate,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CxfaEventParam {
    pub event_type: XfaEventType,
    pub cancel_action: bool,
    pub key_down: bool,
    pub modifier: bool,
    pub reenter: bool,
    pub shift: bool,
    pub is_form_ready: bool,
    pub targeted: bool,
    pub commit_key: i32,
    pub sel_end: i32,
    pub sel_start: i32,
    pub result: Vec<u32>,
    pub change: Vec<u32>,
    pub full_text: Vec<u32>,
    pub new_content_type: Vec<u32>,
    pub prev_content_type: Vec<u32>,
    pub prev_text: Vec<u32>,
    pub soap_fault_code: Vec<u32>,
    pub soap_fault_string: Vec<u32>,
}
impl CxfaEventParam {
    pub fn new(event_type: XfaEventType) -> Self {
        Self {
            event_type,
            cancel_action: false,
            key_down: false,
            modifier: false,
            reenter: false,
            shift: false,
            is_form_ready: false,
            targeted: true,
            commit_key: 0,
            sel_end: 0,
            sel_start: 0,
            result: Vec::new(),
            change: Vec::new(),
            full_text: Vec::new(),
            new_content_type: Vec::new(),
            prev_content_type: Vec::new(),
            prev_text: Vec::new(),
            soap_fault_code: Vec::new(),
            soap_fault_string: Vec::new(),
        }
    }
    pub fn get_new_text(&self) -> Vec<u32> {
        // StringView::First/Substr return an empty view for an out-of-range
        // selection endpoint, including a negative i32 converted to size_t.
        let prefix = usize::try_from(self.sel_start)
            .ok()
            .and_then(|start| self.prev_text.get(..start))
            .unwrap_or(&[]);
        let suffix = usize::try_from(self.sel_end)
            .ok()
            .and_then(|end| self.prev_text.get(end..))
            .unwrap_or(&[]);
        let mut result = Vec::with_capacity(prefix.len() + self.change.len() + suffix.len());
        result.extend_from_slice(prefix);
        result.extend_from_slice(&self.change);
        result.extend_from_slice(suffix);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_change_selection_and_value_copy() {
        let mut event = CxfaEventParam::new(XfaEventType::Change);
        assert!(event.targeted);
        assert!(
            !(event.cancel_action
                || event.key_down
                || event.modifier
                || event.reenter
                || event.shift
                || event.is_form_ready)
        );
        assert_eq!(
            (event.commit_key, event.sel_end, event.sel_start),
            (0, 0, 0)
        );
        assert!(event.get_new_text().is_empty());
        event.prev_text = vec![65, 0x1f600, 66, 67];
        event.change = vec![88, 0, 89];
        event.sel_start = 1;
        event.sel_end = 3;
        assert_eq!(event.get_new_text(), [65, 88, 0, 89, 67]);
        let mut copy = event.clone();
        copy.change[0] = 90;
        assert_eq!(event.change[0], 88);
        copy.sel_start = 9;
        copy.sel_end = -1;
        assert_eq!(copy.get_new_text(), [90, 0, 89]);
        event.sel_start = 3;
        event.sel_end = 1;
        assert_eq!(
            event.get_new_text(),
            [65, 0x1f600, 66, 88, 0, 89, 0x1f600, 66, 67]
        );
    }
}
