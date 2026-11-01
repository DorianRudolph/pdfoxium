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

//! Translation of `third_party/agg23/agg_vertex_sequence.h`.
//! A Rust vector replaces the source's block deque for retained vertices.

use super::agg_math::calc_distance;

pub const VERTEX_DIST_EPSILON: f32 = 1e-14;

pub trait VertexDistance {
    fn test_against(&mut self, other: &Self) -> bool;
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VertexDist {
    pub x: f32,
    pub y: f32,
    pub dist: f32,
}

impl VertexDist {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y, dist: 0.0 }
    }
}

impl VertexDistance for VertexDist {
    fn test_against(&mut self, other: &Self) -> bool {
        self.dist = calc_distance(self.x, self.y, other.x, other.y);
        self.dist > VERTEX_DIST_EPSILON
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct VertexDistCmd {
    pub vertex: VertexDist,
    pub cmd: u32,
}

impl VertexDistCmd {
    pub const fn new(x: f32, y: f32, cmd: u32) -> Self {
        Self {
            vertex: VertexDist::new(x, y),
            cmd,
        }
    }
}

impl VertexDistance for VertexDistCmd {
    fn test_against(&mut self, other: &Self) -> bool {
        self.vertex.test_against(&other.vertex)
    }
}

#[derive(Default)]
pub struct VertexSequence<T> {
    vertices: Vec<T>,
}

impl<T: Clone + VertexDistance> VertexSequence<T> {
    pub fn new() -> Self {
        Self {
            vertices: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.vertices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        self.vertices.get(index)
    }

    pub fn get_mut(&mut self, index: usize) -> Option<&mut T> {
        self.vertices.get_mut(index)
    }

    pub fn remove_last(&mut self) -> Option<T> {
        self.vertices.pop()
    }

    pub fn remove_all(&mut self) {
        self.vertices.clear();
    }

    pub fn add(&mut self, value: T) {
        let len = self.vertices.len();
        if len > 1 {
            let (before, last) = self.vertices.split_at_mut(len - 1);
            if !before[len - 2].test_against(&last[0]) {
                self.vertices.pop();
            }
        }
        self.vertices.push(value);
    }

    pub fn modify_last(&mut self, value: T) {
        self.vertices.pop();
        self.add(value);
    }

    pub fn close(&mut self, closed: bool) {
        while self.vertices.len() > 1 {
            let len = self.vertices.len();
            let (before, last) = self.vertices.split_at_mut(len - 1);
            if before[len - 2].test_against(&last[0]) {
                break;
            }
            let vertex = self.vertices.pop().unwrap();
            self.modify_last(vertex);
        }
        if closed {
            while self.vertices.len() > 1 {
                let (first, rest) = self.vertices.split_at_mut(1);
                if rest.last_mut().unwrap().test_against(&first[0]) {
                    break;
                }
                self.vertices.pop();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_vertices_collapse_and_distances_update() {
        let mut sequence = VertexSequence::new();
        sequence.add(VertexDist::new(0.0, 0.0));
        sequence.add(VertexDist::new(1.0, 0.0));
        sequence.add(VertexDist::new(1.0, 0.0));
        sequence.add(VertexDist::new(2.0, 0.0));
        sequence.close(false);
        assert_eq!(sequence.len(), 3);
        assert_eq!(sequence.get(0).unwrap().dist, 1.0);
        assert_eq!(sequence.get(1).unwrap().dist, 1.0);
        sequence.modify_last(VertexDist::new(0.0, 0.0));
        sequence.close(true);
        assert_eq!(sequence.len(), 2);
    }
}
