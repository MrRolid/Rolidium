// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! A small software renderer: the picture the MCP server returns. It needs no window and no graphics card (the GPU
//! renderer of the application needs an adapter; this one is plain Rust and gives the same picture on every machine).
//! Flat-and-smooth shaded triangles with a depth buffer, edge lines, sketch lines, the axes, highlighted faces and
//! edges, and labels with their ids. Drawn at twice the size and scaled down for smooth edges.

use glam::{Mat4, Vec3, Vec4};
use rolidium_kernel::Mesh;
use rolidium_render::Camera;
use std::collections::HashSet;

/// One body to draw.
pub struct SoftBody<'a> {
    pub mesh: &'a Mesh,
    pub color: [f32; 3],
    pub highlight_faces: HashSet<u32>,
    pub highlight_edges: HashSet<u32>,
}

#[derive(Default)]
pub struct SoftScene<'a> {
    pub bodies: Vec<SoftBody<'a>>,
    /// Polylines drawn on top: (points, colour).
    pub lines: Vec<(Vec<Vec3>, [f32; 3])>,
    /// Texts at 3D points (id labels).
    pub labels: Vec<(Vec3, String)>,
}

const SS: usize = 2;

struct Canvas {
    w: usize,
    h: usize,
    color: Vec<[f32; 3]>,
    depth: Vec<f32>,
}

impl Canvas {
    fn new(w: usize, h: usize) -> Self {
        let mut color = vec![[0.0; 3]; w * h];
        for y in 0..h {
            // a quiet vertical gradient
            let t = y as f32 / h.max(1) as f32;
            let c = [0.945 - 0.10 * t, 0.955 - 0.08 * t, 0.97 - 0.06 * t];
            for x in 0..w {
                color[y * w + x] = c;
            }
        }
        Canvas {
            w,
            h,
            color,
            depth: vec![f32::INFINITY; w * h],
        }
    }

    fn blend(&mut self, x: i32, y: i32, c: [f32; 3], a: f32) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 {
            return;
        }
        let p = &mut self.color[y as usize * self.w + x as usize];
        for i in 0..3 {
            p[i] = p[i] * (1.0 - a) + c[i] * a;
        }
    }

    /// A line with a width in pixels; `depth_test` draws only where nothing is nearer (with a bias).
    fn line(
        &mut self,
        a: (f32, f32, f32),
        b: (f32, f32, f32),
        c: [f32; 3],
        alpha: f32,
        width: f32,
        depth_test: bool,
    ) {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let steps = dx.abs().max(dy.abs()).ceil().max(1.0) as usize;
        let r = (width * 0.5).max(0.5);
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let (x, y, z) = (a.0 + dx * t, a.1 + dy * t, a.2 + (b.2 - a.2) * t);
            let (xi, yi) = (x.round() as i32, y.round() as i32);
            let ri = r.ceil() as i32;
            for oy in -ri..=ri {
                for ox in -ri..=ri {
                    if ((ox * ox + oy * oy) as f32) > r * r + 0.25 {
                        continue;
                    }
                    let (px, py) = (xi + ox, yi + oy);
                    if px < 0 || py < 0 || px >= self.w as i32 || py >= self.h as i32 {
                        continue;
                    }
                    if depth_test && z > self.depth[py as usize * self.w + px as usize] + 2e-4 {
                        continue;
                    }
                    self.blend(px, py, c, alpha);
                }
            }
        }
    }

    fn triangle(&mut self, v: [(f32, f32, f32); 3], shade: impl Fn([f32; 3]) -> [f32; 3]) {
        let min_x = v
            .iter()
            .map(|p| p.0)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as i32;
        let max_x = v
            .iter()
            .map(|p| p.0)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .min(self.w as f32 - 1.0) as i32;
        let min_y = v
            .iter()
            .map(|p| p.1)
            .fold(f32::INFINITY, f32::min)
            .floor()
            .max(0.0) as i32;
        let max_y = v
            .iter()
            .map(|p| p.1)
            .fold(f32::NEG_INFINITY, f32::max)
            .ceil()
            .min(self.h as f32 - 1.0) as i32;
        let area = (v[1].0 - v[0].0) * (v[2].1 - v[0].1) - (v[2].0 - v[0].0) * (v[1].1 - v[0].1);
        if area.abs() < 1e-9 {
            return;
        }
        let inv = 1.0 / area;
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let w0 = ((v[1].0 - px) * (v[2].1 - py) - (v[2].0 - px) * (v[1].1 - py)) * inv;
                let w1 = ((v[2].0 - px) * (v[0].1 - py) - (v[0].0 - px) * (v[2].1 - py)) * inv;
                let w2 = 1.0 - w0 - w1;
                if w0 < -1e-6 || w1 < -1e-6 || w2 < -1e-6 {
                    continue;
                }
                let z = w0 * v[0].2 + w1 * v[1].2 + w2 * v[2].2;
                let idx = y as usize * self.w + x as usize;
                if z < self.depth[idx] && z >= 0.0 {
                    self.depth[idx] = z;
                    self.color[idx] = shade([w0, w1, w2]);
                }
            }
        }
    }
}

