//! Bounded transport of signed RGB differences.
//!
//! Work in encoded sRGB, where coverage is blended in these renderer outputs.
//! Keep the common value at each pixel fixed; match only surplus/deficit within
//! a Euclidean radius. This conservative residual model is NOT full transport
//! of the images' ink: a broad stroke shifted one pixel can have residuals more
//! than one pixel apart. Channels are independent, another explicit limitation.
//! A match requires contrast within the transport-radius neighborhood in both
//! images. Uniform fills cannot explain new light/dark seams as redistribution;
//! equal-gray samples on an existing antialiased stroke can still redistribute.
use std::collections::VecDeque;

#[derive(Clone, Debug)]
struct Edge {
    to: usize,
    reverse: usize,
    capacity: i32,
    cost: i32,
}

#[derive(Default, Debug)]
pub struct FlowStats {
    pub matched: u64,
    pub work: f64,
    pub dx: f64,
    pub dy: f64,
    pub components: usize,
    pub largest_component: usize,
}

pub struct Outcome {
    pub residual: Vec<f32>,
    pub tolerant_residual: Vec<f32>,
    pub tolerated: f64,
    pub movement: Vec<f32>,
    pub remaining: u64,
    pub initial: u64,
    pub stats: FlowStats,
}

fn edge(graph: &mut [Vec<Edge>], from: usize, to: usize, cap: i32, cost: i32) -> usize {
    let (a, b) = (graph[from].len(), graph[to].len());
    graph[from].push(Edge {
        to,
        reverse: b,
        capacity: cap,
        cost,
    });
    graph[to].push(Edge {
        to: from,
        reverse: a,
        capacity: 0,
        cost: -cost,
    });
    a
}

// Successive shortest augmenting paths, including reverse edges to undo an
// earlier assignment. SPFA is adequate for this *small-component prototype*;
// it is not claimed to have linear runtime on large connected photo regions.
fn solve(graph: &mut [Vec<Edge>], source: usize, sink: usize) -> u64 {
    let n = graph.len();
    let mut distance = vec![i64::MAX; n];
    let mut previous = vec![(0, 0); n];
    let mut queued = vec![false; n];
    let mut queue = VecDeque::new();
    let mut matched = 0;
    loop {
        distance.fill(i64::MAX);
        queued.fill(false);
        queue.clear();
        distance[source] = 0;
        queue.push_back(source);
        queued[source] = true;
        while let Some(u) = queue.pop_front() {
            queued[u] = false;
            for (i, e) in graph[u].iter().enumerate() {
                if e.capacity > 0 && distance[e.to] > distance[u] + i64::from(e.cost) {
                    distance[e.to] = distance[u] + i64::from(e.cost);
                    previous[e.to] = (u, i);
                    if !queued[e.to] {
                        queued[e.to] = true;
                        queue.push_back(e.to);
                    }
                }
            }
        }
        if distance[sink] == i64::MAX {
            break;
        }
        let mut amount = i32::MAX;
        let mut v = sink;
        while v != source {
            let (u, i) = previous[v];
            amount = amount.min(graph[u][i].capacity);
            v = u;
        }
        v = sink;
        while v != source {
            let (u, i) = previous[v];
            let back = graph[u][i].reverse;
            graph[u][i].capacity -= amount;
            graph[v][back].capacity += amount;
            v = u;
        }
        matched += amount as u64;
    }
    matched
}

#[cfg(test)]
pub fn compare(a: &[u8], b: &[u8], width: usize, height: usize, radius: usize) -> Outcome {
    compare_with_solver(a, b, width, height, radius, false)
}

