// Anti-Grain Geometry - Version 2.3
// Copyright (C) 2002-2005 Maxim Shemanarev (http://www.antigrain.com)
//
// Permission to copy, use, modify, sell and distribute this software
// is granted provided this copyright notice appears in all copies.
// This software is provided "as is" without express or implied
// warranty, and with no claim as to its suitability for any purpose.

//! Integer rasterizer path from `third_party/agg23/agg_clip_liang_barsky.h`.

use super::agg_basics::Rect;

pub fn clipping_flags(x: i32, y: i32, clip_box: Rect) -> u32 {
    u32::from(x > clip_box.x2)
        | (u32::from(y > clip_box.y2) << 1)
        | (u32::from(x < clip_box.x1) << 2)
        | (u32::from(y < clip_box.y1) << 3)
}

pub fn clip_liang_barsky(
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
    clip_box: Rect,
    x: &mut [i32; 4],
    y: &mut [i32; 4],
) -> usize {
    const NEAR_ZERO: f32 = 1e-30;
    let mut delta_x = x2 as f32 - x1 as f32;
    let mut delta_y = y2 as f32 - y1 as f32;
    if !delta_x.is_finite() || !delta_y.is_finite() {
        return 0;
    }
    let mut count = 0;
    if delta_x == 0.0 {
        delta_x = if x1 > clip_box.x1 {
            -NEAR_ZERO
        } else {
            NEAR_ZERO
        };
    }
    let (xin, xout) = if delta_x > 0.0 {
        (clip_box.x1 as f32, clip_box.x2 as f32)
    } else {
        (clip_box.x2 as f32, clip_box.x1 as f32)
    };
    let tinx = (xin - x1 as f32) / delta_x;
    if delta_y == 0.0 {
        delta_y = if y1 > clip_box.y1 {
            -NEAR_ZERO
        } else {
            NEAR_ZERO
        };
    }
    let (yin, yout) = if delta_y > 0.0 {
        (clip_box.y1 as f32, clip_box.y2 as f32)
    } else {
        (clip_box.y2 as f32, clip_box.y1 as f32)
    };
    let tiny = (yin - y1 as f32) / delta_y;
    let (tin1, tin2) = if tinx < tiny {
        (tinx, tiny)
    } else {
        (tiny, tinx)
    };
    if tin1 <= 1.0 {
        if tin1 > 0.0 {
            x[count] = xin as i32;
            y[count] = yin as i32;
            count += 1;
        }
        if tin2 <= 1.0 {
            let toutx = (xout - x1 as f32) / delta_x;
            let touty = (yout - y1 as f32) / delta_y;
            let tout1 = if toutx < touty { toutx } else { touty };
            if tin2 > 0.0 || tout1 > 0.0 {
                if tin2 <= tout1 {
                    if tin2 > 0.0 {
                        if tinx > tiny {
                            x[count] = xin as i32;
                            y[count] = (y1 as f32 + delta_y * tinx) as i32;
                        } else {
                            x[count] = (x1 as f32 + delta_x * tiny) as i32;
                            y[count] = yin as i32;
                        }
                        count += 1;
                    }
                    if tout1 < 1.0 {
                        if toutx < touty {
                            x[count] = xout as i32;
                            y[count] = (y1 as f32 + delta_y * toutx) as i32;
                        } else {
                            x[count] = (x1 as f32 + delta_x * touty) as i32;
                            y[count] = yout as i32;
                        }
                    } else {
                        x[count] = x2;
                        y[count] = y2;
                    }
                    count += 1;
                } else {
                    if tinx > tiny {
                        x[count] = xin as i32;
                        y[count] = yout as i32;
                    } else {
                        x[count] = xout as i32;
                        y[count] = yin as i32;
                    }
                    count += 1;
                }
            }
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clips_horizontal_entry_and_reports_region_flags() {
        let clip = Rect::new(0, 0, 10, 10);
        assert_eq!(clipping_flags(-5, 5, clip), 4);
        assert_eq!(clipping_flags(12, 12, clip), 3);
        let mut x = [0; 4];
        let mut y = [0; 4];
        let count = clip_liang_barsky(-5, 5, 5, 5, clip, &mut x, &mut y);
        assert_eq!(count, 2);
        assert_eq!(&x[..count], &[0, 5]);
        assert_eq!(&y[..count], &[5, 5]);
    }
}
