// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Path and retained text clipping from `core/fpdfapi/page/cpdf_clippath.{h,cpp}`.

use super::cpdf_pageobject::PageObject;
use super::cpdf_path::CpdfPath;
use super::cpdf_textobject::CpdfTextObject;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix};
use crate::fxcrt::shared_copy_on_write::SharedCopyOnWrite;
use crate::fxge::cfx_fillrenderoptions::FillType;

#[derive(Clone, Default)]
struct PathData {
    paths: Vec<(CpdfPath, FillType)>,
    texts: Vec<Option<Box<CpdfTextObject>>>,
}

impl std::fmt::Debug for PathData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PathData")
            .field("paths", &self.paths)
            .field("text_count", &self.texts.len())
            .finish()
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CpdfClipPath {
    path_data: SharedCopyOnWrite<PathData>,
}

impl CpdfClipPath {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn emplace(&mut self) {
        self.path_data.emplace(PathData::default());
    }
    pub fn set_null(&mut self) {
        self.path_data.set_null();
    }
    pub fn has_ref(&self) -> bool {
        self.path_data.is_some()
    }

    pub fn path_count(&self) -> usize {
        self.path_data
            .get_object()
            .expect("clip path has no reference")
            .paths
            .len()
    }
    pub fn path(&self, index: usize) -> CpdfPath {
        self.path_data
            .get_object()
            .expect("clip path has no reference")
            .paths[index]
            .0
            .clone()
    }
    pub fn clip_type(&self, index: usize) -> FillType {
        self.path_data
            .get_object()
            .expect("clip path has no reference")
            .paths[index]
            .1
    }
    pub fn text_count(&self) -> usize {
        self.path_data
            .get_object()
            .expect("clip path has no reference")
            .texts
            .len()
    }
    pub fn text(&self, index: usize) -> Option<&CpdfTextObject> {
        self.path_data
            .get_object()
            .expect("clip path has no reference")
            .texts[index]
            .as_deref()
    }
    pub fn clip_box(&self) -> CfxFloatRect {
        let data = self
            .path_data
            .get_object()
            .expect("clip path has no reference");
        let mut rect = CfxFloatRect::default();
        let mut started = false;
        if let Some((first, _)) = data.paths.first() {
            rect = first.bounding_box();
            for (path, _) in &data.paths[1..] {
                rect.intersect(path.bounding_box());
            }
            started = true;
        }
        let mut layer_rect = CfxFloatRect::default();
        let mut layer_started = false;
        for text in &data.texts {
            if let Some(text) = text {
                let bounds = CfxFloatRect::from_fx_rect(text.base().get_bbox());
                if layer_started {
                    layer_rect.union(bounds);
                } else {
                    layer_rect = bounds;
                    layer_started = true;
                }
            } else {
                if started {
                    rect.intersect(layer_rect);
                } else {
                    rect = layer_rect;
                    started = true;
                }
                layer_started = false;
            }
        }
        rect
    }
    pub fn append_texts(&mut self, texts: &mut Vec<Box<CpdfTextObject>>) {
        const MAX_TEXT_OBJECTS: usize = 1024;
        let data = self.path_data.get_private_copy(PathData::default);
        if data.texts.len().saturating_add(texts.len()) <= MAX_TEXT_OBJECTS {
            data.texts.extend(texts.drain(..).map(Some));
            data.texts.push(None);
        }
        texts.clear();
    }
    pub fn append_path(&mut self, path: CpdfPath, fill_type: FillType) {
        self.path_data
            .get_private_copy(PathData::default)
            .paths
            .push((path, fill_type));
    }
    pub fn append_path_with_auto_merge(&mut self, path: CpdfPath, fill_type: FillType) {
        let paths = &mut self.path_data.get_private_copy(PathData::default).paths;
        if let Some((old, _)) = paths.last() {
            if old.is_rect() {
                let first = old.get_point(0);
                let third = old.get_point(2);
                let old_rect = CfxFloatRect::new(first.x, first.y, third.x, third.y);
                let new_rect = path.bounding_box();
                if old_rect.contains_rect(new_rect) {
                    paths.pop();
                }
            }
        }
        self.append_path(path, fill_type);
    }
    pub fn copy_clip_path(&mut self, other: &Self) {
        if self == other || !other.has_ref() {
            return;
        }
        for index in 0..other.path_count() {
            self.append_path(other.path(index), other.clip_type(index));
        }
    }
    pub fn transform(&mut self, matrix: CfxMatrix) {
        let data = self.path_data.get_private_copy(PathData::default);
        for (path, _) in &mut data.paths {
            path.transform(matrix);
        }
        for text in data.texts.iter_mut().flatten() {
            text.transform(matrix);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CpdfClipPath;
    use crate::fpdfapi::page::cpdf_path::CpdfPath;
    use crate::fxcrt::fx_coordinates::CfxFloatRect;
    use crate::fxge::cfx_fillrenderoptions::FillType;

    #[test]
    fn intersects_paths_and_preserves_shared_copies() {
        let mut outer = CpdfPath::new();
        outer.append_rect(0.0, 0.0, 10.0, 10.0);
        let mut inner = CpdfPath::new();
        inner.append_rect(2.0, 3.0, 8.0, 9.0);
        let mut clip = CpdfClipPath::new();
        clip.append_path(outer, FillType::Winding);
        let saved = clip.clone();
        clip.append_path(inner, FillType::EvenOdd);
        assert_eq!(saved.path_count(), 1);
        assert_eq!(clip.path_count(), 2);
        assert_eq!(clip.clip_box(), CfxFloatRect::new(2.0, 3.0, 8.0, 9.0));
    }
    #[test]
    fn text_layers_keep_bounds_copy_on_write_and_source_limit() {
        use super::{CpdfTextObject, PageObject};
        let text = |bounds| {
            let mut text = Box::new(CpdfTextObject::new(0));
            text.base_mut().set_rect(bounds);
            text
        };
        let mut clip = CpdfClipPath::new();
        let mut layer = vec![
            text(CfxFloatRect::new(1.2, 2.2, 5.1, 7.1)),
            text(CfxFloatRect::new(8.2, 3.1, 10.8, 8.4)),
        ];
        clip.append_texts(&mut layer);
        assert!(layer.is_empty());
        assert_eq!(clip.clip_box(), CfxFloatRect::new(1.0, 2.0, 11.0, 9.0));
        let saved = clip.clone();
        assert!(std::ptr::eq(saved.text(0).unwrap(), clip.text(0).unwrap()));
        clip.append_texts(&mut vec![text(CfxFloatRect::new(3.0, 4.0, 9.0, 10.0))]);
        assert!(!std::ptr::eq(saved.text(0).unwrap(), clip.text(0).unwrap()));
        assert_eq!(saved.text_count(), 3);
        assert_eq!(clip.clip_box(), CfxFloatRect::new(3.0, 4.0, 9.0, 9.0));
        let mut copy = CpdfClipPath::new();
        copy.copy_clip_path(&clip);
        assert!(!copy.has_ref()); // CopyClipPath intentionally copies paths only.
        let mut limit = CpdfClipPath::new();
        let mut batch = (0..1024).map(|_| text(CfxFloatRect::default())).collect();
        limit.append_texts(&mut batch);
        assert_eq!(limit.text_count(), 1025); // The separator is added after the limit check.
        let mut discarded = vec![text(CfxFloatRect::default())];
        limit.append_texts(&mut discarded);
        assert!(discarded.is_empty());
        assert_eq!(limit.text_count(), 1025);
    }
}
