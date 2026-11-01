// Anti-Grain Geometry - Version 2.3
// Copyright (C) 2002-2005 Maxim Shemanarev (http://www.antigrain.com)
//
// Permission to copy, use, modify, sell and distribute this software
// is granted provided this copyright notice appears in all copies.
// This software is provided "as is" without express or implied
// warranty, and with no claim as to its suitability for any purpose.

//! Translation of `third_party/agg23/agg_vcgen_dash.{h,cpp}`.

use super::agg_basics::{
    get_close_flag, is_move_to, is_vertex, PATH_CMD_LINE_TO, PATH_CMD_MOVE_TO, PATH_CMD_STOP,
};
use super::agg_path_storage::AggVertexSource;
use super::agg_shorten_path::shorten_path;
use super::agg_vertex_sequence::{VertexDist, VertexSequence};

const MAX_DASHES: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Status {
    Initial,
    Ready,
    Polyline,
    Stop,
}

pub struct VcgenDash {
    is_dash: bool,
    dashes: [f32; MAX_DASHES],
    total_dash_len: f32,
    num_dashes: usize,
    dash_start: f32,
    shorten: f32,
    curr_dash_start: f32,
    curr_dash: usize,
    curr_rest: f32,
    v1: usize,
    v2: usize,
    src_vertices: VertexSequence<VertexDist>,
    closed: u32,
    status: Status,
    src_vertex: usize,
}

impl Default for VcgenDash {
    fn default() -> Self {
        Self {
            is_dash: true,
            dashes: [0.0; MAX_DASHES],
            total_dash_len: 0.0,
            num_dashes: 0,
            dash_start: 0.0,
            shorten: 0.0,
            curr_dash_start: 0.0,
            curr_dash: 0,
            curr_rest: 0.0,
            v1: 0,
            v2: 0,
            src_vertices: VertexSequence::new(),
            closed: 0,
            status: Status::Initial,
            src_vertex: 0,
        }
    }
}

impl VcgenDash {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn remove_all_dashes(&mut self) {
        self.is_dash = true;
        self.total_dash_len = 0.0;
        self.num_dashes = 0;
        self.curr_dash_start = 0.0;
        self.curr_dash = 0;
    }

    pub fn add_dash(&mut self, dash_len: f32) {
        if self.num_dashes < MAX_DASHES {
            self.total_dash_len += dash_len;
            self.dashes[self.num_dashes] = dash_len;
            self.num_dashes += 1;
        }
    }

    pub fn dash_start(&mut self, mut phase: f32) {
        assert!(self.total_dash_len > 0.0);
        if phase < 0.0 {
            let sum = self.total_dash_len * 2.0;
            phase += (-phase / sum).ceil() * sum;
        }
        assert!(phase >= 0.0);
        self.dash_start = phase;
        self.calc_dash_start(phase);
    }

    fn calc_dash_start(&mut self, mut phase: f32) {
        assert!(self.total_dash_len > 0.0);
        let cycle_len = if self.num_dashes % 2 == 1 {
            self.total_dash_len * 2.0
        } else {
            self.total_dash_len
        };
        phase -= (phase / cycle_len).floor() * cycle_len;
        self.curr_dash = 0;
        self.curr_dash_start = 0.0;
        self.is_dash = true;
        while phase > 0.0 {
            if phase > self.dashes[self.curr_dash] {
                phase -= self.dashes[self.curr_dash];
                self.to_next_dash();
                self.curr_dash_start = 0.0;
            } else {
                self.curr_dash_start = phase;
                phase = 0.0;
            }
        }
    }

    fn to_next_dash(&mut self) {
        self.curr_dash += 1;
        if self.curr_dash >= self.num_dashes {
            self.curr_dash = 0;
        }
        self.is_dash = !self.is_dash;
    }

    pub fn set_shorten(&mut self, shorten: f32) {
        self.shorten = shorten;
    }

    pub fn shorten(&self) -> f64 {
        f64::from(self.shorten)
    }

    pub fn remove_all(&mut self) {
        self.status = Status::Initial;
        self.src_vertices.remove_all();
        self.closed = 0;
    }

    pub fn add_vertex(&mut self, x: f32, y: f32, command: u32) {
        self.status = Status::Initial;
        if is_move_to(command) {
            self.src_vertices.modify_last(VertexDist::new(x, y));
        } else if is_vertex(command) {
            self.src_vertices.add(VertexDist::new(x, y));
        } else {
            self.closed = get_close_flag(command);
        }
    }

