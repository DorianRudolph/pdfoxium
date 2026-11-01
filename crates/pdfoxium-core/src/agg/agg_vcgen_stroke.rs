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

//! Translation of `third_party/agg23/agg_vcgen_stroke.{h,cpp}`.

use super::agg_basics::{
    get_close_flag, is_move_to, is_vertex, PATH_CMD_END_POLY, PATH_CMD_LINE_TO, PATH_CMD_MOVE_TO,
    PATH_CMD_STOP, PATH_FLAGS_CCW, PATH_FLAGS_CLOSE, PATH_FLAGS_CW, PATH_FLAGS_JR,
};
use super::agg_math_stroke::{
    stroke_calc_cap, stroke_calc_join, InnerJoin, LineCap, LineJoin, StrokePoints,
};
use super::agg_path_storage::AggVertexSource;
use super::agg_vertex_sequence::{VertexDistCmd, VertexSequence};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Status {
    Initial,
    Ready,
    Cap1,
    Cap2,
    Outline1,
    CloseFirst,
    Outline2,
    OutVertices,
    EndPoly1,
    EndPoly2,
    Stop,
}

pub struct VcgenStroke {
    src_vertices: VertexSequence<VertexDistCmd>,
    out_vertices: StrokePoints,
    width: f32,
    miter_limit: f32,
    inner_miter_limit: f32,
    approximation_scale: f32,
    line_cap: LineCap,
    line_join: LineJoin,
    inner_join: InnerJoin,
    closed: u32,
    status: Status,
    previous_status: Status,
    src_vertex: usize,
    out_vertex: usize,
}

impl Default for VcgenStroke {
    fn default() -> Self {
        Self {
            src_vertices: VertexSequence::new(),
            out_vertices: Vec::new(),
            width: 0.5,
            miter_limit: 4.0,
            inner_miter_limit: 1.01,
            approximation_scale: 1.0,
            line_cap: LineCap::Butt,
            line_join: LineJoin::Miter,
            inner_join: InnerJoin::Miter,
            closed: 0,
            status: Status::Initial,
            previous_status: Status::Stop,
            src_vertex: 0,
            out_vertex: 0,
        }
    }
}

impl VcgenStroke {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_line_cap(&mut self, cap: LineCap) {
        self.line_cap = cap;
    }

    pub fn line_cap(&self) -> LineCap {
        self.line_cap
    }

    pub fn set_line_join(&mut self, join: LineJoin) {
        self.line_join = join;
    }

    pub fn line_join(&self) -> LineJoin {
        self.line_join
    }

    pub fn set_inner_join(&mut self, join: InnerJoin) {
        self.inner_join = join;
    }

    pub fn inner_join(&self) -> InnerJoin {
        self.inner_join
    }

    pub fn set_width(&mut self, width: f32) {
        self.width = width / 2.0;
    }

    pub fn width(&self) -> f32 {
        self.width * 2.0
    }

    pub fn set_miter_limit(&mut self, limit: f32) {
        self.miter_limit = limit;
    }

    pub fn miter_limit(&self) -> f32 {
        self.miter_limit
    }

    pub fn set_inner_miter_limit(&mut self, limit: f32) {
        self.inner_miter_limit = limit;
    }

    pub fn inner_miter_limit(&self) -> f32 {
        self.inner_miter_limit
    }

    pub fn set_approximation_scale(&mut self, scale: f32) {
        self.approximation_scale = scale;
    }

    pub fn approximation_scale(&self) -> f32 {
        self.approximation_scale
    }

    pub fn remove_all(&mut self) {
        self.src_vertices.remove_all();
        self.closed = 0;
        self.status = Status::Initial;
    }

    pub fn add_vertex(&mut self, x: f32, y: f32, command: u32) {
        self.status = Status::Initial;
        if is_move_to(command) {
            self.src_vertices
                .modify_last(VertexDistCmd::new(x, y, command));
        } else if is_vertex(command) {
            self.src_vertices.add(VertexDistCmd::new(x, y, command));
        } else {
            self.closed = get_close_flag(command);
        }
    }