const FONT: [(char, [u8; 7]); 12] = [
    (
        '0',
        [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
    ),
    (
        '1',
        [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
    ),
    (
        '2',
        [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
    ),
    (
        '3',
        [0b11110, 0b00001, 0b00001, 0b01110, 0b00001, 0b00001, 0b11110],
    ),
    (
        '4',
        [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
    ),
    (
        '5',
        [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
    ),
    (
        '6',
        [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
    ),
    (
        '7',
        [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
    ),
    (
        '8',
        [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
    ),
    (
        '9',
        [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
    ),
    (
        'F',
        [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b10000],
    ),
    (
        'E',
        [0b11111, 0b10000, 0b10000, 0b11110, 0b10000, 0b10000, 0b11111],
    ),
];

fn glyph(c: char) -> Option<&'static [u8; 7]> {
    FONT.iter().find(|(k, _)| *k == c).map(|(_, g)| g)
}

fn draw_label(cv: &mut Canvas, x: f32, y: f32, text: &str, scale: i32) {
    let w = text.chars().count() as i32 * 6 * scale + 2 * scale;
    let h = 7 * scale + 2 * scale;
    let (x0, y0) = (x as i32 - w / 2, y as i32 - h / 2);
    for yy in 0..h {
        for xx in 0..w {
            cv.blend(x0 + xx, y0 + yy, [1.0, 1.0, 1.0], 0.9);
        }
    }
    for (i, ch) in text.chars().enumerate() {
        let Some(g) = glyph(ch) else { continue };
        for (row, bits) in g.iter().enumerate() {
            for col in 0..5 {
                if bits >> (4 - col) & 1 == 1 {
                    for sy in 0..scale {
                        for sx in 0..scale {
                            cv.blend(
                                x0 + scale + (i as i32 * 6 + col) * scale + sx,
                                y0 + scale + row as i32 * scale + sy,
                                [0.55, 0.25, 0.0],
                                1.0,
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Renders the scene to RGBA bytes (`w` x `h`).
pub fn render(scene: &SoftScene, camera: &Camera, w: usize, h: usize) -> Vec<u8> {
    let (sw, sh) = (w * SS, h * SS);
    let mut cv = Canvas::new(sw, sh);
    let vp: Mat4 = camera.view_proj(w as f32 / h as f32);
    let eye = camera.eye();
    let to_screen = |p: Vec3| -> Option<(f32, f32, f32)> {
        let c = vp * Vec4::new(p.x, p.y, p.z, 1.0);
        if c.w <= 1e-4 {
            return None;
        }
        let (x, y, z) = (c.x / c.w, c.y / c.w, c.z / c.w);
        Some((
            (x * 0.5 + 0.5) * sw as f32,
            (1.0 - (y * 0.5 + 0.5)) * sh as f32,
            z,
        ))
    };
    let key = Vec3::new(0.4, -0.5, 0.8).normalize();
    for body in &scene.bodies {
        let m = body.mesh;
        for (t, tri) in m.indices.as_chunks::<3>().0.iter().enumerate() {
            let ids = [tri[0] as usize, tri[1] as usize, tri[2] as usize];
            let ps = [m.positions[ids[0]], m.positions[ids[1]], m.positions[ids[2]]];
            let (Some(a), Some(b), Some(c)) = (to_screen(ps[0]), to_screen(ps[1]), to_screen(ps[2])) else {
                continue;
            };
            let highlighted = body.highlight_faces.contains(&m.tri_face[t]);
            let ns = [m.normals[ids[0]], m.normals[ids[1]], m.normals[ids[2]]];
            cv.triangle([a, b, c], |bary| {
                let p = ps[0] * bary[0] + ps[1] * bary[1] + ps[2] * bary[2];
                let mut n = (ns[0] * bary[0] + ns[1] * bary[1] + ns[2] * bary[2]).normalize_or_zero();
                let view = (eye - p).normalize_or_zero();
                if n.dot(view) < 0.0 {
                    n = -n;
                }
                let diffuse = 0.30 + 0.50 * n.dot(view).max(0.0) + 0.25 * n.dot(key).max(0.0);
                let mut col = [
                    body.color[0] * diffuse,
                    body.color[1] * diffuse,
                    body.color[2] * diffuse,
                ];
                if highlighted {
                    let o = [1.0, 0.72, 0.25];
                    for i in 0..3 {
                        col[i] = col[i] * 0.45 + o[i] * 0.55;
                    }
                }
                [col[0].min(1.0), col[1].min(1.0), col[2].min(1.0)]
            });
        }
    }
    // edges of the bodies
    for body in &scene.bodies {
        for e in &body.mesh.edges {
            let hl = body.highlight_edges.contains(&e.edge.0);
            let (col, alpha, width) = if hl {
                ([1.0, 0.45, 0.05], 1.0, 3.5 * SS as f32)
            } else if e.smooth {
                ([0.08, 0.09, 0.11], 0.22, 1.0 * SS as f32)
            } else {
                ([0.08, 0.09, 0.11], 1.0, 1.2 * SS as f32)
            };
            for seg in e.points.windows(2) {
                if let (Some(a), Some(b)) = (to_screen(seg[0]), to_screen(seg[1])) {
                    cv.line(a, b, col, alpha, width, !hl);
                }
            }
        }
    }
    // sketch lines and curves on top
    for (pts, col) in &scene.lines {
        for seg in pts.windows(2) {
            if let (Some(a), Some(b)) = (to_screen(seg[0]), to_screen(seg[1])) {
                cv.line(a, b, *col, 1.0, 1.6 * SS as f32, false);
            }
        }
    }
    // axes from the origin
    let len = camera.distance * 0.18;
    for (dir, col) in [
        (Vec3::X, [0.85, 0.2, 0.2]),
        (Vec3::Y, [0.2, 0.65, 0.2]),
        (Vec3::Z, [0.2, 0.35, 0.9]),
    ] {
        if let (Some(a), Some(b)) = (to_screen(Vec3::ZERO), to_screen(dir * len)) {
            cv.line(a, b, col, 0.85, 1.4 * SS as f32, true);
        }
    }
    for (p, text) in &scene.labels {
        if let Some((x, y, _)) = to_screen(*p) {
            draw_label(&mut cv, x, y, text, SS as i32 + 1);
        }
    }
    // scale down
    let mut out = vec![255u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0.0f32; 3];
            for sy in 0..SS {
                for sx in 0..SS {
                    let c = cv.color[(y * SS + sy) * sw + x * SS + sx];
                    for i in 0..3 {
                        acc[i] += c[i];
                    }
                }
            }
            for i in 0..3 {
                out[(y * w + x) * 4 + i] =
                    ((acc[i] / (SS * SS) as f32).clamp(0.0, 1.0) * 255.0).round() as u8;
            }
        }
    }
    out
}
