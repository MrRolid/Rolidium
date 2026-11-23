// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Curves (3D lines and helices) as objects of the document, and chains of sketch shapes as sweep paths.

use rolidium_kernel::{polyline_segments, DVec3, HelixDef, KernelError, Path, PathSeg, PathSource};
use rolidium_sketch::{DVec2, Entity, EntityId, PlaneDef, Sketch};

pub type CurveId = u64;

/// What a curve is made of. A polyline has straight segments between its points; with `radius` above zero the corners
/// are rounded by tangent arcs (the path of a bent tube).
#[derive(Debug, Clone, PartialEq)]
pub enum CurveDef {
    Polyline { points: Vec<DVec3>, radius: f64 },
    Helix(HelixDef),
}

#[derive(Debug, Clone)]
pub struct CurveDoc {
    pub id: CurveId,
    pub name: String,
    pub def: CurveDef,
    pub visible: bool,
    /// A helix made on a cylindrical or conical face: the height of that face. The helix may not be made higher than this (a change of the turns
    /// changes the pitch, not the height), and it never leaves the face. `None`: a free curve, only the general limit of turns holds.
    pub limit_height: Option<f64>,
}

impl CurveDef {
    /// The kernel path of this curve.
    pub fn path(&self) -> Result<Path, KernelError> {
        match self {
            CurveDef::Polyline { points, radius } => {
                let segs = polyline_segments(points, *radius)?;
                Path::build(PathSource::Segments(&segs))
            }
            CurveDef::Helix(h) => Path::build(PathSource::Helix(*h)),
        }
    }

    /// The curve rotated about the axis through `pivot` by `angle` (radians), then moved by `translation`.
    pub fn moved(&self, translation: DVec3, pivot: DVec3, axis: DVec3, angle: f64) -> CurveDef {
        let q = if axis.length() > 1e-9 && angle.abs() > 1e-12 {
            glam::DQuat::from_axis_angle(axis.normalize(), angle)
        } else {
            glam::DQuat::IDENTITY
        };
        let point = |p: DVec3| pivot + q * (p - pivot) + translation;
        match self {
            CurveDef::Polyline { points, radius } => CurveDef::Polyline {
                points: points.iter().map(|p| point(*p)).collect(),
                radius: *radius,
            },
            CurveDef::Helix(h) => CurveDef::Helix(HelixDef {
                origin: point(h.origin),
                axis: q * h.axis,
                x_dir: q * h.x_dir,
                ..*h
            }),
        }
    }
}

/// A shape of a sketch cut into lines and arcs in sketch coordinates.
#[derive(Debug, Clone, Copy)]
enum Seg2 {
    Line {
        a: DVec2,
        b: DVec2,
    },
    Arc {
        center: DVec2,
        radius: f64,
        start: f64,
        sweep: f64,
    },
}

impl Seg2 {
    fn start(&self) -> DVec2 {
        match *self {
            Seg2::Line { a, .. } => a,
            Seg2::Arc {
                center,
                radius,
                start,
                ..
            } => center + DVec2::new(start.cos(), start.sin()) * radius,
        }
    }
    fn end(&self) -> DVec2 {
        match *self {
            Seg2::Line { b, .. } => b,
            Seg2::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                let t = start + sweep;
                center + DVec2::new(t.cos(), t.sin()) * radius
            }
        }
    }
    fn reversed(&self) -> Seg2 {
        match *self {
            Seg2::Line { a, b } => Seg2::Line { a: b, b: a },
            Seg2::Arc {
                center,
                radius,
                start,
                sweep,
            } => Seg2::Arc {
                center,
                radius,
                start: start + sweep,
                sweep: -sweep,
            },
        }
    }
}

fn entity_segs(e: &Entity) -> Vec<Seg2> {
    match *e {
        Entity::Line { a, b } => vec![Seg2::Line { a, b }],
        Entity::Rect { a, b } => {
            let (p0, p1, p2, p3) = (a, DVec2::new(b.x, a.y), b, DVec2::new(a.x, b.y));
            vec![
                Seg2::Line { a: p0, b: p1 },
                Seg2::Line { a: p1, b: p2 },
                Seg2::Line { a: p2, b: p3 },
                Seg2::Line { a: p3, b: p0 },
            ]
        }
        Entity::Circle { center, radius } => vec![
            Seg2::Arc {
                center,
                radius,
                start: 0.0,
                sweep: std::f64::consts::PI,
            },
            Seg2::Arc {
                center,
                radius,
                start: std::f64::consts::PI,
                sweep: std::f64::consts::PI,
            },
        ],
        Entity::Arc {
            center,
            radius,
            start,
            sweep,
        } => vec![Seg2::Arc {
            center,
            radius,
            start,
            sweep,
        }],
    }
}

