// Adapted from PDFium's Anti-Grain Geometry 2.3 for pdfoxium.
// Distributed under the original permissive license notice below.
//
// Anti-Grain Geometry - Version 2.3
// Copyright (C) 2002-2005 Maxim Shemanarev (http://www.antigrain.com)
//
// Permission to copy, use, modify, sell and distribute this software
// is granted provided this copyright notice appears in all copies.
// This software is provided "as is" without express or implied
// warranty, and with no claim as to its suitability for any purpose.

//! Translation of `third_party/agg23/agg_conv_stroke.h`.

use super::agg_conv_adaptor_vcgen::{
    ConvAdaptorVcgen, NullMarkers, VertexGenerator, VertexMarkers,
};
use super::agg_math_stroke::{InnerJoin, LineCap, LineJoin};
use super::agg_path_storage::AggVertexSource;
use super::agg_vcgen_stroke::VcgenStroke;

pub struct ConvStroke<'a, S, M = NullMarkers> {
    adaptor: ConvAdaptorVcgen<'a, S, VcgenStroke, M>,
}

impl<'a, S: AggVertexSource> ConvStroke<'a, S, NullMarkers> {
    pub fn new(source: &'a mut S) -> Self {
        Self {
            adaptor: ConvAdaptorVcgen::new(source),
        }
    }
}

impl<'a, S: AggVertexSource, M: VertexMarkers + Default> ConvStroke<'a, S, M> {
    pub fn with_markers(source: &'a mut S) -> Self {
        Self {
            adaptor: ConvAdaptorVcgen::new(source),
        }
    }

    pub fn set_source(&mut self, source: &'a mut S) {
        self.adaptor.set_source(source);
    }

    pub fn generator(&mut self) -> &mut VcgenStroke {
        self.adaptor.generator()
    }

    pub fn markers(&mut self) -> &mut M {
        self.adaptor.markers()
    }

    pub fn set_line_cap(&mut self, cap: LineCap) {
        self.generator().set_line_cap(cap);
    }

    pub fn line_cap(&self) -> LineCap {
        self.adaptor.generator_ref().line_cap()
    }

    pub fn set_line_join(&mut self, join: LineJoin) {
        self.generator().set_line_join(join);
    }

    pub fn line_join(&self) -> LineJoin {
        self.adaptor.generator_ref().line_join()
    }

    pub fn set_inner_join(&mut self, join: InnerJoin) {
        self.generator().set_inner_join(join);
    }

    pub fn inner_join(&self) -> InnerJoin {
        self.adaptor.generator_ref().inner_join()
    }

    pub fn set_width(&mut self, width: f32) {
        self.generator().set_width(width);
    }

    pub fn width(&self) -> f32 {
        self.adaptor.generator_ref().width()
    }

    pub fn set_miter_limit(&mut self, limit: f32) {
        self.generator().set_miter_limit(limit);
    }

    pub fn miter_limit(&self) -> f32 {
        self.adaptor.generator_ref().miter_limit()
    }

    pub fn set_inner_miter_limit(&mut self, limit: f32) {
        self.generator().set_inner_miter_limit(limit);
    }

    pub fn inner_miter_limit(&self) -> f32 {
        self.adaptor.generator_ref().inner_miter_limit()
    }

    pub fn set_approximation_scale(&mut self, scale: f32) {
        self.generator().set_approximation_scale(scale);
    }

    pub fn approximation_scale(&self) -> f32 {
        self.adaptor.generator_ref().approximation_scale()
    }
}

impl<S: AggVertexSource, M: VertexMarkers> AggVertexSource for ConvStroke<'_, S, M> {
    fn rewind(&mut self, path_id: u32) {
        self.adaptor.rewind(path_id);
    }

    fn vertex(&mut self, x: &mut f32, y: &mut f32) -> u32 {
        self.adaptor.vertex(x, y)
    }
}

impl VertexGenerator for VcgenStroke {
    fn remove_all(&mut self) {
        VcgenStroke::remove_all(self);
    }

    fn add_vertex(&mut self, x: f32, y: f32, command: u32) {
        VcgenStroke::add_vertex(self, x, y, command);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agg::agg_basics::{
        PATH_CMD_END_POLY, PATH_CMD_LINE_TO, PATH_CMD_MOVE_TO, PATH_CMD_STOP,
    };
    use crate::agg::agg_conv_dash::ConvDash;
    use crate::agg::agg_path_storage::PathStorage;

    #[test]
    fn dashed_stroke_generates_separate_closed_outlines() {
        let mut path = PathStorage::new();
        path.move_to(0.0, 0.0);
        path.line_to(10.0, 0.0);
        let mut dash = ConvDash::new(&mut path);
        dash.add_dash(3.0);
        dash.add_dash(2.0);
        dash.dash_start(0.0);
        let mut stroke = ConvStroke::new(&mut dash);
        stroke.set_width(2.0);
        stroke.rewind(0);
        let (mut x, mut y) = (0.0, 0.0);
        let mut move_count = 0;
        let mut close_count = 0;
        loop {
            let command = stroke.vertex(&mut x, &mut y);
            if command == PATH_CMD_STOP {
                break;
            }
            if command == PATH_CMD_MOVE_TO {
                move_count += 1;
            }
            if command & 0x0f == PATH_CMD_END_POLY {
                close_count += 1;
            }
            assert!(command != PATH_CMD_LINE_TO || (0.0..=10.0).contains(&x));
        }
        assert_eq!(move_count, 2);
        assert_eq!(close_count, 2);
    }
}