// Dinic blocking flow, with an explicit DFS stack. Minimizes unmatched mass
// exactly but does not minimize transport distance among maximum matchings.
fn solve_max_flow(graph: &mut [Vec<Edge>], source: usize, sink: usize) -> u64 {
    let mut levels = vec![usize::MAX; graph.len()];
    let mut cursor = vec![0; graph.len()];
    let mut queue = VecDeque::new();
    let mut path: Vec<(usize, usize)> = Vec::new();
    let mut matched = 0;
    loop {
        levels.fill(usize::MAX);
        levels[source] = 0;
        queue.clear();
        queue.push_back(source);
        while let Some(u) = queue.pop_front() {
            for e in &graph[u] {
                if e.capacity > 0 && levels[e.to] == usize::MAX {
                    levels[e.to] = levels[u] + 1;
                    queue.push_back(e.to);
                }
            }
        }
        if levels[sink] == usize::MAX {
            break;
        }
        cursor.fill(0);
        loop {
            path.clear();
            let mut u = source;
            loop {
                if u == sink {
                    break;
                }
                while cursor[u] < graph[u].len() {
                    let e = &graph[u][cursor[u]];
                    if e.capacity > 0 && levels[e.to] == levels[u] + 1 {
                        break;
                    }
                    cursor[u] += 1;
                }
                if cursor[u] == graph[u].len() {
                    levels[u] = usize::MAX;
                    if let Some((parent, _)) = path.pop() {
                        u = parent;
                        cursor[u] += 1;
                    } else {
                        break;
                    }
                } else {
                    let i = cursor[u];
                    path.push((u, i));
                    u = graph[u][i].to;
                }
            }
            if u != sink {
                break;
            }
            let amount = path
                .iter()
                .map(|&(u, i)| graph[u][i].capacity)
                .min()
                .unwrap();
            for &(u, i) in &path {
                let (v, reverse) = (graph[u][i].to, graph[u][i].reverse);
                graph[u][i].capacity -= amount;
                graph[v][reverse].capacity += amount;
            }
            matched += amount as u64;
        }
    }
    matched
}

#[cfg(test)]
pub fn compare_with_solver(
    a: &[u8],
    b: &[u8],
    width: usize,
    height: usize,
    radius: usize,
    fast: bool,
) -> Outcome {
    compare_with_leeway(a, b, width, height, radius, fast, 0.)
}

pub fn compare_with_leeway(
    a: &[u8],
    b: &[u8],
    width: usize,
    height: usize,
    radius: usize,
    fast: bool,
    stroke_leeway: f32,
) -> Outcome {
    compare_with_tolerances(a, b, width, height, radius, fast, stroke_leeway, 0, 1.)
}

