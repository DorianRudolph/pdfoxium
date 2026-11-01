// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2020 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! `xfa/fgas/font/cfgas_gemodule.{h,cpp}`. The Rust engine's existing per-thread
//! module convention replaces the native global pointer. Fonts own weak manager
//! links; callers may retain a module during operations without a RefCell borrow.
use super::cfgas_fontmgr::CfgasFontMgr;
use std::{cell::RefCell, rc::Rc};
thread_local! {static MODULE:RefCell<Option<Rc<CfgasGeModule>>>=const{RefCell::new(None)};}
pub struct CfgasGeModule {
    font_mgr: Rc<CfgasFontMgr>,
}
impl CfgasGeModule {
    pub fn create() {
        MODULE.with(|slot| {
            let mut slot = slot.borrow_mut();
            assert!(slot.is_none(), "FGAS module already initialized");
            *slot = Some(Rc::new(Self {
                font_mgr: Rc::new(CfgasFontMgr::new()),
            }));
        });
    }
    pub fn destroy() {
        MODULE.with(|slot| {
            slot.borrow_mut().take();
        });
    }
    pub fn get() -> Rc<Self> {
        MODULE.with(|slot| {
            let mut slot = slot.borrow_mut();
            slot.get_or_insert_with(|| {
                Rc::new(Self {
                    font_mgr: Rc::new(CfgasFontMgr::new()),
                })
            })
            .clone()
        })
    }
    pub fn font_mgr(&self) -> &Rc<CfgasFontMgr> {
        &self.font_mgr
    }
}
pub fn font_manager() -> Rc<CfgasFontMgr> {
    CfgasGeModule::get().font_mgr.clone()
}
pub fn initialize_ge_module() {
    CfgasGeModule::create();
}
pub fn destroy_ge_module() {
    CfgasGeModule::destroy();
}
