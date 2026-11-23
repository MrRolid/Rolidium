// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Distances measured along the normal of a pulled face: snapping while dragging, the "up to"
//! target and the distance to the opposite face. These are pure functions so they can be tested.

use crate::worker::{FaceMeta, MeshEntry};
use rolidium_doc::BodyId;
use rolidium_kernel::{DVec3, FaceKind};

/// What a dragged distance is attached to.
#[derive(Clone, Debug, PartialEq)]
pub enum SnapTarget {
    /// The plane of a parallel face of some body.
    Face { body: BodyId, face: u32 },
    /// A vertex of some body (the plane of the face passes through it).
    Vertex(glam::Vec3),
    /// The grid step.
    Grid,
}

#[derive(Clone, Debug)]
pub struct Candidate {
    pub dist: f64,
    pub target: SnapTarget,
}

/// Distances closer than this (mm) to the plane of a parallel face are taken to be that plane. Vertex
/// and edge points come from the single-precision display mesh, so they are off by about 1e-5 mm; a
/// pull to such a point would leave a hair-thin step instead of merging into the face.
pub const ALIGN_TOL: f64 = 1e-3;

/// Moves `d` onto the exact plane of a parallel face if one is within [`ALIGN_TOL`].
pub fn align_to_planes(
    d: f64,
    origin: DVec3,
    normal: DVec3,
    plane_points: impl IntoIterator<Item = DVec3>,
) -> f64 {
    let n = normal.normalize();
    plane_points
        .into_iter()
        .map(|p| (p - origin).dot(n))
        .filter(|pd| (pd - d).abs() < ALIGN_TOL)
        .min_by(|a, b| (a - d).abs().total_cmp(&(b - d).abs()))
        .unwrap_or(d)
}

/// Snap radius on the screen, in pixels.
pub const SNAP_PX: f64 = 9.0;

/// All distances (along `normal`, from `origin`) a dragged face could snap to: planes of other
/// parallel faces and vertices, of every body (also other bodies than the one being pulled).
pub fn candidates(meshes: &[MeshEntry], origin: DVec3, normal: DVec3) -> Vec<Candidate> {
    let n = normal.normalize();
    let mut out: Vec<Candidate> = Vec::new();
    let add = |c: Candidate, out: &mut Vec<Candidate>| {
        if c.dist.abs() < 1e-6 {
            return;
        }
        // The same distance twice (two vertices on one plane) is kept once, a face wins.
        if let Some(e) = out.iter_mut().find(|e| (e.dist - c.dist).abs() < ALIGN_TOL) {
            if matches!(c.target, SnapTarget::Face { .. }) && !matches!(e.target, SnapTarget::Face { .. }) {
                *e = c;
            }
            return;
        }
        out.push(c);
    };
    for m in meshes {
        for (i, f) in m.faces.iter().enumerate() {
            if f.kind == FaceKind::Plane && f.normal.dot(n).abs() > 0.9999 {
                add(
                    Candidate {
                        dist: (f.origin - origin).dot(n),
                        target: SnapTarget::Face {
                            body: m.id,
                            face: i as u32,
                        },
                    },
                    &mut out,
                );
            }
        }
        for e in &m.mesh.edges {
            for p in [e.points.first(), e.points.last()].into_iter().flatten() {
                add(
                    Candidate {
                        dist: (p.as_dvec3() - origin).dot(n),
                        target: SnapTarget::Vertex(*p),
                    },
                    &mut out,
                );
            }
        }
    }
    out
}

/// The distance a drag ends up at. `raw` is the mouse distance in mm; `px_per_mm` converts the
/// snap radius. With `alt` all snapping is off. Without candidates in reach the distance goes to the
/// grid `step` (when `grid_on`), otherwise it is only rounded to 0.01 mm.
pub fn snap_distance(
    raw: f64,
    cands: &[Candidate],
    px_per_mm: f64,
    step: f64,
    grid_on: bool,
    alt: bool,
) -> (f64, Option<SnapTarget>) {
    let free = (raw * 100.0).round() / 100.0;
    if alt {
        return (free, None);
    }
    let reach = SNAP_PX / px_per_mm.max(1e-9);
    let best = cands
        .iter()
        .filter(|c| (c.dist - raw).abs() <= reach)
        .min_by(|a, b| (a.dist - raw).abs().total_cmp(&(b.dist - raw).abs()));
    if let Some(c) = best {
        return (c.dist, Some(c.target.clone()));
    }
    if grid_on && step > 0.0 {
        let v = (raw / step).round() * step;
        return ((v * 1e6).round() / 1e6, Some(SnapTarget::Grid));
    }
    (free, None)
}

