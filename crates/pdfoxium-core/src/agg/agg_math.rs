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

//! Translation of `third_party/agg23/agg_math.h`.

pub const INTERSECTION_EPSILON: f32 = 1.0e-30;

pub fn calc_point_location(x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) -> f32 {
    ((x - x2) * (y2 - y1)) - ((y - y2) * (x2 - x1))
}

pub fn calc_distance(x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    let dx = x2 - x1;
    let dy = y2 - y1;
    (dx * dx + dy * dy).sqrt()
}

pub fn calc_line_point_distance(x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) -> f32 {
    let dx = x2 - x1;
    let dy = y2 - y1;
    let distance = (dx * dx + dy * dy).sqrt();
    if distance < INTERSECTION_EPSILON {
        return calc_distance(x1, y1, x, y);
    }
    ((x - x2) * dy / distance) - ((y - y2) * dx / distance)
}

#[allow(clippy::too_many_arguments)]
pub fn calc_intersection(
    ax: f32,
    ay: f32,
    bx: f32,
    by: f32,
    cx: f32,
    cy: f32,
    dx: f32,
    dy: f32,
    x: &mut f32,
    y: &mut f32,
) -> bool {
    let numerator = ((ay - cy) * (dx - cx)) - ((ax - cx) * (dy - cy));
    let denominator = ((bx - ax) * (dy - cy)) - ((by - ay) * (dx - cx));
    if denominator.abs() < INTERSECTION_EPSILON {
        return false;
    }
    *x = ax + ((bx - ax) * numerator / denominator);
    *y = ay + ((by - ay) * numerator / denominator);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distance_and_intersection_keep_source_order() {
        assert_eq!(calc_distance(0.0, 0.0, 3.0, 4.0), 5.0);
        assert_eq!(calc_line_point_distance(1.0, 1.0, 1.0, 1.0, 4.0, 5.0), 5.0);
        assert_eq!(calc_point_location(0.0, 0.0, 4.0, 0.0, 2.0, 1.0), -4.0);
        let (mut x, mut y) = (0.0, 0.0);
        assert!(calc_intersection(
            0.0, 0.0, 4.0, 4.0, 0.0, 4.0, 4.0, 0.0, &mut x, &mut y
        ));
        assert_eq!((x, y), (2.0, 2.0));
        assert!(!calc_intersection(
            0.0, 0.0, 4.0, 0.0, 0.0, 1.0, 4.0, 1.0, &mut x, &mut y
        ));
        assert_eq!((x, y), (2.0, 2.0));
    }
}
