// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Source CFGAS_LinkUserData: shared immutable link text.
#[derive(Clone, Debug, Default)]
pub struct CfgasLinkUserData {
    pub url_content: Vec<u32>,
}
impl CfgasLinkUserData {
    pub fn new(text: &[u32]) -> Self {
        Self {
            url_content: text.to_vec(),
        }
    }
    pub fn get_link_url(&self) -> &[u32] {
        &self.url_content
    }
}
