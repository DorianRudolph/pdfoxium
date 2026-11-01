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

//! Translation of `third_party/agg23/agg_conv_dash.h`.

use super::agg_conv_adaptor_vcgen::{
    ConvAdaptorVcgen, NullMarkers, VertexGenerator, VertexMarkers,
};
use super::agg_path_storage::AggVertexSource;
use super::agg_vcgen_dash::VcgenDash;

pub struct ConvDash<'a, S, M = NullMarkers> {
    adaptor: ConvAdaptorVcgen<'a, S, VcgenDash, M>,
}

impl<'a, S: AggVertexSource> ConvDash<'a, S, NullMarkers> {
    pub fn new(source: &'a mut S) -> Self {
        Self {
            adaptor: ConvAdaptorVcgen::new(source),
        }
    }
}

impl<'a, S: AggVertexSource, M: VertexMarkers + Default> ConvDash<'a, S, M> {
    pub fn with_markers(source: &'a mut S) -> Self {
        Self {
            adaptor: ConvAdaptorVcgen::new(source),
        }
    }

    pub fn set_source(&mut self, source: &'a mut S) {
        self.adaptor.set_source(source);
    }

    pub fn generator(&mut self) -> &mut VcgenDash {
        self.adaptor.generator()
    }

    pub fn markers(&mut self) -> &mut M {
        self.adaptor.markers()
    }

    pub fn remove_all_dashes(&mut self) {
        self.generator().remove_all_dashes();
    }

    pub fn add_dash(&mut self, dash_len: f32) {
        self.generator().add_dash(dash_len);
    }

    pub fn dash_start(&mut self, phase: f32) {
        self.generator().dash_start(phase);
    }

    pub fn set_shorten(&mut self, shorten: f32) {
        self.generator().set_shorten(shorten);
    }

    pub fn shorten(&self) -> f64 {
        self.adaptor.generator_ref().shorten()
    }
}

impl<S: AggVertexSource, M: VertexMarkers> AggVertexSource for ConvDash<'_, S, M> {
    fn rewind(&mut self, path_id: u32) {
        self.adaptor.rewind(path_id);
    }

    fn vertex(&mut self, x: &mut f32, y: &mut f32) -> u32 {
        self.adaptor.vertex(x, y)
    }
}

impl VertexGenerator for VcgenDash {
    fn remove_all(&mut self) {
        VcgenDash::remove_all(self);
    }

    fn add_vertex(&mut self, x: f32, y: f32, command: u32) {
        VcgenDash::add_vertex(self, x, y, command);
    }
}
