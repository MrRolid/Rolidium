// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
use rolidium_kernel::*;

fn bx() -> Solid {
    Solid::cuboid(DVec3::ZERO, DVec3::new(40.0, 30.0, 20.0)).unwrap()
}

fn top_face(s: &Solid) -> FaceId {
    s.faces()
        .find(|f| {
            let i = s.face_info(*f).unwrap();
            i.kind == FaceKind::Plane && i.normal.z > 0.99
        })
        .unwrap()
}

fn close(a: f64, b: f64, tol: f64) {
    assert!((a - b).abs() <= tol, "{a} != {b} (tol {tol})");
}

// ---- point 4: box, tessellation, edges ----

#[test]
fn box_basic_properties() {
    let s = bx();
    assert!(s.is_valid());
    assert_eq!(s.face_count(), 6);
    assert_eq!(s.edge_count(), 12);
    let p = s.props().unwrap();
    close(p.volume, 24000.0, 1e-6);
    close(p.area, 2.0 * (1200.0 + 800.0 + 600.0), 1e-6);
    assert_eq!(p.min, DVec3::ZERO);
    assert_eq!(p.max, DVec3::new(40.0, 30.0, 20.0));
    close(p.centroid.x, 20.0, 1e-9);
}

#[test]
fn box_tessellation_has_face_ids_and_edges() {
    let m = bx().mesh(0.1, 0.5).unwrap();
    assert_eq!(m.triangle_count(), 12);
    assert_eq!(m.edges.len(), 12);
    let mut seen: Vec<u32> = m.tri_face.clone();
    seen.sort();
    seen.dedup();
    assert_eq!(seen, vec![0, 1, 2, 3, 4, 5]);
    close(m.signed_volume(), 24000.0, 1e-3);
    // normals are unit length and consistent with winding
    for t in m.indices.as_chunks::<3>().0 {
        let (a, b, c) = (
            m.positions[t[0] as usize],
            m.positions[t[1] as usize],
            m.positions[t[2] as usize],
        );
        let n = (b - a).cross(c - a).normalize();
        assert!(n.dot(m.normals[t[0] as usize]) > 0.99);
    }
}

#[test]
fn invalid_input_is_an_error_not_a_crash() {
    assert!(Solid::cuboid(DVec3::ZERO, DVec3::new(0.0, 1.0, 1.0)).is_err());
    assert!(Solid::cylinder(-1.0, 5.0, DVec3::ZERO, DVec3::Z).is_err());
    assert!(bx().face_info(FaceId(99)).is_err());
    assert!(bx().pull_face(FaceId(99), 1.0).is_err());
    assert!(Solid::read_step(b"garbage").is_err());
    assert!(Solid::read_brep(b"garbage").is_err());
}

// ---- point 6: pull ----

#[test]
fn pull_out_adds_material() {
    let s = bx();
    let r = s.pull_face(top_face(&s), 10.0).unwrap();
    assert!(r.solid.is_valid());
    close(r.solid.volume().unwrap(), 1200.0 * 30.0, 1e-6);
    assert_eq!(r.solid.face_count(), 6, "walls must be merged, not left split");
    assert_eq!(r.solid.props().unwrap().max.z, 30.0);
}

#[test]
fn pull_in_removes_material() {
    let s = bx();
    let r = s.pull_face(top_face(&s), -5.0).unwrap();
    close(r.solid.volume().unwrap(), 1200.0 * 15.0, 1e-6);
    assert_eq!(r.solid.face_count(), 6);
    assert_eq!(r.solid.props().unwrap().max.z, 15.0);
}

#[test]
fn pull_side_face() {
    let s = bx();
    let f = s
        .faces()
        .find(|f| {
            let i = s.face_info(*f).unwrap();
            i.normal.x > 0.99
        })
        .unwrap();
    let r = s.pull_face(f, 15.0).unwrap();
    close(r.solid.volume().unwrap(), 55.0 * 30.0 * 20.0, 1e-6);
}

#[test]
fn pull_through_everything_fails_cleanly() {
    let s = bx();
    // Removing the whole body leaves nothing: must be an error, original untouched.
    let before = s.volume().unwrap();
    assert!(s.pull_face(top_face(&s), -20.0).is_err());
    assert!(s.pull_face(top_face(&s), -25.0).is_err());
    assert_eq!(s.volume().unwrap(), before);
    assert!(s.is_valid());
}

#[test]
fn pull_zero_and_nan_are_rejected() {
    let s = bx();
    assert!(matches!(
        s.pull_face(top_face(&s), 0.0),
        Err(KernelError::ZeroDistance)
    ));
    assert!(s.pull_face(top_face(&s), f64::NAN).is_err());
    assert!(s.pull_face(top_face(&s), f64::INFINITY).is_err());
}

#[test]
fn pull_curved_face_is_refused() {
    let c = Solid::cylinder(5.0, 10.0, DVec3::ZERO, DVec3::Z).unwrap();
    let side = c
        .faces()
        .find(|f| c.face_info(*f).unwrap().kind == FaceKind::Cylinder)
        .unwrap();
    assert!(matches!(c.pull_face(side, 1.0), Err(KernelError::NotPlanar)));
}

#[test]
fn pull_keeps_history_for_untouched_faces() {
    let s = bx();
    let top = top_face(&s);
    let r = s.pull_face(top, 10.0).unwrap();
    // The bottom face is untouched and must map to exactly one result face.
    let bottom = s
        .faces()
        .find(|f| s.face_info(*f).unwrap().normal.z < -0.99)
        .unwrap();
    assert_eq!(r.history.successors(0, bottom).len(), 1);
}

