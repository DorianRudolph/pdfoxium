// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `xfa/fgas/graphics/cfgas_gegraphics.{h,cpp}` on the existing AGG device.
//! The device is borrowed; retained colors own their pattern/shading resources.
use super::{
    cfgas_gecolor::CfgasGeColor,
    cfgas_gepath::CfgasGePath,
    cfgas_geshading::{CfgasGeShading, ShadingType},
};
use crate::fxcrt::fx_coordinates::{CfxMatrix, CfxRectF, FxRect};
use crate::fxge::{
    agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
    cfx_fillrenderoptions::{CfxFillRenderOptions, FillType},
    cfx_graphstatedata::{CfxGraphStateData, LineCap},
    cfx_renderdevice::draw_path_agg,
    dib::{cfx_dibbase::transform_to, cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat},
};
use std::{
    ops::{Deref, DerefMut},
    rc::Rc,
};

// Each 16-pixel mono row occupies the source's four-byte aligned pitch.
const HATCH_BITMAP_DATA: [[u8; 64]; 6] = [
    [
        0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00,
    ],
    [
        0x80, 0x80, 0x00, 0x00, 0x80, 0x80, 0x00, 0x00, 0x80, 0x80, 0x00, 0x00, 0x80, 0x80, 0x00,
        0x00, 0x80, 0x80, 0x00, 0x00, 0x80, 0x80, 0x00, 0x00, 0x80, 0x80, 0x00, 0x00, 0x80, 0x80,
        0x00, 0x00, 0x80, 0x80, 0x00, 0x00, 0x80, 0x80, 0x00, 0x00, 0x80, 0x80, 0x00, 0x00, 0x80,
        0x80, 0x00, 0x00, 0x80, 0x80, 0x00, 0x00, 0x80, 0x80, 0x00, 0x00, 0x80, 0x80, 0x00, 0x00,
        0x80, 0x80, 0x00, 0x00,
    ],
    [
        0x80, 0x80, 0x00, 0x00, 0x40, 0x40, 0x00, 0x00, 0x20, 0x20, 0x00, 0x00, 0x10, 0x10, 0x00,
        0x00, 0x08, 0x08, 0x00, 0x00, 0x04, 0x04, 0x00, 0x00, 0x02, 0x02, 0x00, 0x00, 0x01, 0x01,
        0x00, 0x00, 0x80, 0x80, 0x00, 0x00, 0x40, 0x40, 0x00, 0x00, 0x20, 0x20, 0x00, 0x00, 0x10,
        0x10, 0x00, 0x00, 0x08, 0x08, 0x00, 0x00, 0x04, 0x04, 0x00, 0x00, 0x02, 0x02, 0x00, 0x00,
        0x01, 0x01, 0x00, 0x00,
    ],
    [
        0x01, 0x01, 0x00, 0x00, 0x02, 0x02, 0x00, 0x00, 0x04, 0x04, 0x00, 0x00, 0x08, 0x08, 0x00,
        0x00, 0x10, 0x10, 0x00, 0x00, 0x20, 0x20, 0x00, 0x00, 0x40, 0x40, 0x00, 0x00, 0x80, 0x80,
        0x00, 0x00, 0x01, 0x01, 0x00, 0x00, 0x02, 0x02, 0x00, 0x00, 0x04, 0x04, 0x00, 0x00, 0x08,
        0x08, 0x00, 0x00, 0x10, 0x10, 0x00, 0x00, 0x20, 0x20, 0x00, 0x00, 0x40, 0x40, 0x00, 0x00,
        0x80, 0x80, 0x00, 0x00,
    ],
    [
        0xff, 0xff, 0x00, 0x00, 0x80, 0x80, 0x00, 0x00, 0x80, 0x80, 0x00, 0x00, 0x80, 0x80, 0x00,
        0x00, 0x80, 0x80, 0x00, 0x00, 0x80, 0x80, 0x00, 0x00, 0x80, 0x80, 0x00, 0x00, 0x80, 0x80,
        0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0x80, 0x80, 0x00, 0x00, 0x80, 0x80, 0x00, 0x00, 0x80,
        0x80, 0x00, 0x00, 0x80, 0x80, 0x00, 0x00, 0x80, 0x80, 0x00, 0x00, 0x80, 0x80, 0x00, 0x00,
        0x80, 0x80, 0x00, 0x00,
    ],
    [
        0x81, 0x81, 0x00, 0x00, 0x42, 0x42, 0x00, 0x00, 0x24, 0x24, 0x00, 0x00, 0x18, 0x18, 0x00,
        0x00, 0x18, 0x18, 0x00, 0x00, 0x24, 0x24, 0x00, 0x00, 0x42, 0x42, 0x00, 0x00, 0x81, 0x81,
        0x00, 0x00, 0x81, 0x81, 0x00, 0x00, 0x42, 0x42, 0x00, 0x00, 0x24, 0x24, 0x00, 0x00, 0x18,
        0x18, 0x00, 0x00, 0x18, 0x18, 0x00, 0x00, 0x24, 0x24, 0x00, 0x00, 0x42, 0x42, 0x00, 0x00,
        0x81, 0x81, 0x00, 0x00,
    ],
];