    fn vertex_at(&self, index: usize) -> VertexDistCmd {
        *self.src_vertices.get(index).expect("stroke source vertex")
    }

    fn around(&self, index: usize) -> (VertexDistCmd, VertexDistCmd, VertexDistCmd) {
        let len = self.src_vertices.len();
        (
            self.vertex_at(if index == 0 { len - 1 } else { index - 1 }),
            self.vertex_at(index),
            self.vertex_at(if index + 1 == len { 0 } else { index + 1 }),
        )
    }

    fn emit_join(&mut self, reversed: bool) {
        let (previous, current, next) = self.around(self.src_vertex);
        let join = if current.cmd & PATH_FLAGS_JR != 0 {
            LineJoin::MiterRound
        } else {
            self.line_join
        };
        let (v0, v1, v2, len1, len2) = if reversed {
            (
                next.vertex,
                current.vertex,
                previous.vertex,
                current.vertex.dist,
                previous.vertex.dist,
            )
        } else {
            (
                previous.vertex,
                current.vertex,
                next.vertex,
                previous.vertex.dist,
                current.vertex.dist,
            )
        };
        stroke_calc_join(
            &mut self.out_vertices,
            v0,
            v1,
            v2,
            len1,
            len2,
            self.width,
            join,
            self.inner_join,
            self.miter_limit,
            self.inner_miter_limit,
            self.approximation_scale,
        );
    }
}

impl AggVertexSource for VcgenStroke {
    fn rewind(&mut self, _path_id: u32) {
        if self.status == Status::Initial {
            self.src_vertices.close(self.closed != 0);
            if self.src_vertices.len() < 3 {
                self.closed = 0;
            }
        }
        self.status = Status::Ready;
        self.src_vertex = 0;
        self.out_vertex = 0;
    }

