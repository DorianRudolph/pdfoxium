// Anti-Grain Geometry - Version 2.3
// Copyright (C) 2002-2005 Maxim Shemanarev (http://www.antigrain.com)
//
// Permission to copy, use, modify, sell and distribute this software
// is granted provided this copyright notice appears in all copies.
// This software is provided "as is" without express or implied
// warranty, and with no claim as to its suitability for any purpose.

//! Translation of `third_party/agg23/agg_conv_adaptor_vcgen.h`.

use super::agg_basics::{
    is_end_poly, is_move_to, is_stop, is_vertex, PATH_CMD_LINE_TO, PATH_CMD_MOVE_TO, PATH_CMD_STOP,
};
use super::agg_path_storage::AggVertexSource;

pub trait VertexGenerator: AggVertexSource {
    fn remove_all(&mut self);
    fn add_vertex(&mut self, x: f32, y: f32, command: u32);
}

pub trait VertexMarkers: AggVertexSource {
    fn remove_all(&mut self);
    fn add_vertex(&mut self, x: f32, y: f32, command: u32);
    fn prepare_src(&mut self);
}

#[derive(Default)]
pub struct NullMarkers;

impl AggVertexSource for NullMarkers {
    fn rewind(&mut self, _path_id: u32) {}

    fn vertex(&mut self, _x: &mut f32, _y: &mut f32) -> u32 {
        PATH_CMD_STOP
    }
}

impl VertexMarkers for NullMarkers {
    fn remove_all(&mut self) {}

    fn add_vertex(&mut self, _x: f32, _y: f32, _command: u32) {}

    fn prepare_src(&mut self) {}
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Status {
    Initial,
    Accumulate,
    Generate,
}

pub struct ConvAdaptorVcgen<'a, S, G, M = NullMarkers> {
    source: &'a mut S,
    generator: G,
    markers: M,
    status: Status,
    last_cmd: u32,
    start_x: f32,
    start_y: f32,
}

impl<'a, S: AggVertexSource, G: VertexGenerator + Default, M: VertexMarkers + Default>
    ConvAdaptorVcgen<'a, S, G, M>
{
    pub fn new(source: &'a mut S) -> Self {
        Self {
            source,
            generator: G::default(),
            markers: M::default(),
            status: Status::Initial,
            last_cmd: PATH_CMD_STOP,
            start_x: 0.0,
            start_y: 0.0,
        }
    }
}

impl<'a, S: AggVertexSource, G: VertexGenerator, M: VertexMarkers> ConvAdaptorVcgen<'a, S, G, M> {
    pub fn set_source(&mut self, source: &'a mut S) {
        self.source = source;
    }

    pub fn generator(&mut self) -> &mut G {
        &mut self.generator
    }

    pub fn generator_ref(&self) -> &G {
        &self.generator
    }

    pub fn markers(&mut self) -> &mut M {
        &mut self.markers
    }

    pub fn markers_ref(&self) -> &M {
        &self.markers
    }
}

impl<S: AggVertexSource, G: VertexGenerator, M: VertexMarkers> AggVertexSource
    for ConvAdaptorVcgen<'_, S, G, M>
{
    fn rewind(&mut self, path_id: u32) {
        self.source.rewind(path_id);
        self.status = Status::Initial;
    }

    fn vertex(&mut self, x: &mut f32, y: &mut f32) -> u32 {
        loop {
            if self.status == Status::Initial {
                self.markers.remove_all();
                self.last_cmd = self.source.vertex(&mut self.start_x, &mut self.start_y);
                self.status = Status::Accumulate;
            }
            if self.status == Status::Accumulate {
                if is_stop(self.last_cmd) {
                    return PATH_CMD_STOP;
                }
                self.generator.remove_all();
                self.generator
                    .add_vertex(self.start_x, self.start_y, PATH_CMD_MOVE_TO);
                self.markers
                    .add_vertex(self.start_x, self.start_y, PATH_CMD_MOVE_TO);
                loop {
                    let command = self.source.vertex(x, y);
                    if is_vertex(command) {
                        self.last_cmd = command;
                        if is_move_to(command) {
                            self.start_x = *x;
                            self.start_y = *y;
                            break;
                        }
                        self.generator.add_vertex(*x, *y, command);
                        self.markers.add_vertex(*x, *y, PATH_CMD_LINE_TO);
                    } else if is_stop(command) {
                        self.last_cmd = PATH_CMD_STOP;
                        break;
                    } else if is_end_poly(command) {
                        self.generator.add_vertex(*x, *y, command);
                        break;
                    }
                }
                self.generator.rewind(0);
                self.status = Status::Generate;
            }
            let command = self.generator.vertex(x, y);
            if is_stop(command) {
                self.status = Status::Accumulate;
            } else {
                return command;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agg::agg_path_storage::PathStorage;
    use crate::agg::agg_vcgen_dash::VcgenDash;

    #[test]
    fn separates_subpaths_before_generating_dashes() {
        let mut path = PathStorage::new();
        path.move_to(0.0, 0.0);
        path.line_to(5.0, 0.0);
        path.move_to(10.0, 0.0);
        path.line_to(15.0, 0.0);
        let mut converter = ConvAdaptorVcgen::<_, VcgenDash>::new(&mut path);
        converter.generator().add_dash(3.0);
        converter.generator().add_dash(2.0);
        converter.generator().dash_start(0.0);
        converter.rewind(0);
        let (mut x, mut y) = (0.0, 0.0);
        let mut vertices = Vec::new();
        loop {
            let command = converter.vertex(&mut x, &mut y);
            if is_stop(command) {
                break;
            }
            vertices.push((command, x));
        }
        assert_eq!(
            vertices,
            vec![
                (PATH_CMD_MOVE_TO, 0.0),
                (PATH_CMD_LINE_TO, 3.0),
                (PATH_CMD_MOVE_TO, 5.0),
                (PATH_CMD_MOVE_TO, 10.0),
                (PATH_CMD_LINE_TO, 13.0),
                (PATH_CMD_MOVE_TO, 15.0),
            ]
        );
    }
}