#[derive(Clone, Default)]
struct GraphicsInfo {
    graph_state: CfxGraphStateData,
    ctm: CfxMatrix,
    act_on_dash: bool,
    stroke_color: CfgasGeColor,
    fill_color: CfgasGeColor,
}

pub struct CfgasGeGraphics<'device, 'bitmap, 'storage> {
    render_device: &'device mut CfxAggDeviceDriver<'bitmap, 'storage>,
    info: GraphicsInfo,
    info_stack: Vec<GraphicsInfo>,
}

impl<'device, 'bitmap, 'storage> CfgasGeGraphics<'device, 'bitmap, 'storage> {
    pub fn new(render_device: &'device mut CfxAggDeviceDriver<'bitmap, 'storage>) -> Self {
        Self {
            render_device,
            info: GraphicsInfo::default(),
            info_stack: Vec::new(),
        }
    }

    pub fn save_graph_state(&mut self) {
        self.render_device.save_state();
        self.info_stack.push(self.info.clone());
    }

    pub fn restore_graph_state(&mut self) -> bool {
        let Some(info) = self.info_stack.pop() else {
            return false;
        };
        self.render_device.restore_state(false);
        self.info = info;
        true
    }

    pub fn state_restorer(&mut self) -> GraphicsStateRestorer<'_, 'device, 'bitmap, 'storage> {
        self.save_graph_state();
        GraphicsStateRestorer { graphics: self }
    }

    pub fn set_line_cap(&mut self, cap: LineCap) {
        self.info.graph_state.set_line_cap(cap);
    }
    pub fn set_line_dash(&mut self, mut dash_array: Vec<f32>) -> bool {
        if dash_array.is_empty() || dash_array.iter().any(|v| !v.is_finite() || *v < 0.) {
            return false;
        }
        let scale = if self.info.act_on_dash {
            self.info.graph_state.line_width()
        } else {
            1.
        };
        for value in &mut dash_array {
            *value *= scale;
            if !value.is_finite() {
                return false;
            }
        }
        self.info.graph_state.set_dash_array(dash_array);
        self.info.graph_state.set_dash_phase(0.);
        true
    }
    pub fn set_solid_line_dash(&mut self) {
        self.info.graph_state.set_dash_array(Vec::new());
    }
    pub fn set_line_width(&mut self, width: f32) {
        if width.is_finite() && width >= 0. {
            self.info.graph_state.set_line_width(width);
        }
    }
    pub fn enable_act_on_dash(&mut self) {
        self.info.act_on_dash = true;
    }
    pub fn set_stroke_color(&mut self, color: &CfgasGeColor) {
        self.info.stroke_color = color.clone();
    }
    pub fn set_fill_color(&mut self, color: &CfgasGeColor) {
        self.info.fill_color = color.clone();
    }
    pub fn concat_matrix(&mut self, matrix: CfxMatrix) {
        if finite_matrix(matrix) {
            let mut result = self.info.ctm;
            result.concat(matrix);
            if finite_matrix(result) {
                self.info.ctm = result;
            }
        }
    }
    pub fn get_matrix(&self) -> CfxMatrix {
        self.info.ctm
    }
    pub fn get_clip_rect(&self) -> CfxRectF {
        let clip = self.render_device.clip_box();
        CfxRectF::new(
            clip.left as f32,
            clip.top as f32,
            clip.width() as f32,
            clip.height() as f32,
        )
    }
    pub fn set_clip_rect(&mut self, rect: CfxRectF) -> bool {
        if ![rect.left, rect.top, rect.right(), rect.bottom()]
            .into_iter()
            .all(f32::is_finite)
        {
            return false;
        }
        self.render_device.set_clip_rect(FxRect::new(
            rect.left.round() as i32,
            rect.top.round() as i32,
            rect.right().round() as i32,
            rect.bottom().round() as i32,
        ))
    }
    pub fn get_render_device(&mut self) -> &mut CfxAggDeviceDriver<'bitmap, 'storage> {
        self.render_device
    }

