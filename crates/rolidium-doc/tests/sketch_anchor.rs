// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The tool "Sketch plane": the planes proposed at the end of a helix, the free plane turned in steps, a sketch laid on the end of a helix that
//! follows it when the helix changes (and the sweep made from it), and the file.
use rolidium_doc::rolidium_sketch::{DVec2, Entity, PlaneDef};
use rolidium_doc::*;
use rolidium_kernel::{DVec3, HelixDef};
use std::f64::consts::PI;

fn helix(pitch: f64, turns: f64) -> CurveDef {
    CurveDef::Helix(HelixDef {
        origin: DVec3::new(3.0, 2.0, 1.0),
        axis: DVec3::new(0.3, 0.1, 1.0).normalize(),
        x_dir: DVec3::X,
        radius: 10.0,
        pitch,
        turns,
        right: true,
        start_deg: 0.0,
        cone_deg: 0.0,
    })
}

#[test]
fn the_proposals_at_the_end_of_a_helix_are_in_the_order_of_the_tool() {
    // a helix whose axis is not along a base axis: the end lies outside the base planes
    let def = helix(4.0, 5.0);
    let f = EndFrame::of(&def, CurveEnd::End).unwrap();
    assert!(f.point.z.abs() > 1.0 && f.point.x.abs() > 1.0);
    let kinds = f.proposals(PlaneDef::base());
    let names: Vec<&str> = kinds.iter().map(|k| k.name()).collect();
    assert_eq!(
        names,
        [
            "Plane through the axis of the helix and the point",
            "Plane square to the curve",
            "Plane square to the axis of the helix",
            "Plane parallel to XY",
            "Plane parallel to XZ",
            "Plane parallel to YZ",
            "Free plane"
        ]
    );
    let (o, d) = f.axis.unwrap();
    // the first plane contains the axis and the point (its normal is square to the axis and to the radius)
    let p1 = f.plane(&kinds[0]).unwrap();
    assert!(p1.normal.dot(d).abs() < 1e-9, "the plane through the axis contains its direction");
    assert!(p1.normal.dot(f.point - o).abs() < 1e-9, "and the point");
    assert!((p1.origin - f.point).length() < 1e-12);
    // the second has the tangent for its normal
    let p2 = f.plane(&kinds[1]).unwrap();
    assert!((p2.normal - f.tangent).length() < 1e-9);
    // the third is square to the axis
    let p3 = f.plane(&kinds[2]).unwrap();
    assert!((p3.normal.dot(d).abs() - 1.0).abs() < 1e-9);
    // the parallel planes pass through the point
    for k in &kinds[3..6] {
        assert!((f.plane(k).unwrap().origin - f.point).length() < 1e-12);
    }
    // every plane has a normal of length 1 and an x direction square to it
    for k in &kinds {
        let p = f.plane(k).unwrap();
        assert!((p.normal.length() - 1.0).abs() < 1e-9 && p.normal.dot(p.x_dir).abs() < 1e-9, "{}", k.name());
    }
    // another curve: the plane square to it comes first (no plane through an axis)
    let line = CurveDef::Polyline {
        points: vec![DVec3::ZERO, DVec3::new(10.0, 5.0, 3.0)],
        radius: 0.0,
    };
    let f = EndFrame::of(&line, CurveEnd::End).unwrap();
    assert_eq!(f.proposals(PlaneDef::base())[0], PlaneKind::NormalToTangent);
    assert!(f.proposals(PlaneDef::base()).iter().all(|k| *k != PlaneKind::AxisPlane));
}

#[test]
fn a_free_plane_turns_in_steps_of_1_and_5_degrees() {
    let p = PlaneDef::base();
    for (step, raw, want) in [(1.0, 3.4, 3.0), (1.0, 3.6, 4.0), (5.0, 7.4, 5.0), (5.0, 7.6, 10.0), (15.0, 22.0, 15.0), (10.0, -14.0, -10.0)] {
        assert_eq!(in_steps(raw, step), want);
    }
    // five turns of 1 degree about the X axis give the plane turned by 5 degrees
    let mut q = p;
    for _ in 0..5 {
        q = turned(&q, DVec3::X, 1.0);
    }
    let five = turned(&p, DVec3::X, 5.0);
    assert!((q.normal - five.normal).length() < 1e-12);
    let [xy, xz, yz] = angles_to_base(&five);
    assert!((xy - 5.0).abs() < 1e-9 && (xz - 85.0).abs() < 1e-9 && (yz - 90.0).abs() < 1e-9, "{xy} {xz} {yz}");
    // an angle typed into the panel
    for which in 0..3 {
        let r = with_angle_to_base(&five, which, 30.0);
        assert!((angles_to_base(&r)[which] - 30.0).abs() < 1e-9, "plane {which}");
        assert!((r.normal.length() - 1.0).abs() < 1e-12 && r.normal.dot(r.x_dir).abs() < 1e-9);
    }
}