#[test]
fn pull_hole_through_plate() {
    // Plate with a boss, cut a through hole by pulling the boss top below the plate.
    let plate = bx();
    let boss = Solid::cylinder(5.0, 10.0, DVec3::new(20.0, 15.0, 20.0), DVec3::Z).unwrap();
    let with_boss = plate.boolean(&boss, BooleanOp::Fuse).unwrap().solid;
    let top = with_boss
        .faces()
        .find(|f| {
            let i = with_boss.face_info(*f).unwrap();
            i.kind == FaceKind::Plane && i.normal.z > 0.99 && i.origin.z > 29.0
        })
        .unwrap();
    let r = with_boss.pull_face(top, -35.0).unwrap();
    assert!(r.solid.is_valid());
    close(
        r.solid.volume().unwrap(),
        24000.0 - std::f64::consts::PI * 25.0 * 20.0,
        1e-3,
    );
    let cyl = r
        .solid
        .faces()
        .filter(|f| r.solid.face_info(*f).unwrap().kind == FaceKind::Cylinder)
        .count();
    assert_eq!(cyl, 1);
}

// ---- point 8: STL ----

#[test]
fn stl_binary_export_is_well_formed() {
    let s = bx();
    let m = s.mesh(0.1, 0.5).unwrap();
    let mut buf = Vec::new();
    m.write_stl_binary(&mut buf).unwrap();
    assert_eq!(buf.len(), 84 + 50 * m.triangle_count());
    assert_eq!(
        u32::from_le_bytes(buf[80..84].try_into().unwrap()) as usize,
        m.triangle_count()
    );
    assert!(
        !buf.starts_with(b"solid"),
        "binary STL must not start with 'solid'"
    );
}

// ---- point 2: profile with holes ----

#[test]
fn profile_with_hole_extrudes() {
    let mut p = Profile::rectangle(40.0, 30.0);
    p.loops.push(vec![ProfileSegment::Circle {
        center: [20.0, 15.0],
        radius: 5.0,
    }]);
    let s = Solid::extrude_profile(&p, DVec3::ZERO, DVec3::Z, DVec3::X, 10.0).unwrap();
    close(s.volume().unwrap(), 12000.0 - std::f64::consts::PI * 250.0, 1e-6);
    assert_eq!(s.face_count(), 7);
    let top = top_face(&s);
    assert_eq!(s.face_info(top).unwrap().inner_wires, 1);
    let cyl = s
        .faces()
        .find(|f| s.face_info(*f).unwrap().kind == FaceKind::Cylinder)
        .unwrap();
    close(s.face_info(cyl).unwrap().radius, 5.0, 1e-9);
}

#[test]
fn profile_with_arcs() {
    // Half-round slot end: line, arc, line, line.
    let p = Profile {
        loops: vec![vec![
            ProfileSegment::Line {
                a: [0.0, 0.0],
                b: [20.0, 0.0],
            },
            ProfileSegment::Arc {
                a: [20.0, 0.0],
                mid: [25.0, 5.0],
                b: [20.0, 10.0],
            },
            ProfileSegment::Line {
                a: [20.0, 10.0],
                b: [0.0, 10.0],
            },
            ProfileSegment::Line {
                a: [0.0, 10.0],
                b: [0.0, 0.0],
            },
        ]],
    };
    let s = Solid::extrude_profile(&p, DVec3::ZERO, DVec3::Z, DVec3::X, 4.0).unwrap();
    close(
        s.volume().unwrap(),
        (200.0 + std::f64::consts::PI * 25.0 / 2.0) * 4.0,
        1e-6,
    );
}

#[test]
fn profile_open_loop_is_rejected() {
    let p = Profile {
        loops: vec![vec![ProfileSegment::Line {
            a: [0.0, 0.0],
            b: [1.0, 0.0],
        }]],
    };
    assert!(Solid::extrude_profile(&p, DVec3::ZERO, DVec3::Z, DVec3::X, 1.0).is_err());
    assert!(Solid::extrude_profile(&Profile::default(), DVec3::ZERO, DVec3::Z, DVec3::X, 1.0).is_err());
}

// ---- booleans and edge cases ----

#[test]
fn boolean_cut_cylinder_hole() {
    let hole = Solid::cylinder(4.0, 30.0, DVec3::new(20.0, 15.0, -5.0), DVec3::Z).unwrap();
    let r = bx().boolean(&hole, BooleanOp::Cut).unwrap();
    close(
        r.solid.volume().unwrap(),
        24000.0 - std::f64::consts::PI * 16.0 * 20.0,
        1e-6,
    );
    assert_eq!(r.solid.face_count(), 7);
}

#[test]
fn boolean_touching_boxes_merge_flush_walls() {
    let a = bx();
    let b = Solid::cuboid(DVec3::new(40.0, 0.0, 0.0), DVec3::new(70.0, 30.0, 20.0)).unwrap();
    let r = a.boolean(&b, BooleanOp::Fuse).unwrap();
    assert_eq!(r.solid.face_count(), 6);
    close(r.solid.volume().unwrap(), 24000.0 + 30.0 * 30.0 * 20.0, 1e-6);
}

#[test]
fn boolean_disjoint_fuse_is_refused_not_corrupted() {
    let a = bx();
    let b = Solid::cuboid(DVec3::new(100.0, 0.0, 0.0), DVec3::new(110.0, 10.0, 10.0)).unwrap();
    assert!(matches!(
        a.boolean(&b, BooleanOp::Fuse),
        Err(KernelError::SplitResult) | Err(KernelError::InvalidResult)
    ));
}

#[test]
fn boolean_common() {
    let a = bx();
    let b = Solid::cuboid(DVec3::new(30.0, 20.0, 10.0), DVec3::new(50.0, 40.0, 30.0)).unwrap();
    let r = a.boolean(&b, BooleanOp::Common).unwrap();
    close(r.solid.volume().unwrap(), 10.0 * 10.0 * 10.0, 1e-6);
}

