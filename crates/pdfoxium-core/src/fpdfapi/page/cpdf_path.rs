// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/page/cpdf_path.{h,cpp}`.

use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF};
use crate::fxcrt::shared_copy_on_write::SharedCopyOnWrite;
use crate::fxge::cfx_path::{CfxPath, PathPoint, PointType};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CpdfPath {
    path: SharedCopyOnWrite<CfxPath>,
}

impl CpdfPath {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn emplace(&mut self) {
        self.path.emplace(CfxPath::new());
    }
    pub fn has_ref(&self) -> bool {
        self.path.is_some()
    }
    pub fn get_object(&self) -> Option<&CfxPath> {
        self.path.get_object()
    }
    pub fn points(&self) -> &[PathPoint] {
        self.path
            .get_object()
            .expect("path has no reference")
            .points()
    }
    pub fn close_path(&mut self) {
        self.path.get_private_copy(CfxPath::new).close_path();
    }
    pub fn get_point(&self, index: usize) -> CfxPointF {
        self.path
            .get_object()
            .expect("path has no reference")
            .get_point(index)
    }
    pub fn bounding_box(&self) -> CfxFloatRect {
        self.path
            .get_object()
            .expect("path has no reference")
            .bounding_box()
    }
    pub fn bounding_box_for_stroke_path(&self, line_width: f32, miter_limit: f32) -> CfxFloatRect {
        self.path
            .get_object()
            .expect("path has no reference")
            .bounding_box_for_stroke_path(line_width, miter_limit)
    }
    pub fn is_rect(&self) -> bool {
        self.path
            .get_object()
            .expect("path has no reference")
            .is_rect()
    }
    pub fn rect_opposite_corner(&self) -> Option<CfxPointF> {
        self.path
            .get_object()
            .expect("path has no reference")
            .rect_opposite_corner()
    }
    pub fn transform(&mut self, matrix: CfxMatrix) {
        self.path.get_private_copy(CfxPath::new).transform(matrix);
    }
    pub fn append(&mut self, path: &CfxPath, matrix: Option<&CfxMatrix>) {
        self.path
            .get_private_copy(CfxPath::new)
            .append(path, matrix);
    }
    pub fn append_float_rect(&mut self, rect: CfxFloatRect) {
        self.path
            .get_private_copy(CfxPath::new)
            .append_float_rect(rect);
    }
    pub fn append_rect(&mut self, left: f32, bottom: f32, right: f32, top: f32) {
        self.path
            .get_private_copy(CfxPath::new)
            .append_rect(left, bottom, right, top);
    }
    pub fn append_point(&mut self, point: CfxPointF, point_type: PointType) {
        let mut path = CfxPath::new();
        path.append_point(point, point_type);
        self.append(&path, None);
    }
    pub fn append_point_and_close(&mut self, point: CfxPointF, point_type: PointType) {
        let mut path = CfxPath::new();
        path.append_point_and_close(point, point_type);
        self.append(&path, None);
    }
}

#[cfg(test)]
mod tests {
    use super::CpdfPath;
    use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxPointF};
    use crate::fxge::cfx_path::PointType;

    #[test]
    fn clones_share_path_until_mutation() {
        let mut original = CpdfPath::new();
        original.append_rect(1.0, 2.0, 3.0, 5.0);
        let mut copy = original.clone();
        assert_eq!(copy, original);
        copy.append_point(CfxPointF::new(8.0, 9.0), PointType::Move);
        assert_ne!(copy, original);
        assert_eq!(
            original.bounding_box(),
            CfxFloatRect::new(1.0, 2.0, 3.0, 5.0)
        );
    }
}
