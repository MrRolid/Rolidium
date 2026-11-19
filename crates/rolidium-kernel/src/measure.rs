// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! Measuring between two elements of bodies: a point, an edge, a face. The shortest distance comes from Open CASCADE
//! (any combination); angles, the distance of parallel faces and the distance of cylinder axes are worked out here.

use crate::{native, DVec3, EdgeId, EdgeKind, FaceId, FaceKind, KernelError, Result, Solid};

/// One measured element.
#[derive(Clone, Copy)]
pub enum Elem<'a> {
    Point(DVec3),
    Edge(&'a Solid, EdgeId),
    Face(&'a Solid, FaceId),
}

/// An angle between two straight things (edges, planes) with the place where to draw it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Angle {
    /// Between 0 and 90 degrees (the angle between the lines or planes, not between their directions).
    pub degrees: f64,
    pub apex: DVec3,
    /// Unit directions of the two arms in the plane of the angle.
    pub dir_a: DVec3,
    pub dir_b: DVec3,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Measurement {
    /// Shortest distance and the two nearest points.
    pub distance: f64,
    pub from: DVec3,
    pub to: DVec3,
    /// `to - from`; its length is `distance`.
    pub delta: DVec3,
    /// Two parallel planar faces: the distance between their planes.
    pub parallel_distance: Option<f64>,
    pub angle: Option<Angle>,
    /// Two cylindrical faces: the distance of their axes (and its nearest points).
    pub axes_distance: Option<(f64, DVec3, DVec3)>,
}

enum Straight {
    /// The unit direction.
    Line(DVec3),
    /// A point on it and the unit normal.
    Plane(DVec3, DVec3),
}

fn straight(e: &Elem) -> Option<Straight> {
    match e {
        Elem::Point(_) => None,
        Elem::Edge(s, id) => {
            let i = s.edge_info(*id).ok()?;
            (i.kind == EdgeKind::Line).then(|| Straight::Line((i.end - i.start).normalize_or_zero()))
        }
        Elem::Face(s, id) => {
            let i = s.face_info(*id).ok()?;
            (i.kind == FaceKind::Plane).then_some(Straight::Plane(i.origin, i.normal))
        }
    }
}

/// Axis (origin, unit direction) of a cylindrical face.
fn cylinder_axis(e: &Elem) -> Option<(DVec3, DVec3)> {
    if let Elem::Face(s, id) = e {
        let i = s.face_info(*id).ok()?;
        if i.kind == FaceKind::Cylinder {
            return Some((i.origin, i.normal.normalize_or_zero()));
        }
    }
    None
}

/// Distance of two infinite lines, with the nearest points.
fn line_line(p1: DVec3, d1: DVec3, p2: DVec3, d2: DVec3) -> (f64, DVec3, DVec3) {
    let n = d1.cross(d2);
    let w = p2 - p1;
    if n.length() < 1e-9 {
        // parallel: the distance of p2 from the first line
        let t = w.dot(d1);
        let a = p1 + d1 * t;
        let b = p2;
        return ((b - a).length(), a, b);
    }
    // closest points: p1 + d1 s, p2 + d2 t
    let (a, b, c) = (d1.dot(d1), d1.dot(d2), d2.dot(d2));
    let (d, e) = (d1.dot(w), d2.dot(w));
    let den = a * c - b * b;
    let s = (c * d - b * e) / den;
    let t = (b * d - a * e) / den;
    let (q1, q2) = (p1 + d1 * s, p2 + d2 * t);
    ((q2 - q1).length(), q1, q2)
}

fn angle_between(a: &Straight, b: &Straight, at: DVec3) -> Option<Angle> {
    use Straight::*;
    let fold = |c: f64| c.abs().clamp(0.0, 1.0).acos().to_degrees();
    match (a, b) {
        (Line(d1), Line(d2)) => {
            let deg = fold(d1.dot(*d2));
            // the arms: d1 and the direction of d2 on the same side
            let d2 = if d1.dot(*d2) < 0.0 { -*d2 } else { *d2 };
            Some(Angle {
                degrees: deg,
                apex: at,
                dir_a: *d1,
                dir_b: d2,
            })
        }
        (Plane(_, n1), Plane(_, n2)) => {
            let deg = fold(n1.dot(*n2));
            let n2 = if n1.dot(*n2) < 0.0 { -*n2 } else { *n2 };
            Some(Angle {
                degrees: deg,
                apex: at,
                dir_a: *n1,
                dir_b: n2,
            })
        }
        (Line(d), Plane(_, n)) | (Plane(_, n), Line(d)) => {
            let deg = 90.0 - fold(d.dot(*n));
            // the line and its shadow in the plane
            let shadow = (*d - *n * d.dot(*n)).normalize_or_zero();
            Some(Angle {
                degrees: deg,
                apex: at,
                dir_a: *d,
                dir_b: if shadow == DVec3::ZERO { *d } else { shadow },
            })
        }
    }
}

fn part<'a>(e: &Elem<'a>) -> (u32, u32, [f64; 3], Option<&'a Solid>) {
    match e {
        Elem::Point(p) => (0, 0, p.to_array(), None),
        Elem::Edge(s, id) => (1, id.0, [0.0; 3], Some(s)),
        Elem::Face(s, id) => (2, id.0, [0.0; 3], Some(s)),
    }
}