// ---- points 10-14 probes ----

#[test]
fn probe_10_section_box_and_cylinder() {
    let segs = bx().section(DVec3::new(0.0, 0.0, 10.0), DVec3::Z, 0.01).unwrap();
    assert_eq!(segs.len(), 4);
    let per: f64 = segs
        .iter()
        .map(|s| match s {
            SectionCurve::Line { a, b } => a.distance(*b),
            _ => panic!("expected lines"),
        })
        .sum();
    close(per, 140.0, 1e-6);

    let c = Solid::cylinder(5.0, 20.0, DVec3::ZERO, DVec3::Z).unwrap();
    let segs = c.section(DVec3::new(0.0, 0.0, 10.0), DVec3::Z, 0.01).unwrap();
    assert_eq!(segs.len(), 1);
    match segs[0] {
        SectionCurve::Circle { radius, .. } => close(radius, 5.0, 1e-9),
        SectionCurve::Arc { .. } => {}
        _ => panic!("expected a circle or arc"),
    }
}

#[test]
fn probe_11_fillet_and_chamfer() {
    let s = bx();
    let vertical: Vec<EdgeId> = s
        .edges()
        .filter(|e| {
            let i = s.edge_info(*e).unwrap();
            (i.start.x - i.end.x).abs() < 1e-9 && (i.start.y - i.end.y).abs() < 1e-9
        })
        .collect();
    assert_eq!(vertical.len(), 4);
    let r = s.fillet(&vertical, 3.0).unwrap();
    close(
        r.solid.volume().unwrap(),
        24000.0 - 4.0 * (9.0 - std::f64::consts::PI * 9.0 / 4.0) * 20.0,
        1e-6,
    );
    assert_eq!(r.solid.face_count(), 10);
    let c = s.chamfer(&vertical, 2.0).unwrap();
    close(c.solid.volume().unwrap(), 24000.0 - 4.0 * 2.0 * 20.0, 1e-6);
    assert!(
        s.fillet(&vertical, 100.0).is_err(),
        "oversized fillet must fail cleanly"
    );
}

#[test]
fn probe_12_step_and_brep_roundtrip() {
    let s = bx().pull_face(top_face(&bx()), 5.0).unwrap().solid;
    let step = s.write_step().unwrap();
    assert!(String::from_utf8_lossy(&step).contains("ISO-10303-21"));
    let back = Solid::read_step(&step).unwrap();
    close(back.volume().unwrap(), s.volume().unwrap(), 1e-6);
    assert_eq!(back.face_count(), s.face_count());
    let brep = s.write_brep().unwrap();
    let back = Solid::read_brep(&brep).unwrap();
    close(back.volume().unwrap(), s.volume().unwrap(), 1e-6);
}

#[test]
fn probe_13_sew_faces_into_solid() {
    let s = bx();
    let src: Vec<(&Solid, FaceId)> = s.faces().map(|f| (&s, f)).collect();
    let sewn = Solid::sew_faces_of(&src, 1e-6).unwrap();
    close(sewn.volume().unwrap().abs(), 24000.0, 1e-6);
    assert!(sewn.is_valid());
    // five faces cannot close a body
    let five: Vec<(&Solid, FaceId)> = s.faces().take(5).map(|f| (&s, f)).collect();
    assert!(Solid::sew_faces_of(&five, 1e-6).is_err());
}

#[test]
fn probe_14_history_through_boolean() {
    let a = bx();
    let hole = Solid::cylinder(4.0, 30.0, DVec3::new(20.0, 15.0, -5.0), DVec3::Z).unwrap();
    let r = a.boolean(&hole, BooleanOp::Cut).unwrap();
    // Side walls are untouched: each maps to exactly one result face.
    for f in a.faces() {
        let i = a.face_info(f).unwrap();
        if i.normal.z.abs() < 0.5 {
            assert_eq!(r.history.successors(0, f).len(), 1);
        }
    }
    // The top face is modified (now has a hole) and still has a successor with one inner wire.
    let succ = r.history.successors(0, top_face(&a));
    assert_eq!(succ.len(), 1);
    assert_eq!(r.solid.face_info(succ[0]).unwrap().inner_wires, 1);
}

#[test]
fn queries_face_and_edge_kinds() {
    let c = Solid::cylinder(5.0, 10.0, DVec3::ZERO, DVec3::Z).unwrap();
    let kinds: Vec<FaceKind> = c.faces().map(|f| c.face_info(f).unwrap().kind).collect();
    assert_eq!(kinds.iter().filter(|k| **k == FaceKind::Plane).count(), 2);
    assert_eq!(kinds.iter().filter(|k| **k == FaceKind::Cylinder).count(), 1);
    assert!(c
        .edges()
        .any(|e| c.edge_info(e).unwrap().kind == EdgeKind::Circle));
    let top = top_face(&c);
    assert_eq!(c.face_edges(top).unwrap().len(), 1);
    let outline = c.face_outline(top, 0.05).unwrap();
    assert_eq!(outline.len(), 1);
    assert!(outline[0].len() > 8);
}

fn plate_with_hole(d: f64) -> Solid {
    let hole = Solid::cylinder(d / 2.0, 30.0, DVec3::new(20.0, 15.0, -5.0), DVec3::Z).unwrap();
    bx().boolean(&hole, BooleanOp::Cut).unwrap().solid
}