    fn vertex(&mut self, x: &mut f32, y: &mut f32) -> u32 {
        let mut command = PATH_CMD_LINE_TO;
        loop {
            match self.status {
                Status::Initial => self.rewind(0),
                Status::Ready => {
                    if self.src_vertices.len() < 2 + usize::from(self.closed != 0) {
                        return PATH_CMD_STOP;
                    }
                    self.status = if self.closed != 0 {
                        Status::Outline1
                    } else {
                        Status::Cap1
                    };
                    command = PATH_CMD_MOVE_TO;
                    self.src_vertex = 0;
                    self.out_vertex = 0;
                }
                Status::Cap1 => {
                    let v0 = self.vertex_at(0).vertex;
                    let v1 = self.vertex_at(1).vertex;
                    stroke_calc_cap(
                        &mut self.out_vertices,
                        v0,
                        v1,
                        v0.dist,
                        self.line_cap,
                        self.width,
                        self.approximation_scale,
                    );
                    self.src_vertex = 1;
                    self.previous_status = Status::Outline1;
                    self.status = Status::OutVertices;
                    self.out_vertex = 0;
                }
                Status::Cap2 => {
                    let last = self.src_vertices.len() - 1;
                    let v0 = self.vertex_at(last).vertex;
                    let v1 = self.vertex_at(last - 1).vertex;
                    stroke_calc_cap(
                        &mut self.out_vertices,
                        v0,
                        v1,
                        v1.dist,
                        self.line_cap,
                        self.width,
                        self.approximation_scale,
                    );
                    self.previous_status = Status::Outline2;
                    self.status = Status::OutVertices;
                    self.out_vertex = 0;
                }
                Status::Outline1 => {
                    if self.closed != 0 {
                        if self.src_vertex >= self.src_vertices.len() {
                            self.previous_status = Status::CloseFirst;
                            self.status = Status::EndPoly1;
                            continue;
                        }
                    } else if self.src_vertex >= self.src_vertices.len() - 1 {
                        self.status = Status::Cap2;
                        continue;
                    }
                    self.emit_join(false);
                    self.src_vertex += 1;
                    self.previous_status = Status::Outline1;
                    self.status = Status::OutVertices;
                    self.out_vertex = 0;
                }
                Status::CloseFirst => {
                    self.status = Status::Outline2;
                    command = PATH_CMD_MOVE_TO;
                }
                Status::Outline2 => {
                    if self.src_vertex <= usize::from(self.closed == 0) {
                        self.status = Status::EndPoly2;
                        self.previous_status = Status::Stop;
                        continue;
                    }
                    self.src_vertex -= 1;
                    self.emit_join(true);
                    self.previous_status = Status::Outline2;
                    self.status = Status::OutVertices;
                    self.out_vertex = 0;
                }
                Status::OutVertices => {
                    if self.out_vertex >= self.out_vertices.len() {
                        self.status = self.previous_status;
                    } else {
                        let (out_x, out_y) = self.out_vertices[self.out_vertex];
                        self.out_vertex += 1;
                        *x = out_x;
                        *y = out_y;
                        return command;
                    }
                }
                Status::EndPoly1 => {
                    self.status = self.previous_status;
                    return PATH_CMD_END_POLY | PATH_FLAGS_CLOSE | PATH_FLAGS_CCW;
                }
                Status::EndPoly2 => {
                    self.status = self.previous_status;
                    return PATH_CMD_END_POLY | PATH_FLAGS_CLOSE | PATH_FLAGS_CW;
                }
                Status::Stop => return PATH_CMD_STOP,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_segment_emits_caps_and_closed_outline() {
        let mut stroke = VcgenStroke::new();
        stroke.set_width(2.0);
        stroke.add_vertex(0.0, 0.0, PATH_CMD_MOVE_TO);
        stroke.add_vertex(10.0, 0.0, PATH_CMD_LINE_TO);
        let (mut x, mut y) = (0.0, 0.0);
        let mut vertices = Vec::new();
        loop {
            let command = stroke.vertex(&mut x, &mut y);
            if command == PATH_CMD_STOP {
                break;
            }
            vertices.push((command, x, y));
        }
        assert_eq!(vertices[0], (PATH_CMD_MOVE_TO, 0.0, 1.0));
        assert_eq!(vertices[1], (PATH_CMD_LINE_TO, 0.0, -1.0));
        assert_eq!(vertices[2], (PATH_CMD_LINE_TO, 10.0, -1.0));
        assert_eq!(vertices[3], (PATH_CMD_LINE_TO, 10.0, 1.0));
        assert_eq!(
            vertices.last().unwrap().0,
            PATH_CMD_END_POLY | PATH_FLAGS_CLOSE | PATH_FLAGS_CW
        );
    }

    #[test]
    fn closed_path_emits_two_oriented_outlines() {
        let mut stroke = VcgenStroke::new();
        stroke.add_vertex(0.0, 0.0, PATH_CMD_MOVE_TO);
        stroke.add_vertex(10.0, 0.0, PATH_CMD_LINE_TO);
        stroke.add_vertex(10.0, 10.0, PATH_CMD_LINE_TO);
        stroke.add_vertex(0.0, 0.0, PATH_CMD_END_POLY | PATH_FLAGS_CLOSE);
        let (mut x, mut y) = (0.0, 0.0);
        let mut commands = Vec::new();
        loop {
            let command = stroke.vertex(&mut x, &mut y);
            if command == PATH_CMD_STOP {
                break;
            }
            commands.push(command);
        }
        assert!(commands.contains(&(PATH_CMD_END_POLY | PATH_FLAGS_CLOSE | PATH_FLAGS_CCW)));
        assert!(commands.contains(&(PATH_CMD_END_POLY | PATH_FLAGS_CLOSE | PATH_FLAGS_CW)));
        assert_eq!(
            commands
                .iter()
                .filter(|&&cmd| cmd == PATH_CMD_MOVE_TO)
                .count(),
            2
        );
    }
}
