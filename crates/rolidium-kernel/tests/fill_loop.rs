// Copyright (c) 2026 Rolid spol. s r.o.
// SPDX-License-Identifier: AGPL-3.0-only
//! The closed planar loop of edges (the Fill tool).
use rolidium_kernel::*;

fn pipe() -> Solid {
    let outer = Solid::cylinder(20.0, 30.0, DVec3::ZERO, DVec3::Z).unwrap();
    let bore = Solid::cylinder(12.0, 30.0, DVec3::ZERO, DVec3::Z).unwrap();
    outer.boolean(&bore, BooleanOp::Cut).unwrap().solid
}

fn circle_edges(s: &Solid, radius: f64, z: f64) -> Vec<EdgeId> {
    s.edges()
        .filter(|e| {
            let i = s.edge_info(*e).unwrap();
            i.kind == EdgeKind::Circle && (i.radius - radius).abs() < 1e-6 && (i.start.z - z).abs() < 1e-6
        })
        .collect()
}

#[test]
fn the_inner_circle_of_a_pipe_is_a_loop_of_its_own_facing_outwards() {
    let s = pipe();
    let e = circle_edges(&s, 12.0, 30.0)[0];
    let l = s.planar_loop(&[e]).unwrap();
    assert_eq!(l.edges.len(), 1);
    assert!((l.origin.z - 30.0).abs() < 1e-6, "{:?}", l.origin);
    assert!(
        (l.normal - DVec3::Z).length() < 1e-9,
        "outward is up on the top of the pipe: {:?}",
        l.normal
    );
    let c = l.edges[0].center.unwrap();
    assert!(c.x.abs() < 1e-6 && c.y.abs() < 1e-6 && (c.z - 30.0).abs() < 1e-6);
    assert!((l.edges[0].radius - 12.0).abs() < 1e-9);
    // the bottom one faces down
    let b = circle_edges(&s, 12.0, 0.0)[0];
    assert!((s.planar_loop(&[b]).unwrap().normal + DVec3::Z).length() < 1e-9);
}

#[test]
fn the_four_edges_of_a_face_form_a_loop_and_three_do_not() {
    let s = Solid::cuboid(DVec3::ZERO, DVec3::new(40.0, 30.0, 20.0)).unwrap();
    let top: Vec<EdgeId> = s
        .edges()
        .filter(|e| {
            let i = s.edge_info(*e).unwrap();
            i.start.z > 19.999 && i.end.z > 19.999
        })
        .collect();
    assert_eq!(top.len(), 4);
    let l = s.planar_loop(&top).unwrap();
    assert_eq!(l.edges.len(), 4);
    assert!((l.normal - DVec3::Z).length() < 1e-9);
    assert_eq!(s.planar_loop(&top[..3]), Err(LoopError::NotClosed));
    // a single edge of a box corner is ambiguous (three edges meet at every corner)
    assert_eq!(s.planar_loop(&top[..1]), Err(LoopError::Ambiguous));
}

#[test]
fn edges_that_are_not_in_one_plane_are_refused() {
    let s = Solid::cuboid(DVec3::ZERO, DVec3::new(40.0, 30.0, 20.0)).unwrap();
    // an edge of the top face, a vertical edge and an edge of the bottom face: they cannot be coplanar and closed
    let e = |f: &dyn Fn(&EdgeInfo) -> bool| s.edges().find(|e| f(&s.edge_info(*e).unwrap())).unwrap();
    let a =
        e(&|i| i.start.z > 19.9 && i.end.z > 19.9 && (i.start.x - i.end.x).abs() > 1.0 && i.start.y < 0.1);
    let b = e(&|i| (i.start.z - i.end.z).abs() > 19.0 && i.start.x < 0.1 && i.start.y < 0.1);
    let r = s.planar_loop(&[a, b]);
    assert!(
        matches!(r, Err(LoopError::NotClosed | LoopError::NotPlanar)),
        "{r:?}"
    );
}
