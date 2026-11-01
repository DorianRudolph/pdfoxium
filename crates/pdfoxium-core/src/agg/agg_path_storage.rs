// Anti-Grain Geometry - Version 2.3
// Copyright (C) 2002-2005 Maxim Shemanarev (http://www.antigrain.com)
//
// Permission to copy, use, modify, sell and distribute this software
// is granted provided this copyright notice appears in all copies.
// This software is provided "as is" without express or implied
// warranty, and with no claim as to its suitability for any purpose.

//! Translation of `third_party/agg23/agg_path_storage.{h,cpp}`.
//! A contiguous vertex vector replaces the source's 256 entry blocks.

use super::agg_basics::{
    is_move_to, is_stop, is_vertex, VertexType, PATH_CMD_CURVE4, PATH_CMD_END_POLY,
    PATH_CMD_LINE_TO, PATH_CMD_MOVE_TO, PATH_CMD_STOP, PATH_FLAGS_CLOSE, PATH_FLAGS_JR,
};

pub trait AggVertexSource {
    fn rewind(&mut self, path_id: u32);
    fn vertex(&mut self, x: &mut f32, y: &mut f32) -> u32;
}

#[derive(Default)]
pub struct PathStorage {
    vertices: Vec<VertexType>,
    iterator: usize,
}

impl PathStorage {
    pub(crate) fn retained_bytes(&self) -> usize {
        self.vertices.capacity() * std::mem::size_of::<VertexType>()
    }
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.vertices.clear();
        self.iterator = 0;
    }

    pub fn total_vertices(&self) -> usize {
        self.vertices.len()
    }

    pub fn vertex_at(&self, index: usize, x: &mut f32, y: &mut f32) -> u32 {
        let vertex = &self.vertices[index];
        *x = vertex.x;
        *y = vertex.y;
        vertex.cmd
    }

    pub fn command(&self, index: usize) -> u32 {
        self.vertices[index].cmd
    }

    pub fn get_flag(&self, index: usize) -> u32 {
        self.command(index) & PATH_FLAGS_JR
    }

    pub fn last_vertex(&self, x: &mut f32, y: &mut f32) -> u32 {
        if self.vertices.is_empty() {
            PATH_CMD_STOP
        } else {
            self.vertex_at(self.vertices.len() - 1, x, y)
        }
    }

    pub fn prev_vertex(&self, x: &mut f32, y: &mut f32) -> u32 {
        if self.vertices.len() < 2 {
            PATH_CMD_STOP
        } else {
            self.vertex_at(self.vertices.len() - 2, x, y)
        }
    }

    pub fn move_to(&mut self, x: f32, y: f32) {
        self.add_vertex(x, y, PATH_CMD_MOVE_TO);
    }

    pub fn line_to(&mut self, x: f32, y: f32) {
        self.add_vertex(x, y, PATH_CMD_LINE_TO);
    }

    pub fn curve4(&mut self, control1: (f32, f32), control2: (f32, f32), to: (f32, f32)) {
        self.add_vertex(control1.0, control1.1, PATH_CMD_CURVE4);
        self.add_vertex(control2.0, control2.1, PATH_CMD_CURVE4);
        self.add_vertex(to.0, to.1, PATH_CMD_CURVE4);
    }

    pub fn add_path<S: AggVertexSource>(&mut self, source: &mut S, path_id: u32, solid_path: bool) {
        let (mut x, mut y) = (0.0, 0.0);
        source.rewind(path_id);
        loop {
            let mut command = source.vertex(&mut x, &mut y);
            if is_stop(command) {
                break;
            }
            if is_move_to(command) && solid_path && !self.vertices.is_empty() {
                command = PATH_CMD_LINE_TO;
            }
            self.add_vertex(x, y, command);
        }
    }

    pub fn rewind(&mut self, path_id: u32) {
        self.iterator = path_id as usize;
    }

    pub fn vertex(&mut self, x: &mut f32, y: &mut f32) -> u32 {
        if self.iterator >= self.vertices.len() {
            return PATH_CMD_STOP;
        }
        let command = self.vertex_at(self.iterator, x, y);
        self.iterator += 1;
        command
    }

    pub fn add_vertex(&mut self, x: f32, y: f32, command: u32) {
        // The source stores the command in an unsigned char block.
        self.vertices
            .push(VertexType::new(x, y, command as u8 as u32));
    }

    pub fn end_poly(&mut self) {
        if self.vertices.last().is_some_and(|v| is_vertex(v.cmd)) {
            self.add_vertex(0.0, 0.0, PATH_CMD_END_POLY | PATH_FLAGS_CLOSE);
        }
    }

    pub fn vertex_source(&self) -> PathVertexSource<'_> {
        PathVertexSource {
            path: self,
            vertex_index: 0,
        }
    }
}

impl AggVertexSource for PathStorage {
    fn rewind(&mut self, path_id: u32) {
        PathStorage::rewind(self, path_id);
    }

    fn vertex(&mut self, x: &mut f32, y: &mut f32) -> u32 {
        PathStorage::vertex(self, x, y)
    }
}

pub struct PathVertexSource<'a> {
    path: &'a PathStorage,
    vertex_index: usize,
}

impl AggVertexSource for PathVertexSource<'_> {
    fn rewind(&mut self, path_id: u32) {
        self.vertex_index = path_id as usize;
    }

    fn vertex(&mut self, x: &mut f32, y: &mut f32) -> u32 {
        if self.vertex_index >= self.path.total_vertices() {
            return PATH_CMD_STOP;
        }
        let command = self.path.vertex_at(self.vertex_index, x, y);
        self.vertex_index += 1;
        command
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_commands_and_sources_follow_storage_order() {
        let mut path = PathStorage::new();
        let (mut x, mut y) = (9.0, 9.0);
        assert_eq!(path.last_vertex(&mut x, &mut y), PATH_CMD_STOP);
        assert_eq!((x, y), (9.0, 9.0));
        path.move_to(1.0, 2.0);
        path.line_to(3.0, 4.0);
        path.curve4((5.0, 6.0), (7.0, 8.0), (9.0, 10.0));
        path.end_poly();
        path.end_poly();
        assert_eq!(path.total_vertices(), 6);
        assert_eq!(path.prev_vertex(&mut x, &mut y), PATH_CMD_CURVE4);
        assert_eq!((x, y), (9.0, 10.0));
        assert_eq!(
            path.last_vertex(&mut x, &mut y),
            PATH_CMD_END_POLY | PATH_FLAGS_CLOSE
        );
        assert_eq!(path.get_flag(5), 0);
        let mut source = path.vertex_source();
        source.rewind(1);
        assert_eq!(source.vertex(&mut x, &mut y), PATH_CMD_LINE_TO);
        assert_eq!((x, y), (3.0, 4.0));

        let mut combined = PathStorage::new();
        let mut source = path.vertex_source();
        combined.add_path(&mut source, 0, true);
        source.rewind(0);
        combined.add_path(&mut source, 0, true);
        assert_eq!(combined.command(6), PATH_CMD_LINE_TO);
    }
}