const TOUCH: f64 = 1e-6;

/// The shapes of `sketch` that continue `seed` end to end (lines and arcs that share end points), the seed included.
pub fn sketch_chain(sketch: &Sketch, seed: EntityId) -> Vec<EntityId> {
    let ents = sketch.entities();
    let ends: Vec<(EntityId, Vec<DVec2>)> = ents
        .iter()
        .map(|(id, e)| {
            let segs = entity_segs(e);
            let mut pts = Vec::new();
            for s in &segs {
                pts.push(s.start());
                pts.push(s.end());
            }
            (*id, pts)
        })
        .collect();
    let mut chain = vec![seed];
    let mut i = 0;
    while i < chain.len() {
        let Some((_, mine)) = ends.iter().find(|(id, _)| *id == chain[i]) else {
            break;
        };
        for (id, theirs) in &ends {
            if chain.contains(id) {
                continue;
            }
            if mine
                .iter()
                .any(|p| theirs.iter().any(|q| (*p - *q).length() < TOUCH))
            {
                chain.push(*id);
            }
        }
        i += 1;
    }
    chain
}

/// The shapes `ids` of a sketch as an ordered path in 3D (every shape oriented so that it continues the previous one).
pub fn sketch_path_segments(
    plane: &PlaneDef,
    sketch: &Sketch,
    ids: &[EntityId],
) -> Result<Vec<PathSeg>, KernelError> {
    let mut pool: Vec<Seg2> = Vec::new();
    for id in ids {
        let e = sketch
            .entities()
            .iter()
            .find(|(i, _)| i == id)
            .map(|(_, e)| e)
            .ok_or_else(|| KernelError::Invalid("a shape of the path no longer exists".into()))?;
        pool.extend(entity_segs(e));
    }
    if pool.is_empty() {
        return Err(KernelError::Invalid("the path has no shapes".into()));
    }
    // start at an end point that belongs to one segment only (an open chain), else anywhere (a closed one)
    let count = |pool: &[Seg2], p: DVec2| -> usize {
        pool.iter()
            .map(|s| {
                usize::from((s.start() - p).length() < TOUCH) + usize::from((s.end() - p).length() < TOUCH)
            })
            .sum()
    };
    let by_start = pool.iter().position(|s| count(&pool, s.start()) == 1);
    let by_end = pool.iter().position(|s| count(&pool, s.end()) == 1);
    let first = match (by_start, by_end) {
        (Some(i), _) => pool.remove(i),
        (None, Some(i)) => pool.remove(i).reversed(),
        _ => pool.remove(0),
    };
    let mut ordered = vec![first];
    while !pool.is_empty() {
        let tail = ordered.last().map(Seg2::end).unwrap_or_default();
        if let Some(i) = pool.iter().position(|s| (s.start() - tail).length() < TOUCH) {
            ordered.push(pool.remove(i));
        } else if let Some(i) = pool.iter().position(|s| (s.end() - tail).length() < TOUCH) {
            ordered.push(pool.remove(i).reversed());
        } else {
            return Err(KernelError::Invalid(
                "The chosen shapes do not form one connected chain".into(),
            ));
        }
    }
    let p3 = |p: DVec2| plane.to_3d(p);
    Ok(ordered
        .iter()
        .map(|s| match *s {
            Seg2::Line { a, b } => PathSeg::Line { a: p3(a), b: p3(b) },
            Seg2::Arc {
                center,
                radius,
                start,
                sweep,
            } => {
                let m = start + sweep / 2.0;
                PathSeg::Arc {
                    a: p3(s.start()),
                    mid: p3(center + DVec2::new(m.cos(), m.sin()) * radius),
                    b: p3(s.end()),
                }
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_on_one_segment_takes_the_whole_connected_chain() {
        let mut sk = Sketch::new();
        let a = sk
            .add(Entity::Line {
                a: DVec2::new(0.0, 0.0),
                b: DVec2::new(10.0, 0.0),
            })
            .unwrap();
        let arc = sk
            .add(Entity::Arc {
                center: DVec2::new(10.0, 5.0),
                radius: 5.0,
                start: -std::f64::consts::FRAC_PI_2,
                sweep: std::f64::consts::FRAC_PI_2,
            })
            .unwrap();
        let c = sk
            .add(Entity::Line {
                a: DVec2::new(15.0, 5.0),
                b: DVec2::new(15.0, 20.0),
            })
            .unwrap();
        let other = sk
            .add(Entity::Line {
                a: DVec2::new(100.0, 0.0),
                b: DVec2::new(110.0, 0.0),
            })
            .unwrap();
        for seed in [a, arc, c] {
            let mut chain = sketch_chain(&sk, seed);
            chain.sort();
            assert_eq!(chain, vec![a, arc, c], "from {seed}");
        }
        assert_eq!(sketch_chain(&sk, other), vec![other]);
        let segs = sketch_path_segments(&PlaneDef::base(), &sk, &[c, a, arc]).unwrap();
        assert_eq!(segs.len(), 3);
        // ordered from one end to the other, however the ids were given
        let path = Path::build(PathSource::Segments(&segs)).unwrap();
        let length = 10.0 + std::f64::consts::FRAC_PI_2 * 5.0 + 15.0;
        assert!((path.length - length).abs() < 1e-6, "{}", path.length);
    }

    /// The longest chord of a drawn curve must stay within the requested deflection of the circle it follows (this is what keeps a
    /// helix round at any zoom: the application asks for 0.4 of a pixel).
    #[test]
    fn a_helix_polyline_stays_within_the_requested_deflection() {
        let h = CurveDef::Helix(HelixDef {
            origin: DVec3::ZERO,
            axis: DVec3::Z,
            x_dir: DVec3::X,
            radius: 20.0,
            pitch: 5.0,
            turns: 4.0,
            right: true,
            start_deg: 0.0,
            cone_deg: 0.0,
        });
        let path = h.path().unwrap();
        let mut last = 0usize;
        for tol in [0.1, 0.02, 0.004] {
            let pts = path.polyline(tol).unwrap();
            // a chord of the circle of radius 20 deviates by at most the sag chord^2 / (8 R) (flat projection)
            let worst = pts
                .windows(2)
                .map(|w| {
                    let (a, b) = (w[0], w[1]);
                    let c = ((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt();
                    c * c / (8.0 * 20.0)
                })
                .fold(0.0, f64::max);
            assert!(worst <= tol * 1.05, "tolerance {tol}: sag {worst}");
            assert!(pts.len() > last, "a finer tolerance gives more points");
            last = pts.len();
        }
    }

    #[test]
    fn moving_a_curve_moves_its_points_and_its_axis() {
        let h = CurveDef::Helix(HelixDef {
            origin: DVec3::ZERO,
            axis: DVec3::Z,
            x_dir: DVec3::X,
            radius: 5.0,
            pitch: 2.0,
            turns: 3.0,
            right: true,
            start_deg: 0.0,
            cone_deg: 0.0,
        });
        let m = h.moved(
            DVec3::new(1.0, 2.0, 3.0),
            DVec3::ZERO,
            DVec3::X,
            std::f64::consts::FRAC_PI_2,
        );
        let CurveDef::Helix(r) = m else { panic!() };
        assert!((r.origin - DVec3::new(1.0, 2.0, 3.0)).length() < 1e-9);
        assert!(
            (r.axis - DVec3::new(0.0, -1.0, 0.0)).length() < 1e-9,
            "{:?}",
            r.axis
        );
    }

    use crate::{file, Command, Document, LoftRef, SweepMode, SweepPath};
    use rolidium_sketch::PlaneDef as Plane;

    fn ring_sketch(d: &mut Document, plane: Plane, outer: f64, inner: f64) -> u64 {
        use crate::SketchTarget;
        let t = SketchTarget::New { plane, body: None };
        let c = d
            .apply(Command::AddEntity {
                target: t,
                entity: Entity::Circle {
                    center: DVec2::ZERO,
                    radius: outer,
                },
            })
            .unwrap();
        let sk = c.created_sketch.unwrap();
        d.apply(Command::AddEntity {
            target: SketchTarget::Existing(sk),
            entity: Entity::Circle {
                center: DVec2::ZERO,
                radius: inner,
            },
        })
        .unwrap();
        sk
    }

    #[test]
    fn a_bent_hollow_tube_is_swept_along_a_curve_and_the_curve_survives_saving() {
        let mut d = Document::new();
        let c = d
            .apply(Command::AddCurve {
                def: CurveDef::Polyline {
                    points: vec![
                        DVec3::ZERO,
                        DVec3::new(100.0, 0.0, 0.0),
                        DVec3::new(100.0, 100.0, 0.0),
                    ],
                    radius: 40.0,
                },
            })
            .unwrap();
        let curve = c.created_curve.unwrap();
        assert_eq!(d.curves().len(), 1);
        // the ring stands at the start of the curve, in the YZ plane
        let sk = ring_sketch(&mut d, Plane::yz(), 10.0, 8.0);
        d.apply(Command::Sweep {
            profile: LoftRef::Region {
                sketch: sk,
                region: 0,
            },
            path: SweepPath::Curve(curve),
            mode: SweepMode::NewBody,
            rounded: false,
        })
        .unwrap();
        let length = 60.0 + std::f64::consts::FRAC_PI_2 * 40.0 + 60.0;
        let area = std::f64::consts::PI * (100.0 - 64.0);
        let v = d.bodies()[0].solid.volume().unwrap();
        assert!((v - area * length).abs() < 1e-3 * area * length, "{v}");
        // save, open: the curve and the body are back, nothing else changed
        let bytes = file::save_to_bytes(&d, None, "t").unwrap();
        let e = Document::from_loaded(file::load_from_bytes(&bytes).unwrap());
        assert_eq!(e.curves().len(), 1);
        assert_eq!(e.curves()[0].def, d.curves()[0].def);
        assert_eq!(e.curves()[0].name, d.curves()[0].name);
        assert_eq!(e.bodies().len(), 1);
    }

    #[test]
    fn a_file_without_curves_opens_unchanged_and_stays_without_them() {
        let mut d = Document::new();
        d.apply(Command::AddBox {
            name: String::new(),
            min: DVec3::ZERO,
            max: DVec3::new(10.0, 10.0, 10.0),
        })
        .unwrap();
        let bytes = file::save_to_bytes(&d, None, "0.1.9").unwrap();
        let e = Document::from_loaded(file::load_from_bytes(&bytes).unwrap());
        assert!(e.curves().is_empty());
        let again = file::save_to_bytes(&e, None, "0.1.9").unwrap();
        let mut z = zip::ZipArchive::new(std::io::Cursor::new(again)).unwrap();
        assert!((0..z.len()).all(|i| !z.by_index(i).unwrap().name().unwrap().starts_with("curves/")));
    }

    #[test]
    fn a_curve_can_be_hidden_renamed_moved_and_deleted() {
        let mut d = Document::new();
        let id = d
            .apply(Command::AddCurve {
                def: CurveDef::Polyline {
                    points: vec![DVec3::ZERO, DVec3::new(10.0, 0.0, 0.0)],
                    radius: 0.0,
                },
            })
            .unwrap()
            .created_curve
            .unwrap();
        d.apply(Command::SetVisible {
            target: crate::ItemId::Curve(id),
            visible: false,
        })
        .unwrap();
        assert!(!d.curves()[0].visible);
        d.apply(Command::Rename {
            target: crate::ItemId::Curve(id),
            name: "Path".into(),
        })
        .unwrap();
        assert_eq!(d.curves()[0].name, "Path");
        d.apply(Command::MoveCurves {
            curves: vec![id],
            translation: DVec3::new(0.0, 5.0, 0.0),
            pivot: DVec3::ZERO,
            axis: DVec3::Z,
            angle: 0.0,
        })
        .unwrap();
        let CurveDef::Polyline { points, .. } = &d.curves()[0].def else {
            panic!()
        };
        assert!((points[0] - DVec3::new(0.0, 5.0, 0.0)).length() < 1e-9);
        assert!(d.undo());
        d.apply(Command::DeleteCurve { curve: id }).unwrap();
        assert!(d.curves().is_empty());
        assert!(d.undo() && d.curves().len() == 1);
    }

    #[test]
    fn a_sweep_that_cuts_through_itself_is_refused_and_the_model_stays() {
        let mut d = Document::new();
        let curve = d
            .apply(Command::AddCurve {
                def: CurveDef::Polyline {
                    points: vec![
                        DVec3::ZERO,
                        DVec3::new(50.0, 0.0, 0.0),
                        DVec3::new(50.0, 50.0, 0.0),
                    ],
                    radius: 3.0,
                },
            })
            .unwrap()
            .created_curve
            .unwrap();
        let sk = ring_sketch(&mut d, Plane::yz(), 12.0, 0.5);
        let before = d.bodies().len();
        let r = d.apply(Command::Sweep {
            profile: LoftRef::Region {
                sketch: sk,
                region: 0,
            },
            path: SweepPath::Curve(curve),
            mode: SweepMode::NewBody,
            rounded: false,
        });
        let msg = r.expect_err("a tight bend must be refused").to_string();
        assert!(msg.contains("not changed"), "{msg}");
        assert_eq!(d.bodies().len(), before);
    }

    #[test]
    fn a_sketch_on_a_slanted_flat_face_is_as_slanted_as_the_face_and_pulls_perpendicular_to_it() {
        use crate::{PullMode, SketchTarget};
        let mut d = Document::new();
        d.apply(Command::AddBox {
            name: String::new(),
            min: DVec3::ZERO,
            max: DVec3::new(60.0, 60.0, 40.0),
        })
        .unwrap();
        let b = d.bodies()[0].id;
        // chamfer the top edge along X at y = 0: a face slanted at 45 degrees
        let edge = {
            let s = &d.bodies()[0].solid;
            s.edges()
                .find(|e| {
                    let i = s.edge_info(*e).unwrap();
                    (i.start.z - 40.0).abs() < 1e-6
                        && (i.end.z - 40.0).abs() < 1e-6
                        && i.start.y.abs() < 1e-6
                        && i.end.y.abs() < 1e-6
                })
                .unwrap()
        };
        d.apply(Command::FilletEdges {
            body: b,
            edges: vec![edge.0],
            size: 20.0,
            chamfer: true,
        })
        .unwrap();
        let before = d.bodies()[0].solid.volume().unwrap();
        // the slanted face: its normal is neither axis-aligned
        let (origin, normal, centroid) = {
            let s = &d.bodies()[0].solid;
            let f = s
                .faces()
                .find(|f| {
                    let i = s.face_info(*f).unwrap();
                    i.kind == rolidium_kernel::FaceKind::Plane
                        && i.normal.z.abs() > 0.3
                        && i.normal.z.abs() < 0.9
                        && i.normal.y.abs() > 0.3
                })
                .expect("a slanted face");
            let i = s.face_info(f).unwrap();
            (i.origin, i.normal, s.props().unwrap().centroid)
        };
        // outward: facing up and towards -Y
        assert!(
            normal.z > 0.3 && normal.y < -0.3,
            "the normal of the slanted face points into the body: {normal:?}"
        );
        let _ = centroid;
        let plane = PlaneDef::from_face(origin, normal);
        // the plane is as slanted as the face
        assert!(
            (plane.normal - normal).length() < 1e-9
                && plane.normal.z.abs() < 0.9
                && plane.normal.z.abs() > 0.3
        );
        // a circle of radius 3 on the face, pulled 4 mm into the body
        let on_face = origin + normal * 0.0;
        let centre = plane.to_2d(DVec3::new(30.0, 10.0, 30.0));
        let _ = on_face;
        let sk = d
            .apply(Command::AddEntity {
                target: SketchTarget::New { plane, body: Some(b) },
                entity: Entity::Circle {
                    center: centre,
                    radius: 3.0,
                },
            })
            .unwrap()
            .created_sketch
            .unwrap();
        d.apply(Command::PullRegion {
            sketch: sk,
            region: 0,
            distance: -4.0,
            mode: PullMode::Cut,
        })
        .unwrap();
        let removed = before - d.bodies()[0].solid.volume().unwrap();
        let expected = std::f64::consts::PI * 9.0 * 4.0;
        assert!(
            (removed - expected).abs() < 0.05,
            "removed {removed}, expected {expected}"
        );
    }

    #[test]
    fn every_new_curve_gets_its_own_id() {
        let mut d = Document::new();
        let mut ids = Vec::new();
        for i in 0..3 {
            let c = d
                .apply(Command::AddCurve {
                    def: CurveDef::Polyline {
                        points: vec![DVec3::ZERO, DVec3::new(10.0 + i as f64, 0.0, 0.0)],
                        radius: 0.0,
                    },
                })
                .unwrap();
            ids.push(c.created_curve.unwrap());
        }
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 3, "curve ids must differ");
    }
}
