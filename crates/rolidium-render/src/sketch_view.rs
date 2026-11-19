// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Drawing helpers for sketches: curve lines, region fills and the region Pull preview.

use crate::PreviewDraw;
use glam::Vec3;
use rolidium_sketch::{PlaneDef, Region, Sketch};

/// A region lifted into 3D: filled triangles and closed outline polylines.
#[derive(Debug, Clone, Default)]
pub struct RegionGeom {
    pub triangles: Vec<Vec3>,
    pub outlines: Vec<Vec<Vec3>>,
    pub centroid: Vec3,
}

fn v3(plane: &PlaneDef, p: rolidium_sketch::DVec2) -> Vec3 {
    plane.to_3d(p).as_vec3()
}

pub fn region_geometry(plane: &PlaneDef, r: &Region) -> RegionGeom {
    let mut g = RegionGeom::default();
    for t in r.triangulate(0.08) {
        g.triangles.extend(t.iter().map(|p| v3(plane, *p)));
    }
    for c in std::iter::once(&r.outer).chain(r.holes.iter()) {
        g.outlines
            .push(c.polyline(0.08).iter().map(|p| v3(plane, *p)).collect());
    }
    if !g.triangles.is_empty() {
        g.centroid = g.triangles.iter().copied().sum::<Vec3>() / g.triangles.len() as f32;
    }
    g
}

/// Line list (pairs of points) of all curves of a sketch.
pub fn sketch_lines(plane: &PlaneDef, s: &Sketch) -> Vec<Vec3> {
    let mut out = Vec::new();
    for (_, e) in s.entities() {
        for pl in e.polylines() {
            for w in pl.windows(2) {
                out.push(v3(plane, w[0]));
                out.push(v3(plane, w[1]));
            }
        }
    }
    out
}

pub fn fill_layer(geoms: &[&RegionGeom], color: [f32; 4]) -> PreviewDraw {
    PreviewDraw {
        triangles: geoms.iter().flat_map(|g| g.triangles.iter().copied()).collect(),
        lines: Vec::new(),
        color,
        line_color: color,
        ..Default::default()
    }
}

/// Translucent prism standing on a region and extending `distance` along `normal`.
pub fn region_pull_preview(g: &RegionGeom, normal: Vec3, distance: f32) -> PreviewDraw {
    let off = normal * distance;
    let color = if distance >= 0.0 {
        [0.30, 0.70, 1.00, 0.32]
    } else {
        [1.00, 0.45, 0.30, 0.32]
    };
    let mut tris: Vec<Vec3> = g.triangles.iter().map(|p| *p + off).collect();
    let mut lines = Vec::new();
    for o in &g.outlines {
        let n = o.len();
        for i in 0..n {
            let (a, b) = (o[i], o[(i + 1) % n]);
            tris.extend([a, b, b + off, a, b + off, a + off]);
            lines.extend([a + off, b + off]);
            // vertical lines only at corners
            let prev = o[(i + n - 1) % n];
            let (d0, d1) = ((a - prev).normalize_or_zero(), (b - a).normalize_or_zero());
            if d0.dot(d1) < 0.97 {
                lines.extend([a, a + off]);
            }
        }
    }
    PreviewDraw {
        triangles: tris,
        lines,
        color,
        line_color: [color[0], color[1], color[2], 1.0],
        ..Default::default()
    }
}
