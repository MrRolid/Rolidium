// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The Pull tool on a round face: the face moves along its outward normal, so the diameter of a hole gets smaller (the normal points to its axis) and that of a pin
//! bigger, and the size of a round gets smaller by the distance; at zero the hole, the pin or the round is gone. The number can be typed, "Up to" works (any distance
//! of the size of the radius or more closes a hole), and the preview shows the new cylinder.

use super::*;
use crate::i18n::trf;

/// What a pull of a round face changes.
#[derive(Debug, Clone, Copy)]
pub(super) struct RoundPull {
    /// The radius now (for a round: its size).
    pub r0: f64,
    /// A hole (the normal points to the axis), else a pin or the outside of a round.
    pub hole: bool,
    /// A round or a chamfer (its size changes), not a whole hole or pin.
    pub blend: bool,
    pub base: DVec3,
    pub axis: DVec3,
    pub height: f64,
}

impl RoundPull {
    /// The radius after a pull by `d` along the normal.
    pub fn new_radius(&self, d: f64) -> f64 {
        let r = if self.blend || self.hole {
            self.r0 - d
        } else {
            self.r0 + d
        };
        r.max(0.0)
    }
}

impl App {
    /// The round face of a body that a pull can change: a whole hole or pin, or a round (also one recognised from a converted mesh).
    pub(super) fn round_pull_info(&self, body: BodyId, face: u32) -> Option<RoundPull> {
        if let Some(b) = self.doc.body(body) {
            let s = &b.solid;
            if let Ok(Some(c)) = s.cylinder_info(FaceId(face)) {
                return Some(RoundPull {
                    r0: c.radius,
                    hole: c.is_hole,
                    blend: false,
                    base: c.base,
                    axis: c.axis,
                    height: c.height,
                });
            }
            if let Some((ci, ri)) = s.blend_map().rounding_of(FaceId(face)) {
                let r = s.blend_map().components[ci].roundings[ri].size;
                return Some(RoundPull {
                    r0: r,
                    hole: false,
                    blend: true,
                    base: DVec3::ZERO,
                    axis: DVec3::Z,
                    height: 0.0,
                });
            }
            return None;
        }
        // a hole or a pin of a mesh (the diameter is what changes)
        let meta = self.entry(body)?.faces.get(face as usize)?;
        let d = meta.cyl_diameter?;
        let hole = self.mesh_hole_is_hole(body, face)?;
        Some(RoundPull {
            r0: d / 2.0,
            hole,
            blend: false,
            base: DVec3::ZERO,
            axis: DVec3::Z,
            height: 0.0,
        })
    }

    /// Where on the round face the pull is grabbed and which way its outward normal points there: the triangle of the face whose normal lies most along the screen's
    /// horizontal, so that the drag across the screen is a drag along the normal.
    pub(super) fn round_pull_grab(&self, body: BodyId, face: u32) -> Option<(Vec3, Vec3)> {
        let m = self.entry(body)?;
        let aspect = 1.6;
        let (_, fwd) = self.camera.ray(glam::Vec2::ZERO, aspect);
        let right = {
            let r = fwd.cross(Vec3::Z);
            if r.length() < 1e-4 {
                Vec3::X
            } else {
                r.normalize()
            }
        };
        let mut best: Option<(f32, Vec3, Vec3)> = None;
        for (t, tri) in m.mesh.indices.as_chunks::<3>().0.iter().enumerate() {
            if m.mesh.tri_face.get(t) != Some(&face) {
                continue;
            }
            let n = (m.mesh.normals[tri[0] as usize]
                + m.mesh.normals[tri[1] as usize]
                + m.mesh.normals[tri[2] as usize])
                .normalize_or_zero();
            // facing the viewer, and along the horizontal
            let score = n.dot(right).abs() - 0.5 * n.dot(fwd).max(0.0);
            if best.is_none_or(|(b, _, _)| score > b) {
                let c = (m.mesh.positions[tri[0] as usize]
                    + m.mesh.positions[tri[1] as usize]
                    + m.mesh.positions[tri[2] as usize])
                    / 3.0;
                best = Some((score, c, n));
            }
        }
        best.map(|(_, c, n)| (c, n))
    }

    /// The new cylinder of a hole or a pin that is pulled by `d` (a translucent surface and its two circles).
    pub(super) fn round_pull_preview(&self, r: &RoundPull, d: f32) -> Option<PreviewDraw> {
        if r.blend || r.height <= 0.0 {
            return None;
        }
        let nr = r.new_radius(d as f64);
        if nr < 1e-3 {
            return None;
        }
        let axis = r.axis.normalize();
        let x = if axis.cross(DVec3::Z).length() > 1e-6 {
            axis.cross(DVec3::Z).normalize()
        } else {
            DVec3::X
        };
        let y = axis.cross(x);
        let n = 64;
        let at = |k: usize, z: f64| -> Vec3 {
            let a = k as f64 / n as f64 * std::f64::consts::TAU;
            (r.base + axis * z + x * (nr * a.cos()) + y * (nr * a.sin())).as_vec3()
        };
        let mut tris = Vec::new();
        let mut lines = Vec::new();
        for k in 0..n {
            let (a, b, c, e) = (at(k, 0.0), at(k + 1, 0.0), at(k, r.height), at(k + 1, r.height));
            tris.extend([a, b, e, a, e, c]);
            lines.extend([a, b, c, e]);
            if k % 8 == 0 {
                lines.extend([a, c]);
            }
        }
        let grows = (nr > r.r0) == !r.hole;
        Some(PreviewDraw {
            triangles: tris,
            lines,
            color: if grows {
                [0.30, 0.70, 1.00, 0.32]
            } else {
                [1.00, 0.45, 0.30, 0.32]
            },
            line_color: [1.0, 1.0, 1.0, 0.9],
            ..Default::default()
        })
    }

    /// The sentence at the status bar while a round face is pulled: what it will be.
    pub(super) fn round_pull_text(&self, r: &RoundPull, d: f32) -> String {
        let nr = r.new_radius(d as f64);
        let (what, old, new) = if r.blend {
            ("Round size {} → {} mm", r.r0, nr)
        } else {
            ("Diameter {} → {} mm", r.r0 * 2.0, nr * 2.0)
        };
        let mut t = trf(what, &[fmt_num(old), fmt_num(new)]);
        if nr <= 1e-6 {
            t.push_str(&format!(
                " ({})",
                tr(if r.blend {
                    "the round is gone"
                } else if r.hole {
                    "the hole is closed"
                } else {
                    "the pin is gone"
                })
            ));
        }
        t
    }
}