#[test]
fn resize_through_hole_reference_volumes() {
    let s = plate_with_hole(10.0);
    let cyl = s
        .faces()
        .find(|f| s.face_info(*f).unwrap().kind == FaceKind::Cylinder)
        .unwrap();
    let info = s.cylinder_info(cyl).unwrap().unwrap();
    assert!(info.is_hole);
    close(info.height, 20.0, 1e-6);
    let bigger = s.set_cylinder_diameter(cyl, 12.0).unwrap().solid;
    close(bigger.volume().unwrap(), 21738.05, 0.01);
    assert!(bigger.is_valid());
    let smaller = s.set_cylinder_diameter(cyl, 8.0).unwrap().solid;
    close(smaller.volume().unwrap(), 22994.69, 0.01);
    assert_eq!(smaller.face_count(), 7);
    assert!(s.set_cylinder_diameter(cyl, 0.0).is_err());
    assert!(
        s.set_cylinder_diameter(cyl, 60.0).is_err(),
        "a hole wider than the plate must fail cleanly"
    );
    close(
        s.volume().unwrap(),
        24000.0 - std::f64::consts::PI * 25.0 * 20.0,
        1e-6,
    );
}

#[test]
fn resize_pin_and_blind_hole() {
    let pin = Solid::cylinder(5.0, 10.0, DVec3::new(20.0, 15.0, 20.0), DVec3::Z).unwrap();
    let s = bx().boolean(&pin, BooleanOp::Fuse).unwrap().solid;
    let cyl = s
        .faces()
        .find(|f| s.face_info(*f).unwrap().kind == FaceKind::Cylinder)
        .unwrap();
    assert!(!s.cylinder_info(cyl).unwrap().unwrap().is_hole);
    let bigger = s.set_cylinder_diameter(cyl, 14.0).unwrap().solid;
    close(
        bigger.volume().unwrap(),
        24000.0 + std::f64::consts::PI * 49.0 * 10.0,
        1e-3,
    );
    let smaller = s.set_cylinder_diameter(cyl, 6.0).unwrap().solid;
    close(
        smaller.volume().unwrap(),
        24000.0 + std::f64::consts::PI * 9.0 * 10.0,
        1e-3,
    );
    // the disc on top of the pin finds its cylinder
    let top = s
        .faces()
        .find(|f| {
            let i = s.face_info(*f).unwrap();
            i.kind == FaceKind::Plane && i.origin.z > 29.0
        })
        .unwrap();
    let (partner, r) = s.disc_partner(top).unwrap().unwrap();
    assert_eq!(partner, cyl);
    close(r, 5.0, 1e-9);
    // blind hole
    let hole = Solid::cylinder(4.0, 8.0, DVec3::new(20.0, 15.0, 12.0), DVec3::Z).unwrap();
    let b = bx().boolean(&hole, BooleanOp::Cut).unwrap().solid;
    let cyl = b
        .faces()
        .find(|f| b.face_info(*f).unwrap().kind == FaceKind::Cylinder)
        .unwrap();
    let w = b.set_cylinder_diameter(cyl, 12.0).unwrap().solid;
    close(
        w.volume().unwrap(),
        24000.0 - std::f64::consts::PI * 36.0 * 8.0,
        1e-3,
    );
    let n = b.set_cylinder_diameter(cyl, 4.0).unwrap().solid;
    close(
        n.volume().unwrap(),
        24000.0 - std::f64::consts::PI * 4.0 * 8.0,
        1e-3,
    );
}

#[test]
fn partial_cylinders_are_not_resizable() {
    let s = bx();
    let vertical: Vec<EdgeId> = s
        .edges()
        .filter(|e| {
            let i = s.edge_info(*e).unwrap();
            (i.start.x - i.end.x).abs() < 1e-9 && (i.start.y - i.end.y).abs() < 1e-9
        })
        .collect();
    let f = s.fillet(&vertical[..1], 5.0).unwrap().solid;
    let cyl = f
        .faces()
        .find(|x| f.face_info(*x).unwrap().kind == FaceKind::Cylinder)
        .unwrap();
    assert!(f.cylinder_info(cyl).unwrap().is_none());
}

#[test]
fn single_edge_fillet_and_chamfer_reference_volumes() {
    let s = bx();
    let e = s
        .edges()
        .find(|e| {
            let i = s.edge_info(*e).unwrap();
            (i.start.x - i.end.x).abs() < 1e-9 && (i.start.y - i.end.y).abs() < 1e-9 && i.length > 19.0
        })
        .unwrap();
    close(
        s.fillet(&[e], 5.0).unwrap().solid.volume().unwrap(),
        23892.70,
        0.01,
    );
    close(
        s.chamfer(&[e], 5.0).unwrap().solid.volume().unwrap(),
        23750.0,
        0.01,
    );
    assert!(s.fillet(&[e], 50.0).is_err());
    assert!(s.chamfer(&[e], -1.0).is_err());
}

#[test]
fn ascii_stl_has_one_facet_per_triangle() {
    let m = bx().mesh(0.1, 0.5).unwrap();
    let mut b = Vec::new();
    m.write_stl_ascii(&mut b).unwrap();
    let t = String::from_utf8(b).unwrap();
    assert!(t.starts_with("solid rolidium"));
    assert_eq!(t.matches("facet normal").count(), m.triangle_count());
}

#[test]
fn cylinder_info_works_after_meshing() {
    let s = plate_with_hole(12.0);
    let cyl = s
        .faces()
        .find(|f| s.face_info(*f).unwrap().kind == FaceKind::Cylinder)
        .unwrap();
    let before = s.cylinder_info(cyl).unwrap();
    let _ = s.display_mesh().unwrap();
    let after = s.cylinder_info(cyl).unwrap();
    assert!(before.is_some() && after.is_some(), "{before:?} {after:?}");
    let i = s.face_info(cyl).unwrap();
    eprintln!("area {} expected {}", i.area, std::f64::consts::TAU * 6.0 * 20.0);
}

// ---- editing finished rounds and chamfers ----

