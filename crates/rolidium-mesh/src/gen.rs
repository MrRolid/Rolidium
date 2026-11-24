// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Meshes made by the program itself (own geometry, CC0): for the examples, the tests and the measurements.

use crate::TriMesh;
use std::f64::consts::TAU;

/// A box 0..w x 0..d x 0..h, every face divided into `n` x `n` squares of two triangles (the vertices are shared).
pub fn box_mesh(w: f32, d: f32, h: f32, n: usize) -> TriMesh {
    let mut m = TriMesh::default();
    let mut index: std::collections::HashMap<[u32; 3], u32> = std::collections::HashMap::new();
    let mut vert = |m: &mut TriMesh, p: [f32; 3]| -> u32 {
        let key = [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()];
        *index.entry(key).or_insert_with(|| {
            m.positions.push(p);
            (m.positions.len() - 1) as u32
        })
    };
    // each face: origin, u, v (counter-clockwise seen from outside)
    let faces: [([f32; 3], [f32; 3], [f32; 3]); 6] = [
        ([0.0, 0.0, h], [w, 0.0, 0.0], [0.0, d, 0.0]),
        ([0.0, d, 0.0], [w, 0.0, 0.0], [0.0, -d, 0.0]),
        ([0.0, 0.0, 0.0], [w, 0.0, 0.0], [0.0, 0.0, h]),
        ([w, d, 0.0], [-w, 0.0, 0.0], [0.0, 0.0, h]),
        ([0.0, d, 0.0], [0.0, -d, 0.0], [0.0, 0.0, h]),
        ([w, 0.0, 0.0], [0.0, d, 0.0], [0.0, 0.0, h]),
    ];
    for (o, u, v) in faces {
        let at = |i: usize, j: usize| {
            let (s, t) = (i as f32 / n as f32, j as f32 / n as f32);
            [o[0] + u[0] * s + v[0] * t, o[1] + u[1] * s + v[1] * t, o[2] + u[2] * s + v[2] * t]
        };
        for i in 0..n {
            for j in 0..n {
                let (a, b, c, e) = (
                    vert(&mut m, at(i, j)),
                    vert(&mut m, at(i + 1, j)),
                    vert(&mut m, at(i + 1, j + 1)),
                    vert(&mut m, at(i, j + 1)),
                );
                m.triangles.push([a, b, c]);
                m.triangles.push([a, c, e]);
            }
        }
    }
    m
}

/// A plate 0..w x 0..d x 0..t with a round hole of radius `r` through the middle; `seg` (a multiple of 4) points round the hole.
pub fn plate_with_hole(w: f64, d: f64, t: f64, r: f64, seg: usize) -> TriMesh {
    let mut m = TriMesh::default();
    let (cx, cy) = (w / 2.0, d / 2.0);
    let mut push = |m: &mut TriMesh, p: [f64; 3]| -> u32 {
        m.positions.push([p[0] as f32, p[1] as f32, p[2] as f32]);
        (m.positions.len() - 1) as u32
    };
    let mut rings: Vec<(Vec<u32>, Vec<u32>, Vec<u32>, Vec<u32>)> = Vec::new();
    // outer ring points on the perimeter in the directions of the hole points (45 degrees plus steps: the corners are among them)
    let mut outer_top = Vec::new();
    let mut outer_bot = Vec::new();
    let mut inner_top = Vec::new();
    let mut inner_bot = Vec::new();
    for i in 0..seg {
        let a = TAU / 8.0 + TAU * i as f64 / seg as f64;
        let (c, s) = (a.cos(), a.sin());
        // the point of the perimeter in this direction
        let k = (w / 2.0 / c.abs().max(1e-12)).min(d / 2.0 / s.abs().max(1e-12));
        let (ox, oy) = (cx + c * k, cy + s * k);
        outer_top.push(push(&mut m, [ox, oy, t]));
        outer_bot.push(push(&mut m, [ox, oy, 0.0]));
        let (hx, hy) = (cx + c * r, cy + s * r);
        inner_top.push(push(&mut m, [hx, hy, t]));
        inner_bot.push(push(&mut m, [hx, hy, 0.0]));
    }
    rings.push((outer_top.clone(), outer_bot.clone(), inner_top.clone(), inner_bot.clone()));
    for i in 0..seg {
        let j = (i + 1) % seg;
        // top (normal +z), bottom (-z), outer wall (outwards), hole wall (towards the axis)
        m.triangles.push([outer_top[i], outer_top[j], inner_top[j]]);
        m.triangles.push([outer_top[i], inner_top[j], inner_top[i]]);
        m.triangles.push([outer_bot[i], inner_bot[j], outer_bot[j]]);
        m.triangles.push([outer_bot[i], inner_bot[i], inner_bot[j]]);
        m.triangles.push([outer_bot[i], outer_bot[j], outer_top[j]]);
        m.triangles.push([outer_bot[i], outer_top[j], outer_top[i]]);
        m.triangles.push([inner_bot[i], inner_top[j], inner_bot[j]]);
        m.triangles.push([inner_bot[i], inner_top[i], inner_top[j]]);
    }
    m
}

/// A sphere of about `triangles` triangles (a latitude and longitude grid), radius `r`.
pub fn sphere(r: f64, triangles: usize) -> TriMesh {
    let n = ((triangles as f64 / 2.0).sqrt()).max(3.0) as usize; // n longitude, n latitude
    let mut m = TriMesh::default();
    m.positions.push([0.0, 0.0, r as f32]);
    for i in 1..n {
        let th = std::f64::consts::PI * i as f64 / n as f64;
        for j in 0..n {
            let ph = TAU * j as f64 / n as f64;
            m.positions.push([(r * th.sin() * ph.cos()) as f32, (r * th.sin() * ph.sin()) as f32, (r * th.cos()) as f32]);
        }
    }
    m.positions.push([0.0, 0.0, -r as f32]);
    let south = (m.positions.len() - 1) as u32;
    let ring = |i: usize, j: usize| (1 + (i - 1) * n + j % n) as u32;
    for j in 0..n {
        m.triangles.push([0, ring(1, j), ring(1, j + 1)]);
    }
    for i in 1..n - 1 {
        for j in 0..n {
            m.triangles.push([ring(i, j), ring(i + 1, j), ring(i + 1, j + 1)]);
            m.triangles.push([ring(i, j), ring(i + 1, j + 1), ring(i, j + 1)]);
        }
    }
    for j in 0..n {
        m.triangles.push([south, ring(n - 1, j + 1), ring(n - 1, j)]);
    }
    m
}