    pub fn stroke_path(&mut self, path: &CfgasGePath, matrix: CfxMatrix) -> bool {
        let CfgasGeColor::Solid(argb) = self.info.stroke_color else {
            return false;
        };
        let mut matrix_with_ctm = self.info.ctm;
        matrix_with_ctm.concat(matrix);
        if !valid_path_matrix(path, matrix_with_ctm) {
            return false;
        }
        draw_path_agg(
            self.render_device,
            path.get_path(),
            Some(matrix_with_ctm),
            Some(&self.info.graph_state),
            0,
            argb,
            CfxFillRenderOptions::default(),
        )
    }

    pub fn fill_path(
        &mut self,
        path: &CfgasGePath,
        fill_type: FillType,
        matrix: CfxMatrix,
    ) -> bool {
        if fill_type == FillType::NoFill {
            return false;
        }
        let mut matrix_with_ctm = self.info.ctm;
        matrix_with_ctm.concat(matrix);
        if !valid_path_matrix(path, matrix_with_ctm) {
            return false;
        }
        let options = CfxFillRenderOptions::new(fill_type);
        // Clone only the small Rc color owner so drawing can borrow the device.
        match self.info.fill_color.clone() {
            // Native calls CFX_RenderDevice::DrawPath, whose rectangle and
            // cosmetic-line handling precedes the AGG driver. Widget cells
            // depend on its hard rounding at fractional device coordinates.
            CfgasGeColor::Solid(argb) => draw_path_agg(
                self.render_device,
                path.get_path(),
                Some(matrix_with_ctm),
                Some(&self.info.graph_state),
                argb,
                0,
                options,
            ),
            CfgasGeColor::Pattern { pattern, .. } => {
                let Some(mut bitmap) = self.snapshot() else {
                    return false;
                };
                let Some(mut mask) = CfxDibitmap::create(16, 16, FxdibFormat::OneBppMask, 0) else {
                    return false;
                };
                mask.buffer_mut()
                    .copy_from_slice(&HATCH_BITMAP_DATA[pattern.get_hatch_style() as usize]);
                let bounds = matrix_with_ctm.transform_float_rect(path.get_path().bounding_box());
                // CFX_FloatRect uses upward top/bottom names; the bitmap uses
                // downward rows. Normalize before FillRect instead of passing
                // the source's inverted rectangle to the raster device.
                let rect = FxRect::new(
                    bounds.left.round() as i32,
                    bounds.bottom.round() as i32,
                    bounds.right.round() as i32,
                    bounds.top.round() as i32,
                );
                let mut clipped = rect;
                clipped.intersect(FxRect::new(0, 0, bitmap.width(), bitmap.height()));
                if !clipped.is_empty() {
                    let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
                    if !device.fill_rect(clipped, pattern.get_back_argb()) {
                        return false;
                    }
                    // Skip tiles outside the device without changing the
                    // pattern phase, including for very large path bounds.
                    let first_x = i64::from(rect.left)
                        + ((i64::from(clipped.left) - i64::from(rect.left)) / 16) * 16;
                    let first_y = i64::from(rect.top)
                        + ((i64::from(clipped.top) - i64::from(rect.top)) / 16) * 16;
                    let mut y = first_y;
                    while y < i64::from(clipped.bottom) {
                        let mut x = first_x;
                        while x < i64::from(clipped.right) {
                            if !device.set_dibits_normal(
                                &mask,
                                pattern.get_fore_argb(),
                                FxRect::new(0, 0, 16, 16),
                                x as i32,
                                y as i32,
                            ) {
                                return false;
                            }
                            x += 16;
                        }
                        y += 16;
                    }
                }
                self.composite_clipped(path, options, matrix_with_ctm, bitmap, CfxMatrix::default())
            }
            CfgasGeColor::Shading(shading) => {
                let begin = shading.get_begin_point();
                let end = shading.get_end_point();
                if ![
                    begin.x,
                    begin.y,
                    end.x,
                    end.y,
                    shading.get_begin_radius(),
                    shading.get_end_radius(),
                ]
                .into_iter()
                .all(f32::is_finite)
                    || shading.get_begin_radius() < 0.
                    || shading.get_end_radius() < 0.
                {
                    return false;
                }
                let Some(mut bitmap) = self.snapshot() else {
                    return false;
                };
                fill_shading(&mut bitmap, &shading);
                self.composite_clipped(path, options, matrix_with_ctm, bitmap, matrix_with_ctm)
            }
            CfgasGeColor::Invalid => false,
        }
    }

