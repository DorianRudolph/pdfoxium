// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `fxjs/cjs_timerobj.{h,cpp}`. The JS handle observes a timer weakly; retaining
//! or collecting it does not keep the host timer alive or cancel it.
use super::global_timer::GlobalTimer;
use boa_engine::{object::ObjectInitializer, Context, JsData, JsObject, JsValue};
use boa_gc::{Finalize, Trace};
use std::rc::{Rc, Weak};

#[derive(Trace, Finalize, JsData)]
struct CjsTimerObj {
    timer_id: i32,
    #[unsafe_ignore_trace]
    timer: Weak<GlobalTimer>,
}
pub fn timer_object(context: &mut Context, timer: &Rc<GlobalTimer>) -> JsObject {
    ObjectInitializer::with_native_data(
        CjsTimerObj {
            timer_id: timer.timer_id(),
            timer: Rc::downgrade(timer),
        },
        context,
    )
    .build()
}
pub fn clear_timer(value: &JsValue) {
    // As in ClearTimerCommon, nonobjects and objects of another native class
    // are accepted and ignored. No user-visible property is trusted as an ID.
    let timer = value.as_object().and_then(|object| {
        let data = object.downcast_ref::<CjsTimerObj>()?;
        data.timer
            .upgrade()
            .filter(|timer| timer.timer_id() == data.timer_id)
    });
    if let Some(timer) = timer {
        timer.cancel_from_app();
    }
}