    fn current_vertices(&self) -> (VertexDist, VertexDist) {
        (
            *self.src_vertices.get(self.v1).expect("dash first vertex"),
            *self.src_vertices.get(self.v2).expect("dash second vertex"),
        )
    }
}

impl AggVertexSource for VcgenDash {
    fn rewind(&mut self, _path_id: u32) {
        if self.status == Status::Initial {
            self.src_vertices.close(self.closed != 0);
            shorten_path(&mut self.src_vertices, self.shorten, self.closed);
        }
        self.status = Status::Ready;
        self.src_vertex = 0;
    }

    fn vertex(&mut self, x: &mut f32, y: &mut f32) -> u32 {
        loop {
            match self.status {
                Status::Initial => self.rewind(0),
                Status::Ready => {
                    if self.src_vertices.len() < 2 {
                        return PATH_CMD_STOP;
                    }
                    self.status = Status::Polyline;
                    self.src_vertex = 1;
                    self.v1 = 0;
                    self.v2 = 1;
                    let (first, _) = self.current_vertices();
                    self.curr_rest = first.dist;
                    *x = first.x;
                    *y = first.y;
                    if self.dash_start >= 0.0 {
                        self.calc_dash_start(self.dash_start);
                    }
                    return PATH_CMD_MOVE_TO;
                }
                Status::Polyline => {
                    let dash_rest = self.dashes[self.curr_dash] - self.curr_dash_start;
                    let command = if self.is_dash {
                        PATH_CMD_LINE_TO
                    } else {
                        PATH_CMD_MOVE_TO
                    };
                    let (first, second) = self.current_vertices();
                    if self.curr_rest > dash_rest {
                        self.curr_rest -= dash_rest;
                        self.to_next_dash();
                        self.curr_dash_start = 0.0;
                        *x = second.x - (second.x - first.x) * self.curr_rest / first.dist;
                        *y = second.y - (second.y - first.y) * self.curr_rest / first.dist;
                    } else {
                        self.curr_dash_start += self.curr_rest;
                        *x = second.x;
                        *y = second.y;
                        self.src_vertex += 1;
                        self.v1 = self.v2;
                        self.curr_rest = second.dist;
                        if self.closed != 0 {
                            if self.src_vertex > self.src_vertices.len() {
                                self.status = Status::Stop;
                            } else {
                                self.v2 = if self.src_vertex >= self.src_vertices.len() {
                                    0
                                } else {
                                    self.src_vertex
                                };
                            }
                        } else if self.src_vertex >= self.src_vertices.len() {
                            self.status = Status::Stop;
                        } else {
                            self.v2 = self.src_vertex;
                        }
                    }
                    return command;
                }
                Status::Stop => return PATH_CMD_STOP,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agg::agg_basics::PATH_CMD_LINE_TO;

    #[test]
    fn even_dash_pattern_alternates_on_and_off_segments() {
        let mut dash = VcgenDash::new();
        dash.add_dash(3.0);
        dash.add_dash(2.0);
        dash.dash_start(0.0);
        dash.add_vertex(0.0, 0.0, PATH_CMD_MOVE_TO);
        dash.add_vertex(10.0, 0.0, PATH_CMD_LINE_TO);
        let (mut x, mut y) = (0.0, 0.0);
        let mut vertices = Vec::new();
        loop {
            let command = dash.vertex(&mut x, &mut y);
            if command == PATH_CMD_STOP {
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
                (PATH_CMD_LINE_TO, 8.0),
                (PATH_CMD_MOVE_TO, 10.0),
            ]
        );
    }

    #[test]
    fn odd_dash_pattern_uses_doubled_phase_cycle() {
        let mut dash = VcgenDash::new();
        for length in [5.0, 2.0, 1.0] {
            dash.add_dash(length);
        }
        dash.dash_start(9.0);
        dash.add_vertex(0.0, 0.0, PATH_CMD_MOVE_TO);
        dash.add_vertex(10.0, 0.0, PATH_CMD_LINE_TO);
        let (mut x, mut y) = (0.0, 0.0);
        assert_eq!(dash.vertex(&mut x, &mut y), PATH_CMD_MOVE_TO);
        assert_eq!(dash.vertex(&mut x, &mut y), PATH_CMD_MOVE_TO);
        assert_eq!(x, 4.0);
    }
}