fn one_vertical_edge(s: &Solid) -> EdgeId {
    s.edges()
        .find(|e| {
            let i = s.edge_info(*e).unwrap();
            (i.start.x - i.end.x).abs() < 1e-9
                && (i.start.y - i.end.y).abs() < 1e-9
                && i.start.x > 39.0
                && i.start.y > 29.0
        })
        .unwrap()
}

#[test]
fn edit_round_and_chamfer_reference_volumes() {
    for chamfer in [false, true] {
        let s = bx();
        let e = one_vertical_edge(&s);
        let r = if chamfer {
            s.chamfer(&[e], 5.0).unwrap()
        } else {
            s.fillet(&[e], 5.0).unwrap()
        };
        let faces = r.history.new_faces(r.solid.face_count());
        assert_eq!(faces.len(), 1, "one new face is the blend");
        let v = |r: &OpResult| r.solid.volume().unwrap();
        if chamfer {
            close(v(&r), 24000.0 - 0.5 * 25.0 * 20.0, 1e-6);
        } else {
            close(v(&r), 23892.7, 0.05);
        }
        // Change the size (R5 -> R8 is 23725.31, chamfer 5 -> 3 is 23910).
        let bigger = r
            .solid
            .edit_blend(&faces, if chamfer { 3.0 } else { 8.0 }, chamfer)
            .unwrap();
        if chamfer {
            close(v(&bigger), 23910.0, 1e-6);
        } else {
            close(v(&bigger), 23725.31, 0.01);
        }
        assert!(bigger.solid.is_valid());
        // The new blend faces are recognisable again.
        let again = bigger.history.new_faces(bigger.solid.face_count());
        assert_eq!(again.len(), 1);
        // Size 0 restores the sharp edge.
        let sharp = bigger.solid.edit_blend(&again, 0.0, chamfer).unwrap();
        close(v(&sharp), 24000.0, 1e-6);
        assert_eq!(sharp.solid.face_count(), 6);
    }
}

#[test]
fn edit_round_after_other_changes_and_failures() {
    let s = bx();
    let e = one_vertical_edge(&s);
    let r = s.fillet(&[e], 5.0).unwrap();
    let blend = r.history.new_faces(r.solid.face_count());
    // Pull the top face up: the round must still be found through the history.
    let top = top_face(&r.solid);
    let p = r.solid.pull_face(top, 10.0).unwrap();
    let blend2: Vec<FaceId> = blend.iter().flat_map(|f| p.history.successors(0, *f)).collect();
    assert!(!blend2.is_empty());
    let before = p.solid.volume().unwrap();
    let edited = p.solid.edit_blend(&blend2, 8.0, false).unwrap();
    assert!(edited.solid.volume().unwrap() < before);
    assert!(edited.solid.is_valid());
    // Too large: refused, the solid is unchanged.
    assert!(p.solid.edit_blend(&blend2, 200.0, false).is_err());
    close(p.solid.volume().unwrap(), before, 1e-9);
    // Negative and NaN sizes are refused.
    assert!(p.solid.edit_blend(&blend2, -1.0, false).is_err());
    assert!(p.solid.edit_blend(&blend2, f64::NAN, false).is_err());
    assert!(p.solid.edit_blend(&[], 3.0, false).is_err());
}

#[test]
fn edit_round_of_several_edges_and_corner_blends() {
    let s = bx();
    // four vertical edges in one round: all four faces are one feature
    let vertical: Vec<EdgeId> = s
        .edges()
        .filter(|e| {
            let i = s.edge_info(*e).unwrap();
            (i.start.x - i.end.x).abs() < 1e-9 && (i.start.y - i.end.y).abs() < 1e-9
        })
        .collect();
    let r = s.fillet(&vertical, 3.0).unwrap();
    let faces = r.history.new_faces(r.solid.face_count());
    assert_eq!(faces.len(), 4);
    let e = r.solid.edit_blend(&faces, 5.0, false).unwrap();
    close(
        e.solid.volume().unwrap(),
        24000.0 - 4.0 * (25.0 - std::f64::consts::PI * 25.0 / 4.0) * 20.0,
        0.05,
    );
    let back = r.solid.edit_blend(&faces, 0.0, false).unwrap();
    close(back.solid.volume().unwrap(), 24000.0, 1e-6);

    // all twelve edges: corner blends meet at the vertices
    let all: Vec<EdgeId> = s.edges().collect();
    let r = s.fillet(&all, 2.0).unwrap();
    let faces = r.history.new_faces(r.solid.face_count());
    assert!(faces.len() > 12, "edges and corner blends");
    let before = r.solid.volume().unwrap();
    match r.solid.edit_blend(&faces, 3.0, false) {
        Ok(e) => {
            assert!(e.solid.volume().unwrap() < before);
            assert!(e.solid.is_valid());
        }
        Err(err) => eprintln!("corner blends cannot be re-rounded: {err}"),
    }
    let sharp = r.solid.edit_blend(&faces, 0.0, false);
    match sharp {
        Ok(e) => close(e.solid.volume().unwrap(), 24000.0, 1e-3),
        Err(err) => eprintln!("corner blends cannot be removed: {err}"),
    }
}

#[test]
fn tangent_chain_is_reported_before_rounding() {
    let s = bx();
    // an isolated box edge is its own chain
    let e = s.edges().next().unwrap();
    assert_eq!(s.blend_chain(&[e], false).unwrap(), vec![e]);
    // After rounding two opposite top edges, the top edge between them continues them tangentially:
    // OCCT would round the whole chain (the rounds, and the vertical edges below), so it is reported.
    let along_x = |i: &EdgeInfo| (i.end - i.start).x.abs() > 1.0;
    let x_edges: Vec<EdgeId> = s
        .edges()
        .filter(|e| {
            let i = s.edge_info(*e).unwrap();
            i.start.z > 19.9 && i.end.z > 19.9 && along_x(&i)
        })
        .collect();
    assert_eq!(x_edges.len(), 2);
    let t = s.fillet(&x_edges, 4.0).unwrap().solid;
    let y_edge = t
        .edges()
        .find(|e| {
            let i = t.edge_info(*e).unwrap();
            (i.end - i.start).y.abs() > 1.0 && i.start.z > 19.9 && i.start.x < 1.0
        })
        .unwrap();
    let chain = t.blend_chain(&[y_edge], false).unwrap();
    assert!(chain.len() > 1 && chain.contains(&y_edge), "{chain:?}");
    assert_eq!(t.blend_chain(&[y_edge], true).unwrap().len(), chain.len());
}

