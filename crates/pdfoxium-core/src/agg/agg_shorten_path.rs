// Anti-Grain Geometry - Version 2.3
// Copyright (C) 2002-2005 Maxim Shemanarev (http://www.antigrain.com)
//
// Permission to copy, use, modify, sell and distribute this software
// is granted provided this copyright notice appears in all copies.
// This software is provided "as is" without express or implied
// warranty, and with no claim as to its suitability for any purpose.

//! `third_party/agg23/agg_shorten_path.h` for the dash generator's vertices.

use super::agg_vertex_sequence::{VertexDist, VertexDistance, VertexSequence};

pub fn shorten_path(vertices: &mut VertexSequence<VertexDist>, mut shorten: f32, closed: u32) {
    if shorten <= 0.0 || vertices.len() <= 1 {
        return;
    }
    let mut index = vertices.len() - 2;
    while index != 0 {
        let distance = vertices.get(index).expect("vertex index").dist;
        if distance > shorten {
            break;
        }
        vertices.remove_last();
        shorten -= distance;
        index -= 1;
    }
    if vertices.len() < 2 {
        vertices.remove_all();
        return;
    }
    let last_index = vertices.len() - 1;
    let previous = *vertices.get(last_index - 1).expect("previous vertex");
    let last = *vertices.get(last_index).expect("last vertex");
    let ratio = (previous.dist - shorten) / previous.dist;
    let x = previous.x + (last.x - previous.x) * ratio;
    let y = previous.y + (last.y - previous.y) * ratio;
    let mut adjusted = last;
    adjusted.x = x;
    adjusted.y = y;
    *vertices.get_mut(last_index).expect("last vertex") = adjusted;
    let keep_last = {
        let previous = vertices.get_mut(last_index - 1).expect("previous vertex");
        previous.test_against(&adjusted)
    };
    if !keep_last {
        vertices.remove_last();
    }
    vertices.close(closed != 0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortens_last_segment_and_updates_distance() {
        let mut vertices = VertexSequence::new();
        vertices.add(VertexDist::new(0.0, 0.0));
        vertices.add(VertexDist::new(10.0, 0.0));
        vertices.add(VertexDist::new(20.0, 0.0));
        vertices.close(false);
        shorten_path(&mut vertices, 5.0, 0);
        assert_eq!(vertices.len(), 3);
        assert_eq!(vertices.get(2).unwrap().x, 15.0);
        assert_eq!(vertices.get(1).unwrap().dist, 5.0);
    }

    #[test]
    fn shortening_past_path_extrapolates_final_segment_like_agg() {
        let mut vertices = VertexSequence::new();
        vertices.add(VertexDist::new(0.0, 0.0));
        vertices.add(VertexDist::new(1.0, 0.0));
        vertices.add(VertexDist::new(2.0, 0.0));
        vertices.close(false);
        shorten_path(&mut vertices, 5.0, 0);
        assert_eq!(vertices.len(), 2);
        assert_eq!(vertices.get(1).unwrap().x, -3.0);
    }
}