    fn snapshot(&self) -> Option<CfxDibitmap> {
        let mut bitmap = CfxDibitmap::create(
            self.render_device.pixel_width(),
            self.render_device.pixel_height(),
            FxdibFormat::Bgra,
            0,
        )?;
        self.render_device
            .get_dibits(&mut bitmap, 0, 0)
            .then_some(bitmap)
    }

    fn composite_clipped(
        &mut self,
        path: &CfgasGePath,
        options: CfxFillRenderOptions,
        clip_matrix: CfxMatrix,
        bitmap: CfxDibitmap,
        bitmap_matrix: CfxMatrix,
    ) -> bool {
        self.render_device.save_state();
        let result =
            self.render_device
                .set_clip_path_fill(path.get_path(), Some(clip_matrix), options)
                && self.set_dibits_with_matrix(bitmap, bitmap_matrix);
        self.render_device.restore_state(false);
        result
    }

    fn set_dibits_with_matrix(&mut self, source: CfxDibitmap, matrix: CfxMatrix) -> bool {
        if matrix.is_identity() {
            let rect = FxRect::new(0, 0, source.width(), source.height());
            return self.render_device.set_dibits_normal(&source, 0, rect, 0, 0);
        }
        // Source FlipImage(false, true) followed by TransformTo. The source is
        // our BGRA scratch bitmap, so reversing rows is the complete flip.
        let Some(mut flipped) =
            CfxDibitmap::create(source.width(), source.height(), source.format(), 0)
        else {
            return false;
        };
        for row in 0..source.height() {
            let from = source.scanline(row).expect("owned source row");
            let dest_row = source.height() - row - 1;
            flipped
                .scanline_mut(dest_row)
                .expect("owned destination row")
                .copy_from_slice(from);
        }
        let mut scaled = CfxMatrix::new(
            source.width() as f32,
            0.,
            0.,
            source.height() as f32,
            0.,
            0.,
        );
        scaled.concat(matrix);
        if !finite_matrix(scaled) {
            return false;
        }
        let (transformed, left, top) = transform_to(Rc::new(flipped), scaled);
        let Some(transformed) = transformed else {
            return false;
        };
        let bitmap = transformed.borrow();
        let rect = FxRect::new(0, 0, bitmap.width(), bitmap.height());
        self.render_device
            .set_dibits_normal(&*bitmap, 0, rect, left, top)
    }
}