// ---- concave (inner) edges ----

fn pocket_box() -> Solid {
    let b = bx();
    let tool = Solid::cuboid(DVec3::new(10.0, 10.0, 15.0), DVec3::new(30.0, 20.0, 25.0)).unwrap();
    b.boolean(&tool, BooleanOp::Cut).unwrap().solid
}

fn stepped_cylinder() -> Solid {
    let a = Solid::cylinder(15.0, 10.0, DVec3::ZERO, DVec3::Z).unwrap();
    let b = Solid::cylinder(10.0, 10.0, DVec3::new(0.0, 0.0, 10.0), DVec3::Z).unwrap();
    a.boolean(&b, BooleanOp::Fuse).unwrap().solid
}

fn concave_vertical_edges(s: &Solid) -> Vec<EdgeId> {
    s.edges()
        .filter(|e| {
            let i = s.edge_info(*e).unwrap();
            (i.start.x - i.end.x).abs() < 1e-9
                && (i.start.y - i.end.y).abs() < 1e-9
                && (i.start.z - i.end.z).abs() > 1.0
                && i.start.x > 1.0
                && i.start.x < 39.0
        })
        .collect()
}

#[test]
fn round_and_chamfer_inner_edges_of_a_pocket() {
    let p = pocket_box();
    close(p.volume().unwrap(), 23000.0, 1e-6);
    let edges = concave_vertical_edges(&p);
    assert_eq!(edges.len(), 4);
    let r = p.fillet(&edges, 2.0).unwrap();
    close(r.solid.volume().unwrap(), 23017.17, 0.01);
    let faces = r.history.new_faces(r.solid.face_count());
    assert_eq!(faces.len(), 4);
    // change, then remove
    let e = r.solid.edit_blend(&faces, 3.0, false).unwrap();
    close(
        e.solid.volume().unwrap(),
        23000.0 + 4.0 * 9.0 * (1.0 - std::f64::consts::PI / 4.0) * 5.0,
        0.01,
    );
    let faces2 = e.history.new_faces(e.solid.face_count());
    let back = e.solid.edit_blend(&faces2, 0.0, false).unwrap();
    close(back.solid.volume().unwrap(), 23000.0, 1e-6);
    assert_eq!(back.solid.face_count(), p.face_count());
    // chamfer, and back
    let c = p.chamfer(&edges, 2.0).unwrap();
    close(c.solid.volume().unwrap(), 23000.0 + 4.0 * 2.0 * 5.0, 1e-6);
    let cf = c.history.new_faces(c.solid.face_count());
    let c0 = c.solid.edit_blend(&cf, 0.0, true).unwrap();
    close(c0.solid.volume().unwrap(), 23000.0, 1e-6);
}

#[test]
fn round_and_chamfer_circular_inner_edge() {
    let s = stepped_cylinder();
    let base = 3250.0 * std::f64::consts::PI;
    close(s.volume().unwrap(), base, 1e-6);
    // the circle of radius 10 at z = 10 is the concave corner
    let corner = s
        .edges()
        .find(|e| {
            let i = s.edge_info(*e).unwrap();
            i.kind == EdgeKind::Circle && (i.radius - 10.0).abs() < 1e-6 && (i.start.z - 10.0).abs() < 1e-6
        })
        .unwrap();
    let r = s.fillet(&[corner], 2.0).unwrap();
    // added volume = the corner region between the walls and the fillet arc, revolved (checked by integration below)
    let added = r.solid.volume().unwrap() - base;
    assert!(added > 0.0);
    let by_integral = {
        let n = 200_000;
        let mut v = 0.0;
        for i in 0..n {
            // distance from the wall d in (0, 2): the corner region height is 2 - sqrt(4 - (2 - d)^2)
            let d = (i as f64 + 0.5) / n as f64 * 2.0;
            let h = 2.0 - (4.0 - (2.0 - d) * (2.0 - d)).sqrt();
            v += std::f64::consts::TAU * (10.0 + d) * h * (2.0 / n as f64);
        }
        v
    };
    close(added, by_integral, 0.02);
    close(r.solid.volume().unwrap(), 10266.52, 0.5);
    let faces = r.history.new_faces(r.solid.face_count());
    assert_eq!(faces.len(), 1);
    let sharp = r.solid.edit_blend(&faces, 0.0, false).unwrap();
    close(sharp.solid.volume().unwrap(), base, 1e-3);
    let c = s.chamfer(&[corner], 2.0).unwrap();
    let cf = c.history.new_faces(c.solid.face_count());
    let c0 = c.solid.edit_blend(&cf, 0.0, true).unwrap();
    close(c0.solid.volume().unwrap(), base, 1e-3);
}

fn top_edges_of_cube(s: &Solid) -> (EdgeId, EdgeId, EdgeId) {
    let top: Vec<EdgeId> = s
        .edges()
        .filter(|e| {
            let i = s.edge_info(*e).unwrap();
            i.start.z > 19.9 && i.end.z > 19.9
        })
        .collect();
    let pick = |f: &dyn Fn(&EdgeInfo) -> bool| {
        top.iter()
            .copied()
            .find(|e| f(&s.edge_info(*e).unwrap()))
            .unwrap()
    };
    (
        pick(&|i| (i.end - i.start).x.abs() > 1.0 && i.start.y < 1.0),
        pick(&|i| (i.end - i.start).x.abs() > 1.0 && i.start.y > 19.0),
        pick(&|i| (i.end - i.start).y.abs() > 1.0 && i.start.x < 1.0),
    )
}