/// The geometry an "up to" click can land on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UpTo {
    /// A parallel planar face: the faces end up in one plane.
    Plane(DVec3),
    /// A vertex or any point of a face: the plane passes through it.
    Point(DVec3),
    /// An edge: if it is parallel to the plane any point of it will do, otherwise the nearer end.
    Edge(DVec3, DVec3),
}

/// Signed distance along `normal` from `origin` to the target.
pub fn up_to_distance(origin: DVec3, normal: DVec3, target: UpTo) -> f64 {
    let n = normal.normalize();
    let d = |p: DVec3| (p - origin).dot(n);
    match target {
        UpTo::Plane(p) | UpTo::Point(p) => d(p),
        UpTo::Edge(a, b) => {
            let (da, db) = (d(a), d(b));
            if (da - db).abs() < 1e-6 || da.abs() <= db.abs() {
                da
            } else {
                db
            }
        }
    }
}

/// Distance from the plane (`origin`, outward `normal`) to the nearest parallel face of the same
/// body that looks the other way, behind it. `None` if there is none.
pub fn opposite_distance(
    faces: &[FaceMeta],
    skip: Option<usize>,
    origin: DVec3,
    normal: DVec3,
) -> Option<f64> {
    let n = normal.normalize();
    faces
        .iter()
        .enumerate()
        .filter(|(i, f)| Some(*i) != skip && f.kind == FaceKind::Plane && f.normal.dot(n) < -0.9999)
        .map(|(_, f)| (origin - f.origin).dot(n))
        .filter(|d| *d > 1e-6)
        .min_by(f64::total_cmp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rolidium_kernel::FaceKind;

    fn cand(dist: f64, t: SnapTarget) -> Candidate {
        Candidate { dist, target: t }
    }

    #[test]
    fn snaps_to_faces_and_vertices_then_the_grid() {
        let cands = vec![
            cand(10.0, SnapTarget::Face { body: 1, face: 3 }),
            cand(25.0, SnapTarget::Vertex(glam::Vec3::ONE)),
        ];
        // 10 px/mm: reach is 0.9 mm.
        let (d, t) = snap_distance(9.4, &cands, 10.0, 1.0, true, false);
        assert_eq!(d, 10.0);
        assert!(matches!(t, Some(SnapTarget::Face { body: 1, face: 3 })));
        let (d, t) = snap_distance(24.5, &cands, 10.0, 1.0, true, false);
        assert_eq!(d, 25.0);
        assert!(matches!(t, Some(SnapTarget::Vertex(_))));
        // Out of reach: the grid step.
        let (d, t) = snap_distance(15.3, &cands, 10.0, 0.5, true, false);
        assert_eq!(d, 15.5);
        assert_eq!(t, Some(SnapTarget::Grid));
        // Grid off: free, rounded to 0.01.
        let (d, t) = snap_distance(15.3333, &cands, 10.0, 0.5, false, false);
        assert_eq!(d, 15.33);
        assert_eq!(t, None);
    }

    #[test]
    fn alt_turns_every_snap_off() {
        let cands = vec![cand(10.0, SnapTarget::Face { body: 1, face: 0 })];
        let (d, t) = snap_distance(9.97, &cands, 10.0, 1.0, true, true);
        assert_eq!(d, 9.97);
        assert_eq!(t, None);
    }

    #[test]
    fn reach_shrinks_when_zoomed_out() {
        let cands = vec![cand(10.0, SnapTarget::Face { body: 1, face: 0 })];
        // 1 px/mm: reach is 9 mm; 100 px/mm: reach is 0.09 mm.
        assert_eq!(snap_distance(5.0, &cands, 1.0, 1.0, false, false).0, 10.0);
        assert_ne!(snap_distance(9.8, &cands, 100.0, 1.0, false, false).0, 10.0);
    }

    #[test]
    fn up_to_planes_points_and_edges() {
        let o = DVec3::new(0.0, 0.0, 20.0);
        let n = DVec3::Z;
        assert_eq!(
            up_to_distance(o, n, UpTo::Plane(DVec3::new(5.0, 5.0, 35.0))),
            15.0
        );
        assert_eq!(
            up_to_distance(o, n, UpTo::Point(DVec3::new(1.0, 1.0, 12.0))),
            -8.0
        );
        // A parallel edge: any point; a sloped edge: the nearer end.
        assert_eq!(
            up_to_distance(
                o,
                n,
                UpTo::Edge(DVec3::new(0.0, 0.0, 30.0), DVec3::new(9.0, 0.0, 30.0))
            ),
            10.0
        );
        assert_eq!(
            up_to_distance(
                o,
                n,
                UpTo::Edge(DVec3::new(0.0, 0.0, 25.0), DVec3::new(9.0, 0.0, 50.0))
            ),
            5.0
        );
        assert_eq!(
            up_to_distance(
                o,
                n,
                UpTo::Edge(DVec3::new(0.0, 0.0, 70.0), DVec3::new(9.0, 0.0, 24.0))
            ),
            4.0
        );
    }

    fn meta(kind: FaceKind, normal: DVec3, origin: DVec3) -> FaceMeta {
        FaceMeta {
            kind,
            normal,
            origin,
            area: 1.0,
            centroid: glam::Vec3::ZERO,
            bounds: (glam::Vec3::ZERO, glam::Vec3::ZERO),
            cyl_diameter: None,
            disc: None,
            cap: None,
        }
    }

    #[test]
    fn opposite_face_of_a_box() {
        let faces = vec![
            meta(FaceKind::Plane, DVec3::Z, DVec3::new(0.0, 0.0, 20.0)),
            meta(FaceKind::Plane, -DVec3::Z, DVec3::ZERO),
            meta(FaceKind::Plane, DVec3::X, DVec3::new(40.0, 0.0, 0.0)),
            meta(FaceKind::Cylinder, DVec3::Z, DVec3::ZERO),
        ];
        let d = opposite_distance(&faces, Some(0), faces[0].origin, DVec3::Z);
        assert_eq!(d, Some(20.0));
        // Nothing opposite a side face of a body that has no face looking back.
        assert_eq!(
            opposite_distance(&faces, Some(2), faces[2].origin, DVec3::X),
            None
        );
    }

    use rolidium_doc::rolidium_sketch::{DVec2, Entity, PlaneDef};
    use rolidium_doc::{Command, Document, PullMode, SketchTarget};
    use rolidium_kernel::FaceId;

    /// Filling a recess with "up to" must give the same clean result whether the target is the
    /// original face, one of its edges or one of its vertices (single-precision points included).
    #[test]
    fn refilling_a_recess_up_to_face_edge_or_vertex_leaves_no_outline() {
        for target in 0..3 {
            let mut d = Document::new();
            d.apply(Command::AddBox {
                name: String::new(),
                min: DVec3::ZERO,
                max: DVec3::new(40.3, 30.7, 20.123),
            })
            .unwrap();
            let body = d.bodies()[0].id;
            let faces_before = d.bodies()[0].solid.face_count();
            let top = |d: &Document| {
                let s = &d.body(body).unwrap().solid;
                s.faces()
                    .map(|f| (f, s.face_info(f).unwrap()))
                    .filter(|(_, i)| i.kind == FaceKind::Plane && i.normal.z > 0.99)
                    .max_by(|a, b| a.1.origin.z.total_cmp(&b.1.origin.z))
                    .unwrap()
            };
            let (_, top_info) = top(&d);
            let sk = d
                .apply(Command::AddEntity {
                    target: SketchTarget::New {
                        plane: PlaneDef::from_face(top_info.origin, DVec3::Z),
                        body: Some(body),
                    },
                    entity: Entity::Circle {
                        center: DVec2::new(20.0, 15.0),
                        radius: 5.0,
                    },
                })
                .unwrap()
                .created_sketch
                .unwrap();
            d.apply(Command::PullRegion {
                sketch: sk,
                region: 0,
                distance: -5.0,
                mode: PullMode::Auto,
            })
            .unwrap();
            assert!(d.bodies()[0].solid.face_count() > faces_before);
            // the recess floor, and where the original face's plane was (as f32 display points)
            let solid = d.body(body).unwrap().solid.clone();
            let (floor, floor_info) = solid
                .faces()
                .map(|f| (f, solid.face_info(f).unwrap()))
                .find(|(_, i)| {
                    i.kind == FaceKind::Plane && i.normal.z > 0.99 && i.origin.z < top_info.origin.z - 1.0
                })
                .unwrap();
            let f32z = |z: f64| z as f32 as f64;
            let target_pt = match target {
                0 => UpTo::Plane(top_info.origin),
                1 => UpTo::Edge(
                    DVec3::new(0.0, 0.0, f32z(20.123)),
                    DVec3::new(f32z(40.3), 0.0, f32z(20.123)),
                ),
                _ => UpTo::Point(DVec3::new(0.0, 0.0, f32z(20.123))),
            };
            let raw = up_to_distance(floor_info.origin, DVec3::Z, target_pt);
            let dist = align_to_planes(raw, floor_info.origin, DVec3::Z, [top_info.origin]);
            d.apply(Command::PullFace {
                body,
                face: FaceId(floor.0),
                distance: dist,
                mode: PullMode::Auto,
            })
            .unwrap();
            let after = &d.body(body).unwrap().solid;
            assert_eq!(
                after.face_count(),
                faces_before,
                "target kind {target}: outline must vanish"
            );
        }
    }
}
