// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Selected graphics lifecycle from `core/fxge/cfx_gemodule.{h,cpp}`.
//! AGG and Fontations are fixed for this build. The portable folder provider
//! selects the host's lookup policy; native graphics hooks are excluded.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use super::cfx_fontmgr::CfxFontMgr;
#[cfg(not(target_os = "macos"))]
use super::cfx_linuxfontinfo::CfxLinuxFontInfo as DefaultFontInfo;
#[cfg(target_os = "macos")]
use super::cfx_macfontinfo::CfxMacFontInfo as DefaultFontInfo;

pub struct CfxGeModule {
    font_mgr: CfxFontMgr,
    user_font_paths: Option<Vec<PathBuf>>,
}

impl CfxGeModule {
    fn new(user_font_paths: Option<Vec<PathBuf>>) -> Self {
        let font_mgr = CfxFontMgr::new();
        // None selects defaults; Some(empty) deliberately scans no paths.
        let provider = match user_font_paths.clone() {
            Some(paths) => DefaultFontInfo::from_paths(paths),
            None => DefaultFontInfo::default(),
        };
        font_mgr
            .builtin_mapper_mut()
            .set_system_font_info(Some(Box::new(provider)));
        Self {
            font_mgr,
            user_font_paths,
        }
    }

    pub fn create_default_system_font_info(
        &self,
    ) -> Box<dyn super::systemfontinfo_iface::SystemFontInfo> {
        Box::new(match self.user_font_paths.clone() {
            Some(paths) => DefaultFontInfo::from_paths(paths),
            None => DefaultFontInfo::default(),
        })
    }
    pub fn font_mgr(&self) -> &CfxFontMgr {
        &self.font_mgr
    }
}

thread_local! {
    // PDFium's API requires serialized use. Thread-local ownership also lets
    // independent Rust clients retain non-Send font faces safely.
    static GE_MODULE: RefCell<Option<Rc<CfxGeModule>>> = const { RefCell::new(None) };
}

pub fn initialize_ge_module(user_font_paths: Option<Vec<PathBuf>>) {
    GE_MODULE.with(|slot| {
        if slot.borrow().is_none() {
            *slot.borrow_mut() = Some(Rc::new(CfxGeModule::new(user_font_paths)));
        }
    });
}

/// Initializes the current thread, or checks that its existing font paths match.
/// Unlike the C API's idempotent initialization, safe configuration reports a
/// changed startup setting instead of silently ignoring it. Existing font
/// managers and font faces are never replaced by this operation.
pub fn initialize_ge_module_with_paths(user_font_paths: Option<Vec<PathBuf>>) -> bool {
    GE_MODULE.with(|slot| {
        if let Some(module) = slot.borrow().as_ref() {
            return module.user_font_paths == user_font_paths;
        }
        let module = Rc::new(CfxGeModule::new(user_font_paths));
        *slot.borrow_mut() = Some(module);
        true
    })
}

pub fn destroy_ge_module() {
    let previous = GE_MODULE.with(|slot| slot.borrow_mut().take());
    drop(previous);
}

pub fn with_ge_module<R>(f: impl FnOnce(&CfxGeModule) -> R) -> R {
    initialize_ge_module(None);
    let module = GE_MODULE
        .with(|slot| slot.borrow().clone())
        .expect("graphics module initialized");
    f(&module)
}