#[test]
fn rounding_next_to_existing_rounds_touches_only_selected_edges() {
    for (chamfer, middle) in [(false, 3.0), (false, 2.0), (true, 3.0)] {
        let s = Solid::cuboid(DVec3::ZERO, DVec3::splat(20.0)).unwrap();
        let (front, back, left) = top_edges_of_cube(&s);
        let first = if chamfer {
            s.chamfer(&[front, back], 3.0)
        } else {
            s.fillet(&[front, back], 3.0)
        }
        .unwrap();
        let faces = first.history.new_faces(first.solid.face_count());
        // the edge between them, in the rounded solid
        let t = &first.solid;
        let mid_edge = t
            .edges()
            .find(|e| {
                let i = t.edge_info(*e).unwrap();
                (i.end - i.start).y.abs() > 1.0 && i.start.z > 19.9 && i.start.x < 1.0
            })
            .unwrap();
        // the plain operation drags the vertical edges along
        let plain = if chamfer {
            t.chamfer(&[mid_edge], middle)
        } else {
            t.fillet(&[mid_edge], middle)
        }
        .unwrap();
        let existing = [BlendInput {
            faces: faces.clone(),
            size: 3.0,
            chamfer,
        }];
        let combo = t
            .blend_with_neighbours(&existing, &[mid_edge], middle, chamfer)
            .unwrap();
        if chamfer {
            // chamfer faces are not tangent to their neighbours, so the chain never runs on: nothing special to do
            assert!(combo.is_none());
            continue;
        }
        let combo = combo.expect("the chain runs through the existing round");
        // reference: the three edges of the plain cube in one operation
        let reference = if chamfer {
            s.chamfer(&[front, back, left], 3.0)
        } else {
            s.fillet_radii(&[(front, 3.0), (back, 3.0), (left, middle)])
        };
        let v = combo.result.solid.volume().unwrap();
        if let Ok(r) = reference {
            close(v, r.solid.volume().unwrap(), 1e-3);
            assert_eq!(combo.result.solid.face_count(), r.solid.face_count());
        }
        assert!(
            (v - plain.solid.volume().unwrap()).abs() > 1e-3,
            "the plain result also rounds the vertical edges"
        );
        assert_eq!(combo.groups.len(), 2);
        assert!(combo.groups.iter().all(|g| !g.faces.is_empty()));
        assert!(combo.groups.iter().any(|g| g.existing.is_none()));
        // each blend stays editable: the new faces are separate and removable
        let g_new = combo.groups.iter().find(|g| g.existing.is_none()).unwrap();
        let back0 = combo.result.solid.edit_blend(&g_new.faces, 0.0, chamfer).unwrap();
        assert!(back0.solid.volume().unwrap() > v + 1e-9 && back0.solid.is_valid());
    }
}

// ---- Move and Revolve ----

#[test]
fn moved_keeps_volume_and_history() {
    let s = bx();
    let r = s
        .moved(DVec3::new(10.0, -5.0, 3.0), DVec3::ZERO, DVec3::Z, 0.0)
        .unwrap();
    close(r.solid.volume().unwrap(), 24000.0, 1e-6);
    let p = r.solid.props().unwrap();
    assert!((p.min - DVec3::new(10.0, -5.0, 3.0)).length() < 1e-9);
    assert!((p.max - DVec3::new(50.0, 25.0, 23.0)).length() < 1e-9);
    assert_eq!(r.history.links.len(), 6);
    // a quarter turn about Z through the box centre swaps the sizes in X and Y
    let c = DVec3::new(20.0, 15.0, 10.0);
    let q = s
        .moved(DVec3::ZERO, c, DVec3::Z, std::f64::consts::FRAC_PI_2)
        .unwrap();
    let p = q.solid.props().unwrap();
    assert!(((p.max - p.min) - DVec3::new(30.0, 40.0, 20.0)).length() < 1e-9);
    close(q.solid.volume().unwrap(), 24000.0, 1e-6);
}

fn rect_profile(x0: f64, x1: f64, y0: f64, y1: f64) -> Profile {
    let p = [[x0, y0], [x1, y0], [x1, y1], [x0, y1]];
    Profile {
        loops: vec![(0..4)
            .map(|i| ProfileSegment::Line {
                a: p[i],
                b: p[(i + 1) % 4],
            })
            .collect()],
    }
}

#[test]
fn revolve_reference_volumes() {
    // plane XZ-like: profile plane z-up with x_dir = X, axis = the Z axis through the origin
    let origin = DVec3::ZERO;
    let n = -DVec3::Y; // plane XZ (x_dir X, y_dir = n x x = Z)
    let rev = |p: &Profile, angle: f64| {
        Solid::revolve_profile(p, origin, n, DVec3::X, DVec3::ZERO, DVec3::Z, angle).unwrap()
    };
    // 10 x 20 rectangle, 5 from the axis, long side parallel to the axis
    let full = rev(&rect_profile(5.0, 15.0, 0.0, 20.0), std::f64::consts::TAU);
    close(full.volume().unwrap(), 12566.37, 0.01);
    let quarter = rev(&rect_profile(5.0, 15.0, 0.0, 20.0), std::f64::consts::FRAC_PI_2);
    close(quarter.volume().unwrap(), 3141.59, 0.01);
    // touching the axis: a solid cylinder of radius 10 and height 20 would be 6283.19 for width 10? (pi r^2 h, r = 10, w = 10 -> pi*100*20/... )
    let touching = rev(&rect_profile(0.0, 10.0, 0.0, 20.0), std::f64::consts::TAU);
    close(
        touching.volume().unwrap(),
        std::f64::consts::PI * 100.0 * 20.0,
        0.01,
    );
}