pub fn compare_with_tolerances(
    a: &[u8],
    b: &[u8],
    width: usize,
    height: usize,
    radius: usize,
    fast: bool,
    stroke_leeway: f32,
    channel_leeway: u8,
    edge_mass_divisor: f32,
) -> Outcome {
    assert_eq!(a.len(), width * height * 3);
    assert_eq!(a.len(), b.len());
    assert!((1..=2).contains(&radius));
    assert!(stroke_leeway.is_finite() && (0.0..=1.0).contains(&stroke_leeway));
    let pixels = width * height;
    let mut result = Outcome {
        residual: vec![0.; pixels],
        tolerant_residual: vec![0.; pixels],
        tolerated: 0.,
        movement: vec![0.; pixels],
        initial: 0,
        remaining: 0,
        stats: FlowStats::default(),
    };
    let radius = radius as isize;
    let mut offsets: Vec<_> = (-radius..=radius)
        .flat_map(|dy| (-radius..=radius).map(move |dx| (dx, dy)))
        .filter(|&(dx, dy)| dx * dx + dy * dy > 0 && dx * dx + dy * dy <= radius * radius)
        .collect();
    offsets.sort_by_key(|&(dx, dy)| dx * dx + dy * dy);
    let neighbors = |p: usize| {
        let (px, py) = ((p % width) as isize, (p / width) as isize);
        offsets.iter().filter_map(move |&(dx, dy)| {
            let (x, y) = (px + dx, py + dy);
            (x >= 0 && y >= 0 && x < width as isize && y < height as isize)
                .then(|| y as usize * width + x as usize)
        })
    };
    let mut deltas = vec![0_i16; pixels];
    let mut seen = vec![false; pixels];
    let mut local = vec![usize::MAX; pixels];
    let mut near_edge = vec![false; pixels];
    let mut contrast = vec![0_u8; pixels];
    let mut component = Vec::new();
    for channel in 0..3 {
        seen.fill(false);
        for p in 0..pixels {
            // Positive = reference has surplus ink; negative = test needs ink.
            deltas[p] = i16::from(b[p * 3 + channel]) - i16::from(a[p * 3 + channel]);
            result.initial += u64::from(deltas[p].unsigned_abs());
        }
        near_edge.fill(false);
        contrast.fill(0);
        for p in 0..pixels {
            if deltas[p] == 0 {
                continue;
            }
            let (x, y) = (p % width, p / width);
            let (mut low_a, mut high_a) = (a[p * 3 + channel], a[p * 3 + channel]);
            let (mut low_b, mut high_b) = (b[p * 3 + channel], b[p * 3 + channel]);
            let context_radius = radius as usize;
            for yy in y.saturating_sub(context_radius)..=(y + context_radius).min(height - 1) {
                for xx in x.saturating_sub(context_radius)..=(x + context_radius).min(width - 1) {
                    let q = yy * width + xx;
                    low_a = low_a.min(a[q * 3 + channel]);
                    high_a = high_a.max(a[q * 3 + channel]);
                    low_b = low_b.min(b[q * 3 + channel]);
                    high_b = high_b.max(b[q * 3 + channel]);
                }
            }
            contrast[p] = (high_a - low_a).min(high_b - low_b);
            near_edge[p] = contrast[p] > 0;
        }
        for seed in 0..pixels {
            if seen[seed] || deltas[seed] == 0 {
                continue;
            }
            component.clear();
            component.push(seed);
            seen[seed] = true;
            let mut head = 0;
            while head < component.len() {
                let p = component[head];
                head += 1;
                for q in neighbors(p) {
                    if !seen[q]
                        && i32::from(deltas[p]) * i32::from(deltas[q]) < 0
                        && near_edge[p]
                        && near_edge[q]
                    {
                        seen[q] = true;
                        component.push(q);
                    }
                }
            }
            result.stats.components += 1;
            result.stats.largest_component = result.stats.largest_component.max(component.len());
            // No opposite neighbor: nothing can be transported.
            if component.len() == 1 {
                continue;
            }
            let n = component.len();
            let source = n;
            let sink = n + 1;
            let mut graph = vec![Vec::new(); n + 2];
            for (i, &p) in component.iter().enumerate() {
                local[p] = i;
            }
            let mut links = Vec::new();
            for (i, &p) in component.iter().enumerate() {
                if deltas[p] > 0 {
                    edge(&mut graph, source, i, i32::from(deltas[p]), 0);
                    for q in neighbors(p).filter(|&q| deltas[q] < 0 && near_edge[p] && near_edge[q])
                    {
                        let dx = (q % width) as i32 - (p % width) as i32;
                        let dy = (q / width) as i32 - (p / width) as i32;
                        let distance = f64::from(dx * dx + dy * dy).sqrt();
                        let cap = i32::from(deltas[p].min(-deltas[q]));
                        let e = edge(
                            &mut graph,
                            i,
                            local[q],
                            cap,
                            (distance * 1000.).round() as i32,
                        );
                        links.push((i, e, p, q, cap, dx, dy, distance));
                    }
                } else {
                    edge(&mut graph, i, sink, i32::from(-deltas[p]), 0);
                }
            }
            result.stats.matched += if fast {
                solve_max_flow(&mut graph, source, sink)
            } else {
                solve(&mut graph, source, sink)
            };
            for (i, e, p, q, cap, dx, dy, distance) in links {
                let moved = cap - graph[i][e].capacity;
                deltas[p] -= moved as i16;
                deltas[q] += moved as i16;
                let work = f64::from(moved) * distance;
                result.stats.work += work;
                result.stats.dx += f64::from(moved * dx);
                result.stats.dy += f64::from(moved * dy);
                // Half the travel work at each endpoint, in RGB-equivalent pixel units.
                result.movement[p] += (work / (2. * 3. * 255.)) as f32;
                result.movement[q] += (work / (2. * 3. * 255.)) as f32;
            }
        }
        for p in 0..pixels {
            result.remaining += u64::from(deltas[p].unsigned_abs());
            let raw = f32::from(deltas[p].unsigned_abs());
            // Explicit edge slack, independent of transport conservation. Use
            // the smaller local range so a missing feature or a previously
            // flat fill cannot supply its own allowance. This is per-channel
            // contrast tolerance, not a percent change in whole-glyph ink area.
            let tolerant =
                (raw - stroke_leeway * f32::from(contrast[p]) - f32::from(channel_leeway)).max(0.);
            result.residual[p] = result.residual[p].max(raw / 255.);
            let edge_weight = 1. + (edge_mass_divisor - 1.) * f32::from(contrast[p]) / 255.;
            result.tolerant_residual[p] =
                result.tolerant_residual[p].max(tolerant / (255. * edge_weight));
            result.tolerated += f64::from(raw - tolerant);
        }
    }
    assert_eq!(result.initial - result.remaining, 2 * result.stats.matched);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edge_leeway_accepts_small_coverage_variation_but_retains_larger_losses() {
        let (w, h) = (16, 16);
        let mut a = vec![255; w * h * 3];
        for y in 4..12 {
            a[(y * w + 7) * 3..(y * w + 8) * 3].fill(0);
        }
        let adjust = |v| {
            let mut b = a.clone();
            for y in 4..12 {
                b[(y * w + 7) * 3..(y * w + 8) * 3].fill(v);
            }
            b
        };
        let small = compare_with_leeway(&a, &adjust(13), w, h, 2, true, 0.1);
        assert!(small.residual.iter().any(|&v| v > 0.));
        assert!(small.tolerant_residual.iter().all(|&v| v == 0.));
        let large = compare_with_leeway(&a, &adjust(51), w, h, 2, true, 0.1);
        assert!(large.tolerant_residual.iter().any(|&v| v > 0.1));
        let missing = compare_with_leeway(&a, &adjust(255), w, h, 2, true, 0.1);
        assert_eq!(missing.residual, missing.tolerant_residual);
        let strict = compare_with_leeway(&a, &adjust(13), w, h, 2, true, 0.);
        assert_eq!(strict.residual, strict.tolerant_residual);
    }
    #[test]
    fn equal_half_coverage_samples_can_concentrate_into_one_black_sample() {
        let (w, h) = (8, 12);
        let mut a = vec![255; w * h * 3];
        let mut b = a.clone();
        for y in 0..h {
            a[(y * w + 3) * 3..(y * w + 5) * 3].fill(128);
            b[(y * w + 3) * 3..(y * w + 4) * 3].fill(0);
        }
        for fast in [false, true] {
            let r = compare_with_solver(&a, &b, w, h, 1, fast);
            // 128+128 versus 0+255 leaves exactly one quantization level per
            // channel per row; the rest is conserved coverage redistribution.
            assert_eq!(r.remaining, (h * 3) as u64);
            assert_eq!(r.stats.matched, (h * 3 * 127) as u64);
        }
    }
    #[test]
    fn reassignment_prevents_greedy_loss_and_conserves_mass() {
        // s->A,B; A->C costs 1, A->D costs 2; B->C costs 2.
        // First choose A->C, then must undo it to match both A and B.
        let mut g = vec![vec![]; 6];
        edge(&mut g, 0, 1, 1, 0);
        edge(&mut g, 0, 2, 1, 0);
        let ac = edge(&mut g, 1, 3, 1, 1);
        let ad = edge(&mut g, 1, 4, 1, 2);
        edge(&mut g, 2, 3, 1, 2);
        edge(&mut g, 3, 5, 1, 0);
        edge(&mut g, 4, 5, 1, 0);
        assert_eq!(solve(&mut g, 0, 5), 2);
        assert_eq!(g[1][ac].capacity, 1);
        assert_eq!(g[1][ad].capacity, 0);
    }
    #[test]
    fn split_coverage_cancels_but_missing_ink_and_distant_compensation_remain() {
        let rgb = |v: &[u8]| v.iter().flat_map(|&v| [v; 3]).collect::<Vec<_>>();
        let a = rgb(&[255, 0, 255]);
        let b = rgb(&[255, 127, 128]);
        let r = compare(&a, &b, 3, 1, 1);
        assert_eq!(r.remaining, 0);
        assert_eq!(r.stats.matched, 127 * 3);
        let missing = compare(&a, &rgb(&[255; 3]), 3, 1, 1);
        assert_eq!(missing.remaining, 255 * 3);
        let distant = compare(&rgb(&[0, 255, 255]), &rgb(&[255, 255, 0]), 3, 1, 1);
        assert_eq!(distant.remaining, 2 * 255 * 3);
        assert_eq!(
            compare(&rgb(&[0, 255, 255]), &rgb(&[255, 255, 0]), 3, 1, 2).remaining,
            0
        );
    }
    #[test]
    fn swapping_images_preserves_total_unmatched_and_minimum_work() {
        let mut seed = 123_u32;
        for _ in 0..20 {
            let mut next = || {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                (seed >> 24) as u8
            };
            let a: Vec<_> = (0..5 * 6 * 3).map(|_| next()).collect();
            let b: Vec<_> = (0..a.len()).map(|_| next()).collect();
            for radius in [1, 2] {
                let x = compare(&a, &b, 5, 6, radius);
                let y = compare(&b, &a, 5, 6, radius);
                let fast = compare_with_solver(&a, &b, 5, 6, radius, true);
                assert_eq!(fast.remaining, x.remaining);
                assert!(fast.stats.work + 1e-6 >= x.stats.work);
                assert_eq!(x.remaining, y.remaining);
                assert!((x.stats.work - y.stats.work).abs() < 1e-6);
            }
        }
    }
}