/// Measures between two elements.
pub fn measure(a: &Elem, b: &Elem) -> Result<Measurement> {
    let (ka, ia, pa, sa) = part(a);
    let (kb, ib, pb, sb) = part(b);
    let (distance, from, to) = match (sa, sb) {
        (None, None) => {
            let d = DVec3::from(pb) - DVec3::from(pa);
            (d.length(), DVec3::from(pa), DVec3::from(pb))
        }
        _ => {
            // a point needs some shape to be passed along; it is ignored
            let sha = sa
                .or(sb)
                .ok_or_else(|| KernelError::Invalid("nothing to measure".into()))?;
            let shb = sb
                .or(sa)
                .ok_or_else(|| KernelError::Invalid("nothing to measure".into()))?;
            let v = native::shape_distance(&sha.shape, [ka, ia], pa, &shb.shape, [kb, ib], pb)?;
            (v[0], DVec3::new(v[1], v[2], v[3]), DVec3::new(v[4], v[5], v[6]))
        }
    };
    let mut m = Measurement {
        distance,
        from,
        to,
        delta: to - from,
        parallel_distance: None,
        angle: None,
        axes_distance: None,
    };
    if let (Some(sa), Some(sb)) = (straight(a), straight(b)) {
        // where to draw the angle: the nearest points' middle (for two lines and planes that is on or near the corner)
        let at = (from + to) * 0.5;
        m.angle = angle_between(&sa, &sb, at);
        if let (Straight::Plane(p1, n1), Straight::Plane(p2, _)) = (&sa, &sb) {
            if m.angle.is_some_and(|an| an.degrees < 1e-6) {
                m.parallel_distance = Some(((*p2 - *p1).dot(*n1)).abs());
            }
        }
    }
    if let (Some((p1, d1)), Some((p2, d2))) = (cylinder_axis(a), cylinder_axis(b)) {
        let (d, q1, q2) = line_line(p1, d1, p2, d2);
        m.axes_distance = Some((d, q1, q2));
        if m.angle.is_none() {
            m.angle = angle_between(&Straight::Line(d1), &Straight::Line(d2), (q1 + q2) * 0.5);
        }
    }
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::BooleanOp;

    fn face_with_normal(s: &Solid, n: DVec3) -> FaceId {
        s.faces()
            .find(|f| {
                s.face_info(*f)
                    .is_ok_and(|i| i.kind == FaceKind::Plane && (i.normal - n).length() < 1e-9)
            })
            .unwrap()
    }

    #[test]
    fn opposite_faces_of_a_box() {
        let b = Solid::cuboid(DVec3::ZERO, DVec3::new(40.0, 30.0, 20.0)).unwrap();
        let (a, c) = (face_with_normal(&b, DVec3::X), face_with_normal(&b, -DVec3::X));
        let m = measure(&Elem::Face(&b, a), &Elem::Face(&b, c)).unwrap();
        assert!((m.distance - 40.0).abs() < 1e-9);
        assert!((m.parallel_distance.unwrap() - 40.0).abs() < 1e-9);
        assert!((m.delta.x.abs() - 40.0).abs() < 1e-9 && m.delta.y.abs() < 1e-9 && m.delta.z.abs() < 1e-9);
        assert!(m.angle.unwrap().degrees < 1e-6);
    }

    #[test]
    fn angle_of_a_chamfered_face_and_vertex_distances() {
        let b = Solid::cuboid(DVec3::ZERO, DVec3::new(40.0, 30.0, 20.0)).unwrap();
        // the edge between the +Z face and the +X face
        let e = b.edges().find(|e| {
            let i = b.edge_info(*e).unwrap();
            i.kind == EdgeKind::Line
                && (i.start.x - 40.0).abs() < 1e-9
                && (i.end.x - 40.0).abs() < 1e-9
                && (i.start.z - 20.0).abs() < 1e-9
                && (i.end.z - 20.0).abs() < 1e-9
        });
        let ch = b.chamfer(&[e.unwrap()], 5.0).unwrap().solid;
        let top = face_with_normal(&ch, DVec3::Z);
        let slanted = ch
            .faces()
            .find(|f| {
                ch.face_info(*f)
                    .is_ok_and(|i| i.kind == FaceKind::Plane && i.normal.z > 0.1 && i.normal.x > 0.1)
            })
            .unwrap();
        let m = measure(&Elem::Face(&ch, top), &Elem::Face(&ch, slanted)).unwrap();
        assert!((m.angle.unwrap().degrees - 45.0).abs() < 1e-6, "{:?}", m.angle);
        assert!(m.distance < 1e-9, "adjacent faces touch");
        let p = measure(
            &Elem::Point(DVec3::ZERO),
            &Elem::Point(DVec3::new(3.0, 4.0, 12.0)),
        )
        .unwrap();
        assert!((p.distance - 13.0).abs() < 1e-12);
    }

    #[test]
    fn distance_of_the_axes_of_two_holes_and_a_diameter() {
        let b = Solid::cuboid(DVec3::new(-30.0, -15.0, 0.0), DVec3::new(30.0, 15.0, 10.0)).unwrap();
        let h1 = Solid::cylinder(3.0, 10.0, DVec3::new(-12.0, 0.0, 0.0), DVec3::Z).unwrap();
        let h2 = Solid::cylinder(4.0, 10.0, DVec3::new(13.0, 5.0, 0.0), DVec3::Z).unwrap();
        let s = b
            .boolean(&h1, BooleanOp::Cut)
            .unwrap()
            .solid
            .boolean(&h2, BooleanOp::Cut)
            .unwrap()
            .solid;
        let cyl: Vec<FaceId> = s
            .faces()
            .filter(|f| s.face_info(*f).is_ok_and(|i| i.kind == FaceKind::Cylinder))
            .collect();
        assert_eq!(cyl.len(), 2);
        let m = measure(&Elem::Face(&s, cyl[0]), &Elem::Face(&s, cyl[1])).unwrap();
        let expect = ((25.0f64).powi(2) + 25.0).sqrt();
        assert!(
            (m.axes_distance.unwrap().0 - expect).abs() < 1e-6,
            "{:?}",
            m.axes_distance
        );
        // the shortest distance between the walls is the axes distance minus both radii
        assert!((m.distance - (expect - 7.0)).abs() < 1e-6, "{}", m.distance);
    }
}