pub struct GraphicsStateRestorer<'graphics, 'device, 'bitmap, 'storage> {
    graphics: &'graphics mut CfgasGeGraphics<'device, 'bitmap, 'storage>,
}
impl<'graphics, 'device, 'bitmap, 'storage> Deref
    for GraphicsStateRestorer<'graphics, 'device, 'bitmap, 'storage>
{
    type Target = CfgasGeGraphics<'device, 'bitmap, 'storage>;
    fn deref(&self) -> &Self::Target {
        self.graphics
    }
}
impl DerefMut for GraphicsStateRestorer<'_, '_, '_, '_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.graphics
    }
}
impl Drop for GraphicsStateRestorer<'_, '_, '_, '_> {
    fn drop(&mut self) {
        self.graphics.restore_graph_state();
    }
}

fn finite_matrix(matrix: CfxMatrix) -> bool {
    [matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f]
        .into_iter()
        .all(f32::is_finite)
}
fn valid_path_matrix(path: &CfgasGePath, matrix: CfxMatrix) -> bool {
    finite_matrix(matrix)
        && path.get_path().points().iter().all(|point| {
            let transformed = matrix.transform(point.point);
            transformed.x.is_finite() && transformed.y.is_finite()
        })
}

fn fill_shading(bitmap: &mut CfxDibitmap, shading: &CfgasGeShading) {
    let begin = shading.get_begin_point();
    let end = shading.get_end_point();
    let x_span = end.x - begin.x;
    let y_span = end.y - begin.y;
    let axis_len_square = x_span * x_span + y_span * y_span;
    let start_r = shading.get_begin_radius();
    let end_r = shading.get_end_radius();
    let a = (begin.x - end.x) * (begin.x - end.x) + (begin.y - end.y) * (begin.y - end.y)
        - (start_r - end_r) * (start_r - end_r);
    let width = bitmap.width() as usize;
    for row in 0..bitmap.height() {
        let pixels = bitmap.scanline_mut(row).expect("owned bitmap row");
        for (column, pixel) in pixels[..width * 4].chunks_exact_mut(4).enumerate() {
            let (x, y) = (column as f32, row as f32);
            let mut scale = match shading.get_type() {
                ShadingType::Axial => {
                    if axis_len_square == 0. {
                        0.
                    } else {
                        (((x - begin.x) * x_span) + ((y - begin.y) * y_span)) / axis_len_square
                    }
                }
                ShadingType::Radial => {
                    let b = -2.
                        * (((x - begin.x) * (end.x - begin.x))
                            + ((y - begin.y) * (end.y - begin.y))
                            + start_r * (end_r - start_r));
                    let c = (x - begin.x) * (x - begin.x) + (y - begin.y) * (y - begin.y)
                        - start_r * start_r;
                    if a == 0. {
                        -c / b
                    } else {
                        let discriminant = b * b - 4. * (a * c);
                        if discriminant < 0. {
                            continue;
                        }
                        let root = discriminant.sqrt();
                        let (s1, s2) = if a > 0. {
                            ((-b - root) / (2. * a), (-b + root) / (2. * a))
                        } else {
                            ((-b + root) / (2. * a), (-b - root) / (2. * a))
                        };
                        let s = if s2 <= 1. || shading.is_extended_end() {
                            s2
                        } else {
                            s1
                        };
                        if start_r + s * (end_r - start_r) < 0. {
                            continue;
                        }
                        s
                    }
                }
            };
            if scale.is_nan() || scale < 0. {
                if !shading.is_extended_begin() {
                    continue;
                }
                scale = 0.;
            }
            if scale > 1. {
                if !shading.is_extended_end() {
                    continue;
                }
                scale = 1.;
            }
            pixel.copy_from_slice(&shading.get_argb(scale).to_le_bytes());
        }
    }
}

#[cfg(test)]
#[path = "cfgas_gegraphics_tests.rs"]
mod tests;