/// A sketch with a circle laid on the end of the helix, and the sweep of the circle along the helix as a link.
fn spring() -> (Document, CurveId, SketchId, BodyId) {
    let mut d = Document::new();
    let curve = d.apply(Command::AddCurve { def: helix(4.0, 5.0) }).unwrap().created_curve.unwrap();
    // the profile at the start of the path (the sweep starts there): the plane square to the curve
    let c = d
        .apply(Command::AddEntity {
            target: SketchTarget::Anchored {
                anchor: SketchAnchor {
                    curve,
                    end: CurveEnd::Start,
                    plane: PlaneKind::NormalToTangent,
                },
            },
            entity: Entity::Circle {
                center: DVec2::ZERO,
                radius: 1.0,
            },
        })
        .unwrap();
    let sk = c.created_sketch.unwrap();
    let kind = d
        .sweep_link(
            &LoftRef::Region { sketch: sk, region: 0 },
            &SweepPath::Curve(curve),
            false,
            SweepMode::NewBody,
        )
        .unwrap();
    let body = d
        .apply(Command::AddLink {
            kind,
            sources: Vec::new(),
            name: String::new(),
        })
        .unwrap()
        .touched_body
        .unwrap();
    (d, curve, sk, body)
}

fn wire_volume(pitch: f64) -> f64 {
    let len = 5.0 * ((2.0 * PI * 10.0).powi(2) + pitch * pitch).sqrt();
    PI * len
}

#[test]
fn a_sketch_on_the_end_of_a_helix_follows_it_and_the_sweep_is_calculated_again() {
    let (mut d, curve, sk, body) = spring();
    // the circle on the plane at the start of the helix makes the body
    let v = d.body(body).unwrap().solid.volume().unwrap();
    assert!((v - wire_volume(4.0)).abs() < 0.01 * wire_volume(4.0), "{v}");
    let start = |d: &Document| EndFrame::of(&d.curves()[0].def, CurveEnd::Start).unwrap();
    assert!((d.sketch(sk).unwrap().plane.origin - start(&d).point).length() < 1e-9);
    // another pitch and another start point: the helix is moved, the sketch goes with its end, and the body follows
    let moved = match helix(6.0, 5.0) {
        CurveDef::Helix(h) => CurveDef::Helix(HelixDef {
            origin: h.origin + DVec3::new(5.0, -4.0, 2.0),
            ..h
        }),
        _ => unreachable!(),
    };
    d.apply(Command::EditCurve { curve, def: moved }).unwrap();
    let f = start(&d);
    let plane = d.sketch(sk).unwrap().plane;
    assert!((plane.origin - f.point).length() < 1e-9, "the sketch moved with the end of the helix");
    assert!((plane.normal - f.tangent).length() < 1e-9, "and turned with its direction");
    assert!(d.state().links[0].error.is_none(), "{:?}", d.state().links[0].error);
    let v = d.body(body).unwrap().solid.volume().unwrap();
    assert!((v - wire_volume(6.0)).abs() < 0.01 * wire_volume(6.0), "the sweep was calculated again: {v}");
}

#[test]
fn the_anchor_of_a_sketch_is_saved_and_opened() {
    let (d, curve, sk, _) = spring();
    let bytes = file::save_to_bytes(&d, None, "test").unwrap();
    let mut d2 = Document::from_loaded(file::load_from_bytes(&bytes).unwrap());
    let a = d2.sketch(sk).unwrap().anchor.clone().unwrap();
    assert_eq!(a.curve, curve);
    assert_eq!(a.end, CurveEnd::Start);
    assert_eq!(a.plane, PlaneKind::NormalToTangent);
    // it still follows after the file was opened
    d2.apply(Command::EditCurve {
        curve,
        def: helix(7.0, 5.0),
    })
    .unwrap();
    let f = EndFrame::of(&d2.curves()[0].def, CurveEnd::Start).unwrap();
    assert!((d2.sketch(sk).unwrap().plane.normal - f.tangent).length() < 1e-9);
}

#[test]
fn a_free_plane_keeps_its_direction_and_the_point_follows_the_end() {
    let mut d = Document::new();
    let curve = d.apply(Command::AddCurve { def: helix(4.0, 5.0) }).unwrap().created_curve.unwrap();
    let free = turned(&PlaneDef::base(), DVec3::X, 30.0);
    let sk = d
        .apply(Command::AddEntity {
            target: SketchTarget::Anchored {
                anchor: SketchAnchor {
                    curve,
                    end: CurveEnd::End,
                    plane: PlaneKind::Free {
                        normal: free.normal,
                        x_dir: free.x_dir,
                    },
                },
            },
            entity: Entity::Circle {
                center: DVec2::ZERO,
                radius: 1.0,
            },
        })
        .unwrap()
        .created_sketch
        .unwrap();
    d.apply(Command::EditCurve { curve, def: helix(9.0, 6.0) }).unwrap();
    let p = d.sketch(sk).unwrap().plane;
    let f = EndFrame::of(&d.curves()[0].def, CurveEnd::End).unwrap();
    assert!((p.origin - f.point).length() < 1e-9);
    assert!((p.normal - free.normal).length() < 1e-9);
}