#[test]
fn angled_chamfer_reference_volumes_and_flip() {
    let s = bx();
    let e = one_vertical_edge(&s);
    let vol = |a: f64, flip: bool| {
        s.chamfer_angled(&[e], 5.0, a, flip)
            .unwrap()
            .solid
            .volume()
            .unwrap()
    };
    close(vol(45.0, false), 23750.0, 1e-6);
    close(vol(30.0, false), 23855.66, 0.01);
    close(vol(60.0, false), 23566.99, 0.01);
    close(vol(30.0, true), vol(30.0, false), 1e-6);
    // flipped: the long and the short side swap, so the chamfer faces differ
    let a = s.chamfer_angled(&[e], 5.0, 30.0, false).unwrap();
    let b = s.chamfer_angled(&[e], 5.0, 30.0, true).unwrap();
    let face = |r: &OpResult| {
        let f = r.history.new_faces(r.solid.face_count())[0];
        r.solid.face_info(f).unwrap().normal
    };
    assert!((face(&a) - face(&b)).length() > 1e-3);
    assert!(s.chamfer_angled(&[e], 5.0, 0.0, false).is_err());
}

#[test]
fn boolean_pieces_split_and_overlap() {
    let a = Solid::cuboid(DVec3::ZERO, DVec3::splat(20.0)).unwrap();
    let b = Solid::cuboid(DVec3::new(10.0, 0.0, 0.0), DVec3::new(30.0, 20.0, 20.0)).unwrap();
    let v = |op| a.boolean(&b, op).unwrap().solid.volume().unwrap();
    close(v(BooleanOp::Fuse), 12000.0, 1e-6);
    close(v(BooleanOp::Cut), 4000.0, 1e-6);
    close(v(BooleanOp::Common), 4000.0, 1e-6);
    // a slot through the middle splits a plate into two pieces
    let plate = Solid::cuboid(DVec3::ZERO, DVec3::new(40.0, 10.0, 5.0)).unwrap();
    let slot = Solid::cuboid(DVec3::new(15.0, -1.0, -1.0), DVec3::new(25.0, 11.0, 6.0)).unwrap();
    assert!(plate.boolean(&slot, BooleanOp::Cut).is_err());
    let pieces = plate.boolean_pieces(&slot, BooleanOp::Cut).unwrap();
    assert_eq!(pieces.len(), 2);
    close(
        pieces[0].volume().unwrap() + pieces[1].volume().unwrap(),
        2000.0 - 500.0,
        1e-6,
    );
}

// ---- Loft ----

fn circle_profile(r: f64) -> Profile {
    Profile {
        loops: vec![vec![ProfileSegment::Circle {
            center: [0.0, 0.0],
            radius: r,
        }]],
    }
}

fn ring_profile(ro: f64, ri: f64) -> Profile {
    Profile {
        loops: vec![
            vec![ProfileSegment::Circle {
                center: [0.0, 0.0],
                radius: ro,
            }],
            vec![ProfileSegment::Circle {
                center: [0.0, 0.0],
                radius: ri,
            }],
        ],
    }
}

fn at(p: &Profile, z: f64) -> LoftInput<'_> {
    // `LoftInput` borrows the profile; the plane is horizontal at height z
    LoftInput::Profile {
        profile: p,
        origin: DVec3::new(0.0, 0.0, z),
        normal: DVec3::Z,
        x_dir: DVec3::X,
    }
}

#[test]
fn loft_reference_volumes() {
    let (a, b) = (circle_profile(20.0), circle_profile(10.0));
    let cone = Solid::loft(&[at(&a, 0.0), at(&b, 30.0)], true).unwrap();
    close(cone.volume().unwrap(), 21991.15, 0.05);
    // the ruled loft of two coaxial circles is a real cone, not a spline surface
    let kinds: Vec<FaceKind> = cone.faces().map(|f| cone.face_info(f).unwrap().kind).collect();
    assert!(kinds.contains(&FaceKind::Cone), "{kinds:?}");
    let (ra, rb) = (ring_profile(20.0, 18.0), ring_profile(10.0, 8.0));
    let funnel = Solid::loft(&[at(&ra, 0.0), at(&rb, 30.0)], true).unwrap();
    close(funnel.volume().unwrap(), 5277.88, 0.05);
    // smooth through three profiles is valid
    let c = circle_profile(15.0);
    let s = Solid::loft(&[at(&a, 0.0), at(&c, 15.0), at(&b, 30.0)], false).unwrap();
    assert!(s.is_valid() && s.volume().unwrap() > 15000.0);
    // a 20 x 20 square to a circle of diameter 20 over 20 mm
    let sq = Profile::rectangle(20.0, 20.0);
    let sq_at = LoftInput::Profile {
        profile: &sq,
        origin: DVec3::new(-10.0, -10.0, 0.0),
        normal: DVec3::Z,
        x_dir: DVec3::X,
    };
    let circ = circle_profile(10.0);
    let tr = Solid::loft(&[sq_at, at(&circ, 20.0)], true).unwrap();
    let v = tr.volume().unwrap();
    assert!(tr.is_valid() && v > 6283.0 && v < 8000.0, "{v}");
    // different numbers of holes are refused
    let err = Solid::loft(&[at(&ra, 0.0), at(&b, 30.0)], true).unwrap_err();
    assert!(err.to_string().contains("same number of holes"), "{err}");
    assert!(Solid::loft(&[at(&a, 0.0)], true).is_err());
}
